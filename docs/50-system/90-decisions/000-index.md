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
| DEC-096 | PH-2 step 1h: release of BPA 1.0-rc8 — notes finalised, VER-INSP-9006, the owner's signed tag `bpa-v1.0.0-rc8`, freeze; the candidate part of EXIT-2-01 | Accepted |
| DEC-097 | PH-2 step 2: extended formal models — approvals and deferred commits (`approvals.tla`, VER-FORM-0100) and sagas (`saga.tla`, VER-FORM-0101) with bounds and mutation checks; terminal state `failed` in DSN-DVM-566 and VER-FORM-0101, saga object wording in DSN-DVM-564; EXIT-2-02 | Accepted |
| DEC-098 | PH-2 step 3 opened in parts 3a–3g; part 3a: BPA 1.0-rc9 — currency reference data (STD-FND-028), FX rates and `fx_convert` (STD-FND-029), chart of accounts (STD-KIND-029), statement lines (STD-KIND-030), projections (STD-TIME-021), `read.projection` (STD-ISA-037), compensation proposals (STD-TIME-043); VER-CONF-3003 superseded, VER-INSP-9007, the owner's signed tag `bpa-v1.0.0-rc9` | Accepted |
| DEC-099 | PH-2 step 3b: currencies, FX rates and accounts in the kernel — ISO 4217 table and added codes, `Currency`, `FxRate` and `LedgerAccount` checks, `fx_convert` and `convert` against a branch with the rates journaled, adapter operations `currency` and `fx_rate`; the conformance test claims STD-FND-028, -029 and STD-KIND-029 of BPA 1.0-rc9 | Accepted |

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

### DEC-096 — Release of BPA 1.0-rc8 (PH-2 step 1, part 1h)
- **Date:** 2026-10-24 · **Status:** Accepted
- **Context:** Part 1h of PH-2 step 1 (DEC-089) releases BPA 1.0-rc8. After parts 1a–1g (DEC-089…095) the release holds 238 clauses and 2,053 vectors, and all 200 MUST clauses with a vector range are covered (`standard/release/1.0-rc8/coverage.md`). The owner confirmed the plan of part 1h with the default suggestions on 2026-10-24.
- **Decision:**
  1. **Release notes.** `standard/release/1.0-rc8/notes.md` is finalised by hand as DEC-045 item 1 prescribes: content, changes since 1.0-rc7 by part, the two superseded vectors and the known limits. The builder lists it in the manifest.
  2. **Pull request and tag.** The commits of PH-2 steps 0b-4 and 1a–1h are merged through one pull request. After merge the owner creates the signed tag `bpa-v1.0.0-rc8` on the merge commit, with the manifest `content_id` in the tag message (DSN-BPA-001, DEC-030).
  3. **Freeze.** Once tagged, BPA 1.0-rc8 is frozen like the earlier candidates (DEC-045 item 3): its clauses and vectors are not changed, and the output of `regen_all.sh` must stay identical until a decision opens the next release. Defects are recorded in `standard/errata/`.
  4. **Verification item.** VER-INSP-9006 records the coverage inspection of 1.0-rc8.
- **Consequences:**
  - The candidate part of EXIT-2-01 is met when the tag exists; its kernel part (100% of the vectors on PostgreSQL, VER-CONF-8405) follows the later PH-2 steps.
  - PH-2 step 1 is complete; step 2 (extended formal models: approvals and sagas, EXIT-2-02) is next.
- **Alternatives considered:**
  - Tagging BPA 1.0.0 now was rejected: the kernel has not run the 1.0-rc8 vectors, so defects may still be found and corrected in a further candidate.
  - Waiting for the kernel before tagging was rejected: the standard precedes the code (AR-033), and the kernel steps implement a frozen text.
- **Affects:** DSN-BPA-001, EXIT-2-01, VER-INSP-9006, DEC-045, DEC-089

