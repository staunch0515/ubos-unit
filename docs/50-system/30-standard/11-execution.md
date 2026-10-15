---
id: UBS-STD-11
title: BPA Standard — Execution Context, Determinism, Metering and Logic Assets (CTX)
status: draft
phase: PH-0
depends_on: [UBS-STD-10]
---

# BPA Standard — Execution (CTX)

## 1. Context and gateway

### STD-CTX-001 — Execution context
- **Clause:** Every execution MUST receive an immutable context conforming to `DAT-ExecutionContext`. It is established by the host (AR-007) and extended only by the implementation (for example the `trigger` of nested runs). Logic MUST NOT be able to modify it.

  In detail (DEC-044): the caller may give `principal`, `principal_kind`, `on_behalf_of`, `auth_level`, `asof`, `locale`, `time_zone`, `channel`, `profile` and `input`; the implementation sets every other member, and a caller that gives one fails with `LOGIC.CONTEXT_IMMUTABLE`. `time` is `head` for the head at start. Binding a name `ctx`, `this`, `params` or `old` in logic also fails with `LOGIC.CONTEXT_IMMUTABLE`.
- **Grammar / Schema:** `DAT-ExecutionContext`
- **Conformance vectors:** VER-CONF-4000…4019
- **Satisfies:** FR-LOGIC-021, FR-LOGIC-022
- **Notes:** Since 1.0-rc2 (DEC-067) the context MAY carry `role`, the role the caller acts in, which the decisions of STD-LGS-031 condition on.

```yaml
DAT-ExecutionContext:
  principal: { type: text, required: true, description: "acting principal DID" }
  principal_kind: { type: enum, required: true, description: "human | service | agent | external | node" }
  on_behalf_of: { type: list, required: false, description: "delegation chain of DIDs, outermost first" }
  auth_level: { type: text, required: true, description: "authentication assurance (for step-up rules)" }
  tenant: { type: text, required: true, description: "tenant id" }
  vae: { type: text, required: true, description: "authority" }
  branch: { type: text, required: true, description: "branch name" }
  asof: { type: text, required: true, description: "valid-time coordinate" }
  time: { type: text, required: true, description: "snapshot selector (head at start, or explicit)" }
  now: { type: instant, required: true, description: "fixed for the whole process; journaled" }
  locale: { type: text, required: true, description: "BCP 47 tag" }
  time_zone: { type: text, required: true, description: "IANA zone of the user (business zone is a VAE setting)" }
  process_id: { type: text, required: true, description: "UUID" }
  trigger: { type: composite, required: true, description: "as DAT-Process.trigger" }
  channel: { type: enum, required: true, description: "as DAT-Process.channel" }
  input: { type: json, required: false, description: "invocation payload" }
  profile: { type: enum, required: true, description: "strict | standard | batch | simulation | agent" }
  budget_remaining: { type: composite, required: true, description: "remaining units, memory, wall time, calls" }
  replay: { type: boolean, required: true, description: "true during replay" }
```

### STD-CTX-002 — Identity comes only from the context
- **Clause:** No instruction MAY accept tenant, VAE or principal as a parameter, except delegation-specific operations defined by the standard. Cross-VAE access is expressed only through canonical URIs of published objects and is authorised against the context principal.

  An identity parameter fails with `LOGIC.PARAMETER_FORBIDDEN`; a URI of another VAE is not found (`IAM.NOT_FOUND`) until published objects are defined (DEC-044).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-4020…4029
- **Satisfies:** FR-LOGIC-022, CST-014
- **Notes:** —

