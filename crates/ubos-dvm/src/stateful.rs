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

/// The member that marks a change of the platform (`system_commit`).
pub(crate) const SYSTEM_MARK: &str = "__platform";

/// State of the adapter besides the store.
pub struct Engine {
    /// The store (memory or PostgreSQL).
    pub store: Box<dyn Store>,
    /// Loaded classes.
    pub registry: Registry,
    /// Set by `system_commit` for the one commit it makes.
    pub(crate) system_write: bool,
    /// Adapter clock.
    pub now: String,
    /// Classes that have instances on some branch (STD-KIND-080).
    pub instances: BTreeSet<String>,
    /// Largest number of derivation inputs (STD-TXN-030, adapter `set_limits`).
    pub max_inputs: u64,
    /// Largest number of violations reported (FR-RULE-034, adapter `set_limits`).
    pub max_violations: u64,
    /// Compiled sheets and the selector index (DSN-DVM-701, -703).
    pub rules: ubos_rules::Cache,
    /// Counter for generated process identifiers.
    pub next_process: u64,
    /// Branch metadata read in this adapter (invalidated when it changes).
    pub branch_cache: std::collections::HashMap<String, ubos_version::BranchMeta>,
    /// Principal of operations that name none (DEC-041, DEC-054).
    pub principal: String,
    /// Whether the L2 host runs scripts (STD-CTX-020); off in the conformance adapter,
    /// whose vectors assume the oracle's declaration (DEC-058).
    pub l2: bool,
    /// The host services (CTR-002) when a host opened this engine; the conformance adapter
    /// has none and seeds processes deterministically.
    pub services: Option<crate::host::Services>,
    /// The largest unindexed scan of a query, when lowered below the profile's budget
    /// (DSN-DVM-903, adapter `set_limits.max_scan`).
    pub max_scan: Option<u64>,
    /// The entity model and the compiled policy sets (DSN-DVM-803, step 13b).
    pub authz: crate::authz::Cache,
    /// Decision records not yet delivered to the host (DSN-DVM-805).
    pub decision_log: Vec<Value>,
    /// The commit from which a rebuilt field index serves a branch (`class.field` →
    /// branch → cseq; DSN-DVM-902, step 13d): queries at an earlier commit scan.
    pub index_since: BTreeMap<String, BTreeMap<String, u64>>,
    /// The sessions of the in-process UBTP binding (step 14a).
    pub ubtp_sessions: BTreeMap<String, crate::ubtp::Session>,
    /// The process of the `mutate` being handled by the binding, named by its error
    /// (STD-PROTO-010, since BPA 1.0-rc5).
    pub ubtp_pid: Option<String>,
}

/// `(class, row name)` of the implicit rows of the field index.
type RowNames = BTreeSet<(String, String)>;

/// A reference row to build: `(class, field, value key, object)`.
type RefRow = (String, String, Vec<u8>, String);

