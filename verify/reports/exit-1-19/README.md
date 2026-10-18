# EXIT-1-19 — The thirteen PH-1 scenarios over UBTP on the single-node Server

| Item | Value |
|---|---|
| Criterion | EXIT-1-19: all 13 PH-1 scenarios pass over UBTP on the single-node Server (UBS-PH-1 §3); VER-SCN-1001 |
| Parts | PH-1 step 18d (DEC-082, DEC-083): 18d-2 the runner and the ten gate-1a scenarios; 18d-3a BPA 1.0-rc6 closing the gaps G1 and G2; 18d-3b SCN-001, SCN-009 and SCN-017, this report and the CI job |
| Result | **The 13 scenarios run over UBTP on the Server and pass, main and alternate flows, with three qualifications.** The scenario project: 23 of 24 tests pass over WebSocket and 23 of 24 over HTTP, and SCN-011 A1 is verified in process only (DEC-082 item 4). The operator scenarios SCN-001 and SCN-017: 19 of 19 checks over HTTP. SCN-009 A1 is verified when the second sheet is authored, not at its merge (F4), and SCN-009's binding postcondition cannot be observed (F5); both are proposed to the owner (section 3) |
| Runners | `forge test verify/scenarios --backend ubtp --server target/debug/ubos-server --database … --binding ws\|http` (`crates/ubos-forge/src/ubtp_test.rs`, the Rust SDK `ubos-sdk` 0.2); `verify/scenarios/server/scn_server.py --binary target/debug/ubos-server` (the node's operator commands on an installed Server, UBTP over HTTP) |
| Node | `ubos-server` (debug build). The scenario project: `dev` preset on PostgreSQL 16, one Server and one fresh tenant per test, the genesis written at the fixture clock (`dev.test_clock`), the test's connectors as `dev.connectors`. The operator scenarios: the `server` preset installed by the steps of SUITE-NOD-OPS on a PostgreSQL 16 cluster of its own with WAL archiving |
| Standard | BPA 1.0-rc6 |
| Environment | development workstation (4 vCPU, linux x86_64), 2026-10-18; the ENV-REF-SMALL run is made with step 19 (UBS-PH-1 §11) |
| Time | the scenario project 87.7 s over WebSocket and 104.2 s over HTTP (sum of the tests); the operator scenarios 48.9 s |
| Reports | `scenarios-ubtp-ws.json`, `scenarios-ubtp-http.json`, `scenarios-server.json` |
| CI | the job `scenarios over UBTP` of `.github/workflows/ci.yml` runs both runners on every push |

## 1. The scenario project over UBTP

SCN-002…008, -010, -011, -018 (gate 1a) and SCN-009 (gate 1b), from `verify/scenarios/`.

| Test | Steps | WebSocket | HTTP | ms (WS / HTTP) |
|---|---|---|---|---|
| SCN-002 main: the subclass narrows title and a 150-character title is rejected | 6 | pass | pass | 7086 / 3948 |
| SCN-002 A1: widening title in the subclass is a narrowing violation | 1 | pass | pass | 3491 / 5426 |
| SCN-002 post: querying Agreement includes the ServiceAgreement instance | 4 | pass | pass | 2079 / 4223 |
| SCN-003 main: the override guard fails inside the notice period; the base logic terminates a plain agreement | 7 | pass | pass | 2847 / 5495 |
| SCN-003 A1: an override narrowing a parameter type is rejected | 1 | pass | pass | 2382 / 2396 |
| SCN-004 main: a draft branch, 15 changes, the diff, the approved change set merged in one commit | 29 | pass | pass | 7768 / 9512 |
| SCN-004 A1: the same field changed on main and the draft is a conflict; nothing merges | 6 | pass | pass | 2731 / 7990 |
| SCN-004 A2: a different field of the same product merges automatically | 6 | pass | pass | 3412 / 4051 |
| SCN-005 main: a stale base is rejected with TXN.HEAD_MOVED and the chain stays linear | 7 | pass | pass | 2368 / 2546 |
| SCN-005 A1: 100 concurrent writers on one base version, exactly one succeeds | 3 | pass | pass | 4984 / 7242 |
| SCN-006 main: a future-dated price is read by asof and becomes effective on its day without a job | 8 | pass | pass | 6207 / 7993 |
| SCN-006 A1: a second future version; every date resolves to one version | 7 | pass | pass | 2161 / 2391 |
| SCN-007 main: what did we believe on March 1 and March 11 about February 15 | 9 | pass | pass | 4684 / 6131 |
| SCN-007 A1: a time before the creation is not found at that time | 2 | pass | pass | 2246 / 2435 |
| SCN-008 main: a wrong posting is reversed and re-posted; history stays | 4 | pass | pass | 2327 / 2348 |
| SCN-008 A1: a ledger entry cannot be updated | 2 | pass | pass | 2236 / 2282 |
| SCN-008 A2: a branch with ledger entries merges by appending in branch order | 6 | pass | pass | 2155 / 3755 |
| SCN-009 main: a control merged as a sheet rejects a large refund without audit data and requires a signature | 12 | pass | pass | 3911 / 3624 |
| SCN-009 A1: a second sheet setting audit_level with equal specificity and priority is rejected when authored | 4 | pass | pass | 2502 / 3230 |
| SCN-010 main: runaway L2 logic is stopped, nothing is committed or released, the run is metered | 4 | pass | pass | 6912 / 6502 |
| SCN-010 A1: a script that opens a network connection is refused | 1 | pass | pass | 4617 / 2552 |
| SCN-011 main: a process with the clock, entropy and a connector replays identically | 4 | pass | pass | 4469 / 2732 |
| SCN-011 A1: a missing journal record stops the replay without calling the connector | 0 | in process only | in process only | 0 / 0 |
| SCN-018 main: a bulk process is reverted; the 5 objects changed since are conflicts, resolved to keep the later change | 59 | pass | pass | 4113 / 5346 |

## 2. The operator scenarios

SCN-001 and SCN-017 run the node's operator commands (`bootstrap`, `backup`, `restore`,
`verify`) on a Server installed as INSTALL.md describes; clients read through UBTP over HTTP
with the administrator's bearer token (`scenarios-server.json`).

