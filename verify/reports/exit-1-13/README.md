# EXIT-1-13 — Gate-1a scenarios through the in-process binding (VER-SCN-1000)

| Item | Value |
|---|---|
| Criterion | EXIT-1-13: the ten gate-1a scenarios (SCN-002, -003, -004, -005, -006, -007, -008, -010, -011, -018) pass through the in-process binding (UBS-PH-1 §3) |
| Verification item | VER-SCN-1000 (SUITE-PH1-SCN) |
| Result | **22 of 22 scenario tests pass on PostgreSQL** (`scenarios-postgres.json`, `junit.xml`) and **22 of 22 in memory** (`scenarios-memory.json`), main and alternate flows |
| Build | PH-1 step 7c (`cargo build --release --locked -p ubos-forge`), DEC-060 |
| Project | `verify/scenarios/` (Forge project written by `verify/scenarios/tools/gen_scenarios.py`: model, logic, seed and one test file per scenario) |
| Binding | `forge test` over the host API `ubos-dvm::host::Vae` with the deterministic services (fixed clock advanced by the tests, seeded entropy, mocked connectors); every test on its own fixture built from the model and seed (genesis root `sha256:c6ef035a6deaf94f…`, seed root `sha256:c83b96221b11ca56…`, equal on both backends) |
| Database | PostgreSQL 16, development workstation (4 vCPU, linux x86_64); the ENV-REF-SMALL run is made at gate-1a acceptance (step 9); whole run 7139 ms |

## 1. Results

| Test | Steps | Result | ms |
|---|---|---|---|
| SCN-002 main: the subclass narrows title and a 150-character title is rejected | 6 | pass | 50 |
| SCN-002 A1: widening title in the subclass is a narrowing violation | 1 | pass | 206 |
| SCN-002 post: querying Agreement includes the ServiceAgreement instance | 4 | pass | 226 |
| SCN-003 main: the override guard fails inside the notice period; the base logic terminates a plain agreement | 7 | pass | 247 |
| SCN-003 A1: an override narrowing a parameter type is rejected | 1 | pass | 197 |
| SCN-004 main: a draft branch, 15 changes, the diff, the approved change set merged in one commit | 29 | pass | 500 |
| SCN-004 A1: the same field changed on main and the draft is a conflict; nothing merges | 6 | pass | 227 |
| SCN-004 A2: a different field of the same product merges automatically | 6 | pass | 224 |
| SCN-005 main: a stale base is rejected with TXN.HEAD_MOVED and the chain stays linear | 7 | pass | 232 |
| SCN-005 A1: 100 concurrent writers on one base version, exactly one succeeds | 3 | pass | 1468 |
| SCN-006 main: a future-dated price is read by asof and becomes effective on its day without a job | 8 | pass | 273 |
| SCN-006 A1: a second future version; every date resolves to one version | 7 | pass | 215 |
| SCN-007 main: what did we believe on March 1 and March 11 about February 15 | 9 | pass | 228 |
| SCN-007 A1: a time before the creation is not found at that time | 2 | pass | 187 |
| SCN-008 main: a wrong posting is reversed and re-posted; history stays | 4 | pass | 201 |
| SCN-008 A1: a ledger entry cannot be updated | 2 | pass | 242 |
| SCN-008 A2: a branch with ledger entries merges by appending in branch order | 6 | pass | 261 |
| SCN-010 main: runaway L2 logic is stopped, nothing is committed or released, the run is metered | 4 | pass | 652 |
| SCN-010 A1: a script that opens a network connection is refused | 1 | pass | 232 |
| SCN-011 main: a process with the clock, entropy and a connector replays identically | 4 | pass | 221 |
| SCN-011 A1: a missing journal record stops the replay without calling the connector | 2 | pass | 209 |
| SCN-018 main: a bulk process is reverted; the 5 objects changed since are conflicts, resolved to keep the later change | 59 | pass | 641 |

## 2. What the scenarios exercise

