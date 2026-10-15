# BPA 1.0.0-rc2 — Release notes

Release candidate 2 of BPA 1.0 (PH-1 step 10, EXIT-1-14, DEC-063, DEC-064, DEC-069). Tag
`bpa-v1.0.0-rc2`, signed by the owner (DSN-BPA-001). The manifest
(`release/1.0-rc2/manifest.json`) lists every released file with its SHA-256; its
`content_id` identifies this release. BPA 1.0-rc1 stays frozen as tagged (DEC-045).

## Content

| Item | Value |
|---|---|
| Profiles | Core, Bitemporal, Ledger, Proof, Governance, Protocol |
| Clauses | 197 (scope by rule, `scope.json`): the 171 clauses of BPA 1.0-rc1 plus the Governance and Protocol clauses and the Core clauses that serve PH-1 gate 1b |
| Added since 1.0-rc1 | Governance: STD-LGS-001, -002, -010, -011, -020, -021, -022, -023, -030, -031; Protocol: STD-PROTO-001, -002, -010, -011, -012, -013, -020; Core: STD-BPU-003, STD-CLS-003, STD-CLS-031, STD-EXPR-010 and the MAY clauses STD-ISA-017, -035, -054, -070, -080 |
| MUST clauses with a vector range | 165, all covered (100%, `coverage.md`) |
| Vectors | 1,586 (Core 1,104, Bitemporal 61, Ledger 55, Proof 73, Governance 205, Protocol 88); 453 tagged `prototype`; the 1,225 vectors of 1.0-rc1 that are not superseded keep their identifiers and content |
| Superseded vectors of 1.0-rc1 | 2 (`vectors/superseded.txt`, STD-CONF-030): VER-CONF-3361 and VER-CONF-4036 asserted limits that 1.0-rc2 lifts; they stay in the 1.0-rc1 manifest and are excluded here |
| JSON Schemas | 38, generated from the `DAT-*` blocks |
| Not in this release | STD-LGS-040 (decision tables) and STD-PROTO-021 (REST facade), whose earliest requirements are in PH-2; the Sync profile and the remaining Core and Proof clauses of PH-2 and later |

## Changes since BPA 1.0-rc1

- Part 10a (DEC-064): the release is opened with its scope by rule (`gate: 1b`); the
  policy-engine library is chosen (DEC-065).
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
- Part 10d (DEC-068): UBTP — the in-process binding as the adapter operation `ubtp`, the
  envelope rules and `INT.PROTOCOL_ERROR`, the handshake with in-process credentials and
  limits, `mutate` with idempotency, `base`, `validate_only` and pending results, the `query`
  kinds, subscriptions with cursors, emits, retention and acknowledgement windows, `cancel`
  and `ping`; 88 vectors.

## Known limits

- The 359 vectors added in 1.0-rc2 are not tagged `prototype` and are verified against the
  reference model only; the kernel runs them when PH-1 steps 11–14 implement governance sheets
  and rules, authorization and queries, and the UBTP Server (EXIT-1-14, kernel part;
  VER-CONF-8404 on PostgreSQL).
- The Protocol vectors exercise the in-process binding (STD-PROTO-020). The WebSocket and HTTP
  bindings replay the same message sequences with the Server of step 14 (VER-CONF-7000); the
  REST facade (STD-PROTO-021) is PH-2.
- Decision tables (STD-LGS-040), the Sync profile, link objects of attributes classes
  (STD-ISA-015) and the pending-change store of a `require` decision over UBTP (PH-2) are not
  in this release.
- Defects found after the tag are recorded in `errata/` and corrected in the next release
  (DEC-045 item 3).
