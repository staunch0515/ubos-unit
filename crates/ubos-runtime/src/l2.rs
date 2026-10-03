//! The L2 host (STD-CTX-020, DSN-DVM-612; PH-1 step 5d, DEC-058): a Rhai engine
//! hardened per profile (no function pointers, reflection, closures or modules), the instruction gateway bound as `sys.<family>.<op>(#{…})` over
//! the same checks and budget as host programs ([`crate::program::instruction`]), the
//! read-only `ctx`, typed object handles generated from the effective definition, the
//! kernel decimal and money as the script number types (W-5), metering by operations,
//! thread CPU time, wall time and per-execution memory (W-2, W-3), and compiled scripts
//! cached by asset version hash.
//!
//! Scripts run on a dedicated worker thread of the calling thread: the run state
//! ([`Core`]) and the engine host ([`Host`]) stay with the caller, which serves the
//! worker's gateway calls one at a time until the script returns. Only JSON crosses
//! between the threads, so the crate needs no unsafe code; a nested script (a port
//! implemented in L2 called from a script) takes the next worker of the stack.

use crate::program::{self, Host};
use crate::{Core, Frame};
use rhai::packages::{
    ArithmeticPackage, BasicArrayPackage, BasicIteratorPackage, BasicMapPackage,
    BasicStringPackage, BitFieldPackage, LogicPackage, MoreStringPackage, Package,
};
use rhai::{AST, Dynamic, Engine, EvalAltResult, ParseErrorType, Scope};
use serde_json::{Map, Value, json};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};
use ubos_expr::model::types::decimal::{Decimal, Rounding};
use ubos_expr::model::types::values::minor_unit;
use ubos_expr::model::types::{BpaError, canonical};
use ubos_expr::{Ty, Val, from_json};

/// `LOGIC.*` codes of the host (UBS-STD-17 §2).
pub mod codes {
    /// Budget exhausted.
    pub const LIMIT_EXCEEDED: &str = "LOGIC.LIMIT_EXCEEDED";
    /// Cancelled.
    pub const CANCELLED: &str = "LOGIC.CANCELLED";
    /// Undeclared or disallowed capability.
    pub const FORBIDDEN_CAPABILITY: &str = "LOGIC.FORBIDDEN_CAPABILITY";
    /// Attempt to modify the context.
    pub const CONTEXT_IMMUTABLE: &str = "LOGIC.CONTEXT_IMMUTABLE";
    /// Type error.
    pub const TYPE_ERROR: &str = "LOGIC.TYPE_ERROR";
    /// Unknown field of a typed handle.
    pub const UNKNOWN_FIELD: &str = "LOGIC.UNKNOWN_FIELD";
    /// Division by zero.
    pub const DIVISION_BY_ZERO: &str = "LOGIC.DIVISION_BY_ZERO";
    /// Floating point reached a decimal.
    pub const FLOAT_TO_DECIMAL: &str = "LOGIC.FLOAT_TO_DECIMAL";
    /// Size or nesting limits of the source.
    pub const SCRIPT_TOO_COMPLEX: &str = "LOGIC.SCRIPT_TOO_COMPLEX";
}

/// Functions whose only purpose is floating point; they fail with `LOGIC.FLOAT_TO_DECIMAL`.
const FLOAT_FUNCTIONS: &[&str] = &[
    "to_float",
    "parse_float",
    "f32_to_f64",
    "sin",
    "cos",
    "tan",
    "sinh",
    "cosh",
    "tanh",
    "asin",
    "acos",
    "atan",
    "atan2",
    "asinh",
    "acosh",
    "atanh",
    "hypot",
    "sqrt",
    "exp",
    "ln",
    "log",
    "log10",
    "pi",
    "e",
    "to_radians",
    "to_degrees",
    "is_nan",
    "is_finite",
    "is_infinite",
];

/// Host functions: a call with wrong argument types is a type error.
const HOST_FUNCTIONS: &[&str] = &[
    "dec",
    "money",
    "div",
    "round",
    "amount",
    "currency",
    "uuid",
    "random_bytes",
];

/// Reserved words and built-in function-pointer and reflection keywords disabled in the L2
/// profile (STD-CTX-020).
const DISABLED_SYMBOLS: &[&str] = &[
    "eval",
    "print",
    "debug",
    "Fn",
    "call",
    "curry",
    "is_def_fn",
    "is_def_var",
    "is_shared",
];

/// Instruction families reachable as `sys.<family>`.
const FAMILIES: &[&str] = &[
    "commit", "read", "run", "emit", "search", "call", "ask", "adapter",
];

/// Engine sizes and clocks of a profile (STD-CTX-020 derives them from STD-CTX-004; the
/// values are those of the PH-0 sandbox report §4, DEC-034).
#[derive(Debug, Clone, Copy)]
pub struct Sizes {
    /// CPU time in milliseconds.
    pub cpu_ms: u64,
    /// Wall time in milliseconds.
    pub wall_ms: u64,
    /// Memory in bytes.
    pub memory: usize,
    /// Maximum string length in bytes.
    pub string_size: usize,
    /// Maximum array length.
    pub array_size: usize,
    /// Maximum map size.
    pub map_size: usize,
    /// Maximum live variables.
    pub variables: usize,
    /// Maximum script functions.
    pub functions: usize,
    /// Maximum source size in bytes.
    pub source_bytes: usize,
    /// Maximum expression depth at top level and inside functions.
    pub expr_depth: (usize, usize),
}

impl Sizes {
    /// The sizes of a profile; `None` for an unknown name.
    pub fn profile(name: &str) -> Option<Sizes> {
        const MB: usize = 1 << 20;
        let (cpu_ms, wall_ms, memory, scale) = match name {
            "strict" => (500, 2_000, 64 * MB, 1),
            "standard" => (3_000, 10_000, 512 * MB, 8),
            "batch" => (600_000, 1_800_000, 4_096 * MB, 64),
            "simulation" => (3_600_000, 7_200_000, 8_192 * MB, 128),
            "agent" => (10_000, 60_000, 512 * MB, 8),
            _ => return None,
        };
        Some(Sizes {
            cpu_ms,
            wall_ms,
            memory,
            string_size: 1_048_576 * scale,
            array_size: 65_536 * scale,
            map_size: 65_536 * scale,
            variables: 1_024,
            functions: 256,
            source_bytes: 65_536,
            expr_depth: (64, 32),
        })
    }
}

