---
id: UBS-SYS-DVM-02-04
title: DVM — Logic Runtime Design
status: draft
phase: PH-1
depends_on: [UBS-SYS-DVM-02-03, UBS-STD-10, UBS-STD-11, UBS-STD-12]
---

# DVM — Logic Runtime Design (DSN-DVM-6xx)

This file designs the crates `ubos-runtime` and `ubos-expr`.
Reading order:
1. instruction dispatch and the capability gateway (601–606);
2. execution tiers: expressions, Rhai, WASM (610–618);
3. profiles, metering and termination (620–625);
4. journal and replay (630–634);
5. logic assets and static analysis (640–646);
6. connectors (650–653);
7. `ask.model` (660–662).

## Instruction dispatch and the capability gateway

### DSN-DVM-601 — Single dispatch table
- **Statement:** All instructions of ABI 1.0 (STD-ISA-001) MUST be implemented once in `ubos-runtime` as handlers with the signature `fn(&mut ProcessCtx, Args) -> Result<Value, Error>`. The three tiers (expressions, Rhai, WASM) MUST call the same handlers through thin bindings, so behaviour is tier-neutral (STD-ISA-007).
- **Rationale:** One effect path (FR-LOGIC-014, FR-LOGIC-015).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given the instruction vectors, when executed through each tier, then results and errors are identical.
- **Verification:** CONF
- **Origin:** FR-LOGIC-011, FR-LOGIC-013, FR-LOGIC-014, FR-LOGIC-015, STD-ISA-001, STD-ISA-007
- **Phase note:** PH-1 step 5b (DEC-056) implements the instructions as one gateway and one host binding: `ubos-runtime` holds the catalogue, the checks of the pipeline, the budget and the program interpreter, and the store-facing half of each instruction is supplied by the engine through the `Host` trait (AR-031: `ubos-runtime` cannot reach the store). The L2 and L3 bindings of step 5d call the same gateway.

### DSN-DVM-602 — Per-instruction pipeline
- **Statement:** Every handler invocation MUST pass these steps in order:
  1. ABI check (`LOGIC.ABI_UNSUPPORTED`);
  2. capability check against the asset's granted capabilities (`LOGIC.FORBIDDEN_CAPABILITY`);
  3. authorization decision (STD-ISA-005, DSN-DVM-801);
  4. metering charge (STD-ISA-004);
  5. execution;
  6. journaling of non-deterministic inputs;
  7. error normalisation (STD-ISA-006).
- **Rationale:** Uniform enforcement regardless of caller.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given an asset without `emit.notify`, when it calls the instruction, then `LOGIC.FORBIDDEN_CAPABILITY` is returned, and nothing is charged beyond the check.
- **Verification:** CONF, SEC
- **Origin:** FR-LOGIC-023, STD-CTX-003, STD-ISA-004, STD-ISA-005, STD-ISA-006

### DSN-DVM-603 — Immutable context
- **Statement:** The execution context (DAT-ExecutionContext) MUST be constructed by the host API from authenticated data and MUST be exposed to logic as read-only. Any attempt to set identity, tenant, branch or time from logic MUST fail with `LOGIC.CONTEXT_IMMUTABLE`.
- **Rationale:** No self-declared identity (FR-LOGIC-022, STD-CTX-002).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a script that sets `ctx.principal`, when run, then it fails, and the process aborts.
- **Verification:** SEC, CONF
- **Origin:** FR-LOGIC-021, FR-LOGIC-022, STD-CTX-001, STD-CTX-002

### DSN-DVM-604 — Context propagation
- **Statement:** Nested calls (`run.port`, `run.logic`, `run.decision`) MUST inherit the parent context, with the capability set narrowed to the intersection of the caller's and callee's grants. `run.async` MUST create a new process with a derived context that records the parent process as trigger.
- **Rationale:** FR-LOGIC-024.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a port with capability C calling a port without C, when the inner port uses C, then it fails.
- **Verification:** CONF, SEC
- **Origin:** FR-LOGIC-024, STD-ISA-050, STD-ISA-055

