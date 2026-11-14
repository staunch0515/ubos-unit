# PH-2 step 3 — money and periods in the kernel (DEC-098), parts 3a–3d

| Item | Value |
|---|---|
| Scope | Parts 3a–3d of DEC-098: currencies, FX rates and calendars (3a), balanced posting groups and hierarchies of ledger keys (3b), periods, close artefacts, reopen and restatement (3c), matching, match records and statement ingestion (3d). Part 3e (balance projections, FR-LEDG-031, -033, FR-ANL-021) and the closing part 3f are **not started**: the owner paused development after 3a–3d on 2026-11-14. |
| Crate | `crates/ubos-ledger` (pure library; UBS-ARC-07 §2 row allows `ubos-types` and `ubos-version`): `currency`, `money`, `fx`, `calendar`, `posting`, `periods`, `matching`. Kernel integration in `crates/ubos-dvm/src/{money,posting,periods,matching}.rs`. No finance vocabulary in non-test kernel code (EXIT-2-04). |
| Execution | Four AI sessions in separate worktrees (3a ∥ 3d, then 3b ∥ 3c), merged by the coordinating session: commits `fc41814` (opening), `3f35f0a` (3d), `1d6292d` (3a), `fffdebc` (3b), `a99d9cf` (3c), `233cb9b` (STD-VER-055 claimed so that VER-CONF-1108 runs). |
| Environment | development workstation (4 vCPU, linux x86_64), PostgreSQL 16, debug build, 2026-11-14 |

## 1. What was built

| Part | Design items | Built |
|---|---|---|
| 3a | DSN-DVM-409, -410, -447; FR-LOC-021, -022, -041, -042, FR-QRY-042, FR-LEDG-053 | ISO 4217 table in `ubos-types::currency` (was 10 currencies); genesis classes `FxRate` (Definition, unique key source/from/to/rate type, valid time = rate date) and `BusinessCalendar` (weekend, holidays, union, cut-off); rates and calendars read at the read's coordinates with the versions returned; host operations `currencies`, `fx_rate`, `convert`, `ledger_totals`, `calendar`, `business_day`; `query` with `convert`; stored calendars and rates usable by name in L1 queries, guards, `actions` and program expressions |
| 3b | DSN-DVM-440, -441, -409; FR-LEDG-051, -052, -053 | `ledger.double_entry` groups balanced per currency at commit and merge, `LEDG.UNBALANCED` with the differences; genesis class `LedgerNode` (Buk classes extend it) with leaf-only posting, `posting: false`, currencies, cycle and validity checks; host operations `ledger_rollup` and `ledger_hierarchy` at valid-time and transaction-time coordinates |
| 3c | STD-KIND-025, -026, STD-TIME-041, -042; DSN-DVM-322, -323, -442…-444 | period transitions and posting-date checks, `close_period` with the close artefact (genesis System class `CloseArtefact`) and its signed tag, `reopen_period` (through a change set when the period class has that write policy), `restate` on a correction branch with the re-evaluation modes recorded in derivations, compensation proposals in the `restate` result |
| 3d | STD-KIND-027, -028; DSN-DVM-445, -446 | deterministic matching (`match`, `unmatch`), match records as entries of the genesis ledger class `MatchRecord` (appends, reversals and adjustments by ordinary commits refused), idempotent statement ingestion `ingest` (key from source, ledger key, statement, line and content hash) |

## 2. Verification

| Check | Result |
|---|---|
| Conformance test (`cargo test -p ubos-forge --test conformance`, release in implementation 1.0-rc8) | 1,747 vectors run, all pass; the 50 vectors of STD-KIND-025…028 and STD-TIME-041, -042 included |
| The 50 rc8 vectors on PostgreSQL 16 (`forge conform --backend postgres`) | 50 of 50 (3d: 20; 3c: 30) |
| Full `forge conform` over `standard/vectors`, memory and PostgreSQL (3c, against a binary of `3f35f0a`) | identical outcomes on both backends; 0 regressions, 23 newly passing vectors; the vectors of clauses not yet implemented fail or are not applicable as before |
| Differential checks against the reference model `periods.py` (random scenarios; generators not in the repository) | matching 400 of 400; periods and close 700 of 700 (400 with branches and merges); restatement 1,000 of 1,000 with the reference's process order (finding C-1) |
| `cargo test -p ubos-ledger` | 65 pass (including property tests) |
| `cargo test -p ubos-dvm` | pass; `tests/money.rs` 10 and `tests/posting.rs` 8 also on PostgreSQL 16 |
| `cargo fmt --check`, `clippy -D warnings`, `check_deps.py`, `regen_all.sh` | clean; errors 0, warnings 0, 0 Must gaps |

Not done because of the pause: the projections of 3e, the full PostgreSQL run of every claimed vector on the merged tree, the benchmark, the scenarios over UBTP and the Server routes of the new host operations.

## 3. Findings for the next release candidate and the design items

BPA 1.0-rc8 is frozen (DEC-096): no clause or vector was changed. The kernel follows the vectors where they disagree with a design item.