/// Error raised by host functions, carried as `ErrorSystem` so that scripts cannot
/// catch it (DSN-DVM-612).
#[derive(Debug)]
struct HostError(BpaError);

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for HostError {}

fn host_error(code: &'static str, detail: impl Into<String>) -> Box<EvalAltResult> {
    bpa_error(BpaError::new(code, detail))
}

fn bpa_error(e: BpaError) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorSystem(
        e.code.into(),
        Box::new(HostError(e)),
    ))
}

// ---- the worker protocol ----

/// A script to run, with everything it sees.
struct Job {
    profile: String,
    sizes: Sizes,
    key: String,
    source: String,
    fname: String,
    args: Value,
    context: Value,
    this: Option<(String, BTreeMap<String, Ty>, Value)>,
    params: Value,
    self_id: Option<String>,
    /// Units the script may spend, and the cancellation point, both as counts.
    budget: u64,
    cancel_after: Option<u64>,
    depth: usize,
    events: Sender<Event>,
}

/// What the worker tells the caller.
enum Event {
    /// A gateway call; the reply carries the result and the units left.
    Call {
        name: String,
        args: Value,
        reply: Sender<Reply>,
    },
    /// The script returned; `units` is what it spent on operations.
    Done {
        result: Result<Value, BpaError>,
        units: u64,
    },
}

struct Reply {
    result: Result<Value, BpaError>,
    budget: u64,
    cancel_after: Option<u64>,
}

/// A worker thread and the channel to it.
struct Worker {
    jobs: Sender<Job>,
}

thread_local! {
    static WORKERS: RefCell<Vec<Worker>> = const { RefCell::new(Vec::new()) };
}

fn spawn_worker() -> Worker {
    let (tx, rx) = channel::<Job>();
    std::thread::Builder::new()
        .name("ubos-l2".into())
        .spawn(move || worker_loop(rx))
        .expect("spawn an L2 worker");
    Worker { jobs: tx }
}

/// The caller side: hand the job to the worker of this nesting level and serve its
/// gateway calls until it is done.
fn run_job(
    core: &mut Core,
    host: &mut dyn Host,
    job: Job,
    events: Receiver<Event>,
) -> Result<Value, BpaError> {
    let depth = job.depth;
    let sent = WORKERS.with(|w| {
        let mut w = w.borrow_mut();
        while w.len() <= depth {
            w.push(spawn_worker());
        }
        w[depth].jobs.send(job).is_ok()
    });
    if !sent {
        return Err(BpaError::new(
            "LOGIC.NOT_AVAILABLE",
            "the L2 worker is gone",
        ));
    }
    loop {
        let Ok(ev) = events.recv() else {
            return Err(BpaError::new(
                "LOGIC.NOT_AVAILABLE",
                "the L2 worker stopped",
            ));
        };
        match ev {
            Event::Call { name, args, reply } => {
                let r = (|| {
                    let size = canonical(&args).map(|c| c.len()).unwrap_or(0) as u64;
                    core.charge(size / 1024)?;
                    let r = program::instruction(core, host, &name, args)?;
                    let size = canonical(&r).map(|c| c.len()).unwrap_or(0) as u64;
                    core.charge(size / 1024)?;
                    Ok(r)
                })();
                let _ = reply.send(Reply {
                    result: r,
                    budget: core.limits.units.saturating_sub(core.units),
                    cancel_after: core.cancel_after.map(|c| c.saturating_sub(core.units)),
                });
            }
            Event::Done { result, units } => {
                let charged = core.charge(units);
                return match result {
                    Ok(v) => {
                        charged?;
                        Ok(v)
                    }
                    Err(e) => Err(e),
                };
            }
        }
    }
}

// ---- the worker side ----

/// Per-job state of the worker: budget, clocks and the channel to the caller.
struct Run {
    events: Sender<Event>,
    budget: u64,
    cancel_after: Option<u64>,
    ops: u64,
    start: Instant,
    cpu_start: Option<Duration>,
    sizes: Sizes,
    memory_base: usize,
}

thread_local! {
    static RUN: RefCell<Option<Run>> = const { RefCell::new(None) };
    static ENGINES: RefCell<HashMap<String, Rc<Engine>>> = RefCell::new(HashMap::new());
    static ASTS: RefCell<HashMap<String, Rc<AST>>> = RefCell::new(HashMap::new());
}

fn cpu_time() -> Option<Duration> {
    cpu_time::ThreadTime::try_now()
        .ok()
        .map(|t| t.as_duration())
}

/// The progress callback: count operations against the budget, then check cancellation,
/// memory, CPU time and wall time (STD-CTX-004…006).
fn progress(ops: u64) -> Option<Dynamic> {
    RUN.with(|r| {
        let mut r = r.borrow_mut();
        let run = r.as_mut()?;
        run.ops = ops;
        if run.cancel_after.is_some_and(|c| ops > c) {
            return Some(Dynamic::from(codes::CANCELLED));
        }
        if ops > run.budget {
            return Some(Dynamic::from(codes::LIMIT_EXCEEDED));
        }
        if let Some(cur) = crate::memory::current()
            && cur.saturating_sub(run.memory_base) > run.sizes.memory
        {
            return Some(Dynamic::from(codes::LIMIT_EXCEEDED));
        }
        if ops.is_multiple_of(8) {
            if run.start.elapsed() > Duration::from_millis(run.sizes.wall_ms) {
                return Some(Dynamic::from(codes::LIMIT_EXCEEDED));
            }
            if let (Some(cpu), Some(start)) = (cpu_time(), run.cpu_start)
                && cpu.saturating_sub(start) > Duration::from_millis(run.sizes.cpu_ms)
            {
                return Some(Dynamic::from(codes::LIMIT_EXCEEDED));
            }
        }
        None
    })
}