### DEC-097 — Extended formal models: approvals and sagas (PH-2 step 2)
- **Date:** 2026-10-24 · **Status:** Accepted
- **Context:** PH-2 step 2 checks the extended formal models of EXIT-2-02: VER-FORM-0100 (approvals and deferred commits) and VER-FORM-0101 (sagas). BPA 1.0-rc8 fixed their semantics (STD-TXN-060, STD-VER-021, STD-LGS-041, -042 in DEC-091; STD-TXN-080 in DEC-092). VER-FORM-0101 and DSN-DVM-566 required every started saga to end completed or compensated, which STD-TXN-080 no longer guarantees. The owner confirmed the plan of step 2 with the default suggestions on 2026-10-24.
- **Decision:**
  1. **Approvals model.** `verify/models/approvals.tla` models editors and approvers who edit objects and make proposals, pending changes with fixed previews and recorded bases, change sets whose preview moves with each source commit, decisions with the preview they saw, separation of duties (maker, one decision per principal, incompatible duties from committed calls), expiry, rejection, and apply with re-validation. Properties: `NoCommitWithoutApprovals`, `StaleNeverCount`, `SoDHolds`, `NoLostUpdate`, `AppliedOnce`, checked on the history of applying commits.
  2. **Saga model.** `verify/models/saga.tla` models one saga with a coordinator that commits every state transition, step processes that commit effect and record atomically, outcomes ok, fail and timeout, backward and forward recovery, failing compensations, and coordinator crashes, with weak fairness on progress. Properties: `AtMostOnce`, `CompAfterExec`, `ReverseOrder`, `CompletedOK`, `CompensatedOK`, `FailedJustified`, `EndsCompletedOrCompensated` and the liveness property `Termination`.
  3. **Bounds.** VER-FORM-0100 at 2 approvers × 3 editors × 3 objects, 2 proposals, object versions up to 2 and one source commit per change set; VER-FORM-0101 at 3 steps, 2 attempts and 2 crashes; the rule of DEC-031 applies. CI runs 2 approvers × 2 editors × 2 objects and 2 steps with 1 crash.
  4. **Sensitivity.** A constant `Mutation` selects five wrong variants per model; each must violate its target property (`verify/models/reports/README.md` §6.3).
  5. **Design text.** DSN-DVM-566 and VER-FORM-0101 state the terminal state `failed`: a saga ends completed, compensated or failed, fails only when a compensation fails or forward recovery is exhausted, and ends completed or compensated under backward recovery with compensations that do not fail. DSN-DVM-564 calls the saga an object of the genesis class `Saga` whose state the execution alone writes, as STD-TXN-080 does. VER-FORM-0100 also verifies STD-LGS-041 and -042.
- **Consequences:** EXIT-2-02 is met by the runs recorded in `verify/models/reports/README.md` §6; no property was violated. The kernel steps of PH-2 implement approvals and sagas against these properties, and the 1.0-rc8 vectors (VER-CONF-2225…2244 and the approvals ranges) exercise the same rules on PostgreSQL.
- **Alternatives considered:**
  - Keeping the stronger saga property and dropping `failed` from STD-TXN-080 was rejected: a compensation that fails cannot be made to succeed, and 1.0-rc8 is frozen (DEC-096).
  - Modelling preview hashes as content hashes was rejected: the base or source revision identifies the preview exactly and keeps the state space checkable.
  - One model for approvals and sagas was rejected: they share no state, and separate models keep the bounds of each item checkable.
- **Affects:** VER-FORM-0100, VER-FORM-0101, EXIT-2-02, DSN-DVM-564, DSN-DVM-566, STD-TXN-060, STD-VER-021, STD-LGS-041, STD-LGS-042, STD-TXN-080, DEC-031, DEC-091, DEC-092

