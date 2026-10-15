//! The state of one attempt of one process ([`Core`]), the engine-facing [`Host`] trait and
//! the interpreter of host-tier programs: a port of the oracle `exec.py::Run`.
//!
//! A program is JSON `{"functions": {name: {"params": [..], "body": [statement, ..],
//! "retryable": bool}}}`; a statement is one of `let`, `do`, `if`, `each`, `repeat`, `try`,
//! `return`, `fail`. A template is JSON whose strings are L1 expressions.

use crate::isa::{self, Instruction, Limits};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::BTreeMap;
use ubos_expr::model::types::BpaError;
use ubos_expr::{Env, Evaluator, Resolver, Ty, Val, check, from_json, parse, to_json};

/// Statement kinds.
pub const STATEMENTS: &[&str] = &["let", "do", "if", "each", "repeat", "try", "return", "fail"];
/// Roots of L1 paths; never assignable.
pub const ROOTS: &[&str] = &["this", "ctx", "params", "old"];
/// Identity parameters come from the context, never from arguments (STD-CTX-002).
pub const IDENTITY: &[&str] = &["tenant", "vae", "principal", "on_behalf_of"];
/// Instructions implemented in BPA 1.0-rc1.
pub const IMPLEMENTED: &[&str] = &[
    "commit.create",
    "commit.update",
    "commit.end",
    "commit.move",
    "commit.append",
    "commit.reverse",
    "commit.adjust",
    "commit.link",
    "commit.unlink",
    "read.get",
    "read.history",
    "read.timeline",
    "read.diff",
    "read.effective",
    "read.explain",
    "run.port",
    "run.super",
    "run.logic",
    "run.async",
    "emit.event",
];
/// System instructions and their determinism class.
pub const SYSTEM: &[(&str, &str)] = &[
    ("sys.uuid", "pure"),
    ("sys.random_bytes", "pure"),
    ("adapter.observe", "journaled"),
];
/// Actions whose denial reads as absence (STD-ISA-005).
pub const READ_ACTIONS: &[&str] = &["read", "read_history", "reflect", "explain"];
/// Branch kinds that run verified assets only (STD-CTX-022).
pub const PROTECTED: &[&str] = &["main", "release", "overlay"];
/// Time-zone database version recorded in journals.
pub const TZDB: &str = "2025b";

fn invalid(m: &str) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", m)
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// A function activation.
#[derive(Debug, Clone, Default)]
pub struct Frame {
    /// The asset body.
    pub asset: Value,
    /// Version hash of the asset (none for the synthetic frame of a port entry).
    pub vh: Option<String>,
    /// Function name.
    pub fname: Option<String>,
    /// Object identifier for port functions (`self_id`).
    pub oid: Option<String>,
    /// Body of `this` for port functions.
    pub this: Option<Value>,
    /// Class of `this`.
    pub cls: Option<String>,
    /// Port name.
    pub port: Option<String>,
    /// Class whose implementation runs (for `run.super`).
    pub impl_class: Option<String>,
    /// Parameters.
    pub params: BTreeMap<String, Val>,
}

/// An object created or updated in the unit of work.
#[derive(Debug, Clone)]
pub struct Pending {
    /// Class.
    pub class: String,
    /// Slug.
    pub slug: String,
    /// Body after the pending changes.
    pub body: Map<String, Value>,
    /// Index of the `create` change in the unit of work.
    pub change: Option<usize>,
    /// Index of the `update` change in the unit of work.
    pub update: Option<usize>,
}

/// Journal records being replayed (STD-CTX-012).
#[derive(Debug, Clone)]
pub struct Replay {
    /// Journal records with their results.
    pub records: Vec<Value>,
    /// First divergence found.
    pub divergence: Option<Value>,
}

