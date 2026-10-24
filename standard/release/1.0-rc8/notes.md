# BPA 1.0.0-rc8 — Release notes (in development)

Release candidate 8 of BPA 1.0 (PH-2 step 1, DEC-089), the candidate for the PH-2 kernel. It is
in development: clauses and vectors are added by parts 1b–1g and the release is made in part 1h
with the owner's signed tag `bpa-v1.0.0-rc8`. BPA 1.0-rc7 stays frozen as tagged.

## Content

| Item | Value |
|---|---|
| Profiles | Core, Bitemporal, Ledger, Proof, Governance, Protocol |
| Clauses | 235 (scope by phase, `scope.json`): the 201 clauses of BPA 1.0-rc7, 23 that satisfy PH-2 requirements, and 11 written in parts 1b–1d |
| MUST clauses with a vector range | 197, 189 covered (`coverage.md`) |
| Vectors | 1,922: those of BPA 1.0-rc7 without VER-CONF-1528 (superseded), 45 of part 1b, 53 of part 1c and 122 of part 1d |

## Changes since BPA 1.0-rc7

- The scope rule names a phase (DEC-089 item 1). Added clauses:
  - addressing: STD-ADDR-003, -004;
  - class system and models: STD-CLS-023, -050, -060, -061, -062;
  - execution: STD-CTX-031;
  - instructions: STD-ISA-053, -071, -081, -082, -083, -090;
  - decision tables: STD-LGS-040;
  - encrypted fields: STD-OBJ-010, -011;
  - REST facade: STD-PROTO-021;
  - re-evaluation: STD-TIME-041;
  - transactions: STD-TXN-051, -060, -080;
  - upstream change detection: STD-VER-082.
- Part 1b (DEC-090), money and periods:
  - **Period transitions** (STD-KIND-025): an update moves a period between `open` and `closing`; `closed` is entered by `close_period` and left by `reopen_period` with a reason.
  - **Close artefacts** (STD-KIND-026, new): balances per ledger key, `as_of`, and a signed tag `close-<slug>-<n>`.
  - **Matching** (STD-KIND-027, new) and **match records** (STD-KIND-028, new) with unmatch.
  - **Re-evaluation modes** for logic assets (STD-TIME-041) and **restatement** on a correction branch (STD-TIME-042, new).
  - **Adapter operations** `close_period`, `reopen_period`, `match`, `unmatch` and `restate`.
  - **Supersession:** VER-CONF-1528 closed a period by an update. It is superseded by VER-CONF-1108, which closes the period with `close_period`.
- Part 1c (DEC-091), approvals and rules:
  - **Deferred commits** with `defer` (STD-TXN-060).
  - **Approval requirements, separation of duties and approval evidence** (STD-LGS-041…043, new).
  - **Decision tables** with hit policies and an authoring check (STD-LGS-040, STD-ISA-053).
  - **Simulation of rule changes** (STD-LGS-044, new).
  - **Adapter operations** `decide_table`, `check_table`, `pending_change`, `approve` and `simulate`.
- Part 1d (DEC-092), long-running work and models:
  - **Workflows** (STD-TXN-090…092, new): versioned definitions with eight step types, System-kind instances pinned to their definition version, idempotent steps, a join that fires once, events matched by type and correlation key with a buffer window, timers, human tasks and explicit migration.
  - **Sagas** (STD-TXN-080): steps as linked processes, backward or forward recovery, committed state transitions and resumption without repeated steps.
  - **Quarantine** (STD-CTX-031): a per-VAE threshold of failures and limit terminations, fail-fast calls, an alert effect and release by an authorised principal.
  - **Schema evolution** (STD-CLS-050, -060…062): computed classification of class changes, the nine lens operations, lensed reads, writes and merges, and the totality check.
  - **Upstream changes** (STD-VER-082): overrides marked when the parent line changes them, resolved by acknowledgement or three-way merge.
  - **Adapter operations** `start_workflow`, `deliver_event`, `complete_task`, `migrate_workflow`, `run_saga`, `quarantine_policy`, `release_quarantine`, `change_class`, `classify_change`, `apply_lens`, `check_lens`, `upstream_changes` and `resolve_upstream`; `read` and `commit` take `schema_version`.
  - **Moved:** STD-TXN-051 (pre-commit external calls) is written in part 1f with `call.connector`.
