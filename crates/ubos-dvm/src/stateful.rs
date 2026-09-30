//! Stateful adapter operations (STD-CONF-005): branches, commits, reads, merge, diff, state.

use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use ubos_model::Registry;
use ubos_store::Store;
use ubos_types::{BpaError, canonical, names};
use ubos_version::{ObjectState, Repo, Tx, at};

/// Default adapter clock (STD-CONF-002 `set_now`).
pub const DEFAULT_NOW: &str = "2026-01-01T00:00:00.000000Z";

/// State of the adapter besides the store.
pub struct Engine {
    /// The store (memory or PostgreSQL).
    pub store: Box<dyn Store>,
    /// Loaded classes.
    pub registry: Registry,
    /// Adapter clock.
    pub now: String,
    /// Classes that have instances on some branch (STD-KIND-080).
    pub instances: BTreeSet<String>,
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn tl_err(e: ubos_time::TimelineError) -> BpaError {
    BpaError::new(e.0, "timeline operation rejected")
}

impl Engine {
    /// A new engine over a store, reset.
    pub fn new(store: Box<dyn Store>) -> Result<Engine, BpaError> {
        let mut e = Engine {
            store,
            registry: Registry::new(),
            now: DEFAULT_NOW.into(),
            instances: BTreeSet::new(),
        };
        e.reset()?;
        Ok(e)
    }

    /// `reset`: empty store, genesis `main` at cseq 0.
    pub fn reset(&mut self) -> Result<(), BpaError> {
        self.store.reset()?;
        self.registry = Registry::new();
        self.now = DEFAULT_NOW.into();
        self.instances.clear();
        Repo::new(self.store.as_mut(), &self.registry).init()
    }

    fn today(&self) -> String {
        self.now[..10].to_string()
    }

    fn default_valid(&self, class: &str) -> String {
        match self.registry.class(class).map(|c| c.valid_time.as_str()) {
            Ok("instant") => self.now.clone(),
            _ => self.today(),
        }
    }

    /// `load_genesis` (STD-KIND-080: a kind does not change while instances exist).
    pub fn load_genesis(&mut self, classes: &Value) -> Result<Value, BpaError> {
        let list = classes.as_array().cloned().unwrap_or_default();
        let next = self.registry.load(&list)?;
        for d in &list {
            let name = s(d, "name").unwrap_or_default();
            if let (Ok(old), Ok(new)) = (self.registry.class(name), next.class(name)) {
                let used = self
                    .registry
                    .subclasses(name)
                    .iter()
                    .any(|c| self.instances.contains(c));
                if old.kind != new.kind && used {
                    return Err(BpaError::new("MODEL.KIND_IMMUTABLE", name.to_string()));
                }
            }
        }
        self.registry = next;
        Ok(Value::Bool(true))
    }

    /// `commit` (STD-TXN-001…024, STD-KIND-020…025, STD-TIME-011, -030).
    pub fn commit(&mut self, input: &Value) -> Result<Value, BpaError> {
        let changes = input
            .get("changes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if input.get("tx_time").is_some() || changes.iter().any(|c| c.get("tx_time").is_some()) {
            return Err(BpaError::new(
                "TIME.RESERVED_FIELD",
                "tx_time is assigned by the implementation",
            ));
        }
        let branch = s(input, "branch").unwrap_or("main").to_string();
        let today = self.today();
        let defaults: Vec<(String, String)> = self
            .registry
            .declarations()
            .iter()
            .filter_map(|d| s(d, "name"))
            .map(|n| (n.to_string(), self.default_valid(n)))
            .collect();
        let default_valid = move |c: &str| {
            defaults
                .iter()
                .find(|(n, _)| n == c)
                .map(|x| x.1.clone())
                .unwrap_or_default()
        };
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let mut tx = repo.begin(&branch)?;
        let mut result = json!({"cseq": tx.cseq, "versions": {}, "entries": {}, "retroactive": []});
        if let Some(p) = input.get("process")
            && p.get("serializable") == Some(&Value::Bool(true))
        {
            for (oid, vh) in p
                .get("reads")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default()
            {
                if tx
                    .state
                    .objects
                    .get(&oid)
                    .map(|o| Value::String(o.head.clone()))
                    != Some(vh)
                {
                    return Err(BpaError::new("TXN.READ_SET_CHANGED", oid));
                }
            }
        }
        let mut stale = Vec::new();
        for ch in &changes {
            if matches!(s(ch, "op"), Some("update" | "end"))
                && let (Some(oid), Some(b)) = (s(ch, "object_id"), s(ch, "base"))
                && tx.state.objects.get(oid).is_some_and(|o| o.head != b)
            {
                stale.push(oid.to_string());
            }
        }
        if !stale.is_empty() {
            stale.sort();
            return Err(BpaError::with_details(
                "TXN.HEAD_MOVED",
                json!({"objects": stale}),
            ));
        }
        for ch in &changes {
            match s(ch, "op").unwrap_or_default() {
                "create" => create(&mut repo, &mut tx, ch, &default_valid, &mut result)?,
                op @ ("update" | "end") => {
                    update(&mut repo, &mut tx, ch, op, &default_valid, &mut result)?
                }
                op @ ("append" | "reverse" | "adjust") => {
                    let (id, seq) = repo.append(&mut tx, ch, op, &today, &default_valid)?;
                    result["entries"][id] = json!({"seq": seq});
                }
                _ => return Err(BpaError::new("MODEL.INVALID_VALUE", "unknown change")),
            }
        }
        repo.check_double_entry(&tx)?;
        let classes: Vec<String> = tx
            .state
            .objects
            .values()
            .map(|o| o.class.clone())
            .chain(tx.appended.iter().map(|e| e.class.clone()))
            .collect();
        let head = tx.head;
        repo.write(tx, vec![head])?;
        self.instances.extend(classes);
        Ok(result)
    }

