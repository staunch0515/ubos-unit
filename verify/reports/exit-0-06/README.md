# EXIT-0-06 — Rhai sandbox spike (VER-SEC-0001)

| Item | Value |
|---|---|
| Criterion | EXIT-0-06: the Rhai sandbox spike blocks all 100 initial escape cases, or a decision revises ASM-008; MET-SEC-001: 100 of 100 blocked |
| Result | **100 of 100 blocked**; ASM-008 confirmed for the L2 language with the conditions of DEC-034 |
| Build | this pull request's head (`cargo build --release --locked -p ubos-runtime`) |
| Engine | Rhai 1.26.1, features `std`, `no_float`, `decimal`, `no_time`, `no_module`, `no_closure`, `no_custom_syntax` |
| Host | prototype L2 host in crate `ubos-runtime` (`src/sandbox.rs`), profile `strict` of STD-CTX-004 |
| Suite | `verify/sandbox/l2-escape-cases.json` (100 cases, IDs L2E-001…100) |
| Workstation | 4 vCPU, Linux, Rust 1.94; whole suite 3 s |

## 1. Results

| Category | Cases | Blocked |
|---|---|---|
| file | 12 | 12 |
| network | 12 | 12 |
| time | 12 | 12 |
| entropy | 12 | 12 |
| float | 14 | 14 |
| host abuse | 20 | 20 |
| resource exhaustion | 18 | 18 |
| **total** | **100** | **100** |

A case is blocked when its outcome equals the expected one **and** a second run, in a
freshly built sandbox on another thread, gives the same outcome (same result or error code,
same journal, same operation count unless the run was stopped by elapsed time or
cancellation). The expected outcome is an error code for 91 cases and a value for 9 cases
that show the safe replacement of a forbidden capability (fixed `ctx.now`, journaled
`sys.uuid` and `sys.random_bytes`, key-ordered maps, exact decimals).

Outcomes: `LOGIC.FORBIDDEN_CAPABILITY` 42, `LOGIC.LIMIT_EXCEEDED` 14, `LOGIC.TYPE_ERROR` 12,
`LOGIC.FLOAT_TO_DECIMAL` 8, `LOGIC.CONTEXT_IMMUTABLE` 6, `LOGIC.UNKNOWN_FIELD` 3,
`LOGIC.SCRIPT_TOO_COMPLEX` 3, `LOGIC.DIVISION_BY_ZERO` 2, `LOGIC.CANCELLED` 1, value 9.
Raw results with engine messages, operation counts and times: `sandbox.json`.

Two expectations were corrected while the suite was written; the attempt was blocked in
every run: L2E-075 (`sys.uuid = "x"`) is refused as `LOGIC.CONTEXT_IMMUTABLE` because the
gateway is a constant, not as `LOGIC.FORBIDDEN_CAPABILITY`; and L2E-084 replaced a second
infinite-loop case after finding F-3 below.

## 2. Hardening applied (STD-CTX-020, DSN-DVM-612)

| Measure | How |
|---|---|
| No file, network, time or random functions | engine built with `Engine::new_raw()` and an explicit list of pure packages (arithmetic, logic, bit fields, strings, iterators, arrays, maps, function pointers); features `no_time`, `no_module` |
| `eval`, `print`, `debug` disabled | `disable_symbol`; module loading removed by `no_module` (`import` is a reserved word) |
| Closures capturing the gateway disabled | `no_closure`; script functions cannot see the scope, so `sys` is unreachable inside them |
| Floating point disabled, exact decimals | `no_float` + `decimal`: literals such as `0.1` are exact decimals (`0.1 + 0.2 == 0.3`); the float functions of Rhai's math package are registered as stubs raising `LOGIC.FLOAT_TO_DECIMAL`; the approximate decimal functions (`sin`, `exp`, `sqrt` …) are not loaded |
| Limits of the profile | operations, call depth (= nesting depth), expression depth, string, array and map sizes, variables, functions, source size |
| Metering (STD-CTX-005) | `on_progress`: cancellation and memory checked at every unit, elapsed time every 8 units; termination through `ErrorTerminated`, which scripts cannot catch |
| Memory | per-execution count from a counting allocator (`cap` crate) read at every metering point |
| Read-only context | `ctx` and `sys` are scope constants; setters and index writes raise `LOGIC.CONTEXT_IMMUTABLE` |
| Journaled entropy (STD-CTX-010) | `sys.uuid()` and `sys.random_bytes(n)` draw from a seeded generator, are journaled and count as instruction calls |
| Typed handles | handle type generated from a class definition; unknown fields raise `LOGIC.UNKNOWN_FIELD`, wrong types `LOGIC.TYPE_ERROR`; handles, the gateway, the context and function pointers cannot leave the sandbox as results |
| Host errors not catchable | host functions raise `ErrorSystem`, which `try … catch` does not intercept |
| Deterministic hashing | fixed engine hashing seed; maps are key-ordered |

