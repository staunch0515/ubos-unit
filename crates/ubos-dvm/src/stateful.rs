//! Stateful adapter operations (STD-CONF-005): branches, commits, reads, merge, diff, state.

use crate::relations::{Checks, extensions_of, needs_full_state};
use crate::transactions::{
    add_derived, derivation_of, derived_entry, effects_of, pending_starts, segment,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
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
    /// Counter for generated process identifiers.
    pub next_process: u64,
    /// Branch metadata read in this adapter (invalidated when it changes).
    pub branch_cache: std::collections::HashMap<String, ubos_version::BranchMeta>,
    /// Principal of operations that name none (DEC-041, DEC-054).
    pub principal: String,
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
            registry: crate::expressions::registry(),
            now: DEFAULT_NOW.into(),
            instances: BTreeSet::new(),
            max_inputs: 10_000,
            next_process: 0,
            branch_cache: Default::default(),
            principal: crate::versioning::PRINCIPAL.into(),
        };
        e.reset()?;
        Ok(e)
    }

    /// An engine over a store that already holds data (for example a loaded benchmark
    /// dataset), with the given class declarations; nothing is reset.
    pub fn attach(store: Box<dyn Store>, classes: &Value) -> Result<Engine, BpaError> {
        let mut e = Engine {
            store,
            registry: crate::expressions::registry(),
            now: DEFAULT_NOW.into(),
            instances: BTreeSet::new(),
            max_inputs: 10_000,
            next_process: 0,
            branch_cache: Default::default(),
            principal: crate::versioning::PRINCIPAL.into(),
        };
        e.load_genesis(classes, None)?;
        Ok(e)
    }

    /// `reset`: empty store, genesis `main` at cseq 0.
    pub fn reset(&mut self) -> Result<(), BpaError> {
        self.store.reset()?;
        self.registry = crate::expressions::registry();
        self.now = DEFAULT_NOW.into();
        self.instances.clear();
        self.next_process = 0;
        self.branch_cache.clear();
        Repo::new(self.store.as_mut(), &self.registry).init()?;
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let mut m = repo.branch_meta("main")?;
        m.created_at = Some(self.now.clone());
        m.active_at = Some(self.now.clone());
        repo.put_branch_meta("main", &m)
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

    /// Default valid time of a class: today, or the clock for instant granularity.
    pub fn default_valid_of(&self, class: &str) -> String {
        self.default_valid(class)
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

    /// Metadata of a branch, cached per adapter (DEC-051).
    pub fn branch_meta(&mut self, name: &str) -> Result<ubos_version::BranchMeta, BpaError> {
        if let Some(m) = self.branch_cache.get(name) {
            return Ok(m.clone());
        }
        let m = Repo::new(self.store.as_mut(), &self.registry).branch_meta(name)?;
        self.branch_cache.insert(name.to_string(), m.clone());
        Ok(m)
    }

    /// `commit` (STD-TXN-001…024, STD-KIND-020…025, STD-TIME-011, -030).
    pub fn commit(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main").to_string();
        let mut next = self.next_process;
        let p = crate::versioning::process(
            self.store.as_mut(),
            input.get("process"),
            "action",
            &mut next,
            &self.principal,
        )?;
        self.next_process = next;
        // STD-TXN-040: a known key returns the stored result without effect.
        let idem = Engine::idempotency(&p, input)?;
        if let Some((scope, key, hash)) = &idem
            && let Some(result) = self.replay(scope, key, hash)?
        {
            return Ok(result);
        }
        let now = self.now.clone();
        let meta = self.branch_meta(&branch)?;
        if ubos_version::expires_of(&meta)?.is_some_and(|exp| now >= exp) {
            return Err(BpaError::new("VER.BRANCH_ARCHIVED", branch));
        }
        if p.p.get("abort") == Some(&json!(true)) {
            crate::versioning::record(self.store.as_mut(), &p, "aborted", None, 0)?;
            return Ok(json!({"state": "aborted"}));
        }
        let r = commit_on(
            self.store.as_mut(),
            &self.registry,
            &self.now,
            input,
            &p,
            false,
            &meta,
            self.max_inputs,
            idem,
        );
        let (r, classes, effects) = match r {
            Ok(x) => x,
            Err(e) => {
                crate::versioning::record(self.store.as_mut(), &p, "failed", None, 0)?;
                return Err(e);
            }
        };
        self.touch_branch(&branch)?;
        crate::versioning::record(
            self.store.as_mut(),
            &p,
            "committed",
            Some((&branch, r["cseq"].as_u64().unwrap_or_default())),
            effects,
        )?;
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
        let mut next = self.next_process;
        for input in inputs {
            let p = match crate::versioning::process(
                &mut group,
                input.get("process"),
                "action",
                &mut next,
                &self.principal,
            ) {
                Ok(p) => p,
                Err(e) => {
                    out.push(Err(e));
                    continue;
                }
            };
            let bname = s(input, "branch").unwrap_or("main").to_string();
            let meta = match self.branch_cache.get(&bname) {
                Some(m) => m.clone(),
                None => match Repo::new(&mut group, &self.registry).branch_meta(&bname) {
                    Ok(m) => m,
                    Err(e) => {
                        out.push(Err(e));
                        continue;
                    }
                },
            };
            match commit_on(
                &mut group,
                &self.registry,
                &self.now,
                input,
                &p,
                false,
                &meta,
                self.max_inputs,
                None,
            ) {
                Ok((r, c, _)) => {
                    out.push(Ok(r));
                    classes.extend(c);
                }
                Err(e) => out.push(Err(e)),
            }
        }
        let (inner, written) = group.finish();
        self.store = inner;
        self.next_process = next;
        if !written? {
            return Err(BpaError::new("TXN.HEAD_MOVED", "the branch head moved"));
        }
        self.instances.extend(classes);
        Ok(out)
    }

    /// `read` (STD-TIME-012).
    /// True when a read needs the resolved view (selectors beyond `time`, provenance, or a
    /// branch with a parent line); otherwise the object index answers directly.
    fn needs_view(&mut self, branch: &str, input: &Value) -> Result<bool, BpaError> {
        if [
            "commit",
            "tag",
            "tx_time",
            "evaluated_at",
            "provenance",
            "follow",
        ]
        .iter()
        .any(|k| {
            input
                .get(k)
                .is_some_and(|v| !v.is_null() && v != &json!(false))
        }) {
            return Ok(true);
        }
        Ok(Repo::new(self.store.as_mut(), &self.registry)
            .branch_meta(branch)?
            .parent_line
            .is_some())
    }

    /// An object of a bitemporal read with its provenance, the evaluation evidence
    /// (STD-TIME-015) and, on the view path, the resolved state.
    #[allow(clippy::type_complexity)]
    pub fn object_view(
        &mut self,
        branch: &str,
        input: &Value,
        oid: &str,
    ) -> Result<
        (
            ObjectState,
            Option<(String, u64)>,
            BTreeMap<String, String>,
            Option<ubos_version::State>,
        ),
        BpaError,
    > {
        let time = input.get("time").and_then(Value::as_u64);
        let view = self.needs_view(branch, input)?;
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        if view {
            let (v, _) = repo.view(branch, input)?;
            let o = v
                .state
                .objects
                .get(oid)
                .cloned()
                .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown object"))?;
            let prov = v.provenance.get(oid).cloned();
            Ok((o, prov, v.evaluated_at, Some(v.state)))
        } else {
            let snap = repo.select(branch, input)?;
            let o = repo
                .object_at(branch, time, oid)?
                .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown object"))?;
            let ev = BTreeMap::from([(branch.to_string(), repo.commit(snap.commit)?.commit_id)]);
            Ok((o, None, ev, None))
        }
    }

    /// Evaluation evidence of a ledger read on the index path.
    fn evidence(
        &mut self,
        branch: &str,
        input: &Value,
    ) -> Result<BTreeMap<String, String>, BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let snap = repo.select(branch, input)?;
        Ok(BTreeMap::from([(
            branch.to_string(),
            repo.commit(snap.commit)?.commit_id,
        )]))
    }

    /// `read` (STD-TIME-012, -013, -015, STD-VER-080, -081, -090).
    pub fn read(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main");
        let oid = s(input, "object_id").unwrap_or_default();
        let (o, prov, ev, st) = self.object_view(branch, input, oid)?;
        let a = s(input, "asof")
            .map(str::to_string)
            .unwrap_or_else(|| self.default_valid(&o.class));
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let seg = at(&o.timeline, &a)
            .ok_or_else(|| BpaError::new("QRY.NOT_FOUND_AT_TIME", "no segment at asof"))?;
        let mut out = segment(&mut repo, &seg.from, seg.to.as_deref(), &seg.version)?;
        out["evaluated_at"] = json!(ev);
        if input.get("provenance") == Some(&json!(true))
            && let Some((b, cid)) = prov
        {
            let c = repo.commit(cid)?;
            out["provenance"] = json!({"branch": b, "cseq": c.cseq, "commit_id": c.commit_id});
        }
        if let Some(field) = s(input, "follow")
            && let Some(st) = &st
        {
            let body = out["body"].clone();
            out["targets"] = json!(self.follow(st, &o, &body, field, &a)?);
        }
        Ok(out)
    }

    /// `read_ledger` (STD-TIME-020).
    pub fn read_ledger(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main");
        let time = input.get("time").and_then(Value::as_u64);
        let key = canonical(input.get("ledger_key").unwrap_or(&Value::Null))?;
        let class = s(input, "class").unwrap_or_default();
        let (entries, ev) = if self.needs_view(branch, input)? {
            let (v, _) = Repo::new(self.store.as_mut(), &self.registry).view(branch, input)?;
            let q = v
                .state
                .ledger
                .iter()
                .find(|(c, k, _)| c == class && *k == key)
                .map(|x| x.2.clone())
                .unwrap_or_default();
            (q, v.evaluated_at)
        } else {
            let ev = self.evidence(branch, input)?;
            let q = Repo::new(self.store.as_mut(), &self.registry)
                .ledger_at(branch, time, class, &key)?;
            (q, ev)
        };
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
            if let Some(p) = &e.provenance_ref {
                r["provenance_ref"] = json!(p);
            }
            out.push(r);
        }
        Ok(json!({"entries": out, "evaluated_at": ev}))
    }

    /// `create_branch`, `merge`, `diff` and `export_state`.
    pub fn other(&mut self, op: &str, input: &Value) -> Result<Value, BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        match op {
            "create_branch" => {
                let now = self.now.clone();
                self.branch_cache
                    .remove(s(input, "name").unwrap_or_default());
                repo.create_branch_meta(
                    s(input, "name").unwrap_or_default(),
                    s(input, "from").unwrap_or("main"),
                    input.get("time").and_then(Value::as_u64),
                    input.get("parent_line").filter(|p| !p.is_null()),
                    Some(&now),
                )
            }
            "merge" => {
                drop(repo);
                self.merge(input)
            }
            "merge_preview" => {
                drop(repo);
                self.merge_preview(
                    s(input, "source").unwrap_or_default(),
                    s(input, "target").unwrap_or("main"),
                )
            }
            "change_set" => {
                drop(repo);
                self.change_set(input)
            }
            "tag" => {
                drop(repo);
                self.tag(input)
            }
            "delete_tag" => {
                drop(repo);
                self.delete_tag(s(input, "name").unwrap_or_default())
            }
            "tag_info" => {
                drop(repo);
                self.tag_info(s(input, "name").unwrap_or_default())
            }
            "release" => {
                drop(repo);
                self.release(input)
            }
            "revert" => {
                drop(repo);
                self.revert(input)
            }
            "branch_info" => {
                drop(repo);
                self.branch_info(s(input, "branch").unwrap_or("main"))
            }
            "set_parent_line" => {
                drop(repo);
                self.branch_cache
                    .remove(s(input, "branch").unwrap_or_default());
                self.set_parent_line(
                    s(input, "branch").unwrap_or_default(),
                    input.get("parent_line"),
                )
            }
            "commit_info" => {
                drop(repo);
                self.commit_info(input)
            }
            "tree_diff" => {
                drop(repo);
                self.tree_diff(
                    input.get("from").unwrap_or(&Value::Null),
                    input.get("to").unwrap_or(&Value::Null),
                )
            }
            "process_info" => {
                drop(repo);
                self.process_info(s(input, "process_id").unwrap_or_default())
            }
            "lineage" => {
                drop(repo);
                self.lineage(
                    s(input, "process_id").unwrap_or_default(),
                    s(input, "direction").unwrap_or("ancestors"),
                )
            }
            "read_range" => {
                drop(repo);
                self.read_range(input)
            }
            "history" => {
                drop(repo);
                self.history(input)
            }
            "derivation" => {
                drop(repo);
                self.derivation(input)
            }
            "impact" => {
                drop(repo);
                self.impact(
                    s(input, "branch").unwrap_or("main"),
                    input.get("time").and_then(Value::as_u64),
                )
            }
            "outbox" => {
                drop(repo);
                self.outbox()
            }
            "ack" => {
                drop(repo);
                self.ack(input.get("effect_ids").unwrap_or(&Value::Null))
            }
            "bulk" => {
                drop(repo);
                self.bulk(input)
            }
            "resolve" => {
                drop(repo);
                self.resolve(input.get("uri").unwrap_or(&Value::Null))
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
                let st = repo.view(branch, input)?.0.state;
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
                let st = repo.view(branch, input)?.0.state;
                let exts = extensions_of(&mut repo, &st)?;
                let reg = self.registry.with_extensions(&exts)?;
                Ok(reg.class(class)?.to_json())
            }
            "traverse" => {
                let branch = s(input, "branch").unwrap_or("main");
                let st = repo.view(branch, input)?.0.state;
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
    provenance: Option<&str>,
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
    let vh = repo.new_version(
        tx, &oid, &class, &slug, &body, &a, None, None, false, provenance,
    )?;
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
    provenance: Option<&str>,
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
            None,
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
            provenance,
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

/// The commit operation over any store; returns the result, the classes touched and the
/// number of effects released.
#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_on(
    store: &mut dyn Store,
    registry: &Registry,
    now_: &str,
    input: &Value,
    process: &crate::versioning::Process,
    via_change_set: bool,
    meta: &ubos_version::BranchMeta,
    max_inputs: u64,
    idempotency: Option<(String, String, String)>,
) -> Result<(Value, Vec<String>, u32), BpaError> {
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
    let has_parent_line = meta.parent_line.is_some();
    let mut tx = if has_parent_line {
        repo.begin_view(&branch)?
    } else {
        repo.begin(&branch)?
    };
    tx.now = Some(now_.to_string());
    if full && !has_parent_line {
        repo.load_all(&mut tx)?;
    }
    let full = full || has_parent_line;
    let mut ended: Vec<(String, String)> = Vec::new();
    // Derived outputs of this commit and the logic that produced them (STD-TXN-030).
    let mut derived: Vec<Value> = Vec::new();
    let mut logic: BTreeSet<String> = BTreeSet::new();
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
        if matches!(s(ch, "op"), Some("update" | "end" | "hide"))
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
        let deriv = derivation_of(&mut repo, &tx, ch, &process.id, max_inputs)?;
        let rid = deriv.as_ref().map(|d| d.id.as_str());
        if let Some(d) = &deriv {
            logic.extend(d.logic.iter().cloned());
        }
        match s(ch, "op").unwrap_or_default() {
            "create" => {
                create(&mut repo, &mut tx, ch, &default_valid, &mut result, rid)?;
                let oid = s(ch, "object_id").unwrap_or_default();
                if let Some(d) = &deriv {
                    let vh = result["versions"][oid]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    derived.push(derived_entry("object", oid, &vh, &d.id));
                    add_derived(&mut tx, d, &vh);
                }
                let a = result["versions"][oid].as_str().map(|_| ());
                let from = tx
                    .state
                    .objects
                    .get(oid)
                    .and_then(|o| o.timeline.first())
                    .map(|x| x.from.clone());
                if a.is_some() {
                    Repo::op(&mut tx, oid, "create", from.as_deref(), None);
                }
            }
            op @ ("hide" | "unhide") => {
                let oid = s(ch, "object_id").unwrap_or_default().to_string();
                if !has_parent_line {
                    return Err(BpaError::new(
                        "MODEL.INVALID_VALUE",
                        "hide and unhide need a parent line",
                    ));
                }
                if op == "hide" {
                    if !tx.state.objects.contains_key(&oid) {
                        return Err(BpaError::new("IAM.NOT_FOUND", "unknown object"));
                    }
                    if !tx.inherited.contains_key(&oid) {
                        return Err(BpaError::new(
                            "MODEL.INVALID_VALUE",
                            "only objects of the parent line can be hidden",
                        ));
                    }
                    if s(ch, "base").is_none() {
                        return Err(BpaError::new(
                            "MODEL.DECLARATION_INCOMPLETE",
                            "hide names its base (STD-TXN-020)",
                        ));
                    }
                    tx.state.objects.remove(&oid);
                    tx.hidden.insert(oid.clone());
                } else {
                    if !tx.hidden.contains(&oid) {
                        return Err(BpaError::new("IAM.NOT_FOUND", "object is not hidden"));
                    }
                    tx.hidden.remove(&oid);
                    if let Some(o) = tx.inherited.get(&oid).cloned() {
                        tx.state.objects.insert(oid.clone(), o);
                    }
                }
                Repo::op(&mut tx, &oid, op, None, None);
            }
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
                let oid = s(ch, "object_id").unwrap_or_default().to_string();
                let before = tx.state.objects.get(&oid).map(|o| o.timeline.clone());
                update(&mut repo, &mut tx, ch, op, &default_valid, &mut result, rid)?;
                if let (Some(d), "update") = (&deriv, op) {
                    let vh = result["versions"][&oid]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    derived.push(derived_entry("object", &oid, &vh, &d.id));
                    add_derived(&mut tx, d, &vh);
                }
                if op == "end" {
                    let d = ended.last().map(|x| x.1.clone()).unwrap_or_default();
                    Repo::op(&mut tx, &oid, "end", Some(&d), None);
                } else {
                    let kind = ch
                        .get("valid")
                        .and_then(|v| s(v, "kind"))
                        .unwrap_or("supersede_from")
                        .to_string();
                    let a = ch
                        .get("valid")
                        .and_then(|v| s(v, "from"))
                        .map(str::to_string);
                    let b = ch.get("valid").and_then(|v| s(v, "to")).map(str::to_string);
                    let from = a.or_else(|| {
                        let tl = before.unwrap_or_default();
                        let _ = tl;
                        tx.state
                            .objects
                            .get(&oid)
                            .and_then(|o| o.timeline.iter().find(|x| x.version == o.head))
                            .map(|x| x.from.clone())
                    });
                    Repo::op(&mut tx, &oid, &kind, from.as_deref(), b.as_deref());
                }
            }
            op @ ("append" | "reverse" | "adjust") => {
                let (id, seq) = repo.append(&mut tx, ch, op, &today, &default_valid, rid)?;
                if let Some(d) = &deriv
                    && let Some(eh) = tx.appended.last().map(|e| e.entry_hash.clone())
                {
                    derived.push(derived_entry("entry", &id, &eh, &d.id));
                    add_derived(&mut tx, d, &eh);
                }
                result["entries"][id] = json!({"seq": seq});
            }
            _ => return Err(BpaError::new("MODEL.INVALID_VALUE", "unknown change")),
        }
    }
    repo.check_double_entry(&tx)?;
    let appended_ops: Vec<(String, String)> = tx
        .appended
        .iter()
        .map(|e| (e.entry_id.clone(), e.valid_at.clone()))
        .collect();
    for (id, at_) in appended_ops {
        Repo::op(&mut tx, &id, "append", Some(&at_), None);
    }
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
    let written_classes: Vec<String> = result["versions"]
        .as_object()
        .map(|m| {
            m.keys()
                .filter_map(|k| tx.state.objects.get(k).map(|o| o.class.clone()))
                .collect()
        })
        .unwrap_or_default();
    let classes: Vec<String> = written_classes
        .iter()
        .cloned()
        .chain(tx.appended.iter().map(|e| e.class.clone()))
        .collect();
    // STD-VER-012: write policy on protected branches.
    if !via_change_set && ubos_version::PROTECTED.contains(&meta.kind.as_str()) {
        for c in &classes {
            if reg.class(c)?.write_policy == "change_set" {
                return Err(BpaError::new("VER.CHANGE_SET_REQUIRED", c.to_string()));
            }
        }
    }
    // Commit record members (STD-TIME-040, DEC-041).
    let binding = rule_binding_of(
        &mut repo,
        &tx,
        reg,
        &result["versions"],
        &result["entries"],
        &logic,
    )?;
    tx.extra.insert("rule_binding".into(), binding);
    if !derived.is_empty() {
        tx.extra.insert("derived".into(), Value::Array(derived));
    }
    if process.explicit {
        tx.extra.insert("process_id".into(), json!(process.id));
    }
    if let Some(a) = process.p.get("initiator").filter(|v| v.is_string()) {
        tx.extra.insert("author".into(), a.clone());
    }
    tx.extra
        .insert("retroactive".into(), result["retroactive"].clone());
    tx.extra.insert(
        "appended".into(),
        json!(
            tx.appended
                .iter()
                .map(|e| e.entry_id.clone())
                .collect::<Vec<_>>()
        ),
    );
    // STD-TXN-050, STD-TIME-050, STD-TXN-040: effects, future segment starts and the
    // idempotency record are written with the commit (DEC-054).
    tx.timers = pending_starts(reg, now_, &branch, &tx.state, &result["versions"]);
    tx.outbox = effects_of(repo.store, process, &branch, tx.cseq)?;
    let effects = u32::try_from(tx.outbox.len()).unwrap_or(u32::MAX);
    if let Some((scope, key, hash)) = idempotency {
        tx.idempotency = Some((scope, key, hash, result.clone()));
    }
    let head = tx.head;
    let id = repo.write(tx, vec![head])?;
    let c = repo.commit(id)?;
    result["commit_id"] = json!(c.commit_id);
    result["tx_time"] = json!(c.tx_time);
    Ok((result, classes, effects))
}

