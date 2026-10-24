---
id: UBS-DEC-000
title: Decision Log
status: complete
phase: ALL
depends_on: [UBS-META-00, UBS-META-06]
---

# Decision Log

Decisions are never deleted. A replaced decision is marked
`Superseded by DEC-<NNN>`.

| ID | Title | Status |
|---|---|---|
| DEC-090 | PH-2 step 1b: money and periods in BPA 1.0-rc8 — period transitions (open ↔ closing by update, `close_period`/`reopen_period` for closed), close artefacts with a signed tag (STD-KIND-026), deterministic matching and match records (STD-KIND-027, -028), restatement on a correction branch with re-evaluation modes (STD-TIME-041, -042); VER-CONF-1528 superseded | Accepted |
| DEC-091 | PH-2 step 1c: approvals and rules in BPA 1.0-rc8 — deferred commits with `defer` (STD-TXN-060), decision tables with hit policies and an authoring check (STD-LGS-040, STD-ISA-053), approval requirements, separation of duties and approval evidence (STD-LGS-041…043), simulation of rule changes (STD-LGS-044) | Accepted |
| DEC-092 | PH-2 step 1d: workflows, sagas, quarantine, lenses and upstream changes in BPA 1.0-rc8 — workflow definitions, instances, events, timers, tasks and migration (STD-TXN-090…092), sagas (STD-TXN-080), quarantine (STD-CTX-031), computed change classification, lenses and their application and totality check (STD-CLS-050, -060…062), upstream change detection (STD-VER-082) | Accepted |
| DEC-093 | PH-2 step 1e: protection in BPA 1.0-rc8 — encrypted fields under subject keys and erasure (STD-OBJ-010, -011), authorization relations (STD-LGS-050), relation-based access rules (STD-LGS-051), field masking (STD-LGS-052) | Accepted |
| DEC-094 | PH-2 step 1f: integration and addressing in BPA 1.0-rc8 — connectors and `call.connector` (STD-ISA-090), compensation of pre-commit calls (STD-TXN-051), `emit.notify` (STD-ISA-071), short forms and VFS paths (STD-ADDR-003, -004), REST facade (STD-PROTO-021) | Accepted |
| DEC-095 | PH-2 step 1g: search and views in BPA 1.0-rc8 — `search.aggregate` (STD-ISA-081), `search.traverse` (STD-ISA-082), `search.text` (STD-ISA-083) with declared parameters, view resolution (STD-CLS-023) | Accepted |

---

The table rows and records of DEC-001 … DEC-029 are in `01-records-dec-001-029.md`
(UBS-DEC-01); those of DEC-030 … DEC-059 in `02-records-dec-030-059.md` (UBS-DEC-02);
those of DEC-060 … DEC-089 in `03-records-dec-060-089.md` (UBS-DEC-03); the records from
DEC-090 on follow.