### DSN-DVM-605 — Instruction catalogue introspection
- **Statement:** The runtime MUST expose the instruction catalogue (name, family, determinism class, metering cost, required capability, availability phase) as reflection data, generated from the handler registry.
- **Rationale:** Tooling and agents get an exact catalogue.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given the catalogue output, when compared with the standard's instruction list, then they match.
- **Verification:** INSP, CONF
- **Origin:** FR-LOGIC-013, STD-ISA-001, STD-ISA-003
- **Phase note:** PH-1 step 5b (DEC-056) embeds the catalogue as data generated from the clause text of the standard (`standard/tools/gen_isa_json.py`), the same source the oracle reads, so that the kernel and the specification cannot drift; generation from a handler registry follows when the handlers of the L2 and L3 tiers exist (step 5d).

### DSN-DVM-606 — Business errors
- **Statement:** Logic MUST raise business errors through a single `fail(code, message, details)` instruction; the code MUST be declared in the asset's error list. Undeclared codes MUST be reported by static analysis.
- **Rationale:** FR-LOGIC-044.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a script raising a declared error, when run, then the process aborts, and the error object carries the code.
- **Verification:** CONF
- **Origin:** FR-LOGIC-044, AR-026

## Execution tiers

### DSN-DVM-610 — Expression engine
- **Statement:** `ubos-expr` MUST parse the expression grammar (STD-EXPR-001), type-check it against the effective definition (STD-EXPR-002), and compile it to a compact bytecode evaluated by a stack machine. Compiled expressions MUST be cached by `(expression hash, model hash)`. Evaluation MUST be total (STD-EXPR-004) and use exact decimal arithmetic.
- **Rationale:** Fast, safe L1 logic for sheets, guards and filters.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessAnalyst
- **Acceptance:**
  1. Given the expression vectors, when evaluated, then results equal the vectors, including null semantics.
  2. Given a typical guard expression, when evaluated 1,000,000 times, then the mean cost is below 1 µs.
- **Verification:** CONF, BENCH
- **Origin:** FR-LOGIC-031, FR-LOGIC-033, FR-LOGIC-034, STD-EXPR-001, STD-EXPR-002, STD-EXPR-003, STD-EXPR-004, FR-LOGIC-032
- **Phase note:** PH-1 step 5a (DEC-055) evaluates expressions by walking the checked syntax tree; step 5d (DEC-058) caches the checked tree by expression and static environment and measured 1.28 µs median per evaluation of a typical guard on the workstation (NR-PERF-010 met; acceptance 2 is measured on ENV-REF-SERVER at step 9, where the bytecode form is reconsidered).

### DSN-DVM-611 — Function library
- **Statement:** The function library (STD-EXPR-005) MUST be versioned with the ABI and implemented once for both expressions and scripts.
- **Rationale:** FR-LOGIC-035.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given every library function, when called from an expression and a script with the same inputs, then results are identical.
- **Verification:** CONF
- **Origin:** FR-LOGIC-035, STD-EXPR-005

### DSN-DVM-612 — Rhai host (L2)
- **Statement:** The L2 host MUST embed Rhai with the L2 profile of STD-CTX-020:
  - no filesystem, network, time or random functions;
  - floating point disabled (`LOGIC.FLOAT_TO_DECIMAL` on conversion attempts), decimal type from `ubos-expr`;
  - operation and call-depth limits mapped to metering units;
  - typed object handles generated from effective definitions (`LOGIC.UNKNOWN_FIELD`, `LOGIC.TYPE_ERROR`);
  - the engine starts without packages and receives an explicit list of pure packages; the language-core package (`sleep`, `exit`, `parse_json`, function reflection) is not loaded;
  - memory is accounted per execution outside the engine and checked at every metering point, because engine size limits bound single values only;
  - errors raised by host functions and limit terminations are not catchable by scripts.

  Compiled ASTs MUST be cached by asset version hash.
- **Amendment:** The last three bullets record the findings of the PH-0 sandbox spike (DEC-034, `verify/reports/exit-0-06/`).
- **Rationale:** Productive scripting with determinism.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given the L2 sandbox test suite, when run, then no forbidden capability is reachable.
  2. Given a cached script, when invoked, then the host overhead is within NR-PERF-010.
