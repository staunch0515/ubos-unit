//! The verification gate of logic assets (STD-CTX-023, -024; PH-1 step 5c, DEC-057):
//! static analysis, type check, declared capabilities, tests and publisher signature, the
//! node-signed verification record and the node identity it signs with; replay of a
//! process from its journal (STD-CTX-012); and the BPU views of STD-BPU-001, -002 and -011.

use crate::execution::{EngineHost, code_identity};
use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use ubos_expr::{Env, Ty};
use ubos_runtime::program::{self, STATEMENTS, SYSTEM};
use ubos_runtime::{Core, Frame, Replay, isa};
use ubos_types::{BpaError, canonical, content_id};
use ubos_version::Repo;

/// Version of the static analyser.
pub const ANALYZER: &str = "1.0.0";
/// Largest source of an asset in bytes.
const MAX_SOURCE: usize = 64 * 1024;
/// Deepest nesting of statements or brackets.
const MAX_NESTING: usize = 32;
/// Constructs an L2 script may not use (STD-CTX-020).
const L2_FORBIDDEN: &[&str] = &[
    "eval",
    "print",
    "debug",
    "import",
    "export",
    "Fn",
    "call",
    "curry",
    "is_def_fn",
    "is_def_var",
    "sleep",
    "exit",
    "parse_json",
];
/// Imports an L3 component may declare.
const L3_WORLD: &[&str] = &["ubos:isa/v1", "ubos:types/v1", "wasi:random"];
/// The adapter's node DID (STD-CONF-005, convention 10); the record lives in the store
/// (`ubos_store::node`, DEC-059).
pub const NODE_DID: &str = ubos_store::node::NODE_DID;

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn not_found(m: &str) -> BpaError {
    BpaError::new("IAM.NOT_FOUND", m)
}

fn rx(p: &str) -> regex::Regex {
    regex::Regex::new(p).expect("grammar")
}

/// Names a host-tier function can see: its parameters and every name it binds.
fn names(f: Option<&Value>) -> BTreeSet<String> {
    fn walk(stmts: Option<&Value>, out: &mut BTreeSet<String>) {
        for st in stmts.and_then(Value::as_array).into_iter().flatten() {
            for k in ["let", "as", "each"] {
                if let Some(n) = s(st, k) {
                    out.insert(n.to_string());
                }
            }
            for k in ["then", "else", "body", "try", "catch"] {
                walk(st.get(k), out);
            }
        }
    }
    let mut out = BTreeSet::new();
    if let Some(f) = f {
        for p in f["params"].as_array().into_iter().flatten() {
            if let Some(n) = p.as_str() {
                out.insert(n.to_string());
            }
        }
        walk(f.get("body"), &mut out);
    }
    out
}