| Check | Result |
|---|---|
| SCN-001 A2: an invalid genesis Buk signature stops bootstrap with PKG.SIGNATURE_INVALID | pass |
| SCN-001 A2: nothing written: no tenant, no setup step, no VAE | pass |
| SCN-001 A1: bootstrap interrupted after the genesis step | pass |
| SCN-001 A1: the rerun completes the missing steps and leaves the done ones | pass |
| SCN-001 step 3: the administrator principal with its DID, role and key pair | pass |
| SCN-001 step 1: the tenant, the default VAE and its branch main answer over UBTP | pass |
| SCN-001 step 2: the genesis Buks system and ontology recorded once each (no duplicates) | pass |
| SCN-001 postcondition 1: the history of main shows exactly two genesis installations, one commit each | pass |
| SCN-001 postcondition 2: every genesis installation records a Buk that verifies against the publisher key | pass |
| SCN-001 step 4: the class Party with its inheritance chain and fields over UBTP | pass |
| SCN-001 postcondition 3: the Merkle root of main is reported | pass |
| SCN-017 precondition: continuous backup (a base backup, WAL archived) | pass |
| SCN-017: commits before and after the chosen time | pass |
| SCN-017 step 1: restored to the chosen time in a separate cluster | pass |
| SCN-017 step 2: the heads and DVM roots of the restored branches equal those recorded at the chosen time | pass |
| SCN-017 step 3: the projections rebuilt from the restored commits equal the restored ones | pass |
| SCN-017 postcondition: the restore report lists the heads, roots and verification results | pass |
| SCN-017: a Server on the restored database answers the root of main recorded at the chosen time over UBTP | pass |
| SCN-017 A1: a broken backup chain: the restore reports the last consistent point and does not restore past it | pass |

How the flows map:
- **SCN-001.** Bootstrap creates the tenant, the default VAE and `main`, installs the genesis
  Buks `system` and `ontology` by one signed commit each (the phase note of SCN-001: the loader
  branch follows in PH-2) and invites the administrator, whose principal holds the role
  `admin` with a DID and a key pair generated by the node (`tenant-admin` in the scenario
  text). Step 4 is `query class` (STD-PROTO-015). Postcondition 2: each installation commit
  records the Buk with its content hash and the publisher's signature, and the node verifies
  those manifests against the publisher key (`genesis-verify`). A1 interrupts bootstrap after
  the genesis step (`UBOS_FAULT_AFTER_STEP`); A2 injects an invalid signature into the
  `ontology` Buk (`UBOS_FAULT_BUK_SIGNATURE`): bootstrap now verifies every genesis Buk before
  its first step, so nothing is written.
- **SCN-017.** The chosen time is that of a restore point taken between two series of
  commits; the restore targets that time (`restore --time`) in a separate cluster; `verify`
  compares the heads and DVM roots with those the point recorded and rebuilds the projections
  (the indexes of the last 1,000 commits) against the restored ones; a Server started on the
  restored database answers the same commit and root of `main` over UBTP. A1 removes a WAL
  segment after the chosen point: a restore to the later point is refused with
  `wal_chain_broken` and the chosen point as the last consistent one.
