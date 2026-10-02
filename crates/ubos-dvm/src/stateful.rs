//! Stateful adapter operations (STD-CONF-005): branches, commits, reads, merge, diff, state.

use crate::relations::{Checks, extensions_of, needs_full_state};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use ubos_model::Registry;
use ubos_store::{Group, MemStore, Store};
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
    /// Largest number of derivation inputs (STD-TXN-030, adapter `set_limits`).
    pub max_inputs: u64,
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
            max_inputs: 10_000,
        };
        e.reset()?;
        Ok(e)
    }

    /// An engine over a store that already holds data (for example a loaded benchmark
    /// dataset), with the given class declarations; nothing is reset.
    pub fn attach(store: Box<dyn Store>, classes: &Value) -> Result<Engine, BpaError> {
        let mut e = Engine {
            store,
            registry: Registry::new(),
            now: DEFAULT_NOW.into(),
            instances: BTreeSet::new(),
            max_inputs: 10_000,
        };
        e.load_genesis(classes, None)?;
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
    pub fn load_genesis(
        &mut self,
        classes: &Value,
        value_types: Option<&Value>,
    ) -> Result<Value, BpaError> {
        let list = classes.as_array().cloned().unwrap_or_default();
        let vts = value_types.and_then(Value::as_array).cloned();
        let next = self.registry.load(&list, vts.as_deref())?;
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
        let (r, classes) = commit_on(self.store.as_mut(), &self.registry, &self.now, input)?;
        self.instances.extend(classes);
        Ok(r)
    }

    /// Several commits as one group (DSN-DVM-006): each commit reads the results of the
    /// earlier ones, and all successful commits are flushed in one storage transaction. A
    /// commit that fails validation is left out without affecting the others; the outer
    /// error reports a failed group flush, in which case nothing is written.
    pub fn commit_group(
        &mut self,
        inputs: &[Value],
    ) -> Result<Vec<Result<Value, BpaError>>, BpaError> {
        let inner = std::mem::replace(&mut self.store, Box::new(MemStore::default()));
        let mut group = Group::new(inner);
        let (mut out, mut classes) = (Vec::new(), Vec::new());
        for input in inputs {
            match commit_on(&mut group, &self.registry, &self.now, input) {
                Ok((r, c)) => {
                    out.push(Ok(r));
                    classes.extend(c);
                }
                Err(e) => out.push(Err(e)),
            }
        }
        let (inner, written) = group.finish();
        self.store = inner;
        if !written? {
            return Err(BpaError::new("TXN.HEAD_MOVED", "the branch head moved"));
        }
        self.instances.extend(classes);
        Ok(out)
    }

    /// `read` (STD-TIME-012).
    pub fn read(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main");
        let oid = s(input, "object_id").unwrap_or_default();
        let time = input.get("time").and_then(Value::as_u64);
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let o = repo
            .object_at(branch, time, oid)?
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
        let entries =
            Repo::new(self.store.as_mut(), &self.registry).ledger_at(branch, time, class, &key)?;
        let asof = s(input, "asof");
        let mut out = Vec::new();
        for e in &entries {
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
                let stored = repo.root_at(
                    s(input, "branch").unwrap_or("main"),
                    input.get("time").and_then(Value::as_u64),
                )?;
                if stored != root {
                    return Err(BpaError::new(
                        "NOD.STORAGE_UNAVAILABLE",
                        "stored state root differs from a rebuild",
                    ));
                }
                Ok(
                    json!({"root": root, "entries": entries.iter().map(|(k, h)| json!([k, h])).collect::<Vec<_>>()}),
                )
            }
            "list_objects" => {
                let class = s(input, "class").unwrap_or_default();
                self.registry.decl(class)?;
                let only = input.get("only").and_then(Value::as_bool).unwrap_or(false);
                let branch = s(input, "branch").unwrap_or("main");
                let time = input.get("time").and_then(Value::as_u64);
                let st = repo.state_at(branch, time)?;
                let mut out = Vec::new();
                for (oid, o) in &st.objects {
                    if repo.version(&o.head)?["tombstone"] == json!(true) {
                        continue;
                    }
                    let lin = &self.registry.class(&o.class)?.linearization;
                    let hit = if only {
                        o.class == class
                    } else {
                        lin.iter().any(|c| c == class)
                    };
                    if hit {
                        out.push(oid.clone());
                    }
                }
                Ok(json!({"objects": out}))
            }
            "class_at" => {
                let class = s(input, "class").unwrap_or_default();
                self.registry.decl(class)?;
                let branch = s(input, "branch").unwrap_or("main");
                let time = input.get("time").and_then(Value::as_u64);
                let st = repo.state_at(branch, time)?;
                let exts = extensions_of(&mut repo, &st)?;
                let reg = self.registry.with_extensions(&exts)?;
                Ok(reg.class(class)?.to_json())
            }
            "traverse" => {
                let branch = s(input, "branch").unwrap_or("main");
                let time = input.get("time").and_then(Value::as_u64);
                let st = repo.state_at(branch, time)?;
                let exts = extensions_of(&mut repo, &st)?;
                drop(repo);
                let reg = self.registry.with_extensions(&exts)?;
                let mut repo = Repo::new(self.store.as_mut(), &reg);
                let mut tx = repo.begin(branch)?;
                tx.state = st;
                let now = self.now.clone();
                let regr = &reg;
                let default_valid =
                    move |c: &str| match regr.class(c).map(|c| c.valid_time.as_str()) {
                        Ok("instant") => now.clone(),
                        _ => now[..10].to_string(),
                    };
                let mut checks = Checks {
                    repo: &mut repo,
                    tx: &mut tx,
                    default_valid: &default_valid,
                };
                checks.traverse(
                    s(input, "object_id").unwrap_or_default(),
                    s(input, "relation").unwrap_or_default(),
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
    if repo.obj(tx, &oid)?.is_some() || repo.find_entry(tx, &oid)?.is_some() {
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
    let Some(o) = repo.obj(tx, &oid)? else {
        return Err(if repo.find_entry(tx, &oid)?.is_some() {
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

/// The commit operation over any store; returns the result and the classes touched.
fn commit_on(
    store: &mut dyn Store,
    registry: &Registry,
    now_: &str,
    input: &Value,
) -> Result<(Value, Vec<String>), BpaError> {
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
    let today = now_[..10].to_string();
    let reg = registry;
    let (now, day) = (now_.to_string(), today.clone());
    let default_valid = move |c: &str| match reg.class(c).map(|c| c.valid_time.as_str()) {
        Ok("instant") => now.clone(),
        Ok(_) => day.clone(),
        Err(_) => String::new(),
    };
    // Extensions in force on the branch, and whether the checks of STD-CLS-030, -040 and
    // -041 need the full state (DEC-050).
    let exts = {
        let mut r = Repo::new(&mut *store, registry);
        let head = r.head(&branch)?;
        let cseq = r.commit(head)?.cseq;
        let objs = r.objects_in(&branch, cseq, Some("ClassExtension"))?;
        let mut exts = Vec::new();
        for o in objs.values() {
            let v = r.version(&o.head)?;
            if v["tombstone"] != json!(true) {
                exts.push(v["body"].clone());
            }
        }
        exts
    };
    let writes_extension = changes
        .iter()
        .any(|c| s(c, "class") == Some("ClassExtension"));
    let full = needs_full_state(registry) || !exts.is_empty() || writes_extension;
    let reg_ext;
    let reg: &Registry = if exts.is_empty() {
        registry
    } else {
        reg_ext = registry.with_extensions(&exts)?;
        &reg_ext
    };
    let mut repo = Repo::new(store, reg);
    let mut tx = repo.begin(&branch)?;
    if full {
        repo.load_all(&mut tx)?;
    }
    let mut ended: Vec<(String, String)> = Vec::new();
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
            if repo
                .obj(&mut tx, &oid)?
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
            && repo.obj(&mut tx, oid)?.is_some_and(|o| o.head != b)
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
                if op == "end" {
                    let oid = s(ch, "object_id").unwrap_or_default().to_string();
                    let d = match ch.get("valid").and_then(|v| s(v, "from")) {
                        Some(d) => d.to_string(),
                        None => repo
                            .obj(&mut tx, &oid)?
                            .map(|o| default_valid(&o.class))
                            .unwrap_or_default(),
                    };
                    ended.push((oid, d));
                }
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
    if full {
        let written: Vec<(String, String)> = result["versions"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let mut checks = Checks {
            repo: &mut repo,
            tx: &mut tx,
            default_valid: &default_valid,
        };
        checks.relationships(&mut ended, &mut result)?;
        checks.integrity(&written)?;
        checks.extensions_apply(registry, &written)?;
    }
    let classes: Vec<String> = tx
        .state
        .objects
        .values()
        .map(|o| o.class.clone())
        .chain(tx.appended.iter().map(|e| e.class.clone()))
        .collect();
    let head = tx.head;
    repo.write(tx, vec![head])?;
    Ok((result, classes))
}
