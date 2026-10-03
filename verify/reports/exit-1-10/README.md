# EXIT-1-10 — L2 escape suite (VER-SEC-0100)

| Item | Value |
|---|---|
| Criterion | EXIT-1-10: 100% of at least 300 L2 escape cases blocked (MET-SEC-010, NR-SEC-002) |
| Result | **382 of 382 blocked** |
| Build | PH-1 step 5d-2 (`cargo build --release --locked -p ubos-dvm --bin ubos-l2-suite`) |
| Engine | Rhai 1.26.1, features `std`, `no_float`, `no_time`, `no_module`, `no_closure`, `no_custom_syntax`; no `decimal` feature (W-5) |
| Host | `ubos-runtime::l2` (DEC-058) through the engine's `execute` operation, profile `strict` unless a case says otherwise |
| Suite | `verify/sandbox/l2-escape-cases.json` version 0.2 (382 cases, IDs L2E-001…382) |
| Runner | `ubos-l2-suite`; report `l2-escape.json`, signed by an ephemeral runner key |
| Workstation | 4 vCPU, linux x86_64, Rust 1.94; whole suite 44.0 s |

## 1. Results

| Category | Cases | Blocked |
|---|---|---|
| context | 27 | 27 |
| determinism | 10 | 10 |
| entropy | 17 | 17 |
| file | 20 | 20 |
| float | 70 | 70 |
| gateway | 66 | 66 |
| handle | 18 | 18 |
| host-abuse | 65 | 65 |
| network | 19 | 19 |
| parsing | 10 | 10 |
| resource-exhaustion | 44 | 44 |
| time | 16 | 16 |
| **total** | **382** | **382** |

A case is blocked when its outcome equals the expected one **and** a second run on a fresh
engine gives the same outcome (same result or error code, same units unless the run was
stopped by a limit or by cancellation). The expected outcome is a registered error code for
318 cases and a value for 64 cases that show the safe replacement of a
forbidden capability (fixed `ctx.now`, deterministic `sys.uuid` and `sys.random_bytes`,
key-ordered maps, exact decimals, local handle changes that do not reach the store).

Outcomes: `LOGIC.FORBIDDEN_CAPABILITY` 118, `LOGIC.FLOAT_TO_DECIMAL` 41, `LOGIC.TYPE_ERROR` 38, `LOGIC.LIMIT_EXCEEDED` 35, `LOGIC.CONTEXT_IMMUTABLE` 28, `MODEL.INVALID_VALUE` 11, `LOGIC.PARAMETER_FORBIDDEN` 9, `LOGIC.SCRIPT_TOO_COMPLEX` 8, `LOGIC.UNKNOWN_FIELD` 7, `LOGIC.DIVISION_BY_ZERO` 5, `IAM.NOT_FOUND` 4, `MODEL.DECLARATION_INCOMPLETE` 3, `LOGIC.CANCELLED` 2, `MODEL.CURRENCY_MISMATCH` 2, `MODEL.PORT_NOT_IMPLEMENTED` 2, `EVT.UNKNOWN_EVENT_TYPE` 1, `IAM.DENIED` 1, `LOGIC.CONNECTOR_UNAVAILABLE` 1, `MODEL.ABSTRACT_INSTANTIATION` 1, `MODEL.CONSTRAINT_VIOLATED` 1, value 64.

Every case runs as the port `run` of an `Invoice` object on a draft branch of a fresh
in-memory engine with the L2 host enabled, so the gateway, the budget, the journal and the
commit path are the production ones: an attempt that reaches the store fails at the commit
with the model's code, and nothing is written.

## 2. What the cases cover

- PH-0 cases L2E-001…100 (file, network, time, entropy, float, host abuse, resource
  exhaustion), migrated to the production host: the typed handle is `this`, decimals are
  `dec(…)`, float literals are refused, gateway argument errors carry the gateway's code.
- gateway (66): identity parameters on every instruction (`LOGIC.PARAMETER_FORBIDDEN`),
  unknown and missing parameters, undeclared families and operations, unknown families,
  arguments that are not a map, reads and updates of unknown or foreign objects, updates
  refused by the model at commit, instruction-call and change limits, limits above the
  profile, the gateway and its families as values, logic that is not imported, recursion
  through `run.port` up to the depth limit, `run.async_` as an effect, entropy bounds.
- context (27): every member of `ctx` assigned, nested members, the indexer, a copy, an
  unknown member, removal, the context as a value, and the members' values.
- handle (18): unknown fields, every type mismatch on set, integer promotion, local changes
  that do not reach the store, the handle as a value, `self_id`, `params`.
- host abuse (65): `eval`, `print`, `debug`, `import`, `export`, `Fn`, `call`, `curry`,
  `is_def_fn`, `is_def_var`, `sleep`, `exit`, `parse_json`, closures, namespaces,
  rebinding `this`, shadowing, host errors inside `try`, integer overflow and shifts,
  division by zero.
- float (70): every float function, float literals in every position, decimal division
  without a scale, rounding modes, exact arithmetic, money arithmetic and currencies.
- resource exhaustion (44): string and array doubling, growth beyond the memory of the
  execution, recursion (direct, exponential, mutual), expression and block depth, variables,
  functions, source size, budgets, cancellation, gateway calls charged against the budget,
  `pad` beyond the profile size.
- determinism (10), parsing (10), and the host-function names of files, processes, the
  network, clocks and randomness.

## 3. Findings (W-2…W-5, W-12)

| Item | Finding | Disposition |
|---|---|---|
| W-2 memory | Rhai bounds single values only; a script can grow memory across values | memory is counted by the host binary's allocator (`cap`) and checked at every metering point against the profile's memory size; the suite's growth cases stop at the bound |
| W-3 metering | one unit could be a native call on a large value; no CPU clock | gateway calls are charged by argument and result size; the thread CPU time (`cpu-time`) and the wall time are checked every eight operations |
| W-4 size checks | Rhai's size checks walk the value on every change: 65,536 pushes took 2376 ms (quadratic) | with memory accounted per execution the engine's string, array and map size checks are off and `pad` is bounded per call by the profile size; the same 65,536 pushes take 45.27 ms (linear) |
| W-5 decimal | Rhai's 96-bit decimal differs from the kernel decimal | the kernel decimal and money are the script number types; float literals and functions fail with `LOGIC.FLOAT_TO_DECIMAL` |
| W-12 corpus | 100 cases | 382 cases, all blocked |

## 4. NR-PERF-010 measurements (this workstation, not ENV-REF-SERVER)

| Measure | Target | Measured |
|---|---|---|
| L1 guard, parse + check + evaluate (median of 20,000) | ≤ 10 µs median | 5.53 µs |
| L1 guard, checked tree reused (median of 20,000; DSN-DVM-610 cache) | 1 µs mean (DSN-DVM-610, acceptance 2) | 1.28 µs |
| warm L2 invocation through `execute` (p95 of 2,000) | ≤ 1 ms p95 | median 661.7 µs, p95 848.8 µs |

Growth under the engine (W-4, standard profile):

| What | Elements | ms |
|---|---|---|
| array | 1,000 | 1.7 |
| array | 10,000 | 7.9 |
| array | 65,536 | 45.27 |
| string | 1,000 | 1.41 |
| string | 10,000 | 4.48 |
| string | 100,000 | 26.24 |

The L2 invocation includes the process record, the journal and metering chunks and the
worker-thread handoff; the L1 cached figure is within a third of the DSN-DVM-610 mean on
this 4-vCPU container and is measured again on ENV-REF-SERVER at step 9 before the
bytecode form is considered (DEC-058).