/// The state of one attempt of one process.
#[derive(Debug)]
pub struct Core {
    /// Branch.
    pub branch: String,
    /// Profile.
    pub profile: String,
    /// Effective limits.
    pub limits: Limits,
    /// Grants (`action` or `action:Class`); `None` means unrestricted.
    pub grants: Option<Vec<String>>,
    /// Observed values of the outside world (`adapter.observe`).
    pub world: Map<String, Value>,
    /// Entropy seed.
    pub seed: [u8; 32],
    /// Replay, when re-executing from a journal.
    pub replay: Option<Replay>,
    /// Units charged.
    pub units: u64,
    /// Instruction calls.
    pub calls: u64,
    entropy_n: u64,
    /// Instruction calls per family.
    pub families: BTreeMap<String, u64>,
    /// Unit of work: the changes of the commit.
    pub uow: Vec<Value>,
    /// Objects created or updated so far, by identifier.
    pub pending: BTreeMap<String, Pending>,
    /// Read set: object → head version read.
    pub reads: BTreeMap<String, String>,
    /// Derivation inputs `(object, version, asof)` read so far.
    pub triples: Vec<Value>,
    /// Effects emitted.
    pub effects: Vec<Value>,
    /// Journal records.
    pub records: Vec<Value>,
    /// Snapshot pins: branch → commit ID.
    pub pins: BTreeMap<String, String>,
    /// Logic assets pinned for the process: object → version hash.
    pub pinned: BTreeMap<String, String>,
    /// Logic versions used, in first-use order.
    pub logic: Vec<String>,
    /// Activation stack.
    pub frames: Vec<Frame>,
    /// Cancel once this many units are spent (adapter test hook).
    pub cancel_after: Option<u64>,
    /// The execution context (STD-CTX-001), as JSON.
    pub context: Value,
    /// Process identifier.
    pub process_id: String,
    /// Process kind.
    pub process_kind: String,
}

/// The engine side of the instructions: everything that touches the store.
pub trait Host {
    /// Run an implemented, validated instruction.
    fn instruction(
        &mut self,
        core: &mut Core,
        decl: &Instruction,
        args: &Map<String, Value>,
    ) -> Result<Value, BpaError>;
    /// The typed object behind a reference in L1 (`ref()`, `exists()`).
    fn typed_ref(&mut self, core: &mut Core, uri: &str) -> Result<Option<Val>, BpaError>;
    /// The static type of a class (custom value types as their base).
    fn class_type(&mut self, class: &str) -> Result<Ty, BpaError>;
    /// Whether the host implements an instruction outside `IMPLEMENTED` (the connector and
    /// model instructions of a node host with services, CTR-002).
    fn implements(&self, _name: &str) -> bool {
        false
    }
}