    /// `read` (STD-TIME-012).
    pub fn read(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main");
        let oid = s(input, "object_id").unwrap_or_default();
        let time = input.get("time").and_then(Value::as_u64);
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let st = repo.state_at(branch, time)?;
        let o = st
            .objects
            .get(oid)
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown object"))?;
        let a = s(input, "asof")
            .map(str::to_string)
            .unwrap_or_else(|| self.default_valid(&o.class));
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let seg = at(&o.timeline, &a)
            .ok_or_else(|| BpaError::new("QRY.NOT_FOUND_AT_TIME", "no segment at asof"))?;
        let v = repo.version(&seg.version)?;
        Ok(
            json!({"version_hash": seg.version, "body": v["body"], "slug": v["slug"], "valid": {"from": seg.from, "to": seg.to}}),
        )
    }

    /// `read_ledger` (STD-TIME-020).
    pub fn read_ledger(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main");
        let time = input.get("time").and_then(Value::as_u64);
        let key = canonical(input.get("ledger_key").unwrap_or(&Value::Null))?;
        let class = s(input, "class").unwrap_or_default();
        let st = Repo::new(self.store.as_mut(), &self.registry).state_at(branch, time)?;
        let asof = s(input, "asof");
        let mut out = Vec::new();
        for e in st.entries(class, &key) {
            if asof.is_some_and(|a| e.valid_at.as_str() > a) {
                continue;
            }
            let mut r = json!({"entry_id": e.entry_id, "seq": e.seq, "valid_at": e.valid_at, "body": e.body, "entry_hash": e.entry_hash});
            if let Some(c) = &e.corrects {
                r["corrects"] = json!(c);
            }
            if let Some(c) = &e.correction_type {
                r["correction_type"] = json!(c);
            }
            out.push(r);
        }
        Ok(json!({"entries": out}))
    }

    /// `create_branch`, `merge`, `diff` and `export_state`.
    pub fn other(&mut self, op: &str, input: &Value) -> Result<Value, BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        match op {
            "create_branch" => repo.create_branch(
                s(input, "name").unwrap_or_default(),
                s(input, "from").unwrap_or("main"),
                input.get("time").and_then(Value::as_u64),
            ),
            "merge" => {
                let r = repo.merge(
                    s(input, "source").unwrap_or_default(),
                    s(input, "target").unwrap_or("main"),
                )?;
                Ok(r)
            }
            "diff" => repo.diff(
                input.get("from").unwrap_or(&Value::Null),
                input.get("to").unwrap_or(&Value::Null),
            ),
            "export_state" => {
                let st = repo.state_at(
                    s(input, "branch").unwrap_or("main"),
                    input.get("time").and_then(Value::as_u64),
                )?;
                let entries = repo.state_entries(&st)?;
                let root = ubos_tree::tree_root(&entries)?;
                Ok(
                    json!({"root": root, "entries": entries.iter().map(|(k, h)| json!([k, h])).collect::<Vec<_>>()}),
                )
            }
            _ => Err(BpaError::new("ADAPTER.UNSUPPORTED", op.to_string())),
        }
    }
}