- **Verification:** SEC, CONF, BENCH
- **Origin:** FR-LOGIC-041, FR-LOGIC-042, FR-LOGIC-075, STD-CTX-020, NR-PERF-010, NR-SEC-002
- **Phase note:** PH-1 step 5d (DEC-058) implements the host on a worker thread per nesting level with the gateway of step 5b, the kernel decimal and money as script numbers, memory accounted through a counting allocator of the host binary and the thread CPU clock; the conformance adapter keeps L2 off (`forge conform --l2` enables it) because the rc1 vectors assume the oracle's declaration.

### DSN-DVM-613 — Script modules and imports
- **Statement:** Scripts MAY import modules by pinned asset version (FR-LOGIC-043). The loader MUST resolve imports from the snapshot, reject unpinned imports on protected branches, and detect cycles.
- **Rationale:** Reproducible logic composition.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a script importing module v1.2, when v1.3 is published, then the script still runs v1.2.
- **Verification:** CONF
- **Origin:** FR-LOGIC-043, STD-CTX-030

### DSN-DVM-614 — WASM host (L3)
- **Statement:** The L3 host MUST use wasmtime with the component model and the world of STD-CTX-021. It MUST enable fuel metering and epoch interruption, cap linear memory per profile, disable non-deterministic features (threads, relaxed SIMD) and canonicalise NaNs. Components MUST be pre-compiled per host CPU class and cached by `(component hash, engine version)`.
- **Rationale:** Language-neutral, sandboxed, metered heavy logic.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given the L3 escape test suite, when run, then no escape is possible.
  2. Given the same component and inputs on x86-64 and ARM64, when executed, then outputs are identical.
- **Verification:** SEC, CONF
- **Origin:** FR-LOGIC-051, FR-LOGIC-052, FR-LOGIC-053, FR-LOGIC-055, STD-CTX-021, NR-SEC-002

### DSN-DVM-615 — Component signature verification
- **Statement:** Before instantiation, the host MUST verify the component's signature and its verification record (STD-CTX-023). Unsigned or unverified components MUST NOT run on protected branches (`LOGIC.UNVERIFIED_ASSET`).
- **Rationale:** FR-LOGIC-054.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a component with one byte changed after signing, when loaded, then it is rejected.
- **Verification:** SEC
- **Origin:** FR-LOGIC-054, STD-CTX-023

### DSN-DVM-616 — Tenant isolation in execution
- **Statement:** Rhai engines and WASM stores MUST NOT be shared across tenants. Instance pools MUST be keyed by tenant; memory MUST be zeroed when an instance is returned to a pool.
- **Rationale:** FR-LOGIC-065, NR-SEC-003.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a script that leaves data in globals, when another tenant's script runs next on the same node, then the data is not observable.
- **Verification:** SEC
- **Origin:** FR-LOGIC-065, NR-SEC-003

### DSN-DVM-617 — Execution off the async reactor
- **Statement:** Logic execution MUST run on a dedicated bounded thread pool, never on the host's async I/O reactor threads. The pool size MUST default to the number of cores.
- **Rationale:** Long scripts do not block I/O.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given 64 scripts at the strict CPU limit, when running, then health endpoints still answer within 50 ms.
- **Verification:** BENCH
- **Origin:** NR-AVAIL-005

### DSN-DVM-618 — Hot swap of logic
- **Statement:** A new logic asset version MUST become active atomically at the commit that activates it; processes that started earlier MUST continue with their pinned version (STD-CTX-030).
- **Rationale:** FR-LOGIC-085.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a long process running v1, when v2 is activated, then that process finishes on v1, and new processes use v2.
- **Verification:** CONF
- **Origin:** FR-LOGIC-085, STD-CTX-030

## Profiles, metering and termination

### DSN-DVM-620 — Profile table
- **Statement:** The runtime MUST enforce the five profiles of STD-CTX-004 from a configuration table (wall time, CPU fuel, memory, instruction count, call depth, external calls, staged writes). The standard profile default MUST be 3,000 ms and 512 MB. Tenants MAY tighten but not loosen limits beyond the plan's ceiling.
- **Rationale:** FR-LOGIC-061.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given each limit, when exceeded by a test script, then `LOGIC.LIMIT_EXCEEDED` names the limit.
- **Verification:** CONF
- **Origin:** FR-LOGIC-061, STD-CTX-004