/// The `index_since` entry of the reference rows (PH-2 step 0b): `index_usable` of
/// lineage `@ref` and field `rows`.
const REF_SINCE: &str = "@ref.rows";

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
            system_write: false,
            store,
            registry: crate::expressions::registry(),
            now: DEFAULT_NOW.into(),
            instances: BTreeSet::new(),
            max_inputs: 10_000,
            max_violations: crate::rules::MAX_VIOLATIONS,
            rules: ubos_rules::Cache::default(),
            next_process: 0,
            branch_cache: Default::default(),
            principal: crate::versioning::PRINCIPAL.into(),
            l2: false,
            services: None,
            max_scan: None,
            authz: Default::default(),
            decision_log: Vec::new(),
            index_since: BTreeMap::new(),
            ubtp_sessions: BTreeMap::new(),
            ubtp_pid: None,
        };
        e.reset()?;
        Ok(e)
    }

    /// An engine over a store that already holds data (for example a loaded benchmark
    /// dataset), with the given class declarations; nothing is reset.
    pub fn attach(store: Box<dyn Store>, classes: &Value) -> Result<Engine, BpaError> {
        let mut e = Engine {
            system_write: false,
            store,
            registry: crate::expressions::registry(),
            now: DEFAULT_NOW.into(),
            instances: BTreeSet::new(),
            max_inputs: 10_000,
            max_violations: crate::rules::MAX_VIOLATIONS,
            rules: ubos_rules::Cache::default(),
            next_process: 0,
            branch_cache: Default::default(),
            principal: crate::versioning::PRINCIPAL.into(),
            l2: false,
            services: None,
            max_scan: None,
            authz: Default::default(),
            decision_log: Vec::new(),
            index_since: BTreeMap::new(),
            ubtp_sessions: BTreeMap::new(),
            ubtp_pid: None,
        };
        e.load_genesis(classes, None)?;
        Ok(e)
    }

    /// `reset`: empty store, genesis `main` at cseq 0.
    pub fn reset(&mut self) -> Result<(), BpaError> {
        self.store.reset()?;
        self.registry = crate::expressions::registry();
        self.max_scan = None;
        self.authz.clear();
        self.decision_log.clear();
        self.index_since.clear();
        self.ubtp_sessions.clear();
        self.now = DEFAULT_NOW.into();
        self.instances.clear();
        self.rules.clear();
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

    /// `system_commit {branch?, changes, process?}` (host-internal, PH-1 step 15c,
    /// DEC-076): a commit of the platform that may create and update System-kind objects
    /// (jobs, FR-EVT-031), which ordinary commits may not; only System-kind classes are
    /// accepted, and an update without `base` is based on the current head.
    pub fn system_commit(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main").to_string();
        let mut changes = input["changes"].as_array().cloned().unwrap_or_default();
        for c in changes.iter_mut() {
            let oid = s(c, "object_id").unwrap_or_default().to_string();
            let existing = match s(c, "op") {
                Some("create") => None,
                _ => Some(self.object_view(&branch, &json!({}), &oid)?.0),
            };
            let class = match (s(c, "class"), &existing) {
                (Some(k), _) => k.to_string(),
                (None, Some(o)) => o.class.clone(),
                (None, None) => String::new(),
            };
            if self.registry.class(&class)?.kind.as_deref() != Some("system") {
                return Err(BpaError::new(
                    "LOGIC.FORBIDDEN_TARGET",
                    format!("{class} is not a System-kind class"),
                ));
            }
            if s(c, "op") == Some("update")
                && c.get("base").is_none()
                && let Some(o) = &existing
            {
                c["base"] = json!(o.head);
            }
            c[SYSTEM_MARK] = json!(true);
        }
        let mut i = input.clone();
        i["changes"] = Value::Array(changes);
        self.system_write = true;
        let r = self.commit(&i);
        self.system_write = false;
        r
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
        self.rules.clear();
        self.authz.clear();
        // the declarations persist with the VAE, so that a host reopens it (step 7a)
        self.store.put_meta(
            "genesis",
            &json!({"classes": self.registry.declarations(), "value_types": self.registry.value_types().values().collect::<Vec<_>>()}),
        )?;
        self.ensure_ref_rows()?;
        Ok(Value::Bool(true))
    }

    /// An engine over a VAE a host opened before: the declarations come from the store
    /// (`genesis`), the process counter continues after the recorded processes; a store
    /// without `main` is initialised (CTR-001 `open_vae`, step 7a).
    pub fn open_existing(store: Box<dyn Store>) -> Result<Engine, BpaError> {
        let mut e = Engine {
            system_write: false,
            store,
            registry: crate::expressions::registry(),
            now: DEFAULT_NOW.into(),
            instances: BTreeSet::new(),
            max_inputs: 10_000,
            max_violations: crate::rules::MAX_VIOLATIONS,
            rules: ubos_rules::Cache::default(),
            next_process: 0,
            branch_cache: Default::default(),
            principal: crate::versioning::PRINCIPAL.into(),
            l2: false,
            services: None,
            max_scan: None,
            authz: Default::default(),
            decision_log: Vec::new(),
            index_since: BTreeMap::new(),
            ubtp_sessions: BTreeMap::new(),
            ubtp_pid: None,
        };
        if e.store.branch("main")?.is_none() {
            Repo::new(e.store.as_mut(), &e.registry).init()?;
        }
        if let Some(g) = e.store.meta("genesis")? {
            let classes = g.get("classes").cloned().unwrap_or(json!([]));
            let vts = g
                .get("value_types")
                .filter(|v| v.as_array().is_some_and(|a| !a.is_empty()));
            e.registry = e.registry.load(
                classes.as_array().map(Vec::as_slice).unwrap_or(&[]),
                vts.and_then(Value::as_array).map(Vec::as_slice),
            )?;
        }
        // the field indexes declared to the kernel (DSN-DVM-902, step 13a)
        if let Some(ix) = e.store.meta("indexes")? {
            e.registry.restore_indexes(&ix);
        }
        if let Some(since) = e.store.meta("index_since")? {
            e.index_since = serde_json::from_value(since).unwrap_or_default();
        }
        e.next_process = e.store.process_ids()?.len() as u64;
        e.ensure_ref_rows()?;
        Ok(e)
    }

    /// Declare a field index of a class (DSN-DVM-902, FR-ANL-014): an index of a class
    /// serves its subclasses; the fields of unique `keys` are indexed without a
    /// declaration. The rows of later heads are written at flush; when the class already
    /// has instances, the index is rebuilt over the head objects of every branch (step
    /// 13d) and serves each branch from its head commit at the rebuild (`index_since`):
    /// a query at an earlier commit scans instead.
    pub fn declare_index(&mut self, class: &str, field: &str) -> Result<Value, BpaError> {
        let subclasses = self.registry.subclasses(class);
        let populated = subclasses.iter().any(|c| self.instances.contains(c));
        self.registry.declare_index(class, field)?;
        self.store
            .put_meta("indexes", &self.registry.declared_indexes())?;
        if !populated {
            return Ok(Value::Bool(true));
        }
        let mut rebuilt = Map::new();
        let branches = self.store.branch_names()?;
        for branch in branches {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let head = repo.head(&branch)?;
            let cseq = repo.meta(head)?.cseq;
            let mut rows: Vec<(String, String, Vec<u8>, String)> = Vec::new();
            for c in &subclasses {
                for (oid, o) in repo.objects_in(&branch, cseq, Some(c))? {
                    let v = repo.version(&o.head)?;
                    if v["tombstone"] == json!(true) {
                        continue;
                    }
                    for (f, key) in ubos_version::state::index_rows(&self.registry, &o.class, &v) {
                        if f == field {
                            rows.push((o.class.clone(), f, key, oid.clone()));
                        }
                    }
                }
            }
            let n = rows.len();
            self.store.put_field_rows(&branch, cseq, &rows)?;
            self.index_since
                .entry(format!("{class}.{field}"))
                .or_default()
                .insert(branch.clone(), cseq);
            rebuilt.insert(branch, json!({"cseq": cseq, "rows": n}));
        }
        let since = serde_json::to_value(&self.index_since).unwrap_or_default();
        self.store.put_meta("index_since", &since)?;
        Ok(json!({"rebuilt": rebuilt}))
    }

    /// Whether the index of `field` declared on a class of `lineage` serves `branch` at
    /// `cseq`: always when it was declared before the class had instances; from the
    /// rebuild commit of the branch otherwise, and for a branch created after the rebuild
    /// of the branch it was created from.
    pub(crate) fn index_usable(
        &mut self,
        branch: &str,
        cseq: u64,
        lineage: &[String],
        field: &str,
    ) -> Result<bool, BpaError> {
        let keys: Vec<String> = lineage
            .iter()
            .map(|c| format!("{c}.{field}"))
            .filter(|k| self.index_since.contains_key(k))
            .collect();
        if keys.is_empty() {
            return Ok(true);
        }
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        for k in keys {
            let since = &self.index_since[&k];
            let (mut b, mut c) = (branch.to_string(), cseq);
            // walk to the branch the index was rebuilt on
            let mut hops = 0;
            loop {
                if let Some(s_) = since.get(&b) {
                    if c < *s_ {
                        return Ok(false);
                    }
                    break;
                }
                let from = repo.branch_meta(&b)?.created_from;
                let fc = repo.meta(from)?;
                if fc.branch == b || hops > 64 {
                    return Ok(false);
                }
                (b, c) = (fc.branch, fc.cseq);
                hops += 1;
            }
        }
        Ok(true)
    }

    /// Whether the reference rows serve the commits on `branch` (PH-2 step 0b): always when
    /// the relationship fields had no instances when they were declared; from the commit at
    /// which their rows were built over the objects of the branch (or the branch it was
    /// created from) otherwise.
    pub(crate) fn refs_indexed(&mut self, branch: &str) -> Result<bool, BpaError> {
        if !self.store.has_field_index() {
            return Ok(false);
        }
        if !self.index_since.contains_key(REF_SINCE) {
            return Ok(true);
        }
        let cseq = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let Ok(head) = repo.head(branch) else {
                return Ok(false);
            };
            repo.meta(head)?.cseq
        };
        self.index_usable(branch, cseq, &["@ref".to_string()], "rows")
    }

    /// The rows the field index carries without a declaration and that commits rely on:
    /// `(class, row name)` of the reference rows of relationship fields (`@ref:<field>`,
    /// PH-2 step 0b-1b), and of the indexable fields of unique keys that are not declared
    /// indexes (step 0b-3).
    fn implicit_rows(&self) -> (RowNames, RowNames) {
        let refs = ubos_version::state::relationship_fields(&self.registry)
            .into_iter()
            .flat_map(|(f, cs)| {
                cs.into_iter()
                    .map(move |c| (c, format!("{}{f}", ubos_version::state::REF_ROW)))
            })
            .collect();
        let mut keys = BTreeSet::new();
        for c in self.registry.classes() {
            let indexed: BTreeMap<String, String> =
                self.registry.indexed_fields(&c.name).into_iter().collect();
            for (_, fields) in self.registry.unique_keys(&c.name) {
                for f in fields {
                    if indexed.contains_key(&f) && !self.registry.is_declared_index(&c.name, &f) {
                        keys.insert((c.name.clone(), f));
                    }
                }
            }
        }
        (refs, keys)
    }

    /// Build the implicit rows that existing objects lack (PH-2 step 0b): for a VAE written
    /// before reference rows existed, a relationship field declared on classes with
    /// instances, or a unique key added to them. The rows of later heads are written at
    /// flush; the objects already on a branch get rows at its head, which serve the branch
    /// from that commit. Key rows have been written at flush since PH-1 step 13a, so a VAE
    /// without the record starts from its current keys.
    pub(crate) fn ensure_ref_rows(&mut self) -> Result<(), BpaError> {
        let (refs, keys) = self.implicit_rows();
        let read =
            |e: &mut Engine, k: &str| -> Result<Option<BTreeSet<(String, String)>>, BpaError> {
                Ok(e.store
                    .meta(k)?
                    .and_then(|v| serde_json::from_value(v).ok()))
            };
        let have_refs = read(self, "implicit_ref_rows")?.unwrap_or_default();
        let have_keys = read(self, "implicit_key_rows")?.unwrap_or_else(|| keys.clone());
        if refs == have_refs && keys == have_keys {
            return Ok(());
        }
        let new: BTreeSet<(String, String)> = refs
            .difference(&have_refs)
            .chain(keys.difference(&have_keys))
            .cloned()
            .collect();
        let classes: BTreeSet<String> = new.iter().map(|(c, _)| c.clone()).collect();
        let mut built: Vec<(String, u64, Vec<RefRow>)> = Vec::new();
        let mut instances = false;
        for branch in self.store.branch_names()? {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let head = repo.head(&branch)?;
            let cseq = repo.meta(head)?.cseq;
            let mut rows = Vec::new();
            for c in &classes {
                for (oid, o) in repo.objects_in(&branch, cseq, Some(c))? {
                    instances = true;
                    let v = repo.version(&o.head)?;
                    for (f, key) in ubos_version::state::index_rows(&self.registry, &o.class, &v) {
                        if new.contains(&(o.class.clone(), f.clone())) {
                            rows.push((o.class.clone(), f, key, oid.clone()));
                        }
                    }
                }
            }
            built.push((branch, cseq, rows));
        }
        // without instances there is nothing to build, and no earlier commit had any
        if instances {
            for (branch, cseq, rows) in built {
                self.store.put_field_rows(&branch, cseq, &rows)?;
                let since = self.index_since.entry(REF_SINCE.into()).or_default();
                let at = since.entry(branch).or_default();
                *at = (*at).max(cseq);
            }
            let since = serde_json::to_value(&self.index_since).unwrap_or_default();
            self.store.put_meta("index_since", &since)?;
        }
        self.store.put_meta("implicit_ref_rows", &json!(refs))?;
        self.store.put_meta("implicit_key_rows", &json!(keys))
    }

    /// The implicit rows are complete for the registry (an imported archive writes them
    /// with every commit).
    pub(crate) fn mark_ref_rows(&mut self) -> Result<(), BpaError> {
        let (refs, keys) = self.implicit_rows();
        self.store.put_meta("implicit_ref_rows", &json!(refs))?;
        self.store.put_meta("implicit_key_rows", &json!(keys))
    }

    /// The key of the signed query cursors (DSN-DVM-908): derived from the VAE
    /// authority until the node host supplies the tenant key (step 14).
    pub(crate) fn cursor_key(&self) -> [u8; 32] {
        ubos_crypto::hkdf_sha256(self.store.authority().as_bytes(), b"ubos.cursor", b"")
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
        let platform = std::mem::take(&mut self.system_write);
        if !platform
            && input["changes"]
                .as_array()
                .is_some_and(|cs| cs.iter().any(|c| c.get(SYSTEM_MARK).is_some()))
        {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "a change does not carry the platform's mark",
            ));
        }
        let branch = s(input, "branch").unwrap_or("main").to_string();
        let mut next = self.next_process;
        // the principal of a host input is the initiator unless the process names one
        let principal = s(input, "principal").unwrap_or(&self.principal).to_string();
        let mut process = input.get("process").cloned();
        if let Some(b) = s(input, "on_behalf_of")
            && let Some(p) = process.get_or_insert_with(|| json!({})).as_object_mut()
        {
            p.entry("on_behalf_of").or_insert_with(|| json!(b));
        }
        let p = crate::versioning::process(
            self.store.as_mut(),
            process.as_ref(),
            "action",
            &mut next,
            &principal,
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
        // DSN-DVM-801, -807 (step 13b): a host commit with a principal is authorized
        self.enforce_commit(input, &branch)?;
        if p.p.get("abort") == Some(&json!(true)) {
            crate::versioning::record(self.store.as_mut(), &p, "aborted", None, 0)?;
            return Ok(json!({"state": "aborted"}));
        }
        let refs = self.refs_indexed(&branch)?;
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
            false,
            self.max_violations,
            &mut self.rules,
            None,
            refs,
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
        // FR-AUD-012 (step 13c): policy changes go to the security log
        self.log_policy_changes(&branch, &r)?;
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
                false,
                self.max_violations,
                &mut self.rules,
                None,
                false,
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
    pub(crate) fn needs_view(&mut self, branch: &str, input: &Value) -> Result<bool, BpaError> {
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
        // the cached metadata, as the query path (DEC-051; PH-1 step 19c)
        Ok(self.branch_meta(branch)?.parent_line.is_some())
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
            let ev = BTreeMap::from([(branch.to_string(), repo.commit_id(snap.commit)?)]);
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
            repo.commit_id(snap.commit)?,
        )]))
    }

    /// `read` (STD-TIME-012, -013, -015, STD-VER-080, -081, -090).
    pub fn read(&mut self, input: &Value) -> Result<Value, BpaError> {
        self.read_classed(input).map(|(r, _)| r)
    }

    /// The rows of a query result read at commit sequence `cseq` of `branch` and valid time
    /// `asof` (else each class's default), in order, as `read` gives them with `object_id`
    /// and `class`: one bulk read of the object states and one of the versions (PH-1 step
    /// 19c). `None` when a row needs the view path (a branch with a parent line).
    pub(crate) fn read_rows(
        &mut self,
        branch: &str,
        cseq: u64,
        commit_id: &Value,
        asof: Option<&str>,
        ids: &[String],
    ) -> Result<Option<Vec<Value>>, BpaError> {
        if self.needs_view(branch, &json!({}))? {
            return Ok(None);
        }
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let states = repo.objects_in_ids(branch, cseq, ids)?;
        let mut segs = Vec::new();
        for id in ids {
            let o = states
                .get(id)
                .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown object"))?;
            let a = asof
                .map(str::to_string)
                .unwrap_or_else(|| self.default_valid(&o.class));
            let seg = at(&o.timeline, &a)
                .ok_or_else(|| BpaError::new("QRY.NOT_FOUND_AT_TIME", "no segment at asof"))?;
            segs.push((id, o.class.clone(), seg.clone()));
        }
        let hashes: Vec<String> = segs.iter().map(|(_, _, g)| g.version.clone()).collect();
        let versions = Repo::new(self.store.as_mut(), &self.registry).versions(&hashes)?;
        let mut out = Vec::new();
        for (id, class, g) in segs {
            let v = versions
                .get(&g.version)
                .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown version"))?;
            let mut r = crate::transactions::segment_of(v, &g.from, g.to.as_deref(), &g.version);
            r["evaluated_at"] = json!({branch: commit_id});
            r["object_id"] = json!(id);
            r["class"] = json!(class);
            out.push(r);
        }
        Ok(Some(out))
    }

    /// A read with the class of the object read (one object view; the rows of a UBTP
    /// query, PH-1 step 19c).
    pub(crate) fn read_classed(&mut self, input: &Value) -> Result<(Value, String), BpaError> {
        let branch = s(input, "branch").unwrap_or("main");
        let oid = s(input, "object_id").unwrap_or_default();
        let (o, prov, ev, st) = self.object_view(branch, input, oid)?;
        // DSN-DVM-804 (step 13b): a host read with a principal sees only what it may read
        if let Some(principal) = Engine::principal_of(input)
            && let Some(set) = self.policy_set_at(branch, None)?
        {
            let v = Repo::new(self.store.as_mut(), &self.registry).version(&o.head)?;
            let resource = self.object_resource(branch, oid, &o, &v)?;
            let ctx = self.access_ctx(input, branch)?;
            self.enforce(&set, branch, &principal, "read", &resource, &ctx)?;
        }
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
        Ok((out, o.class))
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
            "outbox_claim" => {
                drop(repo);
                self.outbox_claim(input)
            }
            "outbox_ack" => {
                drop(repo);
                let ids = input
                    .get("ids")
                    .or_else(|| input.get("effect_ids"))
                    .unwrap_or(&Value::Null);
                self.ack(ids).map(|_| json!({"acked": true}))
            }
            "due_effective" => {
                drop(repo);
                self.due_effective(input)
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

#[allow(clippy::too_many_arguments)]
fn create(
    repo: &mut Repo<'_>,
    tx: &mut Tx,
    ch: &Value,
    default_valid: &dyn Fn(&str) -> String,
    result: &mut Value,
    provenance: Option<&str>,
    rule_ctx: &Map<String, Value>,
    cache: &mut ubos_rules::Cache,
    lcs: &crate::flow::Lifecycles,
    flow: &ubos_flow::Flow,
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
    // System-kind objects are written by the platform only (`system_commit`)
    let platform = ch.get(SYSTEM_MARK) == Some(&json!(true));
    if (kind == "system" && !platform) || kind == "ledger" {
        return Err(BpaError::new("LOGIC.FORBIDDEN_TARGET", class));
    }
    if repo.obj(tx, &oid)?.is_some() || repo.find_entry(tx, &oid)?.is_some() {
        return Err(BpaError::new("TXN.ALREADY_EXISTS", oid));
    }
    let slug = s(ch, "slug").unwrap_or_default().to_string();
    names::check_name("slug", &slug)?;
    let mut given = object(ch.get("body").unwrap_or(&json!({})));
    crate::sheets::sheet_body(&class, &mut given)?;
    // STD-CLS-004 (1.0-rc3): the lifecycle state is set by the kernel, never by a change
    let lc = lcs.lifecycle(repo, Some(tx), &tx.state, repo.reg, &class)?;
    if ch.get("state").is_some() || (lc.is_some() && given.contains_key("state")) {
        return Err(BpaError::new(
            "FLOW.STATE_DIRECT_WRITE",
            "the lifecycle state changes by transitions only",
        ));
    }
    let state: Option<String> = lc.as_ref().map(|(_, _, body)| {
        flow.transitions
            .get(&oid)
            .map(|m| m.to.clone())
            .unwrap_or_else(|| s(body, "initial").unwrap_or_default().to_string())
    });
    crate::rules::apply_defaults(
        repo,
        tx,
        repo.reg,
        cache,
        &class,
        &mut given,
        &default_valid(&class),
        rule_ctx,
        state.as_deref(),
    )?;
    let body = repo.reg.check_body(&class, &Value::Object(given), false)?;
    let a = ch
        .get("valid")
        .and_then(|v| s(v, "from"))
        .map(str::to_string)
        .unwrap_or_else(|| default_valid(&class));
    let vh = repo.new_version(
        tx,
        &oid,
        &class,
        &slug,
        &body,
        &a,
        None,
        None,
        false,
        provenance,
        state.as_deref(),
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

#[allow(clippy::too_many_arguments)]
fn update(
    repo: &mut Repo<'_>,
    tx: &mut Tx,
    ch: &Value,
    op: &str,
    default_valid: &dyn Fn(&str) -> String,
    result: &mut Value,
    provenance: Option<&str>,
    lcs: &crate::flow::Lifecycles,
    flow: &ubos_flow::Flow,
) -> Result<(), BpaError> {
    let oid = s(ch, "object_id").unwrap_or_default().to_string();
    let Some(o) = repo.obj(tx, &oid)? else {
        return Err(if repo.find_entry(tx, &oid)?.is_some() {
            BpaError::new("LEDG.IMMUTABLE_ENTRY", oid)
        } else {
            BpaError::new("IAM.NOT_FOUND", "unknown object")
        });
    };
    if o.kind == "system" && ch.get(SYSTEM_MARK) != Some(&json!(true)) {
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
    // STD-CLS-004 (1.0-rc3): the state moves by transitions only; field rules of the state
    let lc = lcs.lifecycle(repo, Some(tx), &tx.state, repo.reg, &o.class)?;
    let given_body = ch.get("body").and_then(Value::as_object);
    if ch.get("state").is_some()
        || (lc.is_some() && given_body.is_some_and(|b| b.contains_key("state")))
    {
        return Err(BpaError::new(
            "FLOW.STATE_DIRECT_WRITE",
            "the lifecycle state changes by transitions only",
        ));
    }
    let moved = flow.transitions.get(&oid);
    let base_state = crate::flow::state_of(&base);
    let state: Option<String> = moved.map(|m| m.to.clone()).or_else(|| base_state.clone());
    if let (Some((_, _, lcb)), Some(bs), Some(given)) = (&lc, &base_state, given_body)
        && op != "end"
    {
        let read_only = ubos_flow::field_rules(lcb, bs)
            .and_then(|r| r.get("read_only"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for f in read_only.iter().filter_map(Value::as_str) {
            if given.contains_key(f) && !moved.is_some_and(|m| m.fields.contains(f)) {
                return Err(BpaError::with_details(
                    "FLOW.FIELD_RULE_VIOLATED",
                    json!({"state": bs, "field": f, "rule": "read_only"}),
                ));
            }
        }
    }
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
            state.as_deref(),
        )?;
        obj.timeline = ubos_time::apply(&o.timeline, "end", &vh, &d, None).map_err(tl_err)?;
        obj.head = vh;
        retro_from = d;
        retro_to = None;
    } else {
        let mut body = base_body.clone();
        let given = ch.get("body").unwrap_or(&json!({})).clone();
        if o.class == "GovernanceSheet"
            && given.get("source").is_some()
            && given.get("ast").is_none()
        {
            body.remove("ast");
        }
        body.extend(repo.reg.check_body(&o.class, &given, true)?);
        crate::sheets::sheet_body(&o.class, &mut body)?;
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
            state.as_deref(),
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
    dry: bool,
    max_violations: u64,
    cache: &mut ubos_rules::Cache,
    flow: Option<&ubos_flow::Flow>,
    refs_indexed: bool,
) -> Result<(Value, Vec<String>, u32), BpaError> {
    let flow = flow.cloned().unwrap_or_default();
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
    // -041 run (DEC-050): on the indexes at the base when the reference rows serve the
    // branch (PH-2 step 0b), on the full state otherwise.
    let exts = {
        let mut r = Repo::new(&mut *store, registry);
        let head = r.head(&branch)?;
        let cseq = r.meta(head)?.cseq;
        let objs = r.objects_in(&branch, cseq, Some("ClassExtension"))?;
        let mut exts = Vec::new();
        for o in objs.values() {
            let v = r.version(&o.head)?;
            if v["tombstone"] != json!(true) {
                exts.push(v["body"].clone());
            }
        }
        // sheets and property declarations are checked over the whole set (STD-LGS-022)
        let mut has_sheets = false;
        for c in crate::sheets::SHEET_CLASSES {
            has_sheets |= !r.objects_in(&branch, cseq, Some(c))?.is_empty();
        }
        // the lifecycle objects of the head (STD-CLS-004); those of this commit win
        let lcs = crate::flow::Lifecycles::at(&mut r, &branch, cseq)?;
        (exts, has_sheets, lcs)
    };
    let (exts, has_sheets, lcs) = exts;
    let writes_extension = changes
        .iter()
        .any(|c| s(c, "class") == Some("ClassExtension"));
    let writes_sheet = changes
        .iter()
        .any(|c| s(c, "class").is_some_and(|k| crate::sheets::SHEET_CLASSES.contains(&k)));
    let full = needs_full_state(registry)
        || !exts.is_empty()
        || writes_extension
        || has_sheets
        || writes_sheet;
    let reg_ext;
    let reg: &Registry = if exts.is_empty() {
        registry
    } else {
        reg_ext = registry.with_extensions(&exts)?;
        &reg_ext
    };
    // an extension that declares a relationship field has no reference rows before it
    let ext_relationships = !exts.is_empty()
        && ubos_version::state::relationship_fields(reg)
            != ubos_version::state::relationship_fields(registry);
    let mut repo = Repo::new(store, reg);
    let has_parent_line = meta.parent_line.is_some();
    let mut tx = if has_parent_line {
        repo.begin_view(&branch)?
    } else {
        repo.begin(&branch)?
    };
    tx.now = Some(now_.to_string());
    // STD-FND-028 (BPA 1.0-rc9): the codes Currency objects add on the branch
    let _added = crate::finance::currencies_for(&mut repo, &mut tx, &changes)?;
    if full && !has_parent_line {
        if refs_indexed && !ext_relationships {
            // the governance objects the checks and the enforcement read as a set
            tx.partial = true;
            if !exts.is_empty() || writes_extension {
                repo.load_class(&mut tx, "ClassExtension")?;
            }
            if has_sheets || writes_sheet {
                for c in crate::sheets::SHEET_CLASSES {
                    repo.load_class(&mut tx, c)?;
                }
            }
        } else {
            repo.load_all(&mut tx)?;
        }
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
    let rule_ctx = crate::rules::rule_ctx(&process.p, &meta.kind, &repo.store.authority());
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
                create(
                    &mut repo,
                    &mut tx,
                    ch,
                    &default_valid,
                    &mut result,
                    rid,
                    &rule_ctx,
                    cache,
                    &lcs,
                    &flow,
                )?;
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
                update(
                    &mut repo,
                    &mut tx,
                    ch,
                    op,
                    &default_valid,
                    &mut result,
                    rid,
                    &lcs,
                    &flow,
                )?;
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
    // STD-FND-028, -029, STD-KIND-029 (BPA 1.0-rc9): currencies, rates and accounts
    {
        let written: Vec<(String, String)> = result["versions"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                    .collect()
            })
            .unwrap_or_default();
        crate::finance::check(&mut repo, &mut tx, &written)?;
        // DSN-DVM-420 (PH-2 step 3c): the projection rows of the commit
        crate::projections::maintain(&mut repo, &mut tx, &written)?;
        // STD-KIND-025 (BPA 1.0-rc8, PH-2 step 3d): the transitions of period objects
        crate::periods::check_transitions(&mut repo, &mut tx, &written)?;
        // STD-LGS-062 (BPA 1.0-rc10, PH-2 step 4e): the work queues written
        crate::work::check(&mut repo, &mut tx, &written)?;
        // STD-LGS-072 (BPA 1.0-rc11, PH-2 step 5e): the rule tests written
        crate::ruleops::check(&mut repo, &mut tx, &written)?;
        // STD-TXN-090 (BPA 1.0-rc8, PH-2 step 6b): the workflow definitions written
        crate::workflows::check(&mut repo, &mut tx, &written)?;
    }
    // STD-CLS-004: the shape and the extension rule of the lifecycle objects written
    {
        let written: Vec<(String, String)> = result["versions"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                    .collect()
            })
            .unwrap_or_default();
        lcs.check_written(&mut repo, &tx, reg, &written)?;
        crate::authz::check_written(&mut repo, &tx, reg, &written)?;
    }
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
        crate::sheets::check_sheets(&mut repo, &tx, &written, cache)?;
    }
    // STD-CLS-001, DSN-DVM-111: unique keys (PH-2 step 0b-3)
    {
        let written: Vec<(String, String)> = result["versions"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                    .collect()
            })
            .unwrap_or_default();
        crate::keys::check(&mut repo, &mut tx, reg, &written, refs_indexed)?;
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
    // STD-LGS-011, -030: commit properties, validations and invariants (DEC-067).
    let sheets_bound: BTreeSet<String>;
    {
        let written: Vec<(String, String)> = changes
            .iter()
            .filter_map(|c| s(c, "object_id"))
            .filter_map(|oid| {
                result["versions"]
                    .get(oid)
                    .and_then(Value::as_str)
                    .map(|v| (oid.to_string(), v.to_string()))
            })
            .collect();
        let _phase = ubos_types::trace::span("dvm.rules");
        let (warnings, bound) = crate::rules::enforce(
            &mut repo,
            &tx,
            reg,
            cache,
            &crate::rules::Enforce {
                process: &process.p,
                via_change_set,
                branch_kind: &meta.kind,
                protected: ubos_version::PROTECTED.contains(&meta.kind.as_str()),
                written: &written,
                ended: &ended,
                default_valid: &default_valid,
                max_violations,
            },
        )?;
        if !warnings.is_empty() {
            result["warnings"] = Value::Array(warnings);
        }
        sheets_bound = bound;
    }
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
        &lcs,
        &flow,
        &sheets_bound,
    )?;
    tx.extra.insert("rule_binding".into(), binding);
    if dry {
        // the validation preview (FR-RULE-035): the same evaluation, nothing written
        return Ok((result, classes, 0));
    }
    if !derived.is_empty() {
        tx.extra.insert("derived".into(), Value::Array(derived));
    }
    if process.explicit {
        tx.extra.insert("process_id".into(), json!(process.id));
    }
    if let Some(a) = process.p.get("initiator").filter(|v| v.is_string()) {
        tx.extra.insert("author".into(), a.clone());
    }
    // the audit trail (FR-AUD-011, step 13c): on behalf of whom, why and how
    for k in ["on_behalf_of", "channel", "client", "message", "reason"] {
        if let Some(v) = process.p.get(k).filter(|v| v.is_string()) {
            tx.extra.insert(k.into(), v.clone());
        }
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
        tx.idempotency_expires = Some(idempotency_expiry(repo.store, now_)?);
    }
    let head = tx.head;
    let id = {
        let _phase = ubos_types::trace::span("dvm.flush");
        repo.write(tx, vec![head])?
    };
    let c = repo.commit(id)?;
    result["commit_id"] = json!(c.commit_id);
    result["tx_time"] = json!(c.tx_time);
    Ok((result, classes, effects))
}

/// The default and smallest retention of idempotency records, in days (FR-TXN-032).
pub const IDEMPOTENCY_RETENTION_DAYS: i64 = 7;

/// The retention of the VAE's idempotency records in days (FR-TXN-032, DSN-DVM-531).
pub fn idempotency_retention(store: &mut dyn Store) -> Result<i64, BpaError> {
    Ok(store
        .meta("idempotency_retention_days")?
        .and_then(|v| v.as_i64())
        .unwrap_or(IDEMPOTENCY_RETENTION_DAYS))
}

/// When an idempotency record written at `now` expires: `now` plus the VAE's retention.
fn idempotency_expiry(store: &mut dyn Store, now: &str) -> Result<String, BpaError> {
    let days = idempotency_retention(store)?;
    let t = chrono::DateTime::parse_from_rfc3339(now)
        .map_err(|_| BpaError::new("MODEL.INVALID_VALUE", "the clock is not an instant"))?
        .with_timezone(&chrono::Utc);
    Ok((t + chrono::Duration::days(days))
        .format("%Y-%m-%dT%H:%M:%S%.6fZ")
        .to_string())
}

/// The rule binding of a commit over the transaction's state (STD-TIME-040).
#[allow(clippy::too_many_arguments)]
fn rule_binding_of(
    repo: &mut Repo<'_>,
    tx: &Tx,
    reg: &Registry,
    versions: &Value,
    entries: &Value,
    logic: &BTreeSet<String>,
    lcs: &crate::flow::Lifecycles,
    flow: &ubos_flow::Flow,
    sheets: &BTreeSet<String>,
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
    // STD-CLS-004, STD-TIME-040 (1.0-rc3): the lifecycle versions of the changed classes
    for n in &names {
        if let Some((oid, vh, _)) = lcs.lifecycle(repo, Some(tx), &tx.state, reg, n)?
            && !items
                .iter()
                .any(|i| i.get("object_id") == Some(&json!(oid)))
        {
            items.push(json!({"class": "Lifecycle", "object_id": oid, "class_version": vh}));
        }
    }
    let mut out = json!({"classes": items});
    if !logic.is_empty() {
        out["logic"] = json!(logic);
    }
    // STD-TIME-040, STD-ISA-053 (1.0-rc8): the decision-table versions the process evaluated
    if !flow.tables.is_empty() {
        let mut t = flow.tables.clone();
        t.sort();
        out["tables"] = json!(t);
    }
    if !flow.guards.is_empty() {
        out["guards"] = json!(flow.guards);
    }
    // STD-TIME-040 (1.0-rc7): the sheets the commit enforcement evaluated, in the order of
    // the sheet objects
    if !sheets.is_empty() {
        let mut bound: Vec<(String, String)> = Vec::new();
        for (oid, o) in &tx.state.objects {
            if o.class != "GovernanceSheet" {
                continue;
            }
            for vh in std::iter::once(&o.head).chain(o.timeline.iter().map(|x| &x.version)) {
                if sheets.contains(vh) && !bound.iter().any(|(_, v)| v == vh) {
                    bound.push((oid.clone(), vh.clone()));
                }
            }
        }
        bound.sort();
        out["sheets"] = json!(bound.into_iter().map(|(_, v)| v).collect::<Vec<_>>());
    }
    Ok(out)
}