fn create(
    repo: &mut Repo<'_>,
    tx: &mut Tx,
    ch: &Value,
    default_valid: &dyn Fn(&str) -> String,
    result: &mut Value,
) -> Result<(), BpaError> {
    let oid = s(ch, "object_id").unwrap_or_default().to_string();
    let class = s(ch, "class").unwrap_or_default().to_string();
    let c = repo.reg.class(&class)?.clone();
    if !names::matches("uuid", &oid) {
        return Err(BpaError::new("MODEL.INVALID_VALUE", "object_id is a UUID"));
    }
    if c.is_abstract {
        return Err(BpaError::new("MODEL.ABSTRACT_INSTANTIATION", class));
    }
    let kind = c.kind.clone().unwrap_or_default();
    if kind == "system" || kind == "ledger" {
        return Err(BpaError::new("LOGIC.FORBIDDEN_TARGET", class));
    }
    if tx.state.objects.contains_key(&oid) || tx.state.find_entry(&oid).is_some() {
        return Err(BpaError::new("TXN.ALREADY_EXISTS", oid));
    }
    let slug = s(ch, "slug").unwrap_or_default().to_string();
    names::check_name("slug", &slug)?;
    let body = repo
        .reg
        .check_body(&class, ch.get("body").unwrap_or(&json!({})), false)?;
    let a = ch
        .get("valid")
        .and_then(|v| s(v, "from"))
        .map(str::to_string)
        .unwrap_or_else(|| default_valid(&class));
    let vh = repo.new_version(tx, &oid, &class, &slug, &body, &a, None, None, false)?;
    let timeline = ubos_time::apply(&[], "create", &vh, &a, None).map_err(tl_err)?;
    tx.state.objects.insert(
        oid.clone(),
        ObjectState {
            class,
            kind,
            slug,
            head: vh.clone(),
            timeline,
        },
    );
    result["versions"][oid] = json!(vh);
    Ok(())
}

fn update(
    repo: &mut Repo<'_>,
    tx: &mut Tx,
    ch: &Value,
    op: &str,
    default_valid: &dyn Fn(&str) -> String,
    result: &mut Value,
) -> Result<(), BpaError> {
    let oid = s(ch, "object_id").unwrap_or_default().to_string();
    let Some(o) = tx.state.objects.get(&oid).cloned() else {
        return Err(if tx.state.find_entry(&oid).is_some() {
            BpaError::new("LEDG.IMMUTABLE_ENTRY", oid)
        } else {
            BpaError::new("IAM.NOT_FOUND", "unknown object")
        });
    };
    if o.kind == "system" {
        return Err(BpaError::new("LOGIC.FORBIDDEN_TARGET", oid));
    }
    if s(ch, "base").is_none() {
        return Err(BpaError::new(
            "MODEL.DECLARATION_INCOMPLETE",
            "updates name their base (STD-TXN-020)",
        ));
    }
    let base = repo.version_in(tx, &o.head)?;
    let base_body = base["body"].as_object().cloned().unwrap_or_default();
    let valid = ch.get("valid").cloned().unwrap_or(json!({}));
    let dv = default_valid(&o.class);
    let mut obj = o.clone();
    let retro_from;
    let retro_to;
    if op == "end" {
        let d = s(&valid, "from").map(str::to_string).unwrap_or(dv.clone());
        let vh = repo.new_version(
            tx,
            &oid,
            &o.class,
            &o.slug,
            &base_body,
            &d,
            None,
            Some(&o.head),
            true,
        )?;
        obj.timeline = ubos_time::apply(&o.timeline, "end", &vh, &d, None).map_err(tl_err)?;
        obj.head = vh;
        retro_from = d;
        retro_to = None;
    } else {
        let mut body = base_body.clone();
        body.extend(
            repo.reg
                .check_body(&o.class, ch.get("body").unwrap_or(&json!({})), true)?,
        );
        repo.reg
            .check_body(&o.class, &Value::Object(body.clone()), false)?;
        let slug = s(ch, "slug").unwrap_or(&o.slug).to_string();
        names::check_name("slug", &slug)?;
        let kind = s(&valid, "kind").unwrap_or("supersede_from");
        let a = s(&valid, "from").map(str::to_string).unwrap_or(dv.clone());
        let b = s(&valid, "to").map(str::to_string);
        let to = match kind {
            "set_period" => b.clone(),
            "replace_future" => None,
            _ => ubos_time::range(&o.timeline, kind, &a, b.as_deref()).1,
        };
        let (rf, rt) = ubos_time::range(&o.timeline, kind, &a, b.as_deref());
        let vh = repo.new_version(
            tx,
            &oid,
            &o.class,
            &slug,
            &body,
            &a,
            to.as_deref(),
            Some(&o.head),
            false,
        )?;
        obj.timeline =
            ubos_time::apply(&o.timeline, kind, &vh, &a, b.as_deref()).map_err(tl_err)?;
        obj.slug = slug;
        obj.head = vh;
        retro_from = rf;
        retro_to = rt;
    }
    if retro_from < dv {
        let list = result["retroactive"].as_array_mut().expect("list");
        list.push(json!({"object_id": oid, "from": retro_from, "to": retro_to}));
    }
    result["versions"][oid.clone()] = json!(obj.head);
    tx.state.objects.insert(oid, obj);
    Ok(())
}

/// Helper for inputs given as maps.
pub fn object(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}