### DSN-DVM-621 — Uncatchable termination
- **Statement:** Limit breaches, cancellations (`LOGIC.CANCELLED`) and sandbox faults MUST terminate the whole process and discard its staging area; logic MUST NOT be able to catch them (STD-CTX-005).
- **Rationale:** FR-LOGIC-062.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a script wrapping a loop in try/catch, when the fuel runs out, then the process aborts.
- **Verification:** SEC, CONF
- **Origin:** FR-LOGIC-062, STD-CTX-005

### DSN-DVM-622 — Metering record
- **Statement:** Each process MUST produce a `MeteringRecord` (STD-CTX-006) with units per instruction family, CPU fuel, peak memory, wall time and external calls, stored with the commit or with the abort record, and exported to the host for billing and cost attribution.
- **Rationale:** FR-LOGIC-063, NR-COST-004.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given 1,000 processes, when their metering records are summed, then they equal the host's billed units.
- **Verification:** CONF
- **Origin:** FR-LOGIC-063, STD-CTX-006, NR-COST-004

### DSN-DVM-623 — Cancellation
- **Statement:** The host API MUST support cancelling a running process. Cancellation MUST take effect within 10 ms for L1 and L2 (operation checks) and within one epoch tick (default 10 ms) for L3.
- **Rationale:** Operators and kill switches stop work promptly (NR-PERF-019).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-AgentSupervisor
- **Acceptance:**
  1. Given a running infinite loop, when cancelled, then it stops within the bound.
- **Verification:** BENCH
- **Origin:** NR-PERF-019, STD-CTX-005

### DSN-DVM-624 — Quarantine
- **Statement:** Assets whose failure rate over a sliding window exceeds a threshold (default 50% of at least 20 runs) MUST be quarantined automatically (STD-CTX-031); calls MUST then fail with `LOGIC.ASSET_QUARANTINED` until a governed release.
- **Rationale:** FR-LOGIC-064.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given an asset failing 15 of 20 runs, when the next call occurs, then it is rejected, and an event is emitted.
- **Verification:** CONF
- **Origin:** FR-LOGIC-064, STD-CTX-031
- **Phase note:** DEC-122: built in PH-2 step 6d as BPA 1.0-rc8 states it (STD-CTX-031): the policy is a count of failures and limit terminations within a window, configured per VAE, rather than a rate over a minimum of runs; without a policy nothing is quarantined.

### DSN-DVM-625 — Script complexity limits
- **Statement:** The loader MUST reject scripts over the size and complexity limits (AST nodes, nesting depth, function count) with `LOGIC.SCRIPT_TOO_COMPLEX`.
- **Rationale:** FR-LOGIC-045.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a script over the AST limit, when loaded, then it is rejected with the measured value.
- **Verification:** CONF
- **Origin:** FR-LOGIC-045

## Journal and replay

### DSN-DVM-630 — Execution journal
- **Statement:** Each process MUST produce a journal (STD-CTX-011) recording: the context, the snapshot map, asset versions, every non-deterministic input (clock reads, entropy, connector responses, model responses) and the outcome hash. Journals MUST be stored as content-addressed chunks, referenced from the commit, with retention per class policy.
- **Rationale:** FR-LOGIC-072.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a process that reads the clock and calls a connector, when its journal is inspected, then both inputs are present.
- **Verification:** CONF
- **Origin:** FR-LOGIC-071, FR-LOGIC-072, STD-CTX-010, STD-CTX-011
- **Phase note:** PH-1 step 5c (DEC-057) stores the journal and the metering record as chunks under their content IDs, referenced from the process record; the reference from the commit record follows with the signed commit records of step 6.

### DSN-DVM-631 — Replay
- **Statement:** `replay(journal)` MUST re-execute the process against the recorded snapshot with recorded inputs substituted, without effects, and compare the outcome hash. Missing inputs MUST fail with `LOGIC.REPLAY_INPUT_MISSING`. Divergence MUST be reported with the first diverging instruction.
- **Rationale:** FR-LOGIC-073, NR-DET-001.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given 10,000 journaled processes, when replayed, then 100% reproduce their outcome hash.
- **Verification:** CONF, PROP
- **Origin:** FR-LOGIC-073, STD-CTX-012, NR-DET-001