/// A gateway call from the worker: ask the caller, wait for the reply, adopt the
/// updated budget.
fn gateway(name: &str, args: Dynamic) -> Result<Dynamic, Box<EvalAltResult>> {
    let args = if args.is_unit() {
        json!({})
    } else {
        dynamic_to_json(&args, 0).map_err(bpa_error)?
    };
    let (tx, rx) = channel::<Reply>();
    let sent = RUN.with(|r| {
        r.borrow()
            .as_ref()
            .map(|run| {
                run.events
                    .send(Event::Call {
                        name: name.to_string(),
                        args,
                        reply: tx,
                    })
                    .is_ok()
            })
            .unwrap_or(false)
    });
    if !sent {
        return Err(host_error(codes::FORBIDDEN_CAPABILITY, "no running script"));
    }
    let reply = rx
        .recv()
        .map_err(|_| host_error("LOGIC.NOT_AVAILABLE", "the caller is gone"))?;
    RUN.with(|r| {
        if let Some(run) = r.borrow_mut().as_mut() {
            run.budget = reply.budget.saturating_add(run.ops);
            run.cancel_after = reply.cancel_after.map(|c| c.saturating_add(run.ops));
        }
    });
    let v = reply.result.map_err(bpa_error)?;
    Ok(json_to_dynamic(&v))
}

fn worker_loop(jobs: Receiver<Job>) {
    while let Ok(job) = jobs.recv() {
        let events = job.events.clone();
        let (result, units) = execute_job(job);
        let _ = events.send(Event::Done { result, units });
    }
}

fn execute_job(job: Job) -> (Result<Value, BpaError>, u64) {
    let engine = engine_for(&job.profile, job.sizes);
    let mut scope = Scope::new();
    scope.push_constant("ctx", Ctx(Rc::new(job.context.clone())));
    scope.push_constant("sys", Sys);
    let mut this_dyn: Option<Dynamic> = None;
    if let Some((class, fields, body)) = &job.this {
        let mut values = BTreeMap::new();
        for (k, v) in body.as_object().into_iter().flatten() {
            let t = fields.get(k).cloned().unwrap_or(Ty::Any);
            match val_to_dynamic(&from_json(&t, v)) {
                Ok(d) => {
                    values.insert(k.clone(), d);
                }
                Err(e) => return (Err(map_error(&e)), 0),
            }
        }
        this_dyn = Some(Dynamic::from(Handle {
            class: class.clone(),
            fields: Rc::new(fields.clone()),
            values,
        }));
    }
    scope.push_constant("params", json_to_dynamic(&job.params));
    if let Some(oid) = &job.self_id {
        scope.push_constant("self_id", oid.clone());
    }
    let ast = match compile(&engine, &job.key, &job.source, &scope) {
        Ok(a) => a,
        Err(e) => return (Err(e), 0),
    };
    let Some(f) = ast.iter_functions().find(|f| f.name == job.fname) else {
        return (
            Err(BpaError::new(
                codes::FORBIDDEN_CAPABILITY,
                format!("function {} is not defined", job.fname),
            )),
            0,
        );
    };
    let params: Vec<String> = f.params.iter().map(|p| p.to_string()).collect();
    let call_args: Vec<Dynamic> = match &job.args {
        Value::Array(a) => {
            if a.len() > params.len() {
                return (
                    Err(BpaError::new("MODEL.INVALID_VALUE", "too many arguments")),
                    0,
                );
            }
            a.iter().map(json_to_dynamic).collect()
        }
        Value::Object(m) => params
            .iter()
            .map(|p| json_to_dynamic(m.get(p).unwrap_or(&Value::Null)))
            .collect(),
        _ => Vec::new(),
    };
    RUN.with(|r| {
        *r.borrow_mut() = Some(Run {
            events: job.events.clone(),
            budget: job.budget,
            cancel_after: job.cancel_after,
            ops: 0,
            start: Instant::now(),
            cpu_start: cpu_time(),
            sizes: job.sizes,
            memory_base: crate::memory::current().unwrap_or(0),
        });
    });
    let r = match this_dyn.as_mut() {
        Some(t) => engine.call_fn_with_options::<Dynamic>(
            rhai::CallFnOptions::new().bind_this_ptr(t),
            &mut scope,
            &ast,
            &job.fname,
            call_args,
        ),
        None => engine.call_fn::<Dynamic>(&mut scope, &ast, &job.fname, call_args),
    };
    let ops = RUN.with(|r| r.borrow_mut().take().map(|run| run.ops).unwrap_or(0));
    let result = match r {
        Ok(v) => dynamic_to_json(&v, 0),
        Err(e) => Err(map_error(&e)),
    };
    (result, ops)
}

// ---- script-visible types ----

/// The read-only execution context.
#[derive(Clone)]
struct Ctx(Rc<Value>);

/// The instruction gateway.
#[derive(Clone)]
struct Sys;

/// One instruction family of the gateway.
#[derive(Clone)]
struct Family(&'static str);

/// A typed object handle generated from the effective definition (DSN-DVM-612).
#[derive(Clone)]
struct Handle {
    class: String,
    fields: Rc<BTreeMap<String, Ty>>,
    values: BTreeMap<String, Dynamic>,
}

/// Money: an amount and an ISO 4217 code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Money {
    /// Amount.
    pub amount: Decimal,
    /// Currency.
    pub currency: String,
}

fn unknown_field(class: &str, field: &str) -> Box<EvalAltResult> {
    host_error(
        codes::UNKNOWN_FIELD,
        format!("{class} has no field {field}"),
    )
}

impl Handle {
    fn get(&self, field: &str) -> Result<Dynamic, Box<EvalAltResult>> {
        if !self.fields.contains_key(field) {
            return Err(unknown_field(&self.class, field));
        }
        Ok(self.values.get(field).cloned().unwrap_or(Dynamic::UNIT))
    }

