# EXIT-1-19 — The PH-1 scenarios over UBTP on the single-node Server (part 18d-2, interim)

| Item | Value |
|---|---|
| Criterion | EXIT-1-19: all 13 PH-1 scenarios pass over UBTP on the single-node Server (UBS-PH-1 §3) |
| This part | PH-1 step 18d-2 (DEC-082): the ten gate-1a scenarios (SCN-002, -003, -004, -005, -006, -007, -008, -010, -011, -018); SCN-001, -009 and -017 and the final report come with part 18d-3 |
| Result | **20 of 22 scenario tests pass over WebSocket and 20 of 22 over HTTP**; SCN-011 A1 is verified in process only (DEC-082 item 4); SCN-006 main and SCN-018 main fail on two gaps of the Protocol (G1, G2 below) |
| Runner | `forge test verify/scenarios --backend ubtp --server target/debug/ubos-server --database "host=127.0.0.1 user=postgres dbname=bpa" --binding ws|http --report …` (`crates/ubos-forge/src/ubtp_test.rs`), the Rust SDK `ubos-sdk` 0.2 |
| Node | `ubos-server` (debug build), `dev` preset, PostgreSQL 16; one Server and one fresh tenant per test, the genesis written at the fixture clock (`dev.test_clock`), the test's connectors as `dev.connectors`, the clock moved through `/api/v1/admin/clock` |
| Standard | BPA 1.0-rc5 |
| Time | 61.3 s over WebSocket, 65.8 s over HTTP, development workstation (4 vCPU, linux x86_64); 2026-10-18 |
| Reports | `scenarios-ubtp-ws.json`, `scenarios-ubtp-http.json` |

## 1. Results

| Test | Steps | WebSocket | HTTP | ms (WS / HTTP) | Gap |
|---|---|---|---|---|---|
| SCN-002 main: the subclass narrows title and a 150-character title is rejected | 6 | pass | pass | 2152 / 1806 |  |
| SCN-002 A1: widening title in the subclass is a narrowing violation | 1 | pass | pass | 2116 / 2594 |  |
| SCN-002 post: querying Agreement includes the ServiceAgreement instance | 4 | pass | pass | 2414 / 1839 |  |
| SCN-003 main: the override guard fails inside the notice period; the base logic terminates a plain agreement | 7 | pass | pass | 2569 / 2883 |  |
| SCN-003 A1: an override narrowing a parameter type is rejected | 1 | pass | pass | 1970 / 1911 |  |
| SCN-004 main: a draft branch, 15 changes, the diff, the approved change set merged in one commit | 29 | pass | pass | 6555 / 8037 |  |
| SCN-004 A1: the same field changed on main and the draft is a conflict; nothing merges | 6 | pass | pass | 1954 / 1824 |  |
| SCN-004 A2: a different field of the same product merges automatically | 6 | pass | pass | 2045 / 1831 |  |
| SCN-005 main: a stale base is rejected with TXN.HEAD_MOVED and the chain stays linear | 7 | pass | pass | 3282 / 1763 |  |
| SCN-005 A1: 100 concurrent writers on one base version, exactly one succeeds | 3 | pass | pass | 5807 / 3839 |  |
| SCN-006 main: a future-dated price is read by asof and becomes effective on its day without a job | 8 | fail | fail | 6234 / 10451 | G2 |
| SCN-006 A1: a second future version; every date resolves to one version | 7 | pass | pass | 2370 / 1881 |  |
| SCN-007 main: what did we believe on March 1 and March 11 about February 15 | 9 | pass | pass | 4472 / 4380 |  |
| SCN-007 A1: a time before the creation is not found at that time | 2 | pass | pass | 1487 / 1873 |  |
| SCN-008 main: a wrong posting is reversed and re-posted; history stays | 4 | pass | pass | 1664 / 2168 |  |
| SCN-008 A1: a ledger entry cannot be updated | 2 | pass | pass | 1756 / 2023 |  |
| SCN-008 A2: a branch with ledger entries merges by appending in branch order | 6 | pass | pass | 1948 / 2580 |  |
| SCN-010 main: runaway L2 logic is stopped, nothing is committed or released, the run is metered | 4 | pass | pass | 4442 / 4855 |  |
| SCN-010 A1: a script that opens a network connection is refused | 1 | pass | pass | 1508 / 1609 |  |
| SCN-011 main: a process with the clock, entropy and a connector replays identically | 4 | pass | pass | 1604 / 2213 |  |
| SCN-011 A1: a missing journal record stops the replay without calling the connector | 0 | in process only | in process only | 0 / 0 |  |
| SCN-018 main: a bulk process is reverted; the 5 objects changed since are conflicts, resolved to keep the later change | 54 | fail | fail | 2924 / 3441 | G1 |

## 2. How the steps map onto UBTP

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

## 3. Findings of this part

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