### DEC-090 — Money and periods in BPA 1.0-rc8 (PH-2 step 1, part 1b)
- **Date:** 2026-10-19 · **Status:** Accepted
- **Context:** Part 1b of PH-2 step 1 (DEC-089) writes the clauses and vectors of money and periods for the PH-2 kernel. FR-LEDG-061, -062, -071, -073, FR-TIME-043, -064, DSN-DVM-322, -443 and -445 had either no clause (close artefacts, matching, match records, restatement) or a clause without the detail an implementation needs (STD-KIND-025, STD-TIME-041). The owner confirmed the semantic choices with the default suggestions on 2026-10-19.
- **Decision:**
  1. **Period transitions (STD-KIND-025).**
     - A period object may be created in any state.
     - An update moves its `state` only between `open` and `closing` (otherwise `MODEL.CONSTRAINT_VIOLATED`). `closing` is a soft close: appends commit, and governance sheets may restrict them.
     - `closed` is entered only by `close_period` and left only by `reopen_period`, which requires a reason recorded with the process and returns the period to `open`.
     - Corrections post in an open period with the valid time of the original.
  2. **Close artefacts (STD-KIND-026, new).** `close_period` closes a period in `closing` in one commit:
     - the period becomes `closed`;
     - a System object of the genesis class `CloseArtefact` holds the period, the close sequence, the tag, `as_of` (`cseq`, `commit_id` and `root` of the commit the close extends) and the balances per ledger key of the scoped classes for the entries posted before the period's end;
     - a signed tag `close-<period slug>-<n>` names the closing commit.

     A reopened period closed again gets a new artefact, and earlier artefacts stay. The seal of STD-PROOF-055 follows with that clause (PH-5).
  3. **Matching (STD-KIND-027, new).** Rules (`DAT-MatchingRule`) are applied in priority order to the candidates of each side in `(valid_at, entry_id)` order:
     - `one_to_one` matches first fit;
     - `one_to_many` and `many_to_one` group all candidates with equal `equal` fields within the date window and match when the sum is within the tolerance;
     - amounts of different currencies never match, and `negate` compares with the opposite sign.

     No subset-sum search is made. The result is the matches with `rule_ref` (the content ID of the rule) and the breaks; the same inputs give the same result.
  4. **Match records (STD-KIND-028, new).**
     - **Writing.** A committed run appends one entry of the genesis ledger class `MatchRecord` per match (key `{match_set}`, with the entries, the rule, `rule_ref` and the actor), with an `entry_id` derived from its content. Commits cannot append match records.
     - **Undoing.** An unmatch appends a record that names the record it undoes (`unmatches`, with an optional reason); both records stay, and the lines become candidates again.
  5. **Restatement (STD-TIME-042, new) and re-evaluation modes (STD-TIME-041).** A restatement takes a correcting commit:
     - it computes its impact set and creates a `correction` branch;
     - it re-runs the producing processes there with their recorded invocation and `asof`, under the chosen mode (default `as_effective`);
     - it reports the processes, those it cannot re-run, and the old and new body of every changed object, flagged `material` by an optional L1 expression over `this` and `old`.

     For logic assets, `as_effective` takes the version covering the original `asof`, `as_originally` the version the original process ran, and `current` the head. The mode is recorded in the derivation records.
  6. **Adapter operations.** `close_period`, `reopen_period`, `match`, `unmatch` and `restate` (STD-CONF-005). The reference model is in `standard/tools/bpa/periods.py`, the cases in `standard/tools/cases/periods.py`.
  7. **Superseded vector.** VER-CONF-1528 closed a period by an update; under item 1 that update fails. It is superseded (DEC-067) and replaced by a vector that closes the period with `close_period` before the merge.
- **Consequences:**
  - 1.0-rc8 has 228 clauses and 1,747 vectors. The new and clarified clauses are covered by 45 new vectors:
    - Ledger: 37;
    - Bitemporal: 8.
  - MUST coverage is 173/190.
  - `crates/` is unchanged. The kernel takes up these clauses in PH-2 step 3, and VER-CONF-1528 drops out of its conformance run as superseded.
- **Alternatives considered:**
  - Allowing an update to close a period was rejected: the close artefact and its tag would be optional, and FR-LEDG-062 requires them.
  - A subset-sum search for many-to-one matches was rejected: its cost is exponential, and the result depends on search order.
  - Seals of close artefacts in 1.0-rc8 were rejected: STD-PROOF-055 is a PH-5 clause.
- **Affects:** STD-KIND-025, STD-KIND-026, STD-KIND-027, STD-KIND-028, STD-TIME-041, STD-TIME-042, STD-CONF-005, FR-LEDG-061, FR-LEDG-062, FR-LEDG-071, FR-LEDG-073, FR-TIME-043, FR-TIME-064, DSN-DVM-322, DSN-DVM-443, DSN-DVM-445, DEC-067, DEC-089

### DEC-091 — Approvals and rules in BPA 1.0-rc8 (PH-2 step 1, part 1c)
- **Date:** 2026-10-19 · **Status:** Accepted
- **Context:** Part 1c of PH-2 step 1 (DEC-089) writes the clauses and vectors of approvals and rules.
  - STD-TXN-060 (deferred commits), STD-LGS-040 (decision tables) and STD-ISA-053 (`run.decision`) existed without the detail an implementation needs.
  - FR-FLOW-041, -043, -045, -046, FR-RULE-071…073 and DSN-DVM-740, -742, -760…762 had no clause.
  - BPA 1.0-rc2 left the pending change of a `require` outcome to PH-2 (STD-LGS-031 notes).

  The owner confirmed the semantic choices with the default suggestions on 2026-10-19.