    fn set(&mut self, field: &str, value: Dynamic) -> Result<(), Box<EvalAltResult>> {
        let Some(ty) = self.fields.get(field) else {
            return Err(unknown_field(&self.class, field));
        };
        let ok = value.is_unit()
            || match ty {
                Ty::Text | Ty::Date | Ty::Instant | Ty::Time | Ty::Duration => value.is_string(),
                Ty::Integer => value.is_int(),
                Ty::Decimal => value.is::<Decimal>() || value.is_int(),
                Ty::Money => value.is::<Money>(),
                Ty::Boolean => value.is_bool(),
                Ty::List(_) => value.is_array(),
                Ty::Obj(_) | Ty::Period => value.is_map(),
                Ty::Any | Ty::Null => true,
            };
        if !ok {
            return Err(host_error(
                codes::TYPE_ERROR,
                format!(
                    "{}.{field} expects {}, got {}",
                    self.class,
                    ubos_expr::type_name(ty),
                    value.type_name()
                ),
            ));
        }
        let value = if *ty == Ty::Decimal && value.is_int() {
            Dynamic::from(dec_of_int(value.as_int().unwrap_or_default()))
        } else {
            value
        };
        self.values.insert(field.to_string(), value);
        Ok(())
    }
}

fn dec_of_int(i: i64) -> Decimal {
    Decimal::parse(&i.to_string()).expect("integer text")
}

fn as_decimal(v: &Dynamic) -> Option<Decimal> {
    if let Some(d) = v.clone().try_cast::<Decimal>() {
        return Some(d);
    }
    v.as_int().ok().map(dec_of_int)
}

fn number(v: &Dynamic, what: &str) -> Result<Decimal, Box<EvalAltResult>> {
    as_decimal(v).ok_or_else(|| {
        host_error(
            codes::TYPE_ERROR,
            format!("{what} takes numbers, got {}", v.type_name()),
        )
    })
}

// ---- JSON ↔ Dynamic ----

/// A run-time value as a script value.
fn val_to_dynamic(v: &Val) -> Result<Dynamic, Box<EvalAltResult>> {
    Ok(match v {
        Val::Null => Dynamic::UNIT,
        Val::Bool(b) => Dynamic::from(*b),
        Val::Int(i) => match i64::try_from(*i) {
            Ok(x) => Dynamic::from(x),
            Err(_) => Dynamic::from(Decimal::parse(&i.to_string()).map_err(bpa_error)?),
        },
        Val::Dec(d) => Dynamic::from(d.clone()),
        Val::Money(a, c) => Dynamic::from(Money {
            amount: a.clone(),
            currency: c.clone(),
        }),
        Val::Text(s) => Dynamic::from(s.clone()),
        Val::Date(_) | Val::Instant(_) | Val::Duration(_) => {
            let j = ubos_expr::to_json(v).map_err(bpa_error)?;
            Dynamic::from(j.as_str().unwrap_or_default().to_string())
        }
        Val::Period(..) => json_to_dynamic(&ubos_expr::to_json(v).map_err(bpa_error)?),
        Val::List(items) => {
            let mut a = rhai::Array::with_capacity(items.len());
            for x in items {
                a.push(val_to_dynamic(x)?);
            }
            Dynamic::from(a)
        }
        Val::Map(m) => {
            let mut out = rhai::Map::new();
            for (k, x) in m {
                out.insert(k.as_str().into(), val_to_dynamic(x)?);
            }
            Dynamic::from(out)
        }
    })
}

/// Untyped JSON as a script value (instruction results, `params`, `ctx` members).
fn json_to_dynamic(v: &Value) -> Dynamic {
    match v {
        Value::Null => Dynamic::UNIT,
        Value::Bool(b) => Dynamic::from(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => Dynamic::from(i),
            None => Decimal::parse(&n.to_string())
                .map(Dynamic::from)
                .unwrap_or_else(|_| Dynamic::from(n.to_string())),
        },
        Value::String(s) => Dynamic::from(s.clone()),
        Value::Array(a) => Dynamic::from(a.iter().map(json_to_dynamic).collect::<rhai::Array>()),
        Value::Object(m) => Dynamic::from(
            m.iter()
                .map(|(k, x)| (k.as_str().into(), json_to_dynamic(x)))
                .collect::<rhai::Map>(),
        ),
    }
}

/// A script value as canonical JSON; handles and the gateway never leave the engine.
fn dynamic_to_json(v: &Dynamic, depth: usize) -> Result<Value, BpaError> {
    if depth > 64 {
        return Err(BpaError::new(codes::LIMIT_EXCEEDED, "result nesting"));
    }
    if v.is_unit() {
        return Ok(Value::Null);
    }
    if let Ok(b) = v.as_bool() {
        return Ok(Value::Bool(b));
    }
    if let Ok(i) = v.as_int() {
        return Ok(Value::from(i));
    }
    if let Some(d) = v.clone().try_cast::<Decimal>() {
        return Ok(Value::String(d.format(None)));
    }
    if let Some(m) = v.clone().try_cast::<Money>() {
        let scale = minor_unit(&m.currency).unwrap_or(2);
        return Ok(
            json!({"amount": m.amount.round(scale, Rounding::HalfEven).format(Some(scale)), "currency": m.currency}),
        );
    }
    if let Ok(c) = v.as_char() {
        return Ok(Value::String(c.to_string()));
    }
    if v.is_string() {
        return Ok(Value::String(v.clone().into_string().unwrap_or_default()));
    }
    if v.is_array() {
        return v
            .clone()
            .into_array()
            .unwrap_or_default()
            .iter()
            .map(|x| dynamic_to_json(x, depth + 1))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array);
    }
    if v.is_map() {
        let m = v.clone().cast::<rhai::Map>();
        let mut out = Map::new();
        for (k, x) in m.iter() {
            out.insert(k.to_string(), dynamic_to_json(x, depth + 1)?);
        }
        return Ok(Value::Object(out));
    }
    let name = if v.is::<Handle>() {
        "Handle"
    } else if v.is::<Sys>() || v.is::<Family>() {
        "Gateway"
    } else if v.is::<Ctx>() {
        "Context"
    } else {
        v.type_name()
    };
    Err(BpaError::new(
        codes::TYPE_ERROR,
        format!("value of type {name} cannot leave the script"),
    ))
}