### DEC-098 — Money and periods, part 3a: BPA 1.0-rc9 (PH-2 step 3)
- **Date:** 2026-10-24 · **Status:** Accepted
- **Context:** PH-2 step 3 (UBS-PH-2 §11) implements money and periods in the kernel. A run of the 121 money and period vectors of BPA 1.0-rc8 on the kernel passed 78 (calendars, money, double entry, retroactive changes and impact sets). The other 43 need period transitions, close artefacts, matching and restatement. Several capabilities of the step had no clauses: currency reference data, FX rate objects, the chart of accounts, declared projections with their freshness and rebuild, idempotent statement ingestion and compensation proposals (DSN-DVM-323, -409, -410, -420, -422, -441, -446). The standard precedes the code (AR-033). The owner confirmed the plan of step 3 with the default suggestions on 2026-10-24.
- **Decision:**
  1. **Parts.** Step 3 is done in parts, one pull request each. 3a writes BPA 1.0-rc9 (standard only). 3b–3f implement in the kernel:
     - 3b: currencies, FX rates and accounts;
     - 3c: projections and their rebuild, with a storage migration;
     - 3d: periods, close and reopen;
     - 3e: matching and statement ingestion;
     - 3f: re-evaluation, restatement and proposals.

     3g claims the clauses in the kernel's conformance test, runs them on PostgreSQL and closes the step. Bank file formats stay in step 12, projection backfill in step 6, and the close cockpit in step 15.
  2. **Release.** BPA 1.0-rc9 is opened with the scope rule of DEC-089 on base 1.0-rc8 (`standard/release/1.0-rc9/scope.json`). It is released at the end of part 3a with the owner's signed tag `bpa-v1.0.0-rc9`, and is then frozen like the earlier candidates (DEC-045 item 3). 1.0-rc8 stays frozen as built.
  3. **Clauses.**
     - **STD-FND-028.** The ISO 4217 table with minor units, normative in the clause. `Currency` objects add codes that are not in the table.
     - **STD-FND-029.** `FxRate` objects keyed by `(source, base, quote, rate_type)`, whose version valid at a date gives the rate of that date. `fx_convert(m, quote, rate_type, source?)` looks the rate up at the evaluation's `asof`. It uses the direct rate, or else the inverse computed exactly, and rounds half-even to the quote's minor units. An ambiguous source fails with `MODEL.INVALID_VALUE` and a missing rate with `MODEL.CURRENCY_MISMATCH`. `convert(m, cur, rate_ref)` against a branch reads an `FxRate` object. The journal records the rate versions used.
     - **STD-KIND-029.** The genesis class `LedgerAccount`; a ledger class names its `account` field. A posting needs an account that is live, a leaf and in the account's currency at the entry's valid time, otherwise `MODEL.CONSTRAINT_VIOLATED` with the reason. Parent cycles fail.
     - **STD-KIND-030.** `ledger.statement` names source, account, statement and line. The entry id is derived from them (`ubos.statement.v1`). The adapter operation `ingest_statement` skips duplicates and fails a changed line with `TXN.IDEMPOTENCY_MISMATCH`.
     - **STD-TIME-021.** The genesis class `Projection` (`count`, `sum`, `min`, `max`, `last` over the entries of one ledger class). Reads equal the fold of the included entries at any branch and time. `sync` and `async` maintenance with a watermark, `at_least` and `QRY.WATERMARK_TIMEOUT`. A rebuild equals the fold at the head.
     - **STD-ISA-037.** `read.projection`, with `rollup` over the account hierarchy at the `asof` date.
     - **STD-TIME-043.** The `commit.append` changes of a re-run process are its restated postings and are not committed. `proposals` reports the differences per ledger key and currency, dated in the open period (`open_period`, default) or at the original valid time (`original`).
  4. **Deviation from the plan.** The plan named the lookup function `convert(m, quote, rate_type, source?)`. ABI 1.0 already defines `convert(m, cur, rate_ref)` with a rate reference in the third position, so the lookup by key is the new function `fx_convert`.
  5. **Consequences for released text.**
     - `read.projection` changes the instruction catalogue: VER-CONF-3003 is superseded by VER-CONF-3004, and the kernel's `isa.json` is regenerated.
     - The error registry gains `QRY.WATERMARK_TIMEOUT`.
     - `DAT-Journal` gains `rates`, and `DAT-ClassDecl` describes `ledger.account` and `ledger.statement`.
     - The genesis class names avoid the classes of the released vectors (a test class `Account` exists, hence `LedgerAccount`).
  6. **Reference model.**
     - **Code.** `standard/tools/bpa/finance.py` and changes to `values.py`, `expr.py`, `exec.py`, `periods.py`, `classes.py` and `store.py`.
     - **Cases.** `standard/tools/cases/finance.py`.
     - **Vectors.** Projection vectors run on a feature branch, because `Projection` is a configuration asset and a protected branch takes it through change sets.
- **Consequences:**
  - BPA 1.0-rc9 has 245 clauses and 2,125 vectors (73 added, VER-CONF-3003 superseded). MUST coverage is 207/207, and VER-INSP-9007 records it.
  - No released vector changed.
  - Parts 3b–3g implement 1.0-rc9 and the money and period clauses of 1.0-rc8 in the kernel.
- **Alternatives considered:**
  - Materialising the ISO table as genesis `Currency` objects was rejected: every state root of the released vectors would change.
  - Overloading `convert` was rejected: its third argument is a rate reference in released vectors (VER-CONF-4540…4699).
  - Committing the restated appends on the correction branch was rejected: entries are immutable, and the originals would be counted twice on merge.
  - Detecting a changed statement line by a new entry id (content hash in the identifier) was rejected: a corrected redelivery would silently add a second line.