- **Decision:**
  1. **Deferred commits (STD-TXN-060).** A port call with `defer` whose decisions require steps stores its changes as a pending change instead of failing; without `defer` it still fails with `RULE.DECISION_REQUIRES` (released behaviour).
     - **Record.** The pending change is identified by the requesting process. It holds the changes with their bases, the preview (content ID of `{branch, changes}`), the requirements and the approvals.
     - **Expiry and invalidation.** At each read or approval it expires at `expires_at`. It is invalidated when a base is no longer the head, and the voided approvals and the changed bodies stay visible.
     - **Application.** The final approval commits the changes in a new process with the requester as initiator. A failure of that commit invalidates the pending change, and a rejection ends it.
  2. **Approval requirements (STD-LGS-041, new).** The kernel property `approvals` gains `principals`, `incompatible` and `expires_in` beside `count`, `roles` and `order`.
     - **Eligibility.** Eligibility and sequence are checked per approval (`IAM.DENIED` with details).
     - **Steps.** `approval` needs `count` approvals, `second_person` one, and `signature` one signed approval.
  3. **Separation of duties (STD-LGS-042, new).** The approver and the delegator are each checked against three rules:
     - maker–checker;
     - one decision per principal and pending change;
     - the incompatible duties of the requirement: a committed call of a listed port on the same object.

     A breach is denied with `IAM.DENIED` and `details.sod`; no new error code.
  4. **Approval evidence (STD-LGS-043, new).** One `DAT-Approval` item serves pending changes and change sets: approver, `on_behalf_of`, decision, preview, time, authentication level, comment and signature. Change-set approvals record `auth_level` and `comment` when given, so their released results are unchanged. The applying commit references the pending change, its preview and the approvers.
  5. **Decision tables (STD-LGS-040, STD-ISA-053).** Tables are objects of the genesis class `DecisionTable` with typed inputs and outputs and rows of L1 conditions over `params` (`-` for any).
     - **Hit policies.** `unique` (`RULE.NO_MATCH`, `RULE.MULTIPLE_MATCH`), `first`, `priority` (the highest `priority`, first of ties) and `collect` (a list, or `sum`, `min`, `max`, `count`).
     - **Version.** A table is evaluated in its version valid at `asof`. `run.decision` records the table versions in the rule binding (`tables`).
     - **Authoring check.** It enumerates the representative values of the boolean, enum, integer, decimal and date inputs: domains, compared literals, one unit around them and midpoints. It reports gaps and the overlaps the policy forbids, and lists other inputs as unchecked.
  6. **Simulation (STD-LGS-044, new).** The historical port calls of a target branch, selected by a window and an optional port, are re-run in commit order up to a budget of processes.
     - **Re-run.** Each runs in the `simulation` profile on a sandbox branch created as of just before its commit, with the candidate sheets, property declarations and decision tables of the source branch.
     - **Effects.** No effects are released.
     - **Comparison.** The outcome (`committed` with the result hash, or `failed` with the error code) is compared with the actual one.
     - **Coverage.** It states the calls covered and selected, completeness and the dates covered.
     - **Record.** The run is stored as an object of the genesis System class `SimulationRun` and linked from the change set given.
  7. **Adapter operations.** `decide_table`, `check_table`, `pending_change`, `approve` and `simulate`, and `execute` with `defer` (STD-CONF-005). The reference model is in `standard/tools/bpa/approvals.py` and `tables.py`, the cases in `standard/tools/cases/approvals.py`.
- **Consequences:**
  - 1.0-rc8 has 232 clauses and 1,800 vectors.
  - Part 1c adds 53 vectors:
    - Governance: 50;
    - Core (STD-ISA-053): 3.
  - MUST coverage is 179/194.
  - No released vector changed. `crates/` is unchanged; the kernel takes up these clauses in PH-2 steps 4 and 5.