### DSN-DVM-632 — Cross-implementation determinism
- **Statement:** Replay results MUST be identical across operating systems, CPU architectures and host editions (Server, Box, browser).
- **Rationale:** FR-LOGIC-074, NR-DET-002.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given the replay corpus, when replayed on Linux x86-64, Linux ARM64 and macOS ARM64, then all outcome hashes match (the browser build joins in PH-5).
- **Verification:** CONF
- **Origin:** FR-LOGIC-074, NR-DET-002

### DSN-DVM-633 — Journal sampling for low-risk processes
- **Statement:** Classes MAY declare a journal policy (always, on-derivation, sampled, never). Processes that write ledgers or derived values MUST always be journaled.
- **Rationale:** Storage cost where replay is not needed (NR-COST-001).
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a class with policy "never" that posts to a ledger, when committed, then a journal is stored anyway.
- **Verification:** CONF
- **Origin:** NR-COST-001, FR-LOGIC-072

### DSN-DVM-634 — Debug replay
- **Statement:** Replay MUST support a debug mode that emits an instruction trace (instruction, arguments hash, result hash, metering) for Forge's time-travel debugger.
- **Rationale:** Developer loop (NR-PERF-016).
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM, FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a failed process, when replayed in debug mode, then the trace ends at the failing instruction.
- **Verification:** CONF
- **Origin:** NR-PERF-016, STD-CTX-012

## Logic assets and static analysis

### DSN-DVM-640 — Asset store and states
- **Statement:** Logic assets (STD-CTX-022) MUST be Definition objects with states draft, verified, active, deprecated, retired and quarantined. Transitions MUST follow the state machine in UBS-SYS-DVM-05.
- **Rationale:** FR-LOGIC-081, FR-LOGIC-084.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a retired asset, when called, then `LOGIC.NOT_AVAILABLE` is returned.
- **Verification:** CONF
- **Origin:** FR-LOGIC-041, FR-LOGIC-081, FR-LOGIC-084, STD-CTX-022

### DSN-DVM-641 — Verification gate
- **Statement:** Activation on a protected branch MUST require a verification record (STD-CTX-023) produced by Forge, binding the asset hash to the passed checks (types, analysis, tests, coverage threshold). The kernel MUST verify the record's signature and hash before activation.
- **Rationale:** FR-LOGIC-082, FR-LOGIC-083.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given an asset without a record, when activation on `main` is attempted, then it fails with `LOGIC.UNVERIFIED_ASSET`.
- **Verification:** CONF, SEC
- **Origin:** FR-LOGIC-082, FR-LOGIC-083, STD-CTX-023, CR-SOX-003

### DSN-DVM-642 — Static analysis
- **Statement:** The analyser (STD-CTX-024) MUST detect forbidden patterns (FR-LOGIC-091), infer the required capabilities (FR-LOGIC-092) and report complexity findings (FR-LOGIC-093). Findings MUST be stored with the verification record (FR-LOGIC-094). Inferred capabilities MUST be a superset of what the asset uses at run time.
- **Rationale:** Least privilege by construction.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given 1,000 fuzzed scripts, when run with their inferred capabilities, then no run fails with `LOGIC.FORBIDDEN_CAPABILITY`.
- **Verification:** PROP
- **Origin:** FR-LOGIC-091, FR-LOGIC-092, FR-LOGIC-093, FR-LOGIC-094, STD-CTX-024

### DSN-DVM-643 — Type checking against the model
- **Statement:** The analyser MUST type-check scripts against the effective definitions of the target snapshot and report unknown fields and type mismatches before activation.
- **Rationale:** FR-LOGIC-042.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a script reading a removed field, when analysed, then the finding names the field and line.
- **Verification:** CONF
- **Origin:** FR-LOGIC-042

### DSN-DVM-644 — Re-check on model change
- **Statement:** When a model change is committed, active assets that reference changed classes MUST be re-type-checked; failures MUST block the model change on protected branches.
- **Rationale:** Model and logic stay consistent.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a field removal used by an active script, when committed to `main`, then it fails listing the script.
- **Verification:** CONF
- **Origin:** FR-MODEL-027, FR-LOGIC-042

### DSN-DVM-645 — Parameter validation
- **Statement:** Asset parameters MUST be declared with types; forbidden parameter kinds (secrets, raw identities) MUST be rejected with `LOGIC.PARAMETER_FORBIDDEN`.
- **Rationale:** Secrets only through handles.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given an asset declaring a string parameter named as a secret, when verified, then it is rejected.
- **Verification:** SEC
- **Origin:** FR-IAM-091

