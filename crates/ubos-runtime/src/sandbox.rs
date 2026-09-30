//! Hardened Rhai engine, metering, gateway and error mapping.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rhai::packages::{
    ArithmeticPackage, BasicArrayPackage, BasicFnPackage, BasicIteratorPackage, BasicMapPackage,
    BasicStringPackage, BitFieldPackage, LogicPackage, MoreStringPackage, Package,
};
use rhai::{AST, Dynamic, Engine, EvalAltResult, ParseErrorType, Scope};
use rust_decimal::Decimal;
use serde_json::{Map, Value};

/// `LOGIC.*` codes produced by the host (UBS-STD-17 §2).
pub mod codes {
    /// Budget exhausted (STD-CTX-005).
    pub const LIMIT_EXCEEDED: &str = "LOGIC.LIMIT_EXCEEDED";
    /// Cancellation (STD-CTX-005).
    pub const CANCELLED: &str = "LOGIC.CANCELLED";
    /// Undeclared or disallowed instruction.
    pub const FORBIDDEN_CAPABILITY: &str = "LOGIC.FORBIDDEN_CAPABILITY";
    /// Attempt to modify the context.
    pub const CONTEXT_IMMUTABLE: &str = "LOGIC.CONTEXT_IMMUTABLE";
    /// Expression or script type error.
    pub const TYPE_ERROR: &str = "LOGIC.TYPE_ERROR";
    /// Script accesses an unknown field.
    pub const UNKNOWN_FIELD: &str = "LOGIC.UNKNOWN_FIELD";
    /// Division by zero.
    pub const DIVISION_BY_ZERO: &str = "LOGIC.DIVISION_BY_ZERO";
    /// Float assigned to a decimal.
    pub const FLOAT_TO_DECIMAL: &str = "LOGIC.FLOAT_TO_DECIMAL";
    /// Size or AST limits.
    pub const SCRIPT_TOO_COMPLEX: &str = "LOGIC.SCRIPT_TOO_COMPLEX";
}

/// Functions whose only purpose is floating point; they raise `LOGIC.FLOAT_TO_DECIMAL`.
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

/// Host functions: a call with wrong argument types is a type error, not a
/// forbidden capability.
const HOST_FUNCTIONS: &[&str] = &[
    "uuid",
    "random_bytes",
    "parse_decimal",
    "to_decimal",
    "round_dp",
];

/// Reserved words disabled in the L2 profile (STD-CTX-020).
const DISABLED_SYMBOLS: &[&str] = &["eval", "print", "debug"];

/// Execution limits of one profile (STD-CTX-004) plus the engine sizes that
/// STD-CTX-020 derives from it.
#[derive(Clone, Debug)]
pub struct Limits {
    /// Profile name.
    pub profile: &'static str,
    /// Operation units; one Rhai operation is one unit.
    pub operations: u64,
    /// CPU time in milliseconds (enforced through elapsed time in the spike).
    pub cpu_ms: u64,
    /// Memory in bytes, enforced when the host supplies a memory probe.
    pub memory: usize,
    /// Wall time in milliseconds.
    pub wall_ms: u64,
    /// Gateway instruction calls.
    pub instruction_calls: u64,
    /// Nesting depth, mapped to the Rhai call depth.
    pub nesting_depth: usize,
    /// Maximum string length in bytes.
    pub string_size: usize,
    /// Maximum array length (nested arrays count their elements).
    pub array_size: usize,
    /// Maximum map size (nested maps count their members).
    pub map_size: usize,
    /// Maximum live variables.
    pub variables: usize,
    /// Maximum script functions per asset.
    pub functions: usize,
    /// Maximum source size in bytes.
    pub source_bytes: usize,
    /// Maximum expression depth at top level and inside functions.
    pub expr_depth: (usize, usize),
}