- **Alternatives considered:**
  - Making `require` defer by default was rejected: released vectors and clients expect `RULE.DECISION_REQUIRES`.
  - A new SoD error code was rejected: `IAM.DENIED` with details gives the explanation FR-FLOW-043 asks for.
  - Analysing arbitrary conditions symbolically was rejected: sampling at the compared literals finds the gaps and overlaps of range and enumerable inputs, and other inputs are reported as unchecked rather than guessed.
  - Simulating by replaying journals with substituted rules was rejected for 1.0-rc8: the journal pins the original reads and rule bindings, so the candidate rules would not be evaluated. Re-running on a sandbox as of the original snapshot evaluates them.
- **Affects:** STD-TXN-060, STD-LGS-011, STD-LGS-031, STD-LGS-040, STD-LGS-041, STD-LGS-042, STD-LGS-043, STD-LGS-044, STD-ISA-053, STD-VER-021, STD-TIME-040, STD-CONF-005, FR-FLOW-041, FR-FLOW-042, FR-FLOW-043, FR-FLOW-044, FR-FLOW-045, FR-FLOW-046, FR-RULE-041, FR-RULE-042, FR-RULE-071, FR-RULE-072, FR-RULE-073, DSN-DVM-740, DSN-DVM-742, DSN-DVM-760, DSN-DVM-761, DSN-DVM-762, DEC-089

### DEC-092 — Workflows, sagas, quarantine, lenses and upstream changes in BPA 1.0-rc8 (PH-2 step 1, part 1d)
- **Date:** 2026-10-24 · **Status:** Accepted
- **Context:** Part 1d of PH-2 step 1 (DEC-089) writes the clauses and vectors of long-running processes and schema evolution.
  - STD-TXN-080 (sagas), STD-CTX-031 (quarantine), STD-CLS-050, -060…062 (change classification and lenses) and STD-VER-082 (upstream changes) existed without vectors or the detail an implementation needs.
  - FR-FLOW-061, -062, -064 and DSN-DVM-750…753 (workflows) had no clause.
  - STD-TXN-051 (pre-commit external calls) moves to part 1f with `call.connector`.

  The owner confirmed the semantic choices with the default suggestions on 2026-10-24.