### STD-CTX-003 — The capability gateway
- **Clause:** The gateway object (`sys`) given to an asset MUST expose exactly the instructions allowed by the intersection of:
  1. the asset's declared capabilities (`DAT-LogicAsset.capabilities`);
  2. the profile's allowed families;
  3. the ABI version declared by the asset.

  Calls to anything else MUST fail with `LOGIC.FORBIDDEN_CAPABILITY`.

  A declared capability is a family (`read`) or one instruction (`read.get`). An instruction outside the ABI is a forbidden capability; an instruction of the ABI that the release does not provide fails with `LOGIC.NOT_AVAILABLE` (in BPA 1.0-rc1: `commit.transition`, `read.explain`, `run.decision`, `run.validate`, `emit.notify` and the `search`, `call` and `ask` families) (DEC-044).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-4030…4049
- **Satisfies:** FR-LOGIC-023, FR-LOGIC-092
- **Notes:** The 1.0-rc1 vector VER-CONF-4036 used `search.query` as the example of an unavailable instruction; `search.query` is available since 1.0-rc2 (STD-ISA-080) and the vector is superseded (STD-CONF-030, DEC-067).

## 2. Profiles and budgets

### STD-CTX-004 — Execution profiles
- **Clause:** Implementations MUST provide the profiles below with at least these default limits. Tenants MAY lower limits. Raising them requires entitlements (FR-BILL-022).

  | Limit | strict | standard | batch | simulation | agent |
  |---|---|---|---|---|---|
  | operation units | 1,000,000 | 10,000,000 | 500,000,000 | 1,000,000,000 | 20,000,000 |
  | CPU time | 500 ms | 3,000 ms | 600 s | 3,600 s | 10 s |
  | memory | 64 MB | 512 MB | 4 GB | 8 GB | 512 MB |
  | wall time | 2 s | 10 s | 30 min | 2 h | 60 s |
  | instruction calls | 1,000 | 10,000 | 5,000,000 | 50,000,000 | 20,000 |
  | changes per commit | 1,000 | 100,000 | 100,000 | 100,000 (sandbox only) | 5,000 |
  | nesting depth | 16 | 32 | 32 | 32 | 16 |
  | `call.connector` | 0 | 50 | 10,000 | 0 (recorded only) | 20 |
  | `ask.model` | 0 | 5 | 1,000 | 0 (recorded only) | 50 |
  | allowed families | read, search, run, commit, emit | all | all | all, with effects suppressed | all except `call` without an explicit grant |
  | L2 string length | 1 MiB | 8 MiB | 64 MiB | 128 MiB | 8 MiB |
  | L2 array and map size (elements, nested ones counted) | 65,536 | 524,288 | 4,194,304 | 8,388,608 | 524,288 |
  | L2 variables, functions per asset | 1,024, 256 | 1,024, 256 | 1,024, 256 | 1,024, 256 | 1,024, 256 |
  | L2 source size, expression depth (top level, in functions) | 64 KiB, 64/32 | 64 KiB, 64/32 | 64 KiB, 64/32 | 64 KiB, 64/32 | 64 KiB, 64/32 |

  A lowered limit is given per execution; a request to raise one fails with `IAM.DENIED`. CPU time, memory and wall time are measured by the implementation and verified by benchmarks and the L2 escape suite rather than by vectors (DEC-044).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-4050…4079
- **Satisfies:** FR-LOGIC-061, FR-AI-093, FR-RULE-072
- **Notes:** The `standard` CPU and memory values are the EXT-RFC sandbox limits (UBS-META-06 §6). The L2 rows come from the PH-0 sandbox spike and apply from BPA 1.0-rc1 (DEC-036); exceeding them raises `LOGIC.LIMIT_EXCEEDED`, and a source or expression beyond its limit raises `LOGIC.SCRIPT_TOO_COMPLEX`.

### STD-CTX-005 — Termination
- **Clause:** Exceeding any limit, or receiving cancellation, MUST terminate the execution with `LOGIC.LIMIT_EXCEEDED` or `LOGIC.CANCELLED` at the next metering point (at most every 10,000 units or 10 ms). This termination MUST NOT be catchable. The enclosing process MUST abort (STD-TXN-003).

  Host programs reach a metering point at every statement and instruction; the metering record keeps the outcome `limit` or `cancelled` (DEC-044).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-4080…4099