impl Limits {
    /// Default limits of a profile of STD-CTX-004; `None` for an unknown name.
    ///
    /// Sizes not listed in STD-CTX-004 (string, array, map, variables, functions,
    /// source, expression depth) are the spike's proposal (sandbox report §4).
    pub fn profile(name: &str) -> Option<Limits> {
        const MB: usize = 1 << 20;
        let (profile, operations, cpu_ms, memory, wall_ms, calls, depth, scale) = match name {
            "strict" => ("strict", 1_000_000, 500, 64 * MB, 2_000, 1_000, 16, 1),
            "standard" => (
                "standard",
                10_000_000,
                3_000,
                512 * MB,
                10_000,
                10_000,
                32,
                8,
            ),
            "batch" => (
                "batch",
                500_000_000,
                600_000,
                4_096 * MB,
                1_800_000,
                5_000_000,
                32,
                64,
            ),
            "simulation" => (
                "simulation",
                1_000_000_000,
                3_600_000,
                8_192 * MB,
                7_200_000,
                50_000_000,
                32,
                128,
            ),
            "agent" => ("agent", 20_000_000, 10_000, 512 * MB, 60_000, 20_000, 16, 8),
            _ => return None,
        };
        Some(Limits {
            profile,
            operations,
            cpu_ms,
            memory,
            wall_ms,
            instruction_calls: calls,
            nesting_depth: depth,
            string_size: 1_048_576 * scale,
            array_size: 65_536 * scale,
            map_size: 65_536 * scale,
            variables: 1_024,
            functions: 256,
            source_bytes: 65_536,
            expr_depth: (64, 32),
        })
    }

    /// Elapsed-time budget: the smaller of CPU and wall time.
    fn time_budget(&self) -> Duration {
        Duration::from_millis(self.cpu_ms.min(self.wall_ms))
    }
}

/// Type of a field of the class that typed handles are generated from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldType {
    /// `string`
    String,
    /// `integer`
    Integer,
    /// `decimal`
    Decimal,
    /// `boolean`
    Boolean,
}

/// Context of one execution (the read-only `ctx` of STD-CTX-001, reduced).
#[derive(Clone, Debug)]
pub struct Context {
    /// `ctx.now`, fixed for the execution.
    pub now: String,
    /// `ctx.tenant`.
    pub tenant: String,
    /// Seed of the journaled entropy source; replay supplies the same seed.
    pub seed: u64,
}

impl Default for Context {
    fn default() -> Self {
        Context {
            now: "2026-01-01T00:00:00.000000Z".into(),
            tenant: "t1".into(),
            seed: 20_260_930,
        }
    }
}

/// A failed execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// Registered error code.
    pub code: &'static str,
    /// Engine message, for the report only.
    pub detail: String,
}

/// Result of one execution.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    /// Script result as JSON, or the failure.
    pub result: Result<Value, Failure>,
    /// Operation units consumed.
    pub operations: u64,
    /// Gateway calls made.
    pub instruction_calls: u64,
    /// Journal of non-deterministic inputs handed to the script.
    pub journal: Vec<String>,
}

/// Error raised by host functions; carried in `ErrorSystem` so that scripts cannot
/// catch it.
#[derive(Debug)]
struct HostError(&'static str, String);

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.0, self.1)
    }
}

impl std::error::Error for HostError {}

fn host_error(code: &'static str, detail: impl Into<String>) -> Box<EvalAltResult> {
    let detail = detail.into();
    Box::new(EvalAltResult::ErrorSystem(
        code.into(),
        Box::new(HostError(code, detail)),
    ))
}

/// Metering state shared by the progress callback and the gateway.
#[derive(Default)]
struct Meter {
    start: Cell<Option<Instant>>,
    operations: Cell<u64>,
    calls: Cell<u64>,
    journal: RefCell<Vec<String>>,
    rng: Cell<u64>,
    probe: Cell<Option<fn() -> usize>>,
    baseline: Cell<usize>,
}