| ID | Items | Finding | Proposed fix |
|---|---|---|---|
| L-1 | STD-EXPR-005 | The names of calendars and rate objects inside a VAE are undefined. | The name is the canonical URI of a calendar or rate object, read at the evaluation's coordinates; its version is evaluation evidence. |
| L-2 | genesis classes | `FxRate`, `BusinessCalendar`, `LedgerNode`, `CloseArtefact` have no clause; the scenario documents (`10-requirements/04-scenarios/06-finance.md`, `02-fund-operations.md`) list `FxRate` as a Buk class, which a Buk can no longer declare. | STD-KIND/CLS clauses with DAT schemas; scenario documents updated with the owner's confirmation. |
| L-3 | STD-FND-023, `values.py` | The reference model knows 10 currencies; the kernel the full ISO 4217 table, written from memory (dated entries HRK, SLE, BGN). | The clause names the ISO 4217 edition; table checked against the official list. |
| L-4 | STD-ISA-081 | `convert` takes given rates only, with no rate reference (FR-QRY-042). | Add the stored-rate form `{currency, source?, rate_type?}` with rate references. |
| L-5 | STD-CONF-005 | No adapter operations for rates, conversion, calendars, totals, roll-ups or ingestion, so DSN-DVM-409, -410, -441, -446, -447 (Verification: CONF) have no vectors. | Adapter operations and vectors, or verification TEST. |
| L-6 | STD-ISA-050 | The guard context has no `now`; a cut-off guard takes the instant from `this` or `params`. | State it, or add `now`. |
| L-7 | STD-KIND-024 | Which amount fields balance is unstated; the kernel sums all amount fields per currency. | Balance per amount field, optional `ledger.balance` subset. |
| L-8 | STD-KIND-024, STD-ERR | `LEDG.UNBALANCED` details unspecified (DSN-DVM-440 needs the difference); group scope across classes and the reversal group name unstated. | Standardise `details.groups[].differences[]`, scope per class, reversal of a whole group. |
| L-9 | STD-CLS-041 | Reference integrity does not cover a reference-typed ledger key. | Extend the clause to `ledger_key`. |
| L-10 | STD-TIME-042 | Re-run order: the clause says first commits, `periods.py` sorts by object id; the kernel follows the clause. A re-run with a recorded entropy seed regenerates identifiers (`TXN.ALREADY_EXISTS`). The note on protected correction branches contradicts STD-VER-012. | Fix the reference model and add a two-process vector; derive the re-run's seed from the recorded seed and the branch; protect `correction` or drop the note. |
| L-11 | STD-KIND-026 | Close and restate are not atomic (tag after the closing commit; a failed re-run leaves earlier re-runs). The reference keys posting dates by `cseq` over all ancestors. | State the failure semantics; first parents only in the reference. |
| L-12 | STD-KIND-027, -028 | A match cannot be recorded again after its unmatch (same derived identifier); a missing amount field is undefined; the reference guards only `append` of match records; `revert` of a commit with match records undefined. | Include the superseded record in the derived content; negative vectors; guard in the reference. |
| D-1 | DSN-DVM-440 | Says PH-1 and "debits = credits"; FR-LEDG-051 is PH-2. | Phase note and neutral wording. |
| D-2 | DSN-DVM-441, FR-LEDG-052 | Finance terms (accounts, asset/liability). | "Hierarchy of ledger keys, type declared by the Buk". |
| D-3 | DSN-DVM-442 | Valid time and an override in soft close; the clause and vectors use the commit date and accept appends in `closing`. | Align with STD-KIND-025. |
| D-4 | DSN-DVM-444 | Every reopen through an approved change set; VER-CONF-1104 reopens on `main`. | Through the period class's write policy, as built. |
| D-5 | DSN-DVM-445, -446 | "Immutable objects" (they are ledger entries); DSN-DVM-446 has no clause. | Reword; clause or verification TEST. |

## 4. Open items

- Part 3e: declared balance projections, freshness and rebuild (FR-LEDG-031, -033, FR-ANL-021, DSN-DVM-420, -423); close and roll-up read every entry until then.
- Part 3f: full PostgreSQL run of the claimed set on the merged tree, benchmark, Server routes for the host operations (`match`, `unmatch`, `ingest`, `close_period`, `reopen_period`, `restate`, rates, calendars, roll-ups).
- Validations and governance sheets evaluated at commit do not see stored calendars or rates; no policy row filters on `FxRate` and `BusinessCalendar` reads; same-unit-of-work rates not visible to its expressions.
- Hierarchies: no per-dimension balance; retroactive child under a posted node not refused; hierarchy checks only at each version's valid-from; a merge re-checks only the group balance.
- Periods: the superseded artefact on reopen is in the result, not in the commit record; no separate seal object (STD-PROOF-055); the posting-date check exists in `ubos-version` (enforcing) and `ubos-ledger` (pure, unused); `Repo::check_double_entry` in `ubos-version` is unused.
- Ingestion makes one commit per line; a matching side without a ledger key reads all entries of the branch.