- **Decision:**
  1. **Workflows (STD-TXN-090…092, new).**
     - **Definitions.** A definition is a versioned object of the genesis class `WorkflowDefinition` with typed variables, a start step and steps of the eight types of FR-FLOW-061. Conditions, parameters and keys are L1 expressions over the variables.
     - **Instances.** An instance is an object of the genesis System class `WorkflowInstance`, pinned to the definition version current at its start and advanced only by the workflow operations. Each step is one committed state change.
     - **Idempotency.** An action's port call is idempotent by `(instance, step, visit, attempt)`; the visit counts entries into a step, so loops keep distinct identities. A join fires once, when the last branch of its parallel step arrives.
     - **Events.** An event wakes at most one step waiting for its type and correlation key, the one waiting longest; an unmatched event is buffered and delivered to a step that starts waiting within its declared `buffer` window (FR-FLOW-064). A repeated event ID returns the first result.
     - **Timers and tasks.** Timers fire when the clock reaches their due time. A human task is completed by its assignee; completing the same `(instance, step, visit)` again returns the first result.
     - **Migration.** A running instance continues on its pinned version. Moving it to another version maps every waiting step and every join not yet fired to a step of the same type; a missing mapping fails with `MODEL.MIGRATION_REQUIRED`.
  2. **Sagas (STD-TXN-080).** A saga is an object of the genesis class `Saga`; its state members are written by its execution only (`FLOW.STATE_DIRECT_WRITE`).
     - **Steps.** Each step is a port call run as its own process and commit, linked by `saga`. A limit termination is a timeout.
     - **Recovery.** Backward recovery compensates the completed steps in reverse order; forward recovery retries a step up to `max_attempts` and then ends the saga `failed`.
     - **Durability.** Every state transition is committed; a resumed saga does not run again a step whose process committed. Crashes are simulated by `interrupt_after`.
  3. **Quarantine (STD-CTX-031).** A per-VAE policy `{threshold, window, releasers}` counts the failures and limit terminations of a process's entry asset version in the trailing window. Reaching the threshold creates a `Quarantine` System object and releases an `AssetQuarantined` effect; calls of the version then fail fast with `LOGIC.ASSET_QUARANTINED`. A listed principal releases it with a reason (`QuarantineRelease`).
  4. **Change classification (STD-CLS-050).** The kernel computes the classification of a class change made by `change_class` from the effective fields, the ports and the stored current versions. A lens-required change needs a total lens (`MODEL.LENS_REQUIRED`), and a breaking change is refused (`MODEL.MIGRATION_REQUIRED`). Loading genesis declarations is not a class change.
  5. **Lenses (STD-CLS-060…062).**
     - **Operations.** The nine operations are defined in both directions; expressions are L1 over the body. `drop_with_archive` keeps values in `_archived`. L2 residual functions are not available in 1.0-rc8.
     - **Reads.** Reads return the current schema version by default and lens along the chain of schema versions; the result is not stored.
     - **Writes.** A change in an older schema version is lensed forward; an update's patch applies to its base seen in that version. Merges align base, source and target to the current version.
     - **Totality check.** Round trips and target conformance are checked over generated values of the touched fields and the stored current versions (at most 10,000), with up to ten failures reported.
  6. **Upstream changes (STD-VER-082).** The first own version of an inherited object records the parent version it was based on. An override whose parent-line head differs is reported with the versions before and after. A resolution by acknowledgement or by a three-way merge (STD-VER-050) records the parent head as the new base.
  7. **Adapter operations.** `start_workflow`, `deliver_event`, `complete_task`, `migrate_workflow`, `run_saga`, `quarantine_policy`, `release_quarantine`, `change_class`, `classify_change`, `apply_lens`, `check_lens`, `upstream_changes` and `resolve_upstream`. `read` and `commit` take `schema_version` (STD-CONF-005).
     - **Reference model.** `standard/tools/bpa/workflows.py` and `lenses.py`.
     - **Cases.** `standard/tools/cases/workflows.py` and `evolution.py`.
- **Consequences:**
  - 1.0-rc8 has 235 clauses and 1,922 vectors.
  - Part 1d adds 122 Core vectors.
  - MUST coverage is 189/197.
  - No released vector changed. `crates/` is unchanged; the kernel takes up these clauses in PH-2 steps 4 and 5.
- **Alternatives considered:**
  - New error codes for a task that does not wait and an incomplete migration were rejected: `FLOW.TRANSITION_NOT_ALLOWED` and `MODEL.MIGRATION_REQUIRED` state the conditions, and the error registry and the SDKs stay unchanged.
  - Classifying changes as declared by the author was rejected: STD-CLS-050 requires a computed classification.
  - Treating `load_genesis` as a class change was rejected: released vectors reload declarations freely.
  - Lensing reads only on request was rejected: a body in an older schema version does not conform to the current class.
- **Affects:** STD-TXN-080, STD-TXN-090, STD-TXN-091, STD-TXN-092, STD-CTX-031, STD-CLS-050, STD-CLS-060, STD-CLS-061, STD-CLS-062, STD-VER-082, STD-CONF-005, FR-FLOW-061, FR-FLOW-062, FR-FLOW-064, FR-TXN-061, FR-TXN-062, FR-TXN-063, FR-LOGIC-064, FR-MODEL-081, FR-MODEL-082, FR-MODEL-083, FR-MODEL-084, FR-MODEL-085, FR-MODEL-086, FR-VER-086, DSN-DVM-750, DSN-DVM-751, DSN-DVM-752, DSN-DVM-753, DEC-089

### DEC-093 — Protection in BPA 1.0-rc8 (PH-2 step 1, part 1e)
- **Date:** 2026-10-24 · **Status:** Accepted
- **Context:** Part 1e of PH-2 step 1 (DEC-089) writes the clauses and vectors of protection.
  - STD-OBJ-010 (encrypted fields) and STD-OBJ-011 (erased fields) entered 1.0-rc8 without vectors.
  - FR-IAM-051…053, -061…063 and DSN-DVM-810…812, -821, -822 had no clause.

  The owner confirmed the semantic choices with the default suggestions on 2026-10-24.