impl Core {
    /// A new run: profile and limits from the input (`ctx.profile`, `limits`), grants,
    /// world, seed; the context is built by the engine.
    pub fn new(
        i: &Value,
        seed: [u8; 32],
        process_id: &str,
        process_kind: &str,
    ) -> Result<Core, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let profile = i
            .get("ctx")
            .and_then(|c| s(c, "profile"))
            .unwrap_or("standard")
            .to_string();
        let mut limits = isa::limits(&profile).ok_or_else(|| invalid("unknown profile"))?;
        for (k, v) in i
            .get("limits")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            let (Some(cur), Some(n)) = (limits.get(k), v.as_u64().filter(|_| v.is_u64())) else {
                return Err(invalid(&format!("unknown limit {k}")));
            };
            if n > cur {
                return Err(BpaError::new(
                    "IAM.DENIED",
                    "raising a limit needs an entitlement",
                ));
            }
            limits.set(k, n);
        }
        Ok(Core {
            branch,
            profile,
            limits,
            grants: i.get("grants").and_then(Value::as_array).map(|g| {
                g.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            }),
            world: i
                .get("world")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
            seed,
            replay: None,
            units: 0,
            calls: 0,
            entropy_n: 0,
            families: BTreeMap::new(),
            uow: Vec::new(),
            pending: BTreeMap::new(),
            reads: BTreeMap::new(),
            triples: Vec::new(),
            effects: Vec::new(),
            records: Vec::new(),
            pins: BTreeMap::new(),
            pinned: BTreeMap::new(),
            logic: Vec::new(),
            frames: Vec::new(),
            cancel_after: i.get("cancel_after_units").and_then(Value::as_u64),
            context: Value::Null,
            process_id: process_id.to_string(),
            process_kind: process_kind.to_string(),
        })
    }

    /// Build the execution context (STD-CTX-001): the caller sets the members the
    /// standard lets it set; the implementation sets the rest.
    #[allow(clippy::too_many_arguments)]
    pub fn build_context(
        &mut self,
        i: &Value,
        initiator: &str,
        tenant: &str,
        today: &str,
        now: &str,
        trigger: &Value,
    ) -> Result<(), BpaError> {
        let c = i.get("ctx").cloned().unwrap_or_else(|| json!({}));
        let g = |k: &str, d: &str| -> Value { c.get(k).cloned().unwrap_or_else(|| json!(d)) };
        let mut ctx = json!({
            "principal": g("principal", initiator),
            "principal_kind": g("principal_kind", "human"),
            "auth_level": g("auth_level", "aal1"),
            "tenant": tenant, "vae": tenant, "branch": self.branch,
            "asof": c.get("asof").filter(|v| !v.is_null()).cloned().unwrap_or_else(|| json!(today)),
            "time": "head", "now": now,
            "locale": g("locale", "en-US"),
            "time_zone": g("time_zone", "UTC"),
            "process_id": self.process_id,
            "trigger": trigger,
            "channel": g("channel", "api"),
            "profile": self.profile,
            "budget_remaining": {"units": self.limits.units, "instruction_calls": self.limits.instruction_calls},
            "replay": self.replay.is_some(),
        });
        if let Some(o) = c
            .get("on_behalf_of")
            .filter(|v| !v.is_null() && **v != json!(false))
        {
            ctx["on_behalf_of"] = o.clone();
        }
        if let Some(x) = c.get("input").filter(|v| !v.is_null()) {
            ctx["input"] = x.clone();
        }
        for k in c.as_object().map(|m| m.keys()).into_iter().flatten() {
            if !matches!(
                k.as_str(),
                "principal"
                    | "principal_kind"
                    | "auth_level"
                    | "asof"
                    | "locale"
                    | "time_zone"
                    | "channel"
                    | "profile"
                    | "on_behalf_of"
                    | "input"
            ) {
                return Err(BpaError::new(
                    "LOGIC.CONTEXT_IMMUTABLE",
                    format!("the implementation sets {k}"),
                ));
            }
        }
        self.context = ctx;
        Ok(())
    }

    /// `ctx.asof`.
    pub fn asof(&self) -> String {
        s(&self.context, "asof").unwrap_or_default().to_string()
    }

    // ---- metering (STD-CTX-004…006) ----

    /// Charge `n` units; `LOGIC.LIMIT_EXCEEDED` past the budget, `LOGIC.CANCELLED` past the
    /// cancellation point.
    pub fn charge(&mut self, n: u64) -> Result<(), BpaError> {
        self.units = self.units.saturating_add(n);
        if self.cancel_after.is_some_and(|c| self.units > c) {
            return Err(BpaError::new("LOGIC.CANCELLED", "cancelled"));
        }
        if self.units > self.limits.units {
            return Err(BpaError::new("LOGIC.LIMIT_EXCEEDED", "operation units"));
        }
        Ok(())
    }

    /// Charge the per-item cost of an instruction.
    pub fn per_item(&mut self, d: &Instruction, n: u64) -> Result<(), BpaError> {
        self.charge(d.per_item.saturating_mul(n))
    }

    /// The metering record of the run (STD-CTX-005).
    pub fn metering(&self, outcome: &str) -> Value {
        json!({"process_id": self.process_id, "asset": self.logic.first(), "units": self.units,
            "cpu_us": 0, "peak_memory": 0, "wall_us": 0, "instructions": self.families,
            "connector_calls": self.families.get("call").copied().unwrap_or(0), "outcome": outcome})
    }

    // ---- entropy (STD-CTX-010) ----

    /// `n` deterministic bytes from the seed.
    pub fn random_bytes(&mut self, n: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(n + 32);
        while out.len() < n {
            let mut h = Sha256::new();
            h.update(self.seed);
            h.update(self.entropy_n.to_be_bytes());
            out.extend_from_slice(&h.finalize());
            self.entropy_n += 1;
        }
        out.truncate(n);
        out
    }

    /// A deterministic UUID (version 4 layout).
    pub fn uuid(&mut self) -> String {
        let mut b = self.random_bytes(16);
        b[6] = (b[6] & 0x0f) | 0x40;
        b[8] = (b[8] & 0x3f) | 0x80;
        let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
        format!(
            "{}-{}-{}-{}-{}",
            &h[..8],
            &h[8..12],
            &h[12..16],
            &h[16..20],
            &h[20..]
        )
    }

    // ---- authorization (STD-ISA-005) ----

    /// Check an action against the grants.
    pub fn authorize(&self, action: &str, cls: &str) -> Result<(), BpaError> {
        let Some(g) = &self.grants else {
            return Ok(());
        };
        if action == "none"
            || g.iter()
                .any(|x| x == action || *x == format!("{action}:{cls}"))
        {
            return Ok(());
        }
        if READ_ACTIONS.contains(&action) {
            return Err(BpaError::new("IAM.NOT_FOUND", "not visible"));
        }
        Err(BpaError::new("IAM.DENIED", format!("{action} on {cls}")))
    }

    // ---- unit of work ----

    /// Add a change to the unit of work; creates, updates and appends carry the
    /// derivation of the current frame (STD-TXN-030). Returns its index.
    pub fn add(&mut self, mut change: Value) -> Result<usize, BpaError> {
        if (self.uow.len() as u64).saturating_add(1) > self.limits.changes {
            return Err(BpaError::new("LOGIC.LIMIT_EXCEEDED", "changes per commit"));
        }
        if matches!(s(&change, "op"), Some("create" | "update" | "append")) {
            let vh = self.frames.last().and_then(|f| f.vh.clone());
            change["derivation"] =
                json!({"logic": [vh], "inputs": self.triples, "mode": "as_effective"});
        }
        self.uow.push(change);
        Ok(self.uow.len() - 1)
    }

    /// The current frame.
    pub fn frame(&self) -> &Frame {
        self.frames.last().expect("a frame")
    }
}