## 3. Findings

| ID | Finding | Consequence |
|---|---|---|
| F-1 | Rhai's standard and core packages contain `sleep` (blocks the thread without reaching a metering point, so it defeats the time limit), `exit`, `parse_json` and reflection over function metadata. `Engine::new()` must not be used. | DSN-DVM-612 amended: the engine starts empty and receives an explicit list of pure packages. |
| F-2 | Engine size limits bound a single value only. Without memory accounting, 1,000 variables of 65,536-element arrays reach about 450 MB under `strict` (limit 64 MB) before the time budget stops the script. With the counting allocator the script stops at 64 MB (line 65 of L2E-084). | DSN-DVM-612 amended: memory is accounted per execution outside the engine. The spike's process-wide counter is valid for one execution at a time; the kernel needs a per-worker count (one execution per worker thread with a thread-attributed counter, or one per process). |
| F-3 | One operation unit can be a native call on a full-size value (for example `pad` to 65,536 elements, about 1.5 ms). Checking elapsed time every 256 units overshot the 500 ms budget by 220 ms; checking every 8 units keeps the overshoot near 10 ms (STD-CTX-005). Metering costs about 4 ns per unit (1,000,000 units of an empty loop: 2 ms without, 6 ms with memory and time checks). | Recorded for the benchmark of NR-PERF-010 in PH-1; a weighted unit cost for native calls on large values is to be considered then. |
| F-4 | Rhai re-checks container sizes after each mutation in time proportional to the container, so growing an array element by element is quadratic: L2E-087 reaches the 500 ms budget at about 45,000 elements, before the 65,536-element limit. Safe (bounded by the time budget), but slow. | Recorded for PH-1 (benchmark; lower default sizes or a patched check). |
| F-5 | Rhai's `decimal` feature uses `rust_decimal` (96-bit mantissa, 28 digits), while L1 expressions use the arbitrary-precision decimal of `ubos-types`. Division results differ beyond 28 digits and large values overflow in L2 only. | OQ-008: register the kernel decimal as the L2 decimal type, or bound L1 decimals to the same range. |
| F-6 | STD-CTX-020 says string, array and map sizes "follow the profile", but STD-CTX-004 lists no such sizes. The spike uses 1 MiB strings and 65,536 elements for `strict`, scaled ×8 (`standard`, `agent`), ×64 (`batch`), ×128 (`simulation`); 1,024 variables, 256 functions, 64 KiB source, expression depth 64/32. | OQ-007: add the sizes to STD-CTX-004. |
| F-7 | Rhai's own errors for unknown functions (`LOGIC.FORBIDDEN_CAPABILITY`) and unknown properties are catchable by `try … catch`; the capability is still unreachable, but a script can probe which names exist. Host errors and terminations are not catchable. | Accepted for L2; revisit with the full instruction set in PH-1. |
| F-8 | RSK-004 (Rhai not deterministic without floats) did not occur: with `no_float` + `decimal` every case replayed identically, and no fork of Rhai is needed. | RSK-004 can be closed at the PH-1 re-baseline (step 7). |

## 4. Scope and limits of the spike

The spike covers the L2 host only. The WASM half of ASM-008 (L3 components, STD-CTX-021) is
not exercised here; it is verified with the L3 host in PH-3 (DEC-036). CPU time is enforced through
elapsed time, which is conservative (a script is stopped when elapsed time, not CPU time,
reaches the smaller of the CPU and wall budgets). The instruction set, `ctx` beyond `now` and
`tenant`, journal persistence, replay and the AST cache keyed by asset version hash are
PH-1 work (DSN-DVM-612).

Reproduce:

```sh
cargo build --release --locked -p ubos-runtime
./target/release/ubos-sandbox-spike --cases verify/sandbox/l2-escape-cases.json --report sandbox.json
```
