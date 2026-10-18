# BPA 1.0.0-rc6 — Release notes

Release candidate 6 of BPA 1.0 (PH-1 step 18d-3a, DEC-083). Tag `bpa-v1.0.0-rc6`, signed by the
owner (DSN-BPA-001). The manifest (`release/1.0-rc6/manifest.json`) lists every released file
with its SHA-256; its `content_id` identifies this release. BPA 1.0-rc5 stays frozen as tagged
(DEC-082). The candidate was opened on 2026-10-18 because two gate-1a scenarios could not run
over UBTP (EXIT-1-19, part 18d-2): the `revert` request had no `resolutions` (SCN-018), and an
events subscription could not deliver an effect released without a commit such as
`BecameEffective` (SCN-006).

## Content

| Item | Value |
|---|---|
| Profiles | Core, Bitemporal, Ledger, Proof, Governance, Protocol |
| Clauses | 201 (scope by rule, `scope.json`): the clauses of BPA 1.0-rc5, none added |
| Clarified since 1.0-rc5 (`Since 1.0-rc6` notes) | STD-VER-070, -071 (revert resolutions), STD-PROTO-014 (`revert` carries `resolutions`), STD-PROTO-012 (effects released without a commit; the cursor `<branch>:<cseq>.<n>`), STD-TIME-050 (the release has no `cseq`) |
| MUST clauses with a vector range | 168, all covered (100%, `coverage.md`) |
| Vectors | 1,699 (Core 1,164, Bitemporal 61, Ledger 55, Proof 73, Governance 205, Protocol 141); 7 added since 1.0-rc5 (VER-CONF-1591…1593, 5515…5517, 5630); the vectors of 1.0-rc5 keep their identifiers and content |
| Superseded vectors | 3, unchanged since 1.0-rc3 (`vectors/superseded.txt`) |
| Not in this release | STD-LGS-040 (decision tables) and STD-PROTO-021 (REST facade), whose earliest requirements are in PH-2; the Sync profile and the remaining Core and Proof clauses of PH-2 and later |

## Changes since BPA 1.0-rc5

- Revert resolutions (DEC-083 item 2): a revert MAY carry `resolutions` naming conflicts of its
  three-way merge — `source` keeps the value before the reverted commit, `target` the later
  change, `base` the value after the reverted commit, `custom` its value. A resolution that
  matches no conflict of any reverted commit fails the revert with `MODEL.INVALID_VALUE`; the
  resolutions applied are recorded in each compensating commit and returned as `resolutions`.
  Vectors VER-CONF-1591…1593 and 5630.
- Effects released without a commit (DEC-083 item 3): an events subscription delivers them one
  frame each, after the frames of the commits of its branch, in outbox order; the cursor is
  `<branch>:<cseq>.<n>`, written `<branch>:<cseq>` while n is 0, and `from_cursor` and `ack`
  accept both forms. Vectors VER-CONF-5515…5517.
- The reference model `standard/tools/bpa/workflow.py` resolves revert conflicts;
  `standard/tools/bpa/ubtp.py` passes `resolutions` and delivers the effects released without
  a commit.

## Known limits

- As in 1.0-rc5: the pending change of a `require` decision and its approval, the simulation
  of a port call, view payloads and `sync` frames (PH-2, PH-3).
- Defects found after the tag are recorded in `errata/` and corrected in the next release
  (DEC-045 item 3).