/// The rule binding of a commit over the transaction's state (STD-TIME-040).
fn rule_binding_of(
    repo: &mut Repo<'_>,
    tx: &Tx,
    reg: &Registry,
    versions: &Value,
    entries: &Value,
    logic: &BTreeSet<String>,
) -> Result<Value, BpaError> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    for oid in versions.as_object().map(|m| m.keys()).into_iter().flatten() {
        if let Some(o) = tx.state.objects.get(oid) {
            names.insert(o.class.clone());
        }
    }
    for (_, _, q) in &tx.state.ledger {
        for e in q {
            if entries.get(&e.entry_id).is_some() {
                names.insert(e.class.clone());
            }
        }
    }
    let mut lin: BTreeSet<String> = BTreeSet::new();
    for n in &names {
        lin.extend(reg.class(n)?.linearization.iter().cloned());
    }
    let mut items: Vec<Value> = Vec::new();
    for c in &lin {
        items.push(json!({"class": c, "class_version": ubos_types::content_id("ubos.chunk.v1", reg.decl(c)?)?}));
    }
    for (oid, o) in &tx.state.objects {
        if o.class == "ClassExtension" {
            let v = repo.version_in(tx, &o.head)?;
            if v["tombstone"] != json!(true)
                && s(&v["body"], "target").is_some_and(|t| lin.contains(t))
            {
                items.push(
                    json!({"class": "ClassExtension", "object_id": oid, "class_version": o.head}),
                );
            }
        }
    }
    let mut out = json!({"classes": items});
    if !logic.is_empty() {
        out["logic"] = json!(logic);
    }
    Ok(out)
}