/// Resolver of `ref()` and `exists()` inside programs.
struct RefResolver<'a> {
    inner: RefCell<(&'a mut Core, &'a mut dyn Host)>,
}

impl Resolver for RefResolver<'_> {
    fn resolve(&self, uri: &str) -> Result<Option<Val>, BpaError> {
        let mut b = self.inner.borrow_mut();
        let (core, host) = &mut *b;
        host.typed_ref(core, uri)
    }
}

/// Evaluate an L1 expression of a program in the current frame; a non-text value is
/// returned as it is.
pub fn eval(
    core: &mut Core,
    host: &mut dyn Host,
    text: &Value,
    scope: &BTreeMap<String, Val>,
) -> Result<Val, BpaError> {
    if !text.is_string() {
        return Ok(from_json(&Ty::Any, text));
    }
    let f = core.frame().clone();
    let this_t = match &f.cls {
        Some(c) => Some(host.class_type(c)?),
        None => None,
    };
    let mut env = Env::new(
        this_t.clone(),
        Some(Ty::Obj(
            f.params.keys().map(|k| (k.clone(), Ty::Any)).collect(),
        )),
        None,
        None,
    );
    for k in scope.keys() {
        env.vars.insert(k.clone(), Ty::Any);
    }
    let ast = checked(text, &mut env)?;
    let ctx = from_json(&ubos_expr::check::ctx_type(), &core.context);
    let budget = core.limits.units.saturating_sub(core.units);
    let this = match (&this_t, &f.this) {
        (Some(t), Some(b)) => Some(from_json(t, b)),
        _ => None,
    };
    let resolver = RefResolver {
        inner: RefCell::new((core, host)),
    };
    let mut ev = Evaluator::new(&env);
    ev.ctx = ctx;
    ev.max_units = budget;
    ev.this = this;
    ev.params = Some(Val::Map(f.params.clone()));
    ev.vars = scope.clone();
    ev.resolver = Some(&resolver);
    let r = ev.eval(&ast.0);
    let used = ev.units();
    drop(ev);
    let (core, _) = resolver.inner.into_inner();
    core.charge(used)?;
    r
}

thread_local! {
    static L1: RefCell<std::collections::HashMap<String, std::rc::Rc<crate::program::NodeBox>>> =
        RefCell::new(std::collections::HashMap::new());
}

/// A checked syntax tree in the cache.
pub struct NodeBox(pub ubos_expr::Node);