- **Decision:**
  1. **Encrypted fields (STD-OBJ-010).**
     - **Scope.** A class that declares `subject_key` stores its `personal` and `sensitive_personal` fields as `DAT-EncryptedField` items under AES-256-GCM. `subject_key` is `self` or an unprotected text or reference field; a referenced subject is the object ID. A class without `subject_key` stores its fields as they are, so released declarations keep their behaviour.
     - **Hashes.** Version hashes, trees and archives cover the stored form; reads, rules and logic see the plaintext.
     - **Keys.** Subject keys belong to the key service, outside business data and archives. They are created on first use and rotated by `subject_key`; earlier versions stay readable.
     - **Evidence.** `disclose` returns the value and salt of a field commitment, and `verify_disclosure` checks them.
     - **Naming.** The commitment member is renamed from `commit` to `commitment`, which no implementation used: `commit` is a member of released results. `nonce`, `ct`, `salt` and `commitment` become implementation-dependent members (STD-CONF-005, rule 6).
  2. **Erasure (STD-OBJ-011).** `erase_subject` destroys a subject key. Every version encrypted under it reads with erased markers, and hashes and proofs are unchanged. A disclosure or a new protected value for the subject fails with `PROOF.VALUE_ERASED`; other fields stay writable. An unavailable key fails reads with `IAM.KEY_UNAVAILABLE` (DSN-DVM-834). Checking legal holds before erasure stays in PH-3 (DSN-DVM-831).
  3. **Authorization relations (STD-LGS-050, new).** Reference fields declare `authz_relation`, and classes declare `principal_field`. Relations are derived from the versions at the snapshot of the request, with no separate tuple store.
  4. **Access rules (STD-LGS-051, new).**
     - **Rules.** The kernel property `access` (union) holds rules `{action, allow_if}` with paths `principal in this(.relation)*` of at most four relations; other paths fail with `RULE.PARSE_ERROR` at commit.
     - **Enforcement.** Where rules apply to an action, one must reach the principal. Reads made as a viewer fail with `IAM.NOT_FOUND` and port calls with `IAM.DENIED`. A query made as a viewer leaves out the objects the viewer may not read, inside its execution.
     - **Unrestricted actions.** Actions without rules are not restricted, so released vectors are unchanged.
  5. **Masking (STD-LGS-052, new).** A viewer `{principal, clearances, ctx}` given as `as` to `read`, `query` and `outbox` sees:
     - declared masks in their modes: `hide`; `partial`, the last four characters; `hash`, keyed by the tenant lookup key;
     - fields above its clearances hidden.

     Filters and sorts on masked fields fail with `QRY.INVALID_QUERY`. A read without a viewer is the system read.
  6. **Adapter operations.** `subject_key`, `key_availability`, `erase_subject`, `stored_form`, `disclose`, `verify_disclosure`, `lookup_key`, `relations` and `check_access`; `as` on `read`, `query` and `outbox` (STD-CONF-005). The reference model is in `standard/tools/bpa/protection.py`, the cases in `standard/tools/cases/protection.py`.
- **Consequences:**
  - 1.0-rc8 has 238 clauses and 1,965 vectors.
  - Part 1e adds 43 vectors:
    - Core: 15;
    - Governance: 28.
  - MUST coverage is 194/200.
  - No released vector changed. `crates/` is unchanged; the kernel takes up these clauses in PH-2 steps 4 and 5.
- **Alternatives considered:**
  - Encrypting the personal fields of every class was rejected for 1.0-rc8: released declarations have no subject and their vectors would change. A class change can add `subject_key`.
  - A separate relation tuple store was rejected (DSN-DVM-810): derived tuples cannot drift from the data.
  - Masking every adapter read was rejected: the adapter's reads are system reads, and a viewer must be named for its clearances and context.
  - Reporting a node without the subject keys as erased was rejected: only key destruction erases, so a missing key reads as unavailable.
