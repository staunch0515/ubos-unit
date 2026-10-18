# EXIT-1-19 — The PH-1 scenarios over UBTP on the single-node Server (parts 18d-2 and 18d-3a, interim)

| Item | Value |
|---|---|
| Criterion | EXIT-1-19: all 13 PH-1 scenarios pass over UBTP on the single-node Server (UBS-PH-1 §3) |
| This part | PH-1 step 18d-2 (DEC-082): the ten gate-1a scenarios (SCN-002, -003, -004, -005, -006, -007, -008, -010, -011, -018); re-run in part 18d-3a on BPA 1.0-rc6 (DEC-083), which closes the gaps G1 and G2; SCN-001, -009 and -017 and the final report come with part 18d-3b |
| Result | **21 of 22 scenario tests pass over WebSocket and 21 of 22 over HTTP; SCN-011 A1 is verified in process only** (DEC-082 item 4). In part 18d-2 SCN-006 main and SCN-018 main failed on two gaps of the Protocol (G1, G2 below), closed by BPA 1.0-rc6 |
| Runner | `forge test verify/scenarios --backend ubtp --server target/debug/ubos-server --database "host=127.0.0.1 user=postgres dbname=bpa" --binding ws|http --report …` (`crates/ubos-forge/src/ubtp_test.rs`), the Rust SDK `ubos-sdk` 0.2 |
| Node | `ubos-server` (debug build), `dev` preset, PostgreSQL 16; one Server and one fresh tenant per test, the genesis written at the fixture clock (`dev.test_clock`), the test's connectors as `dev.connectors`, the clock moved through `/api/v1/admin/clock` |
| Standard | BPA 1.0-rc6 (part 18d-2: 1.0-rc5) |
| Time | 56.6 s over WebSocket, 57.1 s over HTTP (sum of the tests), development workstation (4 vCPU, linux x86_64); 2026-10-18 |
| Reports | `scenarios-ubtp-ws.json`, `scenarios-ubtp-http.json` |

## 1. Results

| Test | Steps | WebSocket | HTTP | ms (WS / HTTP) |
|---|---|---|---|---|
| SCN-002 main: the subclass narrows title and a 150-character title is rejected | 6 | pass | pass | 1954 / 1675 |
| SCN-002 A1: widening title in the subclass is a narrowing violation | 1 | pass | pass | 1951 / 1794 |
| SCN-002 post: querying Agreement includes the ServiceAgreement instance | 4 | pass | pass | 1813 / 1786 |
| SCN-003 main: the override guard fails inside the notice period; the base logic terminates a plain agreement | 7 | pass | pass | 2555 / 2321 |
| SCN-003 A1: an override narrowing a parameter type is rejected | 1 | pass | pass | 1779 / 1635 |
| SCN-004 main: a draft branch, 15 changes, the diff, the approved change set merged in one commit | 29 | pass | pass | 6099 / 5763 |
| SCN-004 A1: the same field changed on main and the draft is a conflict; nothing merges | 6 | pass | pass | 1741 / 1794 |
| SCN-004 A2: a different field of the same product merges automatically | 6 | pass | pass | 1837 / 1891 |
| SCN-005 main: a stale base is rejected with TXN.HEAD_MOVED and the chain stays linear | 7 | pass | pass | 1738 / 1781 |
| SCN-005 A1: 100 concurrent writers on one base version, exactly one succeeds | 3 | pass | pass | 3770 / 4378 |
| SCN-006 main: a future-dated price is read by asof and becomes effective on its day without a job | 8 | pass | pass | 5538 / 5537 |
| SCN-006 A1: a second future version; every date resolves to one version | 7 | pass | pass | 1891 / 1818 |
| SCN-007 main: what did we believe on March 1 and March 11 about February 15 | 9 | pass | pass | 4981 / 5864 |
| SCN-007 A1: a time before the creation is not found at that time | 2 | pass | pass | 1876 / 1922 |
| SCN-008 main: a wrong posting is reversed and re-posted; history stays | 4 | pass | pass | 1715 / 1901 |
| SCN-008 A1: a ledger entry cannot be updated | 2 | pass | pass | 1777 / 1591 |
| SCN-008 A2: a branch with ledger entries merges by appending in branch order | 6 | pass | pass | 2147 / 1961 |
| SCN-010 main: runaway L2 logic is stopped, nothing is committed or released, the run is metered | 4 | pass | pass | 4896 / 4609 |
| SCN-010 A1: a script that opens a network connection is refused | 1 | pass | pass | 1517 / 1969 |
| SCN-011 main: a process with the clock, entropy and a connector replays identically | 4 | pass | pass | 1718 / 1733 |
| SCN-011 A1: a missing journal record stops the replay without calling the connector | 0 | in process only | in process only | 0 / 0 |
| SCN-018 main: a bulk process is reverted; the 5 objects changed since are conflicts, resolved to keep the later change | 59 | pass | pass | 3263 / 3392 |

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

## 4. Part 18d-3a: BPA 1.0-rc6 (DEC-083)

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