- **Satisfies:** FR-LOGIC-062, FR-AI-022
- **Notes:** —

### STD-CTX-006 — Metering record
- **Clause:** Each execution MUST produce a `DAT-MeteringRecord`, attached to its process and aggregated per tenant for billing.

  For host programs the units are exact: one per statement, the units of the L1 expressions (STD-EXPR-004) and the declared cost of each instruction (STD-ISA-004). `cpu_us`, `peak_memory` and `wall_us` are measured and compared only through `same_as` (DEC-044).
- **Grammar / Schema:** `DAT-MeteringRecord`
- **Conformance vectors:** VER-CONF-4100…4109
- **Satisfies:** FR-LOGIC-063, FR-BILL-011
- **Notes:** —

```yaml
DAT-MeteringRecord:
  process_id: { type: text, required: true, description: "process" }
  asset: { type: text, required: false, description: "logic asset version hash" }
  units: { type: integer, required: true, description: "operation units consumed" }
  cpu_us: { type: integer, required: true, description: "CPU microseconds" }
  peak_memory: { type: integer, required: true, description: "bytes" }
  wall_us: { type: integer, required: true, description: "wall microseconds" }
  instructions: { type: map, required: true, description: "count per family" }
  connector_calls: { type: integer, required: true, description: "count" }
  model_usage: { type: composite, required: false, description: "{calls, input_tokens, output_tokens, cost}" }
  outcome: { type: enum, required: true, description: "ok | error | limit | cancelled" }
```

## 3. Determinism and replay

### STD-CTX-010 — Sources of non-determinism
- **Clause:** Logic MUST be able to observe non-deterministic values only through:
  - `ctx.now` (fixed per process);
  - `sys.random_bytes`, `sys.uuid` (seeded from journaled entropy);
  - `journaled` instructions (STD-ISA-003).

  Host clocks, environment variables, thread scheduling, hash-map iteration order and floating-point modes MUST NOT influence results. Iteration over maps MUST follow key order.

  The entropy of a process is a 32-byte seed: given with the invocation, or SHA-256 of `ubos.entropy.v1` 0x00 and the process ID. Random bytes are the concatenation of SHA-256(seed ‖ uint64 counter) blocks; `sys.uuid` takes 16 bytes and sets the version 4 and variant bits. The seed is journaled, so replay draws the same values (DEC-044).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-4120…4149
- **Satisfies:** FR-LOGIC-071, FR-LOGIC-074, NR-DET-001
- **Notes:** —

### STD-CTX-011 — Execution journal
- **Clause:** Each process MUST record a journal (`DAT-Journal`) sufficient to replay it: context, snapshot commits per branch, logic versions, the ordered list of journaled instruction results, entropy seed and tz database version.

  A record is `{seq, instruction, args_hash, result_ref}` with content IDs of the arguments and the result; `outputs` holds the commit IDs, `result_hash`, `changes_hash` (the unit of work) and the effect types (DEC-044).
- **Grammar / Schema:** `DAT-Journal`
- **Conformance vectors:** VER-CONF-4150…4164
- **Satisfies:** FR-LOGIC-072, FR-AI-014
- **Notes:** —

```yaml
DAT-Journal:
  context: { type: composite, required: true, description: "DAT-ExecutionContext" }
  snapshots: { type: map, required: true, description: "branch → commit id" }
  logic: { type: list, required: true, description: "logic asset version hashes" }
  entropy_seed: { type: text, required: true, description: "base64url seed (encrypted with tenant key)" }
  tzdb: { type: text, required: true, description: "tz database version" }
  records: { type: list, required: true, description: "ordered {seq, instruction, args_hash, result_ref}" }
  outputs: { type: composite, required: true, description: "{commits, result_hash, effects}" }
```