- **Affects:** STD-OBJ-010, STD-OBJ-011, STD-OBJ-012, STD-LGS-011, STD-LGS-050, STD-LGS-051, STD-LGS-052, STD-CONF-005, FR-IAM-051, FR-IAM-052, FR-IAM-053, FR-IAM-061, FR-IAM-062, FR-IAM-063, FR-PROOF-071, FR-PROOF-072, FR-PROOF-073, DSN-DVM-810, DSN-DVM-811, DSN-DVM-812, DSN-DVM-821, DSN-DVM-822, DSN-DVM-830, DSN-DVM-834, DEC-089

### DEC-094 — Integration and addressing in BPA 1.0-rc8 (PH-2 step 1, part 1f)
- **Date:** 2026-10-24 · **Status:** Accepted
- **Context:** Part 1f of PH-2 step 1 (DEC-089) writes the clauses and vectors of integration and addressing. STD-ISA-071, -090, STD-TXN-051 (moved from part 1d by DEC-092), STD-ADDR-003, -004 and STD-PROTO-021 entered 1.0-rc8 without vectors. The owner confirmed the semantic choices with the default suggestions on 2026-10-24.
- **Decision:**
  1. **Connectors (STD-ISA-090).**
     - **Objects.** A connector is an object of the genesis class `Connector`: endpoint, a credential handle, operations with request and response schemas, `now`, compensation and redactions, retry, breaker, rate limit and the classifications allowed out.
     - **Egress.** Hosts outside the VAE's allow-list are refused at commit and at call.
     - **Callers.** Assets name the connectors they may call among their capabilities as `connector:<uri>`. A new field of `Logic` was rejected because it changes the class declaration and with it released verification records.
     - **Calls.** A call checks the asset, `invoke`, the schema, classifications and egress. `now` calls retry unavailable endpoints, respect the breaker and the rate limit, and are recorded with redactions and journaled for replay. `after_commit` calls release an effect.
     - **Outside world.** It is scripted with `connector_stub`; the reference model makes no network call.
  2. **Compensation (STD-TXN-051).** A failed process compensates its successful `now` calls in reverse order, each under the key `comp:<process>:<seq>`. Each compensation is recorded in the process record and the exchanges. One that cannot be made is queued in the outbox as `connector.compensation`, so it runs at least once.
  3. **Notifications (STD-ISA-071).** Templates are objects of the genesis class `NotificationTemplate`. Recipients are DIDs, roles from the adapter's directory (`principal`) or relations. After commit, one `notification` effect per recipient and channel is rendered with the recipient's masking.
  4. **Addressing (STD-ADDR-003, -004).** `resolve` and `resolve_path` accept short forms and VFS paths. Asset paths match `name` = `<namespace>.<name>`. Reference values in changes stay canonical-only, because VER-CONF-0849 requires it.
  5. **REST facade (STD-PROTO-021).** The `rest` operation maps GET, query and POST onto the caller's UBTP session, carries `Idempotency-Key`, and returns RFC 9457 problems with the registry's HTTP status.
  6. **Adapter operations.** `egress_allow`, `connector_stub`, `exchanges`, `principal`, `resolve_path` and `rest`, and the instructions `call.connector` and `emit.notify` in `execute` (STD-CONF-005).
     - **Reference model.** `standard/tools/bpa/integration.py`.
     - **Cases.** `standard/tools/cases/integration.py`.
- **Consequences:**
  - 1.0-rc8 keeps 238 clauses and has 2,016 vectors.
  - Part 1f adds 51 vectors:
    - Core: 41;
    - Protocol: 10.
  - MUST coverage is 198/200; STD-CLS-023 and STD-ISA-083 follow in part 1g.
  - No released vector changed. `crates/` is unchanged; the kernel takes up these clauses in PH-2 steps 6 and 7.
- **Alternatives considered:**
  - Accepting short forms in reference values was rejected: VER-CONF-0849 requires them to be refused, and storage needs canonical URIs.
  - New error codes for an open breaker and a rate limit were rejected: `LOGIC.CONNECTOR_UNAVAILABLE` is the retryable code FR-LOGIC-105 names, with the reason in `details`.
  - Real network calls in the reference model were rejected: vectors must be deterministic and offline.
