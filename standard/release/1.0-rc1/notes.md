# BPA 1.0.0-rc1 — Release notes

Release candidate 1 of BPA 1.0 (PH-1 step 1, EXIT-1-01, DEC-039, DEC-045). Tag
`bpa-v1.0.0-rc1`, signed by the owner (DSN-BPA-001). The manifest
(`release/1.0-rc1/manifest.json`) lists every released file with its SHA-256; its
`content_id` identifies this release.

## Content

| Item | Value |
|---|---|
| Profiles | Core, Bitemporal, Ledger, Proof |
| Clauses | 171 (scope by rule, `scope.json`): the 79 clauses of BPA 0.1 plus the Core, Bitemporal, Ledger and Proof clauses that serve PH-1 gate 1a |
| MUST clauses with a vector range | 146, all covered (100%, `coverage.md`) |
| Vectors | 1,227 (Core 1,038, Bitemporal 61, Ledger 55, Proof 73); 453 tagged `prototype` |
| JSON Schemas | 31, generated from the `DAT-*` blocks |

## Changes since BPA 0.1

- Object and class model: size of the canonical body (DEC-039), ports, selectors, extensions,
  relationships, reference integrity and blind indexes (DEC-040).
- Versioning, time and transactions: branch kinds and write policy, change sets, previews,
  resolutions, tags and releases, revert, snapshots, impact sets, derivations, lineage,
  idempotency, outbox and bulk processes; error codes `VER.MERGE_NOT_ALLOWED` and
  `VER.BRANCH_ARCHIVED` (DEC-041).
- Proofs: signature format and key histories, signed commit records, inclusion and consistency
  proofs, tenant root, archive format `ubos.archive.v1`, AEAD inputs, algorithm registry; error
  code `PROOF.VERIFICATION_FAILED` (DEC-042).
- L1 expressions, queries, addressing and custom value types (DEC-043).
- Execution: context, gateway, profiles, metering, entropy, journal and replay, logic assets and
  their verification, the commit, read, run and emit instructions, BPU views and automatic
  retry; host programs as the conformance binding of logic (STD-CONF-005 convention 11,
  DEC-044).
- STD-CONF-005: adapter operations for all of the above. The vectors of BPA 0.1 keep their
  identifiers and content (DEC-039).

## Known limits

- Vectors added after BPA 0.1 are not tagged `prototype`; the kernel of PH-1 steps 3–7 runs
  them (EXIT-1-02).
- Logic is exercised through host programs. Behaviour specific to the L2 language is verified
  by the escape suite of PH-1 step 5 (W-12, EXIT-1-10).
- Rules that need the principal and policy model are applied with the Governance profile in
  BPA 1.0-rc2 (gate 1b, EXIT-1-14): branch authorisation columns (STD-VER), row visibility and
  masking of queries (STD-EXPR), policies in place of grants (STD-ISA-005), the `lifecycle` and
  `tests` checks of BPU class verification (STD-BPU-011). Link objects of attributes classes
  (STD-ISA-015) follow in the same release.