// ---- the engine ----

fn build_engine(profile: &str, sizes: Sizes) -> Engine {
    // Fixed hashing seed: the engine's internal hashes do not depend on process
    // randomness (STD-CTX-010). Setting it twice is refused and harmless.
    let _ =
        rhai::config::hashing::set_hashing_seed(Some([0x5542_4f53, 0x4250_4100, 0x4c32, 0x0001]));
    let mut engine = Engine::new_raw();
    for module in [
        ArithmeticPackage::new().as_shared_module(),
        LogicPackage::new().as_shared_module(),
        BitFieldPackage::new().as_shared_module(),
        BasicStringPackage::new().as_shared_module(),
        MoreStringPackage::new().as_shared_module(),
        BasicIteratorPackage::new().as_shared_module(),
        BasicArrayPackage::new().as_shared_module(),
        BasicMapPackage::new().as_shared_module(),
    ] {
        engine.register_global_module(module);
    }
    for symbol in DISABLED_SYMBOLS {
        engine.disable_symbol(*symbol);
    }
    let limits = crate::isa::limits(profile);
    // The engine's string, array and map size checks walk the value on every change,
    // which makes element-wise growth quadratic (W-4); with memory accounted per
    // execution (W-2) they are redundant, so they stay on only without a memory probe.
    let (string_size, array_size, map_size) = if crate::memory::current().is_some() {
        (0, 0, 0)
    } else {
        (sizes.string_size, sizes.array_size, sizes.map_size)
    };
    engine
        .set_optimization_level(rhai::OptimizationLevel::None)
        .set_strict_variables(true)
        .set_max_operations(limits.map(|l| l.units).unwrap_or(1_000_000))
        .set_max_call_levels(limits.map(|l| l.depth as usize).unwrap_or(16))
        .set_max_expr_depths(sizes.expr_depth.0, sizes.expr_depth.1)
        .set_max_string_size(string_size)
        .set_max_array_size(array_size)
        .set_max_map_size(map_size)
        .set_max_variables(sizes.variables)
        .set_max_functions(sizes.functions);
    engine.on_progress(progress);

    // The functions that allocate in proportion to an integer argument (`pad`) are bound
    // by the profile sizes per call, so that no single operation can outrun the memory
    // check of the next metering point; growth across operations is bounded by the
    // memory accounting (W-2, W-4).
    let (max_array, max_string) = (sizes.array_size, sizes.string_size);
    engine.register_fn(
        "pad",
        move |a: &mut rhai::Array, len: i64, value: Dynamic| -> Result<(), Box<EvalAltResult>> {
            let len = usize::try_from(len).unwrap_or(0);
            if len > max_array {
                return Err(host_error(codes::LIMIT_EXCEEDED, "array size"));
            }
            while a.len() < len {
                a.push(value.clone());
            }
            Ok(())
        },
    );
    engine.register_fn(
        "pad",
        move |s: &mut rhai::ImmutableString,
              len: i64,
              ch: char|
              -> Result<(), Box<EvalAltResult>> {
            let len = usize::try_from(len).unwrap_or(0);
            if len > max_string {
                return Err(host_error(codes::LIMIT_EXCEEDED, "string size"));
            }
            let n = s.chars().count();
            if n < len {
                let mut out = s.to_string();
                out.extend(std::iter::repeat_n(ch, len - n));
                *s = out.into();
            }
            Ok(())
        },
    );
    engine.register_fn(
        "pad",
        move |s: &mut rhai::ImmutableString,
              len: i64,
              fill: &str|
              -> Result<(), Box<EvalAltResult>> {
            let len = usize::try_from(len).unwrap_or(0);
            if len > max_string || fill.is_empty() {
                return Err(host_error(codes::LIMIT_EXCEEDED, "string size"));
            }
            let mut out = s.to_string();
            let mut n = out.chars().count();
            while n < len {
                for c in fill.chars() {
                    if n >= len {
                        break;
                    }
                    out.push(c);
                    n += 1;
                }
            }
            *s = out.into();
            Ok(())
        },
    );

    for name in FLOAT_FUNCTIONS {
        let f0 = move || -> Result<Dynamic, Box<EvalAltResult>> {
            Err(host_error(codes::FLOAT_TO_DECIMAL, *name))
        };
        let f1 = move |_: Dynamic| -> Result<Dynamic, Box<EvalAltResult>> {
            Err(host_error(codes::FLOAT_TO_DECIMAL, *name))
        };
        let f2 = move |_: Dynamic, _: Dynamic| -> Result<Dynamic, Box<EvalAltResult>> {
            Err(host_error(codes::FLOAT_TO_DECIMAL, *name))
        };
        engine.register_fn(*name, f0);
        engine.register_fn(*name, f1);
        engine.register_fn(*name, f2);
    }

    // Kernel decimal (W-5, STD-FND-022).
    engine.register_type_with_name::<Decimal>("Decimal");
    engine.register_fn("dec", |s: &str| {
        Decimal::parse(s).map_err(|e| host_error(codes::TYPE_ERROR, format!("decimal {s:?}: {e}")))
    });
    engine.register_fn("dec", dec_of_int);
    engine.register_fn("to_string", |d: &mut Decimal| d.format(None));
    engine.register_fn("to_debug", |d: &mut Decimal| d.format(None));
    engine.register_fn("is_zero", |d: &mut Decimal| d.is_zero());
    engine.register_fn(
        "abs",
        |d: Decimal| {
            if d < dec_of_int(0) { d.neg() } else { d }
        },
    );
    engine.register_fn("-", |d: Decimal| d.neg());
    type Arith = fn(&Decimal, &Decimal) -> Decimal;
    for (op, f) in [
        ("+", (|a: &Decimal, b: &Decimal| a.add(b)) as Arith),
        ("-", (|a: &Decimal, b: &Decimal| a.sub(b)) as Arith),
        ("*", (|a: &Decimal, b: &Decimal| a.mul(b)) as Arith),
    ] {
        engine.register_fn(op, move |a: Decimal, b: Decimal| f(&a, &b));
        engine.register_fn(op, move |a: Decimal, b: i64| f(&a, &dec_of_int(b)));
        engine.register_fn(op, move |a: i64, b: Decimal| f(&dec_of_int(a), &b));
    }
    fn no_div() -> Box<EvalAltResult> {
        host_error(
            codes::TYPE_ERROR,
            "division needs a scale; use div(a, b, scale, rounding)",
        )
    }
    engine.register_fn(
        "/",
        |_: Decimal, _: Decimal| -> Result<Decimal, Box<EvalAltResult>> { Err(no_div()) },
    );
    engine.register_fn(
        "/",
        |_: Decimal, _: i64| -> Result<Decimal, Box<EvalAltResult>> { Err(no_div()) },
    );
    engine.register_fn(
        "/",
        |_: i64, _: Decimal| -> Result<Decimal, Box<EvalAltResult>> { Err(no_div()) },
    );
    engine.register_fn(
        "div",
        |a: Dynamic, b: Dynamic, scale: i64, mode: &str| -> Result<Decimal, Box<EvalAltResult>> {
            let (x, y) = (number(&a, "div")?, number(&b, "div")?);
            if y.is_zero() {
                return Err(host_error(codes::DIVISION_BY_ZERO, "division by zero"));
            }
            let scale = u32::try_from(scale)
                .ok()
                .filter(|s| *s <= 100)
                .ok_or_else(|| host_error(codes::TYPE_ERROR, "the scale is 0 to 100"))?;
            x.div(&y, scale, Rounding::parse(mode).map_err(bpa_error)?)
                .map_err(bpa_error)
        },
    );
    engine.register_fn(
        "round",
        |a: Dynamic, scale: i64, mode: &str| -> Result<Decimal, Box<EvalAltResult>> {
            let scale = u32::try_from(scale)
                .ok()
                .filter(|s| *s <= 100)
                .ok_or_else(|| host_error(codes::TYPE_ERROR, "the scale is 0 to 100"))?;
            Ok(number(&a, "round")?.round(scale, Rounding::parse(mode).map_err(bpa_error)?))
        },
    );
    type Cmp = fn(std::cmp::Ordering) -> bool;
    for (op, f) in [
        ("==", (|o: std::cmp::Ordering| o.is_eq()) as Cmp),
        ("!=", (|o: std::cmp::Ordering| o.is_ne()) as Cmp),
        ("<", (|o: std::cmp::Ordering| o.is_lt()) as Cmp),
        ("<=", (|o: std::cmp::Ordering| o.is_le()) as Cmp),
        (">", (|o: std::cmp::Ordering| o.is_gt()) as Cmp),
        (">=", (|o: std::cmp::Ordering| o.is_ge()) as Cmp),
    ] {
        engine.register_fn(op, move |a: Decimal, b: Decimal| f(a.cmp(&b)));
        engine.register_fn(op, move |a: Decimal, b: i64| f(a.cmp(&dec_of_int(b))));
        engine.register_fn(op, move |a: i64, b: Decimal| f(dec_of_int(a).cmp(&b)));
        engine.register_fn(
            op,
            move |a: Money, b: Money| -> Result<bool, Box<EvalAltResult>> {
                if a.currency != b.currency {
                    return Err(host_error("MODEL.CURRENCY_MISMATCH", "currencies differ"));
                }
                Ok(f(a.amount.cmp(&b.amount)))
            },
        );
    }

    // Money.
    engine.register_type_with_name::<Money>("Money");
    engine.register_fn(
        "money",
        |amount: Dynamic, currency: &str| -> Result<Money, Box<EvalAltResult>> {
            if minor_unit(currency).is_none() {
                return Err(host_error(
                    "MODEL.INVALID_VALUE",
                    "unknown ISO 4217 currency",
                ));
            }
            let amount = match amount.clone().into_string() {
                Ok(s) => Decimal::parse(&s).map_err(bpa_error)?,
                Err(_) => number(&amount, "money")?,
            };
            Ok(Money {
                amount,
                currency: currency.to_string(),
            })
        },
    );
    engine.register_fn("amount", |m: Money| m.amount);
    engine.register_fn("currency", |m: Money| m.currency);
    engine.register_fn("to_string", |m: &mut Money| {
        format!("{} {}", m.amount.format(None), m.currency)
    });
    engine.register_fn("-", |m: Money| Money {
        amount: m.amount.neg(),
        currency: m.currency,
    });
    for (op, plus) in [("+", true), ("-", false)] {
        engine.register_fn(
            op,
            move |a: Money, b: Money| -> Result<Money, Box<EvalAltResult>> {
                if a.currency != b.currency {
                    return Err(host_error("MODEL.CURRENCY_MISMATCH", "currencies differ"));
                }
                Ok(Money {
                    amount: if plus {
                        a.amount.add(&b.amount)
                    } else {
                        a.amount.sub(&b.amount)
                    },
                    currency: a.currency,
                })
            },
        );
    }
    engine.register_fn(
        "*",
        |m: Money, n: Dynamic| -> Result<Money, Box<EvalAltResult>> {
            Ok(Money {
                amount: m.amount.mul(&number(&n, "*")?),
                currency: m.currency,
            })
        },
    );
    engine.register_fn(
        "*",
        |n: Dynamic, m: Money| -> Result<Money, Box<EvalAltResult>> {
            Ok(Money {
                amount: m.amount.mul(&number(&n, "*")?),
                currency: m.currency,
            })
        },
    );

    // Read-only context (STD-CTX-001, DSN-DVM-603).
    engine.register_type_with_name::<Ctx>("Context");
    engine.register_indexer_get(
        |c: &mut Ctx, k: &str| -> Result<Dynamic, Box<EvalAltResult>> {
            match c.0.get(k) {
                Some(v) => Ok(json_to_dynamic(v)),
                None => Err(unknown_field("ctx", k)),
            }
        },
    );
    engine.register_indexer_set(
        |_: &mut Ctx, k: &str, _: Dynamic| -> Result<(), Box<EvalAltResult>> {
            Err(host_error(codes::CONTEXT_IMMUTABLE, format!("ctx.{k}")))
        },
    );

    // The gateway: `sys.<family>.<op>(#{…})`, `sys.uuid()`, `sys.random_bytes(n)`.
    engine.register_type_with_name::<Sys>("Gateway");
    engine.register_type_with_name::<Family>("Family");
    engine.register_fn("uuid", |_: Sys| gateway("sys.uuid", Dynamic::UNIT));
    engine.register_fn("random_bytes", |_: Sys, n: i64| {
        gateway("sys.random_bytes", json_to_dynamic(&json!({"n": n})))
    });
    engine.register_indexer_get(
        |_: &mut Sys, k: &str| -> Result<Dynamic, Box<EvalAltResult>> {
            match FAMILIES.iter().find(|f| **f == k) {
                Some(f) => Ok(Dynamic::from(Family(f))),
                None => Err(host_error(codes::FORBIDDEN_CAPABILITY, format!("sys.{k}"))),
            }
        },
    );
    engine.register_indexer_set(
        |_: &mut Sys, k: &str, _: Dynamic| -> Result<(), Box<EvalAltResult>> {
            Err(host_error(codes::FORBIDDEN_CAPABILITY, format!("sys.{k}")))
        },
    );
    let mut ops: Vec<&'static str> = Vec::new();
    for name in crate::isa::instruction_names() {
        if let Some((_, op)) = name.split_once('.')
            && !ops.contains(&op)
        {
            ops.push(op);
        }
    }
    ops.push("observe");
    for op in ops {
        // Operation names that are reserved words of the script language (`super`,
        // `async`) are also bound with a trailing underscore.
        for name in [op.to_string(), format!("{op}_")] {
            engine.register_fn(&name, move |f: Family, args: rhai::Map| {
                gateway(&format!("{}.{op}", f.0), Dynamic::from(args))
            });
            engine.register_fn(&name, move |f: Family| {
                gateway(&format!("{}.{op}", f.0), Dynamic::UNIT)
            });
        }
    }
    engine.register_indexer_get(
        |f: &mut Family, k: &str| -> Result<Dynamic, Box<EvalAltResult>> {
            Err(host_error(
                codes::FORBIDDEN_CAPABILITY,
                format!("sys.{}.{k}", f.0),
            ))
        },
    );
    engine.register_indexer_set(
        |f: &mut Family, k: &str, _: Dynamic| -> Result<(), Box<EvalAltResult>> {
            Err(host_error(
                codes::FORBIDDEN_CAPABILITY,
                format!("sys.{}.{k}", f.0),
            ))
        },
    );

    // Typed handles.
    engine.register_type_with_name::<Handle>("Handle");
    engine.register_indexer_get(|h: &mut Handle, k: &str| h.get(k));
    engine.register_indexer_set(|h: &mut Handle, k: &str, v: Dynamic| h.set(k, v));
    engine
}