- **SCN-009.** The sheet `MoneyMovement[this.amount > 10000] { audit_level: "strict";
  require_signature: true; }` is authored on a draft branch and merged into the branch where
  the ports run. A refund of 12,000 committed without audit data is rejected with
  `RULE.DECLARATION_UNSATISFIED` naming the declaration (`audit_level`); with a message and a
  reason it commits, and its command port then requires a signature step
  (`RULE.DECISION_REQUIRES`, a pending change over UBTP; `require_signature` is an action
  property, STD-LGS-011, -031). A payment of 9,000 needs neither. The cascade explanation
  (`query explain`, kind `cascade`) names the sheet, the selector, the specificity and the
  winning value. A1 is rejected with `RULE.AMBIGUOUS_CASCADE` when the second sheet is
  authored on the branch that holds the first.

## 3. Findings of part 18d-3b (proposed to the owner)

- **F4 — a merge does not check the sheet set.** The static checks of STD-LGS-022
  (ambiguity, locked overrides) run when a commit writes a sheet; neither the reference model
  nor the kernel runs them on the sheets a merge brings into the target. Two sheets authored
  on separate branches with equal specificity and priority both merge (SCN-009 A1 as the
  scenario states it), and the cascade of a matching object then fails with
  `RULE.AMBIGUOUS_CASCADE`. SCN-009 A1 is verified at authoring.
- **F5 — the rule binding does not list sheets.** STD-TIME-040 binds the governance sheets of
  a commit with the Governance profile, but the reference model and the kernel record only
  classes, extensions, lifecycles, logic and guards. SCN-009's postcondition (the control is
  recorded in the rule-version binding of each checked commit) is therefore not observable;
  the check is limited to the binding being present.

## 4. Part 18d-2: how the steps map onto UBTP

The runner runs the same project as EXIT-1-13 (`verify/scenarios/`), test by test on its own
fixture: the genesis on a fresh tenant at the fixture clock, the seed on `main` and the logic on
`feature/forge` by raw commits, then the clock of the test. Reads go through the object's URI
(the runner knows the slugs of the seed and of the objects a test creates); the versioning
operations and introspection go through the requests of STD-PROTO-014 and -015; a port call
is a `mutate port` whose result carries the process's metering; the effects released are the
emits of events subscriptions on `main` and `feature/forge`; process identifiers named by a
test are mapped to the Server's (from the result, or from the error of a failed mutation,
BPA 1.0-rc5). Commit sequences are reported relative to the fixture, because the Server's
genesis Buks add commits on `main`. An expected error of `effective_class` or `dispatch` over
inline declarations (SCN-002 A1, SCN-003 A1) is a `bootstrap` whose genesis the node rejects
with that code (DEC-082 item 2).

## 5. Findings of part 18d-2

- **Fixed in the node:** the Server's engines ran with the L2 host off (only the conformance
  adapter keeps it off, DEC-058), so the L2 ports of SCN-003 and SCN-010 failed with
  `LOGIC.NOT_AVAILABLE`; the node now runs L2 scripts.
- **Changed in the scenario model:** the platform genesis Buks hold `Party` (ontology) and
  `Job` (system) on every Server, so the scenario classes are now `Counterparty` and
  `BatchJob`; `FeeCase` gained the port `calculate` through which SCN-011 starts its process
  (DEC-082 item 3). EXIT-1-13 was re-run: 22 of 22 in memory and on PostgreSQL.
- **G1 — the revert request has no `resolutions`.** STD-PROTO-014 lists `revert` with
  `{process_id? | change_set? | time? | commit?, reversal_ids?, dry_run?}`; the revert of
  STD-VER-071 takes resolutions like a merge (DEC-060), and SCN-018 resolves the conflicts of a
  reverted bulk process with them. Over UBTP the revert fails with `VER.MERGE_CONFLICT`.
- **G2 — an events subscription cannot deliver an effect released without a commit.**
  STD-PROTO-012 emits the effects released by each commit; `BecameEffective` (STD-TIME-050) is
  released by the timers of the node, not by a commit, so a client never receives it over UBTP
  (the node's event log has it). SCN-006 main expects it.

## 6. Part 18d-3a: BPA 1.0-rc6 (DEC-083)

- **G1 closed.** A revert takes `resolutions` as a merge does (STD-VER-070, -071 notes): `source`
  keeps the reverted value, `target` the later change, `base` the value after the reverted
  commit; an unused resolution fails the revert with `MODEL.INVALID_VALUE`; the `revert`
  request of STD-PROTO-014 carries them. SCN-018 main passes over both bindings.
- **G2 closed.** An events subscription delivers the effects released without a commit, one
  frame each, after the commits of its branch; the cursor `<branch>:<cseq>.<n>` counts them
  (STD-PROTO-012, STD-TIME-050 notes). The Server's poll loop delivers the `BecameEffective`
  its timers release; SCN-006 main passes over both bindings.
- **Node.** A project genesis that declares a class held by a platform genesis Buk in another
  namespace is refused with `PKG.NAMESPACE_VIOLATION` before the tenant is provisioned (in
  18d-2 the bootstrap skipped such a class and the in-memory Server replaced the platform
  class).