/// Static analysis findings of an asset (STD-CTX-024).
pub fn analyse(body: &Value) -> Result<Vec<Value>, BpaError> {
    let mut f: Vec<Value> = Vec::new();
    let tier = s(body, "tier").unwrap_or_default();
    let caps: Vec<&str> = body["capabilities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let src = body.get("source").cloned().unwrap_or(Value::Null);
    let size = if tier == "l2" {
        src.as_str().unwrap_or_default().len()
    } else {
        canonical(&src)?.len()
    };
    if size > MAX_SOURCE {
        f.push(json!({"rule": "source_size"}));
    }
    match tier {
        "host" => {
            let funcs = src["functions"].as_object().cloned().unwrap_or_default();
            let name = s(body, "name").unwrap_or_default();
            for (fname, fun) in &funcs {
                walk_host(fun.get("body"), 1, fname, fun, name, &caps, &mut f)?;
            }
        }
        "l2" => {
            let text = src.as_str().unwrap_or_default();
            for word in L2_FORBIDDEN {
                if rx(&format!(r"\b{}\s*\(", regex::escape(word))).is_match(text)
                    || (*word == "import" && rx(r"\bimport\b").is_match(text))
                {
                    f.push(json!({"rule": "forbidden_construct", "construct": word}));
                }
            }
            let closure = rx(r"\|[^|]*\|\s*(\{[^}]*\}|[^;\n]*)");
            let sys = rx(r"\bsys\b");
            for m in closure.captures_iter(text) {
                if sys.is_match(&m[1]) {
                    f.push(json!({"rule": "closure_captures_gateway"}));
                }
            }
            for m in rx(r"\bsys\.([a-z]+)\.([a-z_]+)\s*\(").captures_iter(text) {
                let (fam, op) = (&m[1], &m[2]);
                let full = format!("{fam}.{op}");
                if !caps.contains(&fam) && !caps.contains(&full.as_str()) {
                    f.push(json!({"rule": "undeclared_instruction", "instruction": full}));
                }
            }
            let bodies: Vec<&str> = rx(r"\bfn\s+").split(text).collect();
            for m in rx(r"\bfn\s+([a-z_][a-z0-9_]*)\s*\(").captures_iter(text) {
                let fname = &m[1];
                let own = bodies
                    .iter()
                    .find(|b| rx(&format!(r"^{}\s*\(", regex::escape(fname))).is_match(b))
                    .copied()
                    .unwrap_or("");
                let rest = own.get(fname.len()..).unwrap_or("");
                if rx(&format!(r"\b{}\s*\(", regex::escape(fname))).is_match(rest)
                    && !own.contains("@max_depth")
                {
                    f.push(json!({"rule": "unbounded_recursion", "function": fname}));
                }
            }
            let (mut depth, mut mx) = (0i64, 0i64);
            for ch in text.chars() {
                if "({[".contains(ch) {
                    depth += 1;
                }
                if ")}]".contains(ch) {
                    depth -= 1;
                }
                mx = mx.max(depth);
            }
            if mx > MAX_NESTING as i64 {
                f.push(json!({"rule": "nesting_depth"}));
            }
        }
        "l3" => {
            for imp in src["imports"].as_array().into_iter().flatten() {
                if !L3_WORLD.contains(&imp.as_str().unwrap_or_default()) {
                    f.push(json!({"rule": "import_outside_world", "import": imp}));
                }
            }
        }
        _ => {}
    }
    Ok(f)
}

fn walk_host(
    stmts: Option<&Value>,
    depth: usize,
    fname: &str,
    fun: &Value,
    asset_name: &str,
    caps: &[&str],
    f: &mut Vec<Value>,
) -> Result<(), BpaError> {
    if depth > MAX_NESTING {
        f.push(json!({"rule": "nesting_depth", "function": fname}));
        return Ok(());
    }
    for st in stmts.and_then(Value::as_array).into_iter().flatten() {
        let kinds: Vec<&str> = STATEMENTS
            .iter()
            .copied()
            .filter(|k| st.is_object() && st.get(k).is_some())
            .collect();
        if kinds.len() != 1 {
            f.push(json!({"rule": "forbidden_construct", "function": fname}));
            continue;
        }
        if kinds[0] == "do" {
            let name = s(st, "do").unwrap_or_default();
            let fam = name.split('.').next().unwrap_or_default();
            let system = SYSTEM.iter().any(|(n, _)| *n == name);
            if !(system || caps.contains(&fam) || caps.contains(&name)) {
                f.push(json!({"rule": "undeclared_instruction", "function": fname, "instruction": name}));
            }
            if !system && isa::by_name(name).is_none() {
                f.push(
                    json!({"rule": "unknown_instruction", "function": fname, "instruction": name}),
                );
            }
            if name == "run.logic"
                && st["args"].get("asset").and_then(Value::as_str)
                    == Some(&format!("\"{asset_name}\""))
                && fun
                    .get("max_depth")
                    .is_none_or(|v| v.is_null() || v == &json!(false) || v == &json!(0))
            {
                f.push(json!({"rule": "unbounded_recursion", "function": fname}));
            }
        }
        for key in ["then", "else", "body", "try", "catch"] {
            if st.get(key).is_some() {
                walk_host(st.get(key), depth + 1, fname, fun, asset_name, caps, f)?;
            }
        }
        for key in ["let", "if", "in", "repeat", "return"] {
            let src = if key == "let" {
                st.get("value")
            } else {
                st.get(key)
            };
            if let Some(src) = src.filter(|v| v.is_string()) {
                let mut env = Env::new(Some(Ty::Any), Some(Ty::Any), None, None);
                for v in names(Some(fun)).into_iter().chain(["self_id".to_string()]) {
                    env.vars.insert(v, Ty::Any);
                }
                let r = ubos_expr::parse(src).and_then(|ast| ubos_expr::check(&ast, &mut env));
                if let Err(e) = r
                    && e.code == "LOGIC.TYPE_ERROR"
                {
                    f.push(json!({"rule": "type_error", "function": fname}));
                }
            }
        }
    }
    Ok(())
}

impl Engine {
    // ---- node identity (STD-PROOF-010): the store's node record (DEC-059) ----

    /// The node record `{seed, n, keys}`.
    pub fn node_rec(&mut self) -> Result<Value, BpaError> {
        ubos_store::node::record(self.store.as_mut())
    }

    /// Sign a content ID with the node's current key.
    pub fn node_sign(&mut self, cid: &str, role: &str) -> Result<Value, BpaError> {
        ubos_store::node::sign(self.store.as_mut(), cid, role)
    }

    /// `node_did`: the node's DID document (STD-ADDR-010).
    pub fn node_did(&mut self) -> Result<Value, BpaError> {
        ubos_store::node::did(self.store.as_mut())
    }

    /// Whether an asset's verification record is known, passed and covers its code
    /// (STD-CTX-023).
    pub fn asset_verified(&mut self, body: &Value) -> Result<bool, BpaError> {
        let Some(rid) = s(body, "verification") else {
            return Ok(false);
        };
        let Some(rec) = self.store.meta(&format!("verification:{rid}"))? else {
            return Ok(false);
        };
        let passed = rec["record"]["checks"]
            .as_array()
            .is_some_and(|cs| cs.iter().all(|c| c["result"] == json!("pass")));
        Ok(passed && rec["code"] == json!(code_identity(body)?))
    }

    // ---- verification gate (STD-CTX-023, -024) ----

    /// `verify_asset`: analyse, type-check, check capabilities, run the tests and check the
    /// publisher signature of a logic asset; a passing record is signed by the node and
    /// recorded on the asset.
    pub fn verify_asset(
        &mut self,
        branch: &str,
        object_id: &str,
        verifier: Option<&str>,
    ) -> Result<Value, BpaError> {
        let (class, head) = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let (v, _) = repo.view(branch, &json!({}))?;
            let o = v
                .state
                .objects
                .get(object_id)
                .ok_or_else(|| not_found("unknown object"))?;
            (o.class.clone(), o.head.clone())
        };
        if !self
            .registry
            .class(&class)?
            .linearization
            .iter()
            .any(|c| c == "LogicAsset")
        {
            return Err(BpaError::new(
                "MODEL.REFERENCE_INVALID",
                "not a logic asset",
            ));
        }
        let body = Repo::new(self.store.as_mut(), &self.registry).version(&head)?["body"].clone();
        let findings = analyse(&body)?;
        let rule = |x: &Value| s(x, "rule").unwrap_or_default().to_string();
        let statics: Vec<Value> = findings
            .iter()
            .filter(|x| !matches!(rule(x).as_str(), "undeclared_instruction" | "type_error"))
            .cloned()
            .collect();
        let caps: Vec<Value> = findings
            .iter()
            .filter(|x| rule(x) == "undeclared_instruction")
            .cloned()
            .collect();
        let types: Vec<Value> = findings
            .iter()
            .filter(|x| rule(x) == "type_error")
            .cloned()
            .collect();
        let sig_ok = s(&body, "tier") != Some("l3")
            || body
                .get("publisher_signature")
                .is_some_and(|v| !v.is_null() && v != &json!(false) && v != &json!(""));
        let tests = self.run_tests(branch, object_id, &head, &body)?;
        let check = |name: &str, findings: Vec<Value>| json!({"name": name, "result": if findings.is_empty() { "pass" } else { "fail" }, "findings": findings});
        let checks = vec![
            check("static_analysis", statics),
            check("type_check", types),
            check("capabilities", caps),
            check("tests", tests),
            json!({"name": "signature", "result": if sig_ok { "pass" } else { "fail" },
                "findings": if sig_ok { json!([]) } else { json!([{"rule": "publisher_signature_missing"}]) }}),
        ];
        let mut rec = json!({"asset": head, "checks": checks, "analyzer_version": ANALYZER,
            "verified_by": verifier.unwrap_or(NODE_DID), "at": self.now});
        let rid = content_id("ubos.chunk.v1", &rec)?;
        rec["signature"] = self.node_sign(&rid, "signer")?;
        let passed = rec["checks"]
            .as_array()
            .is_some_and(|cs| cs.iter().all(|c| c["result"] == json!("pass")));
        let mut out = json!({"verified": passed, "record": rec});
        if passed {
            self.store.put_meta(
                &format!("verification:{rid}"),
                &json!({"record": out["record"], "code": code_identity(&body)?}),
            )?;
            self.commit(&json!({"branch": branch, "changes": [{"op": "update", "object_id": object_id, "base": head,
                "body": {"state": "verified", "verification": rid}}]}))?;
            out["record_id"] = json!(rid);
        }
        Ok(out)
    }

    /// The failing tests of an asset, each run as a simulation on the branch without
    /// effect on the store.
    fn run_tests(
        &mut self,
        branch: &str,
        oid: &str,
        head: &str,
        body: &Value,
    ) -> Result<Vec<Value>, BpaError> {
        let mut fails = Vec::new();
        let tests: Vec<Value> = body["tests"].as_array().cloned().unwrap_or_default();
        for t in tests.iter().filter(|t| t.is_object()) {
            let i = json!({"branch": branch, "ctx": {"profile": "simulation"}});
            let mut h = Sha256::new();
            h.update(b"ubos.entropy.v1\0test");
            let seed: [u8; 32] = h.finalize().into();
            let pid = "01900000-0000-7000-8000-000000000000";
            let got: Result<Value, BpaError> = (|| {
                let mut core = Core::new(&i, seed, pid, "action")?;
                let principal = self.principal.clone();
                let (today, now, authority) = (
                    self.now[..10].to_string(),
                    self.now.clone(),
                    self.store.authority(),
                );
                core.build_context(
                    &i,
                    &principal,
                    &authority,
                    &today,
                    &now,
                    &json!({"type": "user"}),
                )?;
                core.pinned.insert(oid.to_string(), head.to_string());
                let mut host = EngineHost::new(self);
                let frame = Frame {
                    asset: body.clone(),
                    vh: Some(head.to_string()),
                    fname: s(t, "function").map(str::to_string),
                    ..Frame::default()
                };
                let args = t.get("args").cloned().unwrap_or_else(|| json!([]));
                program::run_function(&mut core, &mut host, frame, &args)
            })();
            match got {
                Ok(v) => {
                    if v != t.get("expect").cloned().unwrap_or(Value::Null) {
                        fails.push(json!({"rule": "test_failed", "function": t.get("function")}));
                    }
                }
                Err(e) => {
                    if t.get("error").and_then(Value::as_str) != Some(e.code) {
                        fails.push(json!({"rule": "test_failed", "function": t.get("function"), "error": e.code}));
                    }
                }
            }
        }
        Ok(fails)
    }

    // ---- replay (STD-CTX-012) ----

    /// `replay`: re-execute a process with its journaled context, snapshots, logic and
    /// records, and compare the outputs.
    pub fn replay_process(
        &mut self,
        process_id: &str,
        tamper: Option<&Value>,
        drop: Option<u64>,
    ) -> Result<Value, BpaError> {
        let j = self.journal_full(process_id)?;
        let mut records: Vec<Value> = j["records"].as_array().cloned().unwrap_or_default();
        if let Some(t) = tamper {
            let seq = t.get("seq").and_then(Value::as_u64).unwrap_or(0) as usize;
            if seq == 0 || seq > records.len() {
                return Err(BpaError::new("MODEL.INVALID_VALUE", "no such record"));
            }
            records[seq - 1]["result"] = t.get("result").cloned().unwrap_or(Value::Null);
        }
        if let Some(d) = drop {
            records.truncate((d as usize).saturating_sub(1));
        }
        let mut inv = j["invocation"].clone();
        if !inv.is_object() {
            inv = json!({});
        }
        let mut ctx = Map::new();
        for k in [
            "principal",
            "principal_kind",
            "auth_level",
            "asof",
            "locale",
            "time_zone",
            "channel",
            "profile",
        ] {
            if let Some(v) = j["context"].get(k) {
                ctx.insert(k.into(), v.clone());
            }
        }
        inv["ctx"] = Value::Object(ctx);
        let seed = unb64u32(s(&j, "entropy_seed").unwrap_or_default())?;
        inv["entropy_seed"] = json!(seed.iter().map(|b| format!("{b:02x}")).collect::<String>());
        inv["process"] = json!({"process_id": process_id});
        let saved_now = std::mem::replace(
            &mut self.now,
            s(&j["context"], "now").unwrap_or_default().to_string(),
        );
        let r = self.run(
            &inv,
            Some(Replay {
                records,
                divergence: None,
            }),
        );
        self.now = saved_now;
        r
    }

    /// Compare the outputs of a replayed run with its journal.
    pub(crate) fn compare_replay(
        core: &Core,
        result: &Value,
        journal: &Value,
    ) -> Result<Value, BpaError> {
        if let Some(d) = core.replay.as_ref().and_then(|r| r.divergence.clone()) {
            return Ok(json!({"outcome": "diverged", "divergence": d}));
        }
        let got = json!({
            "result_hash": content_id("ubos.chunk.v1", result)?,
            "changes_hash": crate::execution::changes_hash(&core.uow)?,
            "effects": core.effects.iter().map(|e| e["type"].clone()).collect::<Vec<_>>(),
        });
        for k in ["result_hash", "changes_hash", "effects"] {
            if got[k] != journal["outputs"][k] {
                let seq = journal["records"].as_array().map(Vec::len).unwrap_or(0) + 1;
                return Ok(
                    json!({"outcome": "diverged", "divergence": {"seq": seq, "output": k,
                    "expected": journal["outputs"][k], "actual": got[k]}}),
                );
            }
        }
        Ok(json!({"outcome": "identical"}))
    }

    // ---- BPU (STD-BPU-001, -002, -011) ----

    fn bpu_class(&self, cls: &str) -> Result<(), BpaError> {
        let c = self
            .registry
            .class(cls)
            .ok()
            .filter(|_| !ubos_model::is_genesis(cls))
            .ok_or_else(|| {
                BpaError::new("MODEL.REFERENCE_INVALID", format!("unknown class {cls}"))
            })?;
        let d = self.registry.decl(cls)?;
        if c.kind.as_deref() != Some("definition")
            || d.get("abstract").is_some_and(|v| v == &json!(true))
            || d.get("trait").is_some_and(|v| v == &json!(true))
        {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "a BPU class is a concrete Definition class",
            ));
        }
        Ok(())
    }

    /// `bpu_descriptor` (DAT-BpuDescriptor): a pure function of the effective definitions
    /// (STD-BPU-002).
    pub fn bpu_descriptor(&mut self, branch: &str, cls: &str) -> Result<Value, BpaError> {
        self.branch_meta(branch)?;
        self.bpu_class(cls)?;
        let st = Repo::new(self.store.as_mut(), &self.registry)
            .view(branch, &json!({}))?
            .0
            .state;
        let exts = crate::relations::extensions_of(
            &mut Repo::new(self.store.as_mut(), &self.registry),
            &st,
        )?;
        let reg = self.registry.with_extensions(&exts)?;
        let eff = reg.class(cls)?;
        let lin = eff.linearization.clone();
        let decl_fields = |c: &str| -> Vec<String> {
            self.registry
                .decl(c)
                .ok()
                .and_then(|d| d["fields"].as_array().cloned())
                .unwrap_or_default()
                .iter()
                .filter_map(|x| s(x, "name").map(str::to_string))
                .collect()
        };
        let mut fields = Vec::new();
        for (name, f) in &eff.fields {
            let origin = lin
                .iter()
                .rev()
                .find(|c| decl_fields(c).iter().any(|x| x == name))
                .cloned()
                .unwrap_or_else(|| match name.rsplit_once('.') {
                    Some((p, _)) => p.to_string(),
                    None => cls.to_string(),
                });
            fields.push(json!({"name": name, "type": f["type"], "origin": origin,
                "classification": f.get("classification").cloned().unwrap_or_else(|| json!("internal"))}));
        }
        let mut ledgers = Vec::new();
        let mut names: Vec<String> = self
            .registry
            .declarations()
            .iter()
            .filter_map(|d| s(d, "name").map(str::to_string))
            .collect();
        names.sort();
        for n in names {
            let e = self.registry.class(&n)?;
            if e.kind.as_deref() != Some("ledger") {
                continue;
            }
            let refs: Vec<String> = e
                .fields
                .iter()
                .filter(|(_, f)| {
                    s(f, "type") == Some("reference")
                        && f["constraints"]
                            .get("target_class")
                            .and_then(Value::as_str)
                            .is_some_and(|t| lin.iter().any(|c| c == t))
                })
                .map(|(k, _)| k.clone())
                .collect();
            if !refs.is_empty() {
                ledgers.push(json!({"class": n, "key": self.registry.decl(&n)?["ledger"]["key"], "fields": refs}));
            }
        }
        let ports: Vec<Value> = eff
            .ports
            .iter()
            .map(|(k, p)| {
                json!({"name": k, "kind": p.decl.get("kind"), "params": p.decl.get("params").cloned().unwrap_or_else(|| json!([])),
                    "result": p.decl.get("result"), "permission": p.decl.get("permission"), "implemented": p.impl_by.is_some()})
            })
            .collect();
        let mut events = Vec::new();
        for c in lin.iter().rev() {
            events.extend(
                self.registry.decl(c)?["events"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        let mut props = Map::new();
        for f in &fields {
            props.insert(
                s(f, "name").unwrap_or_default().to_string(),
                json!({"type": f["type"]}),
            );
        }
        let required: Vec<String> = eff
            .fields
            .iter()
            .filter(|(_, f)| f.get("required") == Some(&json!(true)))
            .map(|(k, _)| k.clone())
            .collect();
        let schema = json!({"type": "object", "properties": props, "required": required});
        Ok(json!({
            "class": {"name": cls, "class_version": eff.class_version},
            "linearization": lin, "fields": fields, "ledgers": ledgers, "ports": ports, "events": events,
            "governance": [], "knowledge": [],
            "proof": {"signed_commits": true, "merkle": true, "anchoring": false},
            "json_schema": content_id("ubos.chunk.v1", &schema)?,
        }))
    }

    /// `bpu_state` (STD-BPU-001, element 2): the object's versions and the entries that
    /// name it in `source_refs`.
    pub fn bpu_state(
        &mut self,
        branch: &str,
        object_id: &str,
        sel: &Value,
    ) -> Result<Value, BpaError> {
        let st = Repo::new(self.store.as_mut(), &self.registry)
            .view(branch, sel)?
            .0
            .state;
        let o = st
            .objects
            .get(object_id)
            .ok_or_else(|| not_found("unknown object"))?;
        self.bpu_class(&o.class)?;
        let uri = crate::relations::uri_of(&self.store.authority(), &o.class, &o.slug);
        let mut entries: Vec<String> = st
            .ledger
            .iter()
            .flat_map(|(_, _, q)| q.iter())
            .filter(|e| e.source_refs.as_ref().is_some_and(|r| r.contains(&uri)))
            .map(|e| e.entry_id.clone())
            .collect();
        entries.sort();
        Ok(
            json!({"timeline": o.timeline.iter().map(|x| json!({"from": x.from, "to": x.to})).collect::<Vec<_>>(),
            "entries": entries}),
        )
    }

    /// `verify_bpu` (STD-BPU-011): the checks before a BPU class version becomes effective
    /// on a protected branch.
    pub fn verify_bpu(&mut self, branch: &str, cls: &str) -> Result<Value, BpaError> {
        self.bpu_class(cls)?;
        let eff = self.registry.class(cls)?.clone();
        let st = Repo::new(self.store.as_mut(), &self.registry)
            .view(branch, &json!({}))?
            .0
            .state;
        let mut assets: Vec<(String, Value)> = Vec::new();
        for o in st.objects.values() {
            let is_logic = self
                .registry
                .class(&o.class)
                .is_ok_and(|c| c.linearization.iter().any(|x| x == "LogicAsset"));
            if !is_logic {
                continue;
            }
            let body =
                Repo::new(self.store.as_mut(), &self.registry).version(&o.head)?["body"].clone();
            if let Some(n) = s(&body, "name") {
                assets.push((n.to_string(), body.clone()));
            }
        }
        let mut unverified = Vec::new();
        for (name, p) in &eff.ports {
            let implementation = s(&p.decl, "impl").unwrap_or_default();
            let asset = implementation.split('#').next().unwrap_or_default();
            let mut ok = false;
            for (n, body) in &assets {
                if n == asset {
                    ok = matches!(s(body, "state"), Some("verified" | "deprecated"))
                        && self.asset_verified(body)?;
                }
            }
            if !ok {
                unverified.push(json!({"port": name}));
            }
        }
        let emitted: BTreeSet<String> = eff
            .ports
            .values()
            .flat_map(|p| p.decl["emits"].as_array().cloned().unwrap_or_default())
            .filter_map(|e| e.as_str().map(str::to_string))
            .collect();
        let mut silent = Vec::new();
        for c in eff.linearization.iter().rev() {
            for e in self.registry.decl(c)?["events"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let n = s(e, "name").unwrap_or_default();
                if !emitted.contains(n) {
                    silent.push(json!({"event": n}));
                }
            }
        }
        let documented = |v: Option<&Value>| v.is_some_and(|d| !d.is_null() && d != &json!(""));
        let mut undocumented = Vec::new();
        if !documented(self.registry.decl(cls)?.get("description")) {
            undocumented.push(json!({"item": cls}));
        }
        for (k, f) in &eff.fields {
            if !documented(f.get("description")) {
                undocumented.push(json!({"item": k}));
            }
        }
        for (k, p) in &eff.ports {
            if !documented(p.decl.get("description")) {
                undocumented.push(json!({"item": k}));
            }
        }
        let check = |name: &str, findings: Vec<Value>| json!({"name": name, "result": if findings.is_empty() { "pass" } else { "fail" }, "findings": findings});
        let checks = vec![
            check("ports_verified", unverified),
            json!({"name": "lifecycle", "result": "not_applicable", "findings": []}),
            check("events_emitted", silent),
            check("documented", undocumented),
            json!({"name": "tests", "result": "not_applicable", "findings": []}),
        ];
        let passed = checks.iter().all(|c| c["result"] != json!("fail"));
        Ok(json!({"passed": passed, "checks": checks}))
    }
}

/// 32 bytes from base64url without padding.
fn unb64u32(v: &str) -> Result<[u8; 32], BpaError> {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let bad = || {
        BpaError::new(
            "MODEL.INVALID_VALUE",
            "seed is 32 bytes in base64url without padding",
        )
    };
    if v.len() != 43 {
        return Err(bad());
    }
    let mut out = Vec::with_capacity(33);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in v.bytes() {
        let x = A.iter().position(|&a| a == c).ok_or_else(bad)? as u32;
        acc = (acc << 6) | x;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out.truncate(32);
    out.try_into().map_err(|_| bad())
}