fn engine_for(profile: &str, sizes: Sizes) -> Rc<Engine> {
    ENGINES.with(|e| {
        e.borrow_mut()
            .entry(profile.to_string())
            .or_insert_with(|| Rc::new(build_engine(profile, sizes)))
            .clone()
    })
}

/// Engine failures mapped to registered codes (UBS-STD-17 §2).
fn map_error(e: &EvalAltResult) -> BpaError {
    let detail = e.to_string();
    let code = match e {
        EvalAltResult::ErrorSystem(_, inner) => match inner.downcast_ref::<HostError>() {
            Some(h) => return h.0.clone(),
            None => codes::TYPE_ERROR,
        },
        EvalAltResult::ErrorTerminated(token, _) => {
            if token.clone().into_string().ok().as_deref() == Some(codes::CANCELLED) {
                codes::CANCELLED
            } else {
                codes::LIMIT_EXCEEDED
            }
        }
        EvalAltResult::ErrorInFunctionCall(_, _, inner, _) => {
            let mut e = map_error(inner);
            if e.details.is_none() {
                e.message = detail;
            }
            return e;
        }
        EvalAltResult::ErrorTooManyOperations(_)
        | EvalAltResult::ErrorTooManyVariables(_)
        | EvalAltResult::ErrorTooManyModules(_)
        | EvalAltResult::ErrorStackOverflow(_)
        | EvalAltResult::ErrorDataTooLarge(..) => codes::LIMIT_EXCEEDED,
        EvalAltResult::ErrorParsing(p, _) => match p {
            ParseErrorType::ExprTooDeep
            | ParseErrorType::TooManyFunctions
            | ParseErrorType::LiteralTooLarge(..) => codes::SCRIPT_TOO_COMPLEX,
            // a disabled keyword after a dot (`.call`, `.eval`) reads as a missing property
            ParseErrorType::PropertyExpected
            | ParseErrorType::Reserved(_)
            | ParseErrorType::VariableUndefined(_)
            | ParseErrorType::ModuleUndefined(_)
            | ParseErrorType::ForbiddenVariable(_) => codes::FORBIDDEN_CAPABILITY,
            ParseErrorType::BadInput(l) if l.to_string().contains("is disabled") => {
                codes::FORBIDDEN_CAPABILITY
            }
            ParseErrorType::BadInput(l)
                if l.to_string().to_lowercase().contains("float")
                    || l.to_string().to_lowercase().contains("decimal") =>
            {
                codes::FLOAT_TO_DECIMAL
            }
            ParseErrorType::AssignmentToConstant(_) => codes::CONTEXT_IMMUTABLE,
            _ => codes::TYPE_ERROR,
        },
        EvalAltResult::ErrorFunctionNotFound(sig, _)
            if FLOAT_FUNCTIONS.contains(&function_name(sig)) =>
        {
            codes::FLOAT_TO_DECIMAL
        }
        EvalAltResult::ErrorFunctionNotFound(sig, _)
            if HOST_FUNCTIONS.contains(&function_name(sig)) =>
        {
            codes::TYPE_ERROR
        }
        EvalAltResult::ErrorFunctionNotFound(..)
        | EvalAltResult::ErrorVariableNotFound(..)
        | EvalAltResult::ErrorForbiddenVariable(..)
        | EvalAltResult::ErrorModuleNotFound(..)
        | EvalAltResult::ErrorInModule(..)
        | EvalAltResult::ErrorCustomSyntax(..) => codes::FORBIDDEN_CAPABILITY,
        EvalAltResult::ErrorPropertyNotFound(..)
        | EvalAltResult::ErrorIndexNotFound(..)
        | EvalAltResult::ErrorDotExpr(..) => codes::UNKNOWN_FIELD,
        EvalAltResult::ErrorAssignmentToConstant(..)
        | EvalAltResult::ErrorNonPureMethodCallOnConstant(..) => codes::CONTEXT_IMMUTABLE,
        EvalAltResult::ErrorArithmetic(msg, _) if msg.to_lowercase().contains("zero") => {
            codes::DIVISION_BY_ZERO
        }
        _ => codes::TYPE_ERROR,
    };
    BpaError::new(code, detail)
}