- **SCN-002** — `Agreement` with a to-many relationship `parties` (2..n) and `ServiceAgreement` narrowing `title` to 120 characters: a 150-character title and a single party are `MODEL.CONSTRAINT_VIOLATED`; widening `title` is `MODEL.NARROWING_VIOLATION`; a query of `Agreement` includes the subclass instance, `only` excludes it; the effective class carries the narrowed constraint. The JSON Schema export of postcondition 2 is `forge types` (TypeScript and Rust at gate 1a; JSON Schema with the SDKs of gate 1b).
- **SCN-003** — L2 ports: the override `ServiceAgreement.terminate` returns the guard that denied the call and the rule chain; the base `Agreement.terminate` sets the state; `dispatch` names the implementation chosen for each class; an override narrowing a parameter is `MODEL.PORT_SIGNATURE_INCOMPATIBLE`. Step 3 (a governance sheet) belongs to the rules of gate 1b and is not run.
- **SCN-004** — a draft branch over 50 seeded products, 12 updates and 3 creates visible on the draft only, the diff with field-level old and new values for 15 entries, a change set opened, submitted, approved and merged in one commit; a same-field conflict is `VER.MERGE_CONFLICT` naming object and path with nothing merged; a different field merges automatically; reading `main` before the merge shows the old state.
- **SCN-005** — a stale base is `TXN.HEAD_MOVED` naming the object; the re-based change commits; 100 writers on one base version at once on PostgreSQL (16 at a time, each in its own adapter): exactly one succeeds, 99 receive `TXN.HEAD_MOVED`.
- **SCN-006** — a price list with a version valid from 2026-10-01 committed on 2026-09-15: `asof` reads return the old and the new price; when the test advances the clock to 2026-10-01 the current read changes and a `BecameEffective` effect is released to the host with no job; a second future version gives three non-overlapping periods.
- **SCN-007** — a back-dated correction on 2026-03-10 of a rate valid from 2026-01-01: reads at `asof` 2026-02-15 with `tx_time` 2026-03-01 and 2026-03-11 return the pre- and post-correction values, repeatably and with the commit used; the history lists both versions; a time before the creation is `QRY.NOT_FOUND_AT_TIME`.
- **SCN-008** — entry #1041 of 10,000.00 USD reversed (#1187, opposite amount, `corrects`) and adjusted (#1188, 1,000.00 USD); the sequence is gap-free; an update of an entry is `LEDG.IMMUTABLE_ENTRY`; a merge of a branch with entries appends them in branch order with new sequence numbers.
- **SCN-010** — an L2 script committing in an unbounded loop stops with `LOGIC.LIMIT_EXCEEDED` after 9,999 commits of its budget, the object is unchanged, nothing is released, the metering record shows the units; a script opening a network connection is `LOGIC.FORBIDDEN_CAPABILITY`.
- **SCN-011** — a fee calculation reading `ctx.now`, generating an identifier and calling a mocked rate connector: the journal records the call, the replay is `identical`; a replay with a dropped record is `LOGIC.REPLAY_INPUT_MISSING` and never calls the connector.
- **SCN-018** — a bulk process updating 40 objects and appending 12 entries, 5 objects changed later: the revert reports the 5 conflicts; with resolutions keeping the later change it commits inverse updates for the other 35 and 12 reversals; nothing is deleted and the history keeps both. Dependent processes (A1) need the event triggers of gate 1b.

## 3. Reproduce

```
cargo build --release --locked -p ubos-forge
python3 verify/scenarios/tools/gen_scenarios.py
./target/release/forge test verify/scenarios
./target/release/forge test verify/scenarios --backend postgres --database "host=127.0.0.1 user=postgres dbname=bpa" \
    --report scenarios-postgres.json --junit junit.xml
```

CI runs both on every push (`.github/workflows/ci.yml`).

## Since PH-1 step 18d-2 (DEC-082)

The project runs over UBTP as well (`verify/reports/exit-1-19/`), which changed its model: the
classes `Party` and `Job` are named `Counterparty` and `BatchJob`, because the platform genesis
Buks of every Server hold `Party` and `Job`, and `FeeCase` has the port `calculate` through
which SCN-011 starts its process (its journal now begins with the port's decision record). The
in-process run after the change: 22 of 22 in memory and 22 of 22 on PostgreSQL (2026-10-18).

## Since PH-1 step 18d-3b (DEC-082)

The project also holds SCN-009 of gate 1b (`tests/scn-009.yaml`: the classes `MoneyMovement`,
`Payment` and `Refund`, the logic `acme.money`), run over UBTP for EXIT-1-19. The gate-1a
tests are unchanged; the in-process run after the change: 24 of 24 in memory and 24 of 24 on
PostgreSQL (2026-10-18).