### STD-CTX-012 — Replay
- **Clause:** Replay MUST re-execute a process with the journaled context, snapshots and logic versions:
  - it serves `journaled` instructions from the journal in order;
  - it suppresses `deferred` effects and writes;
  - it compares outputs.

  The result MUST be `identical`, or `diverged` with the first divergence (record sequence, expected, actual). A missing journal record MUST fail with `LOGIC.REPLAY_INPUT_MISSING`, never with a live call.

  The first divergence is the first record whose instruction or arguments differ, or else the first output that differs, reported with `seq` one after the last record. Replay writes nothing and produces no metering record (DEC-044).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-4165…4184
- **Satisfies:** FR-LOGIC-073, FR-AI-092, FR-DEV-092
- **Notes:** —

## 4. Logic tiers

### STD-CTX-020 — L2 scripting profile (Rhai)
- **Clause:** L2 scripts MUST be executed by a Rhai engine configured as follows:
  - `eval`, module file loading, `print` and `debug` to the host console are disabled;
  - closures capturing the gateway are disabled;
  - exact decimals are used (`decimal` feature);
  - maximum operations, call depth, string length, array and map sizes follow the profile;
  - the `on_progress` callback implements metering (STD-CTX-005);
  - custom types are generated from the model.

  Implementations in other languages MUST provide an L2 language with equivalent observable semantics for the conformance vectors, or declare the L2 profile unsupported.

  Static analysis of L2 sources rejects `eval`, `print`, `debug`, module statements (`import`, `export`), function pointers and reflection (`Fn`, `call`, `curry`, `is_def_fn`, `is_def_var`), `sleep`, `exit` and `parse_json`, and closures that name `sys`. Instructions are bound as `sys.<family>.<operation>(…)`. An implementation without an L2 engine fails L2 execution with `LOGIC.NOT_AVAILABLE`. The run-time behaviour of the engine is verified by the L2 escape suite (EXIT-1-10) (DEC-044).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-4200…4239
- **Satisfies:** FR-LOGIC-041, FR-LOGIC-042, FR-LOGIC-045
- **Notes:** The Rhai language is part of profile `Core` for scripts. Third-party engines may certify without L2 if they execute only L1 and L3 logic.

### STD-CTX-021 — L3 component world
- **Clause:** L3 components MUST be WebAssembly components (component model) that:
  - import only the world `ubos:isa/v1` (instructions and context) and optionally `ubos:types/v1`;
  - export the functions or ports declared by the asset.

  WASI imports other than `wasi:random` (served from journaled entropy) MUST be rejected. Execution uses fuel metering mapped to units (1 unit = 100 fuel), and linear memory limits per profile. Threads are disabled, and NaN results are canonicalised.
- **Grammar / Schema:** WIT package `ubos:isa@1.0.0` (published with the standard)
- **Conformance vectors:** VER-CONF-4240…4269
- **Satisfies:** FR-LOGIC-051, FR-LOGIC-053, FR-LOGIC-055
- **Notes:** —

### STD-CTX-022 — Logic asset declaration
- **Clause:** Every L2 or L3 asset MUST be an object of class `LogicAsset` (`DAT-LogicAsset`).

  The genesis model defines the concrete class `Logic` below `LogicAsset` with the members of `DAT-LogicAsset`. The tier `host` marks host programs of conformance adapters (STD-ISA-007); it is a test interface under STD-CONF-002 (DEC-044).
- **Grammar / Schema:** `DAT-LogicAsset`
- **Conformance vectors:** VER-CONF-4270…4284
- **Satisfies:** FR-LOGIC-041, FR-LOGIC-081
- **Notes:** —