/// True when the source holds a floating-point literal (`1.5`, `1e9`, `2.5e-3`) outside
/// strings; the engine has no float type and would read `1.5` as a property access.
fn float_literal(source: &str) -> bool {
    let mut in_str: Option<char> = None;
    let b: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if let Some(q) = in_str {
            if c == '\\' {
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' {
            in_str = Some(c);
            i += 1;
            continue;
        }
        if c.is_ascii_digit() && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == '_'))
        {
            let mut j = i;
            while j < b.len() && (b[j].is_ascii_digit() || b[j] == '_') {
                j += 1;
            }
            if j + 1 < b.len() && b[j] == '.' && b[j + 1].is_ascii_digit() {
                return true;
            }
            if j < b.len()
                && (b[j] == 'e' || b[j] == 'E')
                && j + 1 < b.len()
                && (b[j + 1].is_ascii_digit()
                    || ((b[j + 1] == '+' || b[j + 1] == '-')
                        && j + 2 < b.len()
                        && b[j + 2].is_ascii_digit()))
            {
                return true;
            }
            i = j.max(i + 1);
            continue;
        }
        i += 1;
    }
    false
}

fn function_name(signature: &str) -> &str {
    signature.split([' ', '(']).next().unwrap_or("")
}

/// Compile a script, or return the AST cached under the asset version hash.
fn compile(engine: &Engine, key: &str, source: &str, scope: &Scope) -> Result<Rc<AST>, BpaError> {
    if let Some(ast) = ASTS.with(|a| a.borrow().get(key).cloned()) {
        return Ok(ast);
    }
    if float_literal(source) {
        return Err(BpaError::new(
            codes::FLOAT_TO_DECIMAL,
            "floating-point literals are not available; use dec(\"…\")",
        ));
    }
    let ast = engine
        .compile_with_scope(scope, source)
        .map_err(|e| map_error(&EvalAltResult::ErrorParsing(*e.0, e.1)))?;
    let ast = Rc::new(ast);
    ASTS.with(|a| {
        let mut a = a.borrow_mut();
        if a.len() > 4096 {
            a.clear();
        }
        a.insert(key.to_string(), ast.clone());
    });
    Ok(ast)
}

