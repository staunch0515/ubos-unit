# BPA 1.0.0-rc4 — Release notes

Release candidate 4 of BPA 1.0 (PH-1 step 18a, DEC-079). Tag `bpa-v1.0.0-rc4`, signed by the
owner (DSN-BPA-001). The manifest (`release/1.0-rc4/manifest.json`) lists every released file
with its SHA-256; its `content_id` identifies this release. BPA 1.0-rc3 stays frozen as tagged
(DEC-072). The candidate was opened on 2026-10-17 because the Protocol of BPA 1.0-rc3 has no
messages for branches, change sets, merges, reverts and bulk processes and no queries for the
effective class, commits, ledgers and processes, while the PH-1 scenarios that gate 1b runs over
UBTP (EXIT-1-19) need them (DEC-079).

## Content

| Item | Value |
|---|---|
| Profiles | Core, Bitemporal, Ledger, Proof, Governance, Protocol |
| Clauses | 201 (scope by rule, `scope.json`): the 199 clauses of BPA 1.0-rc3 plus STD-PROTO-014 (versioning messages) and STD-PROTO-015 (introspection queries) |
| Clarified since 1.0-rc3 (`Since 1.0-rc4` notes) | STD-PROTO-010 (`mutate` alternatives), STD-PROTO-011 (`query` request kinds) |
| MUST clauses with a vector range | 168, all covered (100%, `coverage.md`) |
| Vectors | 1,689 (Core 1,160, Bitemporal 61, Ledger 55, Proof 73, Governance 205, Protocol 135); 47 added since 1.0-rc3 (VER-CONF-5600…5629, 5650…5666); the vectors of 1.0-rc3 that are not superseded keep their identifiers and content |
| Superseded vectors | 3, unchanged since 1.0-rc3 (`vectors/superseded.txt`) |
| Not in this release | STD-LGS-040 (decision tables) and STD-PROTO-021 (REST facade), whose earliest requirements are in PH-2; the Sync profile and the remaining Core and Proof clauses of PH-2 and later |

## Changes since BPA 1.0-rc3

- Part 18a (DEC-079): the versioning requests of `mutate` — `create_branch`, `change_set`,
  `merge`, `revert` and `bulk` — with their permissions (`branch.create`, `changeset.approve`,
  `branch.merge`, `revert`, `commit.raw`) and a uniform result `{process_id?, commits, versions,
  warnings, result}` (STD-PROTO-014); the introspection requests of `query` — `class`,
  `dispatch`, `commit_info` with the state root, `merge_preview`, `ledger`, `process` with the
  journal and metering record, and `replay` (STD-PROTO-015); the approver of a change set is
  the session's principal; the reference model `standard/tools/bpa/ubtp.py` (the bulk process
  takes the process of the caller); 47 vectors.

## Known limits

- The vectors added in 1.0-rc4 are verified against the reference model; the kernel and the
  Server implement them in PH-1 step 18b (in-process binding and SUITE-NOD-PROTO over
  WebSocket and HTTP). The kernel keeps claiming 1.0-rc1 for its conformance test (STD-CONF-004)
  and runs the vectors of later releases clause by clause as the steps implement them.
- The pending change of a `require` decision and its approval, the simulation of a port call,
  view payloads and `sync` frames remain as in 1.0-rc3 (PH-2, PH-3).
- Defects found after the tag are recorded in `errata/` and corrected in the next release
  (DEC-045 item 3).
