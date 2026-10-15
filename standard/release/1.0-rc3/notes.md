# BPA 1.0.0-rc3 — Release notes (in development)

Release candidate 3 of BPA 1.0 (PH-1 step 12, DEC-071). Opened on 2026-10-15 because the
lifecycle of a class, named by `lifecycle` since BPA 0.1, had no declaration in the standard
and `commit.transition` (STD-ISA-017) had no vectors; the tag `bpa-v1.0.0-rc3` follows part
12c. BPA 1.0-rc2 stays frozen as tagged (DEC-069).

## Content

| Item | Value |
|---|---|
| Profiles | Core, Bitemporal, Ledger, Proof, Governance, Protocol |
| Clauses | the clauses of BPA 1.0-rc2 plus STD-CLS-004 (lifecycle declaration) and STD-ISA-036 (`read.actions`), by the scope rule of gate 1b (`scope.json`) |

## Changes since BPA 1.0-rc2

- Part 12a (DEC-071): the lifecycle declaration `DAT-Lifecycle` with its transitions, guards,
  effects and per-state field rules (STD-CLS-004); `commit.transition` clarified with its
  vectors (STD-ISA-017); `read.actions` and the adapter operation `actions` (STD-ISA-036);
  port availability by state and port guards (STD-ISA-050, `FLOW.PORT_UNAVAILABLE`); guard
  evaluations and lifecycle versions in the rule binding (STD-TIME-040); the lifecycle check of
  BPU class verification (STD-BPU-011); error codes `FLOW.PORT_UNAVAILABLE`,
  `FLOW.EFFECT_FAILED` and `FLOW.FIELD_RULE_VIOLATED`.

## Known limits

- Until part 12c the coverage report lists the MUST clauses still without vectors.
