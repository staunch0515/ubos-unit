# BPA 1.0.0-rc2 — Release notes (in development)

Release candidate 2 of BPA 1.0 (PH-1 step 10, EXIT-1-14, DEC-063, DEC-064). Opened on
2026-10-14; the tag `bpa-v1.0.0-rc2` follows part 10e, when every MUST clause with a vector
range is covered. BPA 1.0-rc1 stays frozen as tagged (DEC-045).

## Content

| Item | Value |
|---|---|
| Profiles | Core, Bitemporal, Ledger, Proof, Governance, Protocol |
| Clauses | 197 (scope by rule, `scope.json`): the 171 clauses of BPA 1.0-rc1 plus the Governance and Protocol clauses and the Core clauses that serve PH-1 gate 1b |
| Added since 1.0-rc1 | Governance: STD-LGS-001, -002, -010, -011, -020, -021, -022, -023, -030, -031; Protocol: STD-PROTO-001, -002, -010, -011, -012, -013, -020; Core: STD-BPU-003, STD-CLS-003, STD-CLS-031, STD-EXPR-010 and the MAY clauses STD-ISA-017, -035, -054, -070, -080 |
| Not in this release | STD-LGS-040 (decision tables) and STD-PROTO-021 (REST facade), whose earliest requirements are in PH-2; the Sync profile and the remaining Core and Proof clauses of PH-2 and later |

## Changes since BPA 1.0-rc1

- Part 10a: the release is opened with its scope by rule (`gate: 1b`); vectors follow in parts
  10b–10d (governance sheets, validations and decisions, UBTP messages), the release in 10e.
- Part 10b (DEC-066): governance sheets — the grammar (map literals, keyed map paths, comments,
  canonical form), the genesis classes `GovernanceSheet` and `PropertyDecl`, the combine rules
  of the kernel properties, matching, specificity, the cascade with its authoring checks, the
  explanation; adapter operations `parse_sheet`, `check_sheet`, `specificity`, `cascade` and
  `explain_cascade`; error code `RULE.PARSE_ERROR`; 157 vectors.
- Part 10c (DEC-067): validations, invariants and decisions as kernel properties with commit
  enforcement, the validation preview and the decisions before command ports; port declaration
  checks, overlay policy, the textual query form; the instructions `read.explain`,
  `run.validate`, `search.query` and `read.effective declarations`; adapter operations
  `validate`, `decide`, `parse_query` and `query.text`; `set_limits.max_violations`; the members
  `message` and `reason` of `DAT-Process`; error code `RULE.VALIDATION_FAILED`; the supersession
  of released vectors (`superseded.txt`, STD-CONF-030): VER-CONF-3361 and VER-CONF-4036 of
  1.0-rc1; 116 vectors.

## Known limits

- Until part 10e the coverage report lists the MUST clauses still without vectors.
- New vectors are not tagged `prototype` until the kernel implements them (steps 11–14).
