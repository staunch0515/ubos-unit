# BPA 1.0.0-rc7 — Release notes

Release candidate 7 of BPA 1.0 (PH-1 step 18d-3b, DEC-084). Tag `bpa-v1.0.0-rc7`, signed by the
owner (DSN-BPA-001). The manifest (`release/1.0-rc7/manifest.json`) lists every released file
with its SHA-256; its `content_id` identifies this release. BPA 1.0-rc6 stays frozen as tagged
(DEC-083). The candidate was opened on 2026-10-18 because SCN-009 over UBTP (EXIT-1-19) found
two defects shared by the reference model and the kernel: a merge did not run the static sheet
checks on the sheets it brought into the target (F4), and the rule binding did not list the
governance sheets STD-TIME-040 binds with the Governance profile (F5).

## Content

| Item | Value |
|---|---|
| Profiles | Core, Bitemporal, Ledger, Proof, Governance, Protocol |
| Clauses | 201 (scope by rule, `scope.json`): the clauses of BPA 1.0-rc6, none added |
| Clarified since 1.0-rc6 (`Since 1.0-rc7` notes) | STD-LGS-022 (the checks run on the sheets a merge or a revert brings in; `merge_preview` reports them), STD-LGS-030 and STD-TIME-040 (the `sheets` of the rule binding) |
| MUST clauses with a vector range | 168, all covered (100%, `coverage.md`) |
| Vectors | 1,703 (Core 1,164, Bitemporal 61, Ledger 55, Proof 73, Governance 209, Protocol 141); 4 added since 1.0-rc6 (VER-CONF-5013…5015, 5063); the vectors of 1.0-rc6 keep their identifiers and content |
| Superseded vectors | 3, unchanged since 1.0-rc3 (`vectors/superseded.txt`) |
| Not in this release | STD-LGS-040 (decision tables) and STD-PROTO-021 (REST facade), whose earliest requirements are in PH-2; the Sync profile and the remaining Core and Proof clauses of PH-2 and later |

## Changes since BPA 1.0-rc6

- F4 (DEC-084 item 2): the ambiguity and locked-override checks of STD-LGS-022 run on the
  sheets and property declarations that a merge or a revert changes relative to the target,
  over the live sheets of the result, and fail it with the same errors; `merge_preview` lists
  them among its validation results. Vectors VER-CONF-5013…5015.
- F5 (DEC-084 item 3): the `sheets` member of the rule binding lists the versions of the sheets
  that apply to the objects the commit enforcement evaluated, in object-identifier order of the
  sheets. Vector VER-CONF-5063.
- The reference model checks merged sheets (`standard/tools/bpa/sheets.py`, `store.py`,
  `workflow.py`) and binds the sheets of the enforcement (`rules.py`, `store.py`).

## Known limits

- As in 1.0-rc6: the pending change of a `require` decision and its approval, the simulation
  of a port call, view payloads and `sync` frames (PH-2, PH-3).
- Defects found after the tag are recorded in `errata/` and corrected in the next release
  (DEC-045 item 3).