impl Meter {
    fn next_u64(&self) -> u64 {
        // SplitMix64: deterministic for a given seed (STD-CTX-010 journaled entropy).
        let mut z = self.rng.get().wrapping_add(0x9E37_79B9_7F4A_7C15);
        self.rng.set(z);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// The read-only execution context seen by scripts.
#[derive(Clone)]
struct Ctx {
    now: String,
    tenant: String,
}

/// The `sys` gateway: the only source of entropy.
#[derive(Clone)]
struct Sys {
    meter: Rc<Meter>,
    limit: u64,
}

impl Sys {
    fn call(&self, name: &str) -> Result<(), Box<EvalAltResult>> {
        let n = self.meter.calls.get() + 1;
        self.meter.calls.set(n);
        if n > self.limit {
            return Err(host_error(
                codes::LIMIT_EXCEEDED,
                format!("instruction calls exceed {} at {name}", self.limit),
            ));
        }
        Ok(())
    }

    fn bytes(&self, n: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(n + 8);
        while out.len() < n {
            out.extend_from_slice(&self.meter.next_u64().to_be_bytes());
        }
        out.truncate(n);
        out
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A typed object handle generated from a class definition (DSN-DVM-612).
#[derive(Clone)]
struct Handle {
    class: Rc<Class>,
    values: BTreeMap<String, Dynamic>,
}

struct Class {
    name: String,
    fields: BTreeMap<String, FieldType>,
}

impl Handle {
    fn get(&self, field: &str) -> Result<Dynamic, Box<EvalAltResult>> {
        if !self.class.fields.contains_key(field) {
            return Err(unknown_field(&self.class.name, field));
        }
        Ok(self.values.get(field).cloned().unwrap_or(Dynamic::UNIT))
    }

    fn set(&mut self, field: &str, value: Dynamic) -> Result<(), Box<EvalAltResult>> {
        let Some(ty) = self.class.fields.get(field) else {
            return Err(unknown_field(&self.class.name, field));
        };
        let value = match ty {
            FieldType::String if value.is_string() => value,
            FieldType::Integer if value.is_int() => value,
            FieldType::Boolean if value.is_bool() => value,
            FieldType::Decimal if value.is::<Decimal>() => value,
            FieldType::Decimal if value.is_int() => {
                Dynamic::from(Decimal::from(value.as_int().unwrap_or_default()))
            }
            _ => {
                return Err(host_error(
                    codes::TYPE_ERROR,
                    format!(
                        "{}.{field} expects {ty:?}, got {}",
                        self.class.name,
                        value.type_name()
                    ),
                ));
            }
        };
        self.values.insert(field.to_string(), value);
        Ok(())
    }
}

fn unknown_field(class: &str, field: &str) -> Box<EvalAltResult> {
    host_error(
        codes::UNKNOWN_FIELD,
        format!("{class} has no field {field}"),
    )
}

/// A hardened Rhai engine for one profile.
pub struct Sandbox {
    engine: Engine,
    limits: Limits,
    meter: Rc<Meter>,
    cancel: Arc<AtomicBool>,
    class: Rc<Class>,
    cache: RefCell<BTreeMap<String, AST>>,
}

impl Sandbox {
    /// Builds a sandbox with the given limits and the class that the `rec` handle
    /// is generated from.
    pub fn new(limits: Limits, class: &str, fields: &[(&str, FieldType)]) -> Sandbox {
        // Fixed hashing seed: the engine's internal hashes do not depend on process
        // randomness (STD-CTX-010). Setting it twice is refused and harmless.
        let _ = rhai::config::hashing::set_hashing_seed(Some([
            0x5542_4f53,
            0x4250_4100,
            0x4c32,
            0x0001,
        ]));
        let meter = Rc::new(Meter::default());
        let cancel = Arc::new(AtomicBool::new(false));
        let class = Rc::new(Class {
            name: class.to_string(),
            fields: fields.iter().map(|(n, t)| (n.to_string(), *t)).collect(),
        });

        // `new_raw` starts without packages, `print`/`debug` sinks or module
        // resolvers; only the pure packages below are added. The language core
        // package is left out: it carries `sleep`, `exit`, `parse_json` and
        // reflection over function metadata.
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
            BasicFnPackage::new().as_shared_module(),
        ] {
            engine.register_global_module(module);
        }
        for symbol in DISABLED_SYMBOLS {
            engine.disable_symbol(*symbol);
        }
        engine
            .set_strict_variables(true)
            .set_max_operations(limits.operations)
            .set_max_call_levels(limits.nesting_depth)
            .set_max_expr_depths(limits.expr_depth.0, limits.expr_depth.1)
            .set_max_string_size(limits.string_size)
            .set_max_array_size(limits.array_size)
            .set_max_map_size(limits.map_size)
            .set_max_variables(limits.variables)
            .set_max_functions(limits.functions);

        // Metering (STD-CTX-005): Rhai calls back at every operation. Cancellation
        // and memory are checked at every unit, elapsed time every 8 units (one unit
        // can be a native call on a full-size value, so a coarser step overshoots
        // the 10 ms bound).
        let m = meter.clone();
        let c = cancel.clone();
        let budget = limits.time_budget();
        let memory = limits.memory;
        engine.on_progress(move |ops| {
            m.operations.set(ops);
            if c.load(Ordering::Relaxed) {
                return Some(Dynamic::from(codes::CANCELLED));
            }
            if let Some(probe) = m.probe.get()
                && probe().saturating_sub(m.baseline.get()) > memory
            {
                return Some(Dynamic::from(codes::LIMIT_EXCEEDED));
            }
            if ops % 8 == 0
                && let Some(start) = m.start.get()
                && start.elapsed() > budget
            {
                return Some(Dynamic::from(codes::LIMIT_EXCEEDED));
            }
            None
        });

        // Exact decimals only: construction from text or integers.
        engine.register_fn("parse_decimal", |s: &str| {
            s.parse::<Decimal>()
                .map_err(|e| host_error(codes::TYPE_ERROR, format!("decimal {s:?}: {e}")))
        });
        engine.register_fn("to_decimal", |i: i64| Decimal::from(i));
        engine.register_fn("round_dp", |d: Decimal, digits: i64| {
            let digits = u32::try_from(digits)
                .ok()
                .filter(|d| *d <= 28)
                .ok_or_else(|| host_error(codes::TYPE_ERROR, "digits out of range"))?;
            Ok::<_, Box<EvalAltResult>>(d.round_dp(digits))
        });
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

        // Read-only context.
        engine.register_type_with_name::<Ctx>("Context");
        engine.register_get("now", |c: &mut Ctx| c.now.clone());
        engine.register_get("tenant", |c: &mut Ctx| c.tenant.clone());
        for field in ["now", "tenant"] {
            engine.register_set(field, move |_: &mut Ctx, _: Dynamic| {
                Err::<(), _>(host_error(codes::CONTEXT_IMMUTABLE, field))
            });
        }
        engine.register_indexer_get(|_: &mut Ctx, k: &str| {
            Err::<Dynamic, _>(unknown_field("Context", k))
        });
        engine.register_indexer_set(|_: &mut Ctx, k: &str, _: Dynamic| {
            Err::<(), _>(host_error(codes::CONTEXT_IMMUTABLE, k))
        });

        // Gateway: journaled entropy only.
        engine.register_type_with_name::<Sys>("Gateway");
        engine.register_fn("uuid", |s: &mut Sys| {
            s.call("sys.uuid")?;
            let mut b = s.bytes(16);
            b[6] = (b[6] & 0x0f) | 0x40;
            b[8] = (b[8] & 0x3f) | 0x80;
            let h = hex(&b);
            let id = format!(
                "{}-{}-{}-{}-{}",
                &h[0..8],
                &h[8..12],
                &h[12..16],
                &h[16..20],
                &h[20..32]
            );
            s.meter.journal.borrow_mut().push(format!("sys.uuid {id}"));
            Ok::<_, Box<EvalAltResult>>(id)
        });
        engine.register_fn("random_bytes", |s: &mut Sys, n: i64| {
            s.call("sys.random_bytes")?;
            let n = usize::try_from(n)
                .ok()
                .filter(|n| (1..=1024).contains(n))
                .ok_or_else(|| host_error(codes::TYPE_ERROR, "random_bytes takes 1..1024"))?;
            let h = hex(&s.bytes(n));
            s.meter
                .journal
                .borrow_mut()
                .push(format!("sys.random_bytes {h}"));
            Ok::<_, Box<EvalAltResult>>(h)
        });
        engine.register_indexer_get(|_: &mut Sys, k: &str| {
            Err::<Dynamic, _>(host_error(codes::FORBIDDEN_CAPABILITY, format!("sys.{k}")))
        });
        engine.register_indexer_set(|_: &mut Sys, k: &str, _: Dynamic| {
            Err::<(), _>(host_error(codes::FORBIDDEN_CAPABILITY, format!("sys.{k}")))
        });

        // Typed handles generated from the class.
        engine.register_type_with_name::<Handle>("Handle");
        for field in class.fields.keys() {
            let f = field.clone();
            engine.register_get(field, move |h: &mut Handle| h.get(&f));
            let f = field.clone();
            engine.register_set(field, move |h: &mut Handle, v: Dynamic| h.set(&f, v));
        }
        engine.register_indexer_get(|h: &mut Handle, k: &str| h.get(k));
        engine.register_indexer_set(|h: &mut Handle, k: &str, v: Dynamic| h.set(k, v));

        Sandbox {
            engine,
            limits,
            meter,
            cancel,
            class,
            cache: RefCell::new(BTreeMap::new()),
        }
    }

    /// The limits in force.
    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Enforces the memory limit with a probe returning the bytes currently
    /// allocated (for example by a counting global allocator). The probe must count
    /// only this execution's allocations, so one execution runs per process or
    /// the probe is per thread.
    pub fn with_memory_probe(self, probe: fn() -> usize) -> Sandbox {
        self.meter.probe.set(Some(probe));
        self
    }

    /// A handle that cancels the running execution at its next metering point.
    pub fn cancel_handle(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }

    fn scope(&self, ctx: &Context) -> Scope<'static> {
        let mut scope = Scope::new();
        scope.push_constant(
            "ctx",
            Ctx {
                now: ctx.now.clone(),
                tenant: ctx.tenant.clone(),
            },
        );
        scope.push_constant(
            "sys",
            Sys {
                meter: self.meter.clone(),
                limit: self.limits.instruction_calls,
            },
        );
        scope.push(
            "rec",
            Handle {
                class: self.class.clone(),
                values: BTreeMap::new(),
            },
        );
        scope
    }

    /// Compiles a script, or returns the cached AST for the same source.
    fn compile(&self, script: &str, scope: &Scope) -> Result<AST, Failure> {
        if script.len() > self.limits.source_bytes {
            return Err(Failure {
                code: codes::SCRIPT_TOO_COMPLEX,
                detail: format!("source exceeds {} bytes", self.limits.source_bytes),
            });
        }
        if let Some(ast) = self.cache.borrow().get(script) {
            return Ok(ast.clone());
        }
        let ast = self
            .engine
            .compile_with_scope(scope, script)
            .map_err(|e| map_error(&EvalAltResult::ErrorParsing(*e.0, e.1)))?;
        self.cache
            .borrow_mut()
            .insert(script.to_string(), ast.clone());
        Ok(ast)
    }

    /// Runs a script under the profile and returns its outcome.
    pub fn run(&self, script: &str, ctx: &Context) -> Outcome {
        self.meter.operations.set(0);
        self.meter.calls.set(0);
        self.meter.rng.set(ctx.seed);
        self.meter.journal.borrow_mut().clear();
        let mut scope = self.scope(ctx);
        let result = self.compile(script, &scope).and_then(|ast| {
            if let Some(probe) = self.meter.probe.get() {
                self.meter.baseline.set(probe());
            }
            self.meter.start.set(Some(Instant::now()));
            let r = self.engine.eval_ast_with_scope::<Dynamic>(&mut scope, &ast);
            self.meter.start.set(None);
            match r {
                Ok(v) => to_json(&v, 0),
                Err(e) => Err(map_error(&e)),
            }
        });
        Outcome {
            result,
            operations: self.meter.operations.get(),
            instruction_calls: self.meter.calls.get(),
            journal: self.meter.journal.borrow().clone(),
        }
    }
}

/// Converts a script result to JSON; handles and function pointers never leave
/// the sandbox.
fn to_json(v: &Dynamic, depth: usize) -> Result<Value, Failure> {
    if depth > 64 {
        return Err(Failure {
            code: codes::LIMIT_EXCEEDED,
            detail: "result nesting".into(),
        });
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
        return Ok(Value::String(d.normalize().to_string()));
    }
    if let Ok(c) = v.as_char() {
        return Ok(Value::String(c.to_string()));
    }
    if v.is_string() {
        return Ok(Value::String(v.clone().into_string().unwrap_or_default()));
    }
    if v.is_array() {
        let a = v.clone().into_array().unwrap_or_default();
        return a
            .iter()
            .map(|x| to_json(x, depth + 1))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array);
    }
    if v.is_map() {
        let m = v.clone().cast::<rhai::Map>();
        let mut out = Map::new();
        for (k, x) in m.iter() {
            out.insert(k.to_string(), to_json(x, depth + 1)?);
        }
        return Ok(Value::Object(out));
    }
    let name = if v.is::<Handle>() {
        "Handle"
    } else if v.is::<Sys>() {
        "Gateway"
    } else if v.is::<Ctx>() {
        "Context"
    } else {
        v.type_name()
    };
    Err(Failure {
        code: codes::TYPE_ERROR,
        detail: format!("value of type {name} cannot leave the sandbox"),
    })
}

fn function_name(signature: &str) -> &str {
    signature.split([' ', '(']).next().unwrap_or("")
}

/// Maps an engine failure to a registered code (UBS-STD-17 §2).
fn map_error(e: &EvalAltResult) -> Failure {
    let detail = e.to_string();
    let code = match e {
        EvalAltResult::ErrorSystem(_, inner) => match inner.downcast_ref::<HostError>() {
            Some(h) => {
                return Failure {
                    code: h.0,
                    detail: h.1.clone(),
                };
            }
            None => codes::TYPE_ERROR,
        },
        EvalAltResult::ErrorTerminated(token, _) => {
            if token.clone().into_string().ok().as_deref() == Some(codes::CANCELLED) {
                codes::CANCELLED
            } else {
                codes::LIMIT_EXCEEDED
            }
        }
        EvalAltResult::ErrorInFunctionCall(_, _, inner, _) => return with_detail(inner, detail),
        EvalAltResult::ErrorTooManyOperations(_)
        | EvalAltResult::ErrorTooManyVariables(_)
        | EvalAltResult::ErrorTooManyModules(_)
        | EvalAltResult::ErrorStackOverflow(_)
        | EvalAltResult::ErrorDataTooLarge(..) => codes::LIMIT_EXCEEDED,
        EvalAltResult::ErrorParsing(p, _) => match p {
            ParseErrorType::ExprTooDeep
            | ParseErrorType::TooManyFunctions
            | ParseErrorType::LiteralTooLarge(..) => codes::SCRIPT_TOO_COMPLEX,
            ParseErrorType::Reserved(_)
            | ParseErrorType::VariableUndefined(_)
            | ParseErrorType::ModuleUndefined(_)
            | ParseErrorType::ForbiddenVariable(_) => codes::FORBIDDEN_CAPABILITY,
            ParseErrorType::BadInput(l) if l.to_string().contains("is disabled") => {
                codes::FORBIDDEN_CAPABILITY
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
    Failure { code, detail }
}

fn with_detail(inner: &EvalAltResult, detail: String) -> Failure {
    Failure {
        code: map_error(inner).code,
        detail,
    }
}