/// The parsed and type-checked tree of an expression, cached by the expression text and
/// the static environment it was checked in (DSN-DVM-610: by expression and model).
fn checked(text: &Value, env: &mut Env) -> Result<std::rc::Rc<NodeBox>, BpaError> {
    let key = format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}",
        text, env.this, env.params, env.vars, env.target
    );
    if let Some(n) = L1.with(|c| c.borrow().get(&key).cloned()) {
        return Ok(n);
    }
    let ast = parse(text)?;
    check(&ast, env)?;
    let n = std::rc::Rc::new(NodeBox(ast));
    L1.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 8192 {
            c.clear();
        }
        c.insert(key, n.clone());
    });
    Ok(n)
}

/// Instantiate a template: strings are L1 expressions.
pub fn template(
    core: &mut Core,
    host: &mut dyn Host,
    t: &Value,
    scope: &BTreeMap<String, Val>,
) -> Result<Value, BpaError> {
    Ok(match t {
        Value::Object(m) => {
            let mut out = Map::new();
            for (k, v) in m {
                out.insert(k.clone(), template(core, host, v, scope)?);
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(
            a.iter()
                .map(|v| template(core, host, v, scope))
                .collect::<Result<_, _>>()?,
        ),
        Value::String(_) => to_json(&eval(core, host, t, scope)?)?,
        other => other.clone(),
    })
}

/// Run an exported function of the frame's asset with `args` (a list or a map).
pub fn run_function(
    core: &mut Core,
    host: &mut dyn Host,
    mut frame: Frame,
    args: &Value,
) -> Result<Value, BpaError> {
    if s(&frame.asset, "tier") == Some("l2") {
        return crate::l2::run_function(core, host, frame, args);
    }
    if (core.frames.len() as u64).saturating_add(1) > core.limits.depth {
        return Err(BpaError::new("LOGIC.LIMIT_EXCEEDED", "nesting depth"));
    }
    let fname = frame.fname.clone().unwrap_or_default();
    let fun = frame.asset["source"]["functions"].get(&fname).cloned();
    let exported = frame.asset["exports"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|e| e.as_str() == Some(fname.as_str()) || s(e, "name") == Some(fname.as_str()));
    let Some(fun) = fun.filter(|_| exported) else {
        return Err(BpaError::new(
            "LOGIC.FORBIDDEN_CAPABILITY",
            format!("function {fname} is not exported"),
        ));
    };
    let names: Vec<String> = fun["params"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    let given: Map<String, Value> = match args {
        Value::Array(a) => {
            if a.len() > names.len() {
                return Err(invalid("too many arguments"));
            }
            names.iter().cloned().zip(a.iter().cloned()).collect()
        }
        Value::Object(m) => m.clone(),
        _ => Map::new(),
    };
    let mut params: BTreeMap<String, Val> = names
        .iter()
        .map(|n| {
            (
                n.clone(),
                from_json(&Ty::Any, given.get(n).unwrap_or(&Value::Null)),
            )
        })
        .collect();
    if frame.port.is_some() {
        for (k, v) in &given {
            params
                .entry(k.clone())
                .or_insert_with(|| from_json(&Ty::Any, v));
        }
    }
    frame.params = params;
    let mut scope = BTreeMap::new();
    if let Some(oid) = &frame.oid {
        scope.insert("self_id".to_string(), Val::Text(oid.clone()));
    }
    core.frames.push(frame);
    let out = block(
        core,
        host,
        fun["body"].as_array().map(Vec::as_slice).unwrap_or(&[]),
        &mut scope,
    );
    core.frames.pop();
    Ok(out?.unwrap_or(Value::Null))
}

/// Execute statements; `Some(value)` when a `return` was reached.
fn block(
    core: &mut Core,
    host: &mut dyn Host,
    stmts: &[Value],
    scope: &mut BTreeMap<String, Val>,
) -> Result<Option<Value>, BpaError> {
    for st in stmts {
        core.charge(1)?;
        let kinds: Vec<&str> = STATEMENTS
            .iter()
            .copied()
            .filter(|k| st.is_object() && st.get(k).is_some())
            .collect();
        if kinds.len() != 1 {
            return Err(invalid("a statement has one kind"));
        }
        let body = |k: &str| -> &[Value] { st[k].as_array().map(Vec::as_slice).unwrap_or(&[]) };
        match kinds[0] {
            "let" => {
                let name = s(st, "let").unwrap_or_default().to_string();
                if ROOTS.contains(&name.as_str()) {
                    return Err(BpaError::new(
                        "LOGIC.CONTEXT_IMMUTABLE",
                        format!("{name} cannot be assigned"),
                    ));
                }
                let v = eval(core, host, st.get("value").unwrap_or(&Value::Null), scope)?;
                scope.insert(name, v);
            }
            "do" => {
                let args = template(core, host, st.get("args").unwrap_or(&json!({})), scope)?;
                let res = instruction(core, host, s(st, "do").unwrap_or_default(), args)?;
                if let Some(name) = s(st, "as") {
                    if ROOTS.contains(&name) {
                        return Err(BpaError::new(
                            "LOGIC.CONTEXT_IMMUTABLE",
                            format!("{name} cannot be assigned"),
                        ));
                    }
                    scope.insert(name.to_string(), from_json(&Ty::Any, &res));
                }
            }
            "if" => {
                let c = eval(core, host, &st["if"], scope)?;
                let branch = if c == Val::Bool(true) { "then" } else { "else" };
                if let Some(out) = block(core, host, body(branch), scope)? {
                    return Ok(Some(out));
                }
            }
            "each" => {
                let Val::List(items) = eval(core, host, &st["in"], scope)? else {
                    return Err(BpaError::new("LOGIC.TYPE_ERROR", "each needs a list"));
                };
                let var = s(st, "each").unwrap_or_default().to_string();
                for x in items {
                    scope.insert(var.clone(), x);
                    if let Some(out) = block(core, host, body("body"), scope)? {
                        return Ok(Some(out));
                    }
                }
            }
            "repeat" => {
                let n = match eval(core, host, &st["repeat"], scope)? {
                    Val::Int(n) if n > 0 => n,
                    _ => 0,
                };
                for _ in 0..n {
                    if let Some(out) = block(core, host, body("body"), scope)? {
                        return Ok(Some(out));
                    }
                }
            }
            "try" => {
                let out = match block(core, host, body("try"), scope) {
                    Ok(out) => out,
                    Err(e) => {
                        if isa::catchable(e.code) != Some(true) {
                            return Err(e);
                        }
                        if let Some(name) = s(st, "as") {
                            scope.insert(
                                name.to_string(),
                                Val::Map(
                                    [("code".to_string(), Val::Text(e.code.to_string()))].into(),
                                ),
                            );
                        }
                        block(core, host, body("catch"), scope)?
                    }
                };
                if let Some(out) = out {
                    return Ok(Some(out));
                }
            }
            "return" => {
                return Ok(Some(to_json(&eval(core, host, &st["return"], scope)?)?));
            }
            _ => {
                let Some(code) = isa::registered(s(st, "fail").unwrap_or_default()) else {
                    return Err(invalid("unregistered error code"));
                };
                return Err(BpaError::new(code, s(st, "message").unwrap_or_default()));
            }
        }
    }
    Ok(None)
}

/// The gateway (STD-CTX-003): limits, capabilities, ABI, parameters and cost of an
/// instruction call, then the host's implementation.
pub fn instruction(
    core: &mut Core,
    host: &mut dyn Host,
    name: &str,
    args: Value,
) -> Result<Value, BpaError> {
    core.calls += 1;
    if core.calls > core.limits.instruction_calls {
        return Err(BpaError::new("LOGIC.LIMIT_EXCEEDED", "instruction calls"));
    }
    if SYSTEM.iter().any(|(n, _)| *n == name) {
        return system(core, name, &args);
    }
    let asset = core.frame().asset.clone();
    let fam = name.split('.').next().unwrap_or_default();
    let caps: Vec<&str> = asset["capabilities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let Some(d) = isa::by_name(name).filter(|_| {
        isa::families(&core.profile).iter().any(|f| f == fam)
            && (caps.contains(&fam) || caps.contains(&name))
    }) else {
        return Err(BpaError::new("LOGIC.FORBIDDEN_CAPABILITY", name));
    };
    let since = isa::abi_version(Some(&json!(d.since)))?;
    let abi = isa::abi_version(Some(asset.get("abi").unwrap_or(&json!("1.0"))))?;
    if since > abi {
        return Err(BpaError::new(
            "LOGIC.FORBIDDEN_CAPABILITY",
            format!("{name} is newer than the asset ABI"),
        ));
    }
    let Value::Object(args) = args else {
        return Err(invalid("arguments are a map"));
    };
    if let Some(k) = args.keys().find(|k| IDENTITY.contains(&k.as_str())) {
        return Err(BpaError::new(
            "LOGIC.PARAMETER_FORBIDDEN",
            format!("identity comes from the context ({k})"),
        ));
    }
    if args
        .keys()
        .any(|k| !d.params.iter().any(|(n, _, _)| n == k))
    {
        return Err(invalid(&format!("unknown parameter of {name}")));
    }
    if name != "run.async"
        && let Some((p, _, _)) = d.params.iter().find(|(p, _, opt)| {
            !opt && args.get(p).is_none_or(Value::is_null)
                && !(name == "commit.update" && p == "set" && args.contains_key("timeline"))
        })
    {
        return Err(BpaError::new(
            "MODEL.DECLARATION_INCOMPLETE",
            format!("missing parameter {p} of {name}"),
        ));
    }
    core.charge(d.base_cost)?;
    if !IMPLEMENTED.contains(&name) && !host.implements(name) {
        return Err(BpaError::new(
            "LOGIC.NOT_AVAILABLE",
            format!("{name} is not available in BPA 1.0-rc1"),
        ));
    }
    *core.families.entry(fam.to_string()).or_insert(0) += 1;
    host.instruction(core, d, &args)
}

/// System instructions: entropy, and the journaled observation of the adapter.
fn system(core: &mut Core, name: &str, args: &Value) -> Result<Value, BpaError> {
    match name {
        "sys.uuid" => Ok(json!(core.uuid())),
        "sys.random_bytes" => {
            let n = args.get("n").and_then(Value::as_u64);
            let Some(n) = n.filter(|n| (1..=1024).contains(n)) else {
                return Err(invalid("n is 1 to 1024"));
            };
            let b = core.random_bytes(n as usize);
            Ok(json!(
                b.iter().map(|x| format!("{x:02x}")).collect::<String>()
            ))
        }
        _ => journaled(core, name, args, |core| {
            let key = s(args, "name").unwrap_or_default();
            match core.world.get(key) {
                Some(v) => Ok(v.clone()),
                None => Err(BpaError::new(
                    "LOGIC.CONNECTOR_UNAVAILABLE",
                    format!("nothing observed as {key}"),
                )),
            }
        }),
    }
}

/// A journaled instruction (STD-CTX-011): on a replay the result comes from the journal
/// record of this sequence number (`LOGIC.REPLAY_INPUT_MISSING` when absent, a divergence
/// when the instruction or its arguments differ); otherwise `fetch` produces it. The
/// record `{instruction, args_hash, seq, result_ref, result}` is appended either way.
pub fn journaled(
    core: &mut Core,
    name: &str,
    args: &Value,
    fetch: impl FnOnce(&mut Core) -> Result<Value, BpaError>,
) -> Result<Value, BpaError> {
    use ubos_expr::model::types::content_id;
    let req = json!({"instruction": name, "args_hash": content_id("ubos.chunk.v1", args)?});
    let seq = core.records.len() + 1;
    let val = if let Some(rp) = &mut core.replay {
        let Some(r) = rp.records.get(seq - 1).cloned() else {
            return Err(BpaError::new(
                "LOGIC.REPLAY_INPUT_MISSING",
                format!("no journal record {seq}"),
            ));
        };
        if r.get("instruction") != req.get("instruction")
            || r.get("args_hash") != req.get("args_hash")
        {
            rp.divergence.get_or_insert_with(
                || json!({"seq": seq, "expected": r["instruction"], "actual": name}),
            );
        }
        r["result"].clone()
    } else {
        fetch(core)?
    };
    let mut rec = req;
    rec["seq"] = json!(seq);
    rec["result_ref"] = json!(content_id("ubos.chunk.v1", &val)?);
    rec["result"] = val.clone();
    core.records.push(rec);
    Ok(val)
}