- **Affects:** STD-FND-023, STD-FND-028, STD-FND-029, STD-KIND-029, STD-KIND-030, STD-TIME-021, STD-TIME-042, STD-TIME-043, STD-ISA-001, STD-ISA-037, STD-EXPR-005, STD-CTX-011, STD-ERR-002, STD-CONF-005, DSN-DVM-323, DSN-DVM-409, DSN-DVM-410, DSN-DVM-420, DSN-DVM-422, DSN-DVM-441, DSN-DVM-446, FR-LOC-021, FR-LOC-022, FR-LEDG-031, FR-LEDG-033, FR-LEDG-052, FR-LEDG-053, FR-LEDG-074, FR-ANL-021, FR-ANL-023, FR-TIME-045, VER-INSP-9007, DEC-045, DEC-089

### DEC-099 — Currencies, FX rates and accounts in the kernel (PH-2 step 3, part 3b)
- **Date:** 2026-10-24 · **Status:** Accepted
- **Context:** Part 3b of PH-2 step 3 (DEC-098) implements STD-FND-028, STD-FND-029 and STD-KIND-029 of BPA 1.0-rc9 in the kernel. Before the part, the 28 vectors of these clauses failed on the kernel except one: the genesis classes, the table, the checks and the conversion were missing. The owner confirmed the part with the default suggestions on 2026-10-24.
- **Decision:**
  1. **Table and added codes.** `ubos-types` holds the ISO 4217 table of STD-FND-028 (`ISO_4217`, `table_unit`). `minor_unit` reads the table first, then the codes added on the branch of the commit being made. Those codes are set for the duration of the commit, with a guard that restores the previous set. The engine loads the live `Currency` objects only when the changes carry a money value outside the table or write a currency, rate or account, so other commits pay nothing.
  2. **Genesis classes.** `ubos-model` declares `Currency`, `FxRate` and `LedgerAccount` as Definition classes below `Object`, as the reference model does.
  3. **Commit checks.** `crates/ubos-dvm/src/finance.rs` runs after the double-entry check and before the relationship and integrity checks, in the order of the reference model. It covers:
     - the code, numeric code, minor units and uniqueness of a `Currency`;
     - the currencies, base and quote, positive rate and unique key of an `FxRate`;
     - the currency and parent cycle of a `LedgerAccount`;
     - for entries of classes with `ledger.account`, an account that is live, a leaf and in the entry's currency at its valid time.

     The error details name the reason, as in the vectors.
  4. **Conversion.**
     - **`ubos-expr`.** `fx_convert` and the rate lookup are added: the key's rate, or the opposite key's rate applied as a division at 60 digits, then rounded half-even to the quote's minor units.
     - **Rate sources.** The evaluator asks its resolver for FX rates and for the `FxRate` object of a `convert` reference; `eval_expr` takes `fx_rates`.
     - **Runtime and engine.** The runtime's `Host` gains `fx_rate` and `rate_object`. The engine host reads the `FxRate` objects at the process snapshot and `asof`, and records each rate version once in `Core.rates`; the journal carries them as `rates` when any was used.
  5. **Decimal strings in `money`.** `money(amount, cur)` accepts a decimal string where the amount is dynamically typed: decimals are strings in canonical JSON (STD-FND-022), and the reference model accepts them.
  6. **Adapter operations.** `currency` and `fx_rate`.
  7. **Claim.** The kernel's conformance test reads the vectors of BPA 1.0-rc9 (`NEXT_RELEASE`) and claims STD-FND-028, -029 and STD-KIND-029. No other vector changed outcome when the release in implementation moved from 1.0-rc7 to 1.0-rc9.
- **Consequences:**
  - The 28 vectors pass in memory and on PostgreSQL 16; the conformance test passes with 2,131 vectors, 401 outside the claim.
  - The prototype vectors on PostgreSQL, the `ubos-forge`, `ubos-dvm` and `ubos-node` tests, fmt, clippy and the dependency check pass.
  - **Known limit.** The leaf check loads the accounts of the branch for each commit that posts to a class with `ledger.account`. That is fine for charts of thousands of accounts; a referrer lookup replaces it if the benchmarks of step 18 show a cost.
- **Alternatives considered:**
  - Passing the added codes as a parameter through every value check was rejected: it would change the signatures of the value, model and expression crates for a set that is empty in almost every commit.
  - Computing the inverse rate as `1 / rate` before multiplying was rejected: dividing the amount by the stored rate gives the same rounded result with one division.
  - Accepting decimal strings in every numeric function was rejected: only `money` needs it to match the reference model, and wider coercion would change the errors of released vectors.
- **Affects:** STD-FND-022, STD-FND-028, STD-FND-029, STD-KIND-029, DSN-DVM-409, DSN-DVM-410, DSN-DVM-441, EXIT-2-01, VER-CONF-8405, DEC-098