/// Run an exported function of an L2 asset in the current frame of the run: the
/// in-process host binding of STD-ISA-007 for scripts.
pub fn run_function(
    core: &mut Core,
    host: &mut dyn Host,
    mut frame: Frame,
    args: &Value,
) -> Result<Value, BpaError> {
    if (core.frames.len() as u64).saturating_add(1) > core.limits.depth {
        return Err(BpaError::new(codes::LIMIT_EXCEEDED, "nesting depth"));
    }
    let sizes = Sizes::profile(&core.profile)
        .ok_or_else(|| BpaError::new("MODEL.INVALID_VALUE", "unknown profile"))?;
    let fname = frame.fname.clone().unwrap_or_default();
    let exported = frame.asset["exports"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|e| {
            e.as_str() == Some(fname.as_str())
                || e.get("name").and_then(Value::as_str) == Some(fname.as_str())
        });
    if !exported {
        return Err(BpaError::new(
            codes::FORBIDDEN_CAPABILITY,
            format!("function {fname} is not exported"),
        ));
    }
    let source = frame.asset["source"]
        .as_str()
        .ok_or_else(|| BpaError::new(codes::TYPE_ERROR, "an L2 source is text"))?
        .to_string();
    if source.len() > sizes.source_bytes {
        return Err(BpaError::new(
            codes::SCRIPT_TOO_COMPLEX,
            format!("source exceeds {} bytes", sizes.source_bytes),
        ));
    }
    let this = match (&frame.cls, &frame.this) {
        (Some(c), Some(body)) => match host.class_type(c)? {
            Ty::Obj(fields) => Some((c.clone(), fields, body.clone())),
            _ => None,
        },
        _ => None,
    };
    let given: Map<String, Value> = match args {
        Value::Object(m) => m.clone(),
        _ => Map::new(),
    };
    frame.params = given
        .iter()
        .map(|(k, v)| (k.clone(), from_json(&Ty::Any, v)))
        .collect();
    let key = format!(
        "{}:{}",
        core.profile,
        frame.vh.clone().unwrap_or_else(|| {
            ubos_expr::model::types::content_id("ubos.chunk.v1", &json!(source)).unwrap_or_default()
        })
    );
    let depth = core.frames.len();
    let (events_tx, events_rx) = channel::<Event>();
    let job = Job {
        profile: core.profile.clone(),
        sizes,
        key,
        source,
        fname,
        args: args.clone(),
        context: core.context.clone(),
        this,
        params: Value::Object(given),
        self_id: frame.oid.clone(),
        budget: core.limits.units.saturating_sub(core.units),
        cancel_after: core.cancel_after.map(|c| c.saturating_sub(core.units)),
        depth,
        events: events_tx,
    };
    core.frames.push(frame);
    let r = run_job(core, host, job, events_rx);
    core.frames.pop();
    r
}