- **Affects:** STD-ISA-071, STD-ISA-090, STD-TXN-051, STD-ADDR-003, STD-ADDR-004, STD-PROTO-021, STD-CONF-005, FR-LOGIC-101, FR-LOGIC-102, FR-LOGIC-103, FR-LOGIC-104, FR-LOGIC-105, FR-TXN-053, FR-NTF-011, FR-NTF-012, FR-FILE-041, FR-INT-021, FR-INT-023, DEC-089, DEC-092

### DEC-095 — Search and views in BPA 1.0-rc8 (PH-2 step 1, part 1g)
- **Date:** 2026-10-24 · **Status:** Accepted
- **Context:** Part 1g of PH-2 step 1 (DEC-089) writes the clauses and vectors of search and views. STD-ISA-081…083 and STD-CLS-023 entered 1.0-rc8 without vectors, and the three instructions had no parameters in their descriptors. The owner confirmed the semantic choices with the default suggestions on 2026-10-24.
- **Decision:**
  1. **Descriptors.** `search.aggregate`, `search.traverse` and `search.text` declare their parameters (and the last two their results and errors). This changes the catalogue of ABI 1.0: VER-CONF-3002 is superseded by VER-CONF-3003, and the kernel's `isa.json` is regenerated from the clauses.
  2. **Aggregates (STD-ISA-081).**
     - **Functions.** `count`, `sum`, `avg`, `min`, `max` and `percentile` (nearest rank).
     - **Rows.** Only the rows the principal may read count; masked fields are refused.
     - **Money.** It is grouped by currency unless `convert` gives rates; a missing rate fails with `MODEL.CURRENCY_MISMATCH`.
     - **Rounding.** Fixed scales, half-even.
  3. **Traversal (STD-ISA-082).** Breadth-first over references, out, in or both, to depth 8, with field and class filters and a limit. Each node is authorised, and nodes that may not be read are neither returned nor expanded.
  4. **Text search (STD-ISA-083).**
     - **Index.** Fields are declared `text_index`; analysis is English with plural stemming.
     - **Results.** Ranked by matched words and frequency, with `<em>` snippets; masked fields are not searched.
     - **Watermark.** It is reported with `lagging`, and `index_lag` freezes the reference index.
     - **Journal.** The result is journaled for replay.
  5. **Views (STD-CLS-023).** Views are declared per mode with context variants and resolved along the linearization, extensions included; the best-matching variant wins. Otherwise the view is generated from the effective definition, field visibility and the lifecycle state. Views are objects of the genesis class `View`.
  6. **Principals.** The search instructions evaluate as the process principal, with its clearances from the adapter's directory (default `internal`).
  7. **Adapter operations.** `resolve_view` and `index_lag` (STD-CONF-005).
     - **Reference model.** `standard/tools/bpa/search.py`.
     - **Cases.** `standard/tools/cases/search.py`.
- **Consequences:**
  - 1.0-rc8 keeps 238 clauses and has 2,053 vectors, with VER-CONF-3002 superseded.
  - Part 1g adds 38 Core vectors (VER-CONF-3003 among them) and supersedes one.
  - MUST coverage is 200/200.
  - `crates/` changes only in the regenerated `isa.json`; the kernel takes up these clauses in PH-2 steps 9 and 10.
- **Alternatives considered:**
  - Passing the arguments inside a query object, as `search.query` does with `limit` and `cursor`, was rejected: traversal and text search have no query object, and undeclared parameters cannot be validated.
  - Ranking by BM25 was rejected for the standard: exact integer scores keep results identical across implementations; implementations may rank better inside equal scores only when the standard allows it in a later release.
- **Affects:** STD-ISA-001, STD-ISA-081, STD-ISA-082, STD-ISA-083, STD-CLS-023, STD-CONF-005, STD-CONF-030, FR-QRY-021, FR-QRY-022, FR-QRY-023, FR-QRY-041, FR-QRY-042, FR-MODEL-036, FR-MODEL-065, FR-UX-012, FR-UX-015, DEC-089
