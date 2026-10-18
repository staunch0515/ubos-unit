# BPA 1.0.0-rc5 — Release notes

Release candidate 5 of BPA 1.0 (PH-1 step 18d, DEC-082). Tag `bpa-v1.0.0-rc5`, signed by the
owner (DSN-BPA-001). The manifest (`release/1.0-rc5/manifest.json`) lists every released file
with its SHA-256; its `content_id` identifies this release. BPA 1.0-rc4 stays frozen as tagged
(DEC-079). The candidate was opened on 2026-10-18 because a client of UBTP could not read the
record, journal or metering of a process whose `mutate` failed: the error frame did not name the
process (SCN-010 over UBTP, EXIT-1-19), and because the replay of a process that updates what it
read had no vector (STD-CTX-012; the defect found in part 18a and corrected in part 18b).

## Content

| Item | Value |
|---|---|
| Profiles | Core, Bitemporal, Ledger, Proof, Governance, Protocol |
| Clauses | 201 (scope by rule, `scope.json`): the clauses of BPA 1.0-rc4, none added |
| Clarified since 1.0-rc4 (`Since 1.0-rc5` notes) | STD-PROTO-010 (an `error` answering a `mutate` whose process was recorded carries `process_id`), STD-PROTO-001 (the error body) |
| MUST clauses with a vector range | 168, all covered (100%, `coverage.md`) |
| Vectors | 1,692 (Core 1,161, Bitemporal 61, Ledger 55, Proof 73, Governance 205, Protocol 137); 3 added since 1.0-rc4 (VER-CONF-4170, 5459, 5460); the vectors of 1.0-rc4 keep their identifiers and content |
| Superseded vectors | 3, unchanged since 1.0-rc3 (`vectors/superseded.txt`) |
| Not in this release | STD-LGS-040 (decision tables) and STD-PROTO-021 (REST facade), whose earliest requirements are in PH-2; the Sync profile and the remaining Core and Proof clauses of PH-2 and later |

## Changes since BPA 1.0-rc4

- Part 18d-1 (DEC-082): the error body of a `mutate` is `{code, message, details?, process_id?}`;
  `process_id` names the process when the request reached it (a raw commit or a port call that
  failed validation, a guard, a limit or a decision; a merge, revert or bulk process that
  failed after its process was recorded), so that the caller can read the failed process with
  the `process` query (STD-PROTO-015). Vectors VER-CONF-5459 and 5460.
- STD-CTX-012: a replay reads the snapshots its journal recorded; VER-CONF-4170 replays a
  process that updates the object it read.
- The reference model `standard/tools/bpa/ubtp.py` names the process in the error frame;
  `standard/tools/bpa/exec.py` starts a replay from the journal's snapshot pins (part 18b).

## Known limits

- The pending change of a `require` decision and its approval, the simulation of a port call,
  view payloads and `sync` frames remain as in 1.0-rc4 (PH-2, PH-3).
- Defects found after the tag are recorded in `errata/` and corrected in the next release
  (DEC-045 item 3).