```yaml
DAT-LogicAsset:
  name: { type: text, required: true, description: "namespaced name" }
  tier: { type: enum, required: true, description: "l2 | l3 | host (conformance adapters only, DEC-044)" }
  abi: { type: text, required: true, description: "ABI version M.m" }
  source: { type: blob, required: true, description: "Rhai source, or component binary" }
  source_ref: { type: text, required: false, description: "for L3: content-id of the source archive (for audit)" }
  exports: { type: list, required: true, description: "exported functions/ports with signatures" }
  imports: { type: list, required: false, description: "pinned logic asset versions" }
  capabilities: { type: list, required: true, description: "declared instruction families and targets" }
  profile: { type: enum, required: true, description: "minimum profile needed" }
  tests: { type: list, required: false, description: "test case URIs" }
  state: { type: enum, required: true, description: "draft | committed | verified | deprecated | retired" }
  verification: { type: text, required: false, description: "content-id of DAT-VerificationRecord" }
  publisher_signature: { type: composite, required: false, description: "DAT-Signature (L3 required)" }
```

### STD-CTX-023 — Verification record and gate
- **Clause:** An asset becomes `verified` only when a `DAT-VerificationRecord` exists with all checks passed and a signature by an authorised verifier principal. On `main`, `release` and `overlay` branches, only `verified` and `deprecated` assets MUST execute (`LOGIC.UNVERIFIED_ASSET` otherwise).

  An asset is verified when its `verification` names a record whose checks (`static_analysis`, `type_check`, `capabilities`, `tests`, `signature`) all passed and whose asset has the same code as the current version (tier, ABI, source, exports, imports and capabilities). Verification commits `state` and `verification`; setting the state alone does not verify, and a change of code needs a new record. `retired` assets do not execute on any branch (DEC-044).
- **Grammar / Schema:** `DAT-VerificationRecord`
- **Conformance vectors:** VER-CONF-4285…4304
- **Satisfies:** FR-LOGIC-082, FR-LOGIC-083, FR-LOGIC-054
- **Notes:** —

```yaml
DAT-VerificationRecord:
  asset: { type: text, required: true, description: "asset version hash" }
  checks: { type: list, required: true, description: "{name: static_analysis|type_check|tests|capabilities|signature, result, findings_ref}" }
  analyzer_version: { type: text, required: true, description: "version of the forbidden-pattern list and analyser" }
  verified_by: { type: text, required: true, description: "principal DID (human or pipeline service)" }
  at: { type: instant, required: true, description: "time" }
  signature: { type: composite, required: true, description: "DAT-Signature" }
```

### STD-CTX-024 — Static analysis
- **Clause:** Static analysis MUST reject:
  - use of forbidden constructs (dynamic evaluation, reflection on the host, dynamic imports);
  - calls to undeclared instructions;
  - recursion without a declared bound;
  - L3 imports outside the world;
  - scripts exceeding size or AST-depth limits.

  The forbidden list is versioned with the ABI. Findings MUST be recorded (FR-LOGIC-094).

  Findings are recorded as `{rule, …}` with the rules `forbidden_construct`, `unknown_instruction`, `undeclared_instruction`, `unbounded_recursion`, `type_error`, `nesting_depth`, `source_size` and `import_outside_world`. For host programs, recursion is bounded by a function's `max_depth`, and nesting is limited to 32 per function (DEC-044).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-4305…4344
- **Satisfies:** FR-LOGIC-091, FR-LOGIC-092, FR-LOGIC-094
- **Notes:** —

## 5. Deployment behaviour of logic

### STD-CTX-030 — Version pinning per process
- **Clause:** A process MUST resolve every logic asset once, at first use, from its snapshot, and MUST use that version for its whole duration. New versions affect only processes that start after the commit making them effective.

  Assets are pinned per asset object at the process snapshot. A change of an asset committed by the process itself takes effect for later processes only (DEC-044).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-4345…4354
- **Satisfies:** FR-LOGIC-085
- **Notes:** —

### STD-CTX-031 — Quarantine
- **Clause:** Implementations MUST quarantine an asset version for a VAE when its limit-termination or failure rate exceeds the configured threshold. While quarantined, invocations MUST fail fast with `LOGIC.ASSET_QUARANTINED` until release by an authorised principal. Quarantine and release are recorded as System-kind objects.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-4355…4364
- **Satisfies:** FR-LOGIC-064
- **Notes:** —