### DSN-DVM-646 — Deprecation warnings
- **Statement:** Calls to deprecated assets MUST succeed but add a warning to the process result and a metric per caller.
- **Rationale:** FR-LOGIC-084.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a deprecated asset, when called, then the result carries a deprecation warning.
- **Verification:** CONF
- **Origin:** FR-LOGIC-084

## Connectors

### DSN-DVM-650 — Connector call path
- **Statement:** `call.connector` (STD-ISA-090) MUST resolve the connector object from the snapshot, check the egress allow-list and classification rules (`LOGIC.EGRESS_NOT_ALLOWED`, `LOGIC.CLASSIFICATION_EGRESS_DENIED`), and delegate the network call to the host through CTR-002 with a secret handle. The kernel MUST NOT hold secrets or open sockets.
- **Rationale:** FR-LOGIC-101, FR-LOGIC-102, FR-LOGIC-104.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a connector to a host not on the allow-list, when called, then it fails before any network activity.
- **Verification:** SEC, CONF
- **Origin:** FR-LOGIC-101, FR-LOGIC-102, FR-LOGIC-104, STD-ISA-090, AR-032

### DSN-DVM-651 — Recorded exchanges
- **Statement:** Every connector exchange MUST be journaled (request without secrets, response, status, duration). Responses MUST be validated against the connector's declared schema before being returned to logic.
- **Rationale:** FR-LOGIC-103.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a connector response not matching its schema, when returned, then `LOGIC.CONNECTOR_ERROR` is raised.
- **Verification:** CONF
- **Origin:** FR-LOGIC-103, NR-SEC-009

### DSN-DVM-652 — Connector resilience
- **Statement:** The host MUST apply the connector's declared timeout, retry policy and circuit breaker; an open circuit MUST fail fast with `LOGIC.CONNECTOR_UNAVAILABLE`.
- **Rationale:** FR-LOGIC-105.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a failing endpoint, when 10 calls fail, then the circuit opens, and later calls fail in under 1 ms.
- **Verification:** FAULT
- **Origin:** FR-LOGIC-105

### DSN-DVM-653 — Connector mocks in simulation
- **Statement:** In the simulation profile, connector calls MUST be served from declared mocks or recorded responses; real calls MUST be impossible.
- **Rationale:** Simulations have no external effects.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessAnalyst
- **Acceptance:**
  1. Given a simulation with no mock for a connector, when called, then `LOGIC.NOT_AVAILABLE` is raised.
- **Verification:** CONF
- **Origin:** FR-RULE-071, STD-CTX-004

## `ask.model`

### DSN-DVM-660 — Model call path
- **Statement:** `ask.model` (STD-ISA-100) MUST check the classification of every input field against the provider allow-list (`AI.CLASSIFICATION_BLOCKED`), charge the AI budget (`AI.BUDGET_EXHAUSTED`), delegate the call to the host model gateway, and validate the output against the declared schema (`AI.OUTPUT_INVALID`).
- **Rationale:** FR-AI-091, FR-AI-093, FR-AI-094, AR-029.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM, AGT
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a restricted field in the prompt inputs, when called, then it fails before the provider is contacted.
- **Verification:** SEC, CONF
- **Origin:** FR-AI-091, FR-AI-093, FR-AI-094, FR-AI-013, STD-ISA-100, AR-029

### DSN-DVM-661 — Model calls in the journal
- **Statement:** Model requests and responses MUST be journaled so replay uses the recorded response (FR-AI-092).
- **Rationale:** Deterministic replay of AI-assisted processes.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a journaled process with a model call, when replayed with the provider offline, then it reproduces its outcome.
- **Verification:** CONF
- **Origin:** FR-AI-092, FR-AI-014

### DSN-DVM-662 — AI output never bypasses validation
- **Statement:** Values derived from model output MUST pass through the same validation plans, sheets and decisions as any other write, and MUST carry a provenance marker `ai-generated` on the version.
- **Rationale:** FR-AI-094, AR-029.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a model output that violates a constraint, when staged, then the process fails like any other invalid write.
- **Verification:** CONF
- **Origin:** FR-AI-094, FR-AI-073, AR-029
