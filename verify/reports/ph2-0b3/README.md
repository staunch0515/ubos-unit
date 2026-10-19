# PH-2 step 0b-3 — unique keys at commit (DSN-DVM-111, FR-MODEL-013)

| Item | Value |
|---|---|
| Finding | PH-2 step 0b-1b: the commit checked no unique keys. A class's `keys` (STD-CLS-001, "unique keys: lists of field names") were indexed, but `MODEL.KEY_CONFLICT` (STD-ISA-010) was raised only by `release`. FR-MODEL-013 had counted as verified in PH-1 through structural coverage. |
| Change | `crates/ubos-dvm/src/keys.rs`, run on every commit after the relationship checks. A key declared on a class is unique among the live objects of that class and its subclasses on the branch: no two of them carry the same values in all the key's fields. A key with an absent or null field constrains nothing; an ended object releases its key. The objects a commit writes are checked against each other, and against the branch at the base through the field index (one query per key field and branch of the chain), or over the objects of the classes on a branch with a parent line or before the index serves the branch. The error names the declaring class, the key's fields and the two objects. Rows for a key added to a class that already has objects are built when the VAE opens (as for the reference rows of DEC-088). |
| Result | Met for FR-MODEL-013 acceptance 1 and DSN-DVM-111 at commit. Not built: slugs unique per class and branch, and the key conflict of a merge (FR-MODEL-013 acceptance 2) — the PH-1 results record FR-MODEL-013 as `not-met` (UBS-VER-30) |
| Environment | development workstation (4 vCPU, linux x86_64), PostgreSQL 16, release build, 2026-10-19 |

## Verification

| Check | Result |
|---|---|
| Scenarios KEY-1…KEY-6 (`verify/scenarios/tests/keys.yaml`): a second object with the same key; an update taking another object's key; a key across a subclass; two objects of one commit; an absent key field; a composite key; an ended object releasing its key | 6 of 6 in memory, on PostgreSQL and over UBTP (WebSocket and HTTP) |
| Simulation (SUITE-DVM-SIM): `Badge` with the unique key `code` over three values, created by 2–4 nodes with crashes and aborted flushes; the checker requires at most one live badge per code on each branch | 2,000 seeds, 0 violations, 12,292 key conflicts |
| Sensitivity: the same simulation with the key check removed | 200 seeds, 20 violations (duplicates detected) |
| Conformance (BPA 1.0-rc7), in memory and on PostgreSQL | 0 failures (no vector covers keys at commit) |

| Replay corpus | 10,000 of 10,000 outcomes and 1,000 of 1,000 replays identical |
| Properties; L2 escape; storage; fault | 10,000 cases each, 0 failures; 382 of 382 blocked; 20 of 20; 40 SIGKILL events, 0 findings |
| Scenarios (all) | 34 of 34 in memory, on PostgreSQL and over UBTP (WebSocket and HTTP) |
| Node | node suite 12 of 12; SUITE-NOD-EVENTS 15 of 15; SUITE-NOD-JOBS 13 of 13; SUITE-NOD-ISO 544 of 544; `cargo test` of node, forge, dvm, store, version and model on PostgreSQL |
| Benchmark tools | `ubos-bench` small dataset and `ubos-edge-bench --quick` run |
| fmt, clippy, `check_deps.py`, `regen_all.sh` | clean; PH-1 results 313 Must, 291 verified, 8 on the workstation only, 14 gaps |

## Not built (FR-MODEL-013)

1. **Slugs unique per class and branch.** Today two objects may share a slug; a URI resolves
   to the smaller identifier. Enforcing it changes `create` and `move` (STD-ISA-013 lists
   `MODEL.KEY_CONFLICT` for `move`).
2. **The key conflict of a merge** (STD-VER-050: "created on both sides with different
   `object_id`s but the same natural key: key conflict"). The merge does not detect it.

Both were proposed to the owner as part 0b-4 and are built there (`verify/reports/ph2-0b4/`).
