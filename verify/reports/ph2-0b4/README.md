# PH-2 step 0b-4 — slugs unique per class and branch, key conflicts at merge (FR-MODEL-013)

| Item | Value |
|---|---|
| Scope | The two parts of FR-MODEL-013 that step 0b-3 left unbuilt (`verify/reports/ph2-0b3/`): slugs unique per class and branch, and the key conflict of a merge (acceptance 2, STD-VER-050). Confirmed by the owner. |
| Slugs | A slug is unique among the objects of a class on the branch. Ended objects are included, because an ended object stays readable by its URI (STD-OBJ-004, -005). A subclass is a different class: its objects may reuse a slug of the parent class. The check runs when an object takes a slug: on create, on a slug change, or when a merge brings the object. It reads the slug index at the base in one query per class. A former slug (a redirect) does not block a new object, which then holds the URI. The error is `MODEL.KEY_CONFLICT` with `class`, `slug` and the two objects. |
| Merge | `merge` and `merge_preview` run the commit check (`crates/ubos-dvm/src/keys.rs`) over the objects the merge brings into the target, against the merged state and the target's indexes. A key or slug found is a merge conflict of kind `key` (STD-VER-050): `object_id` is the object brought by the merge, `with` the object that holds the key; `fields` names the key, or `slug` the slug. `merge` fails `VER.MERGE_CONFLICT`; the preview lists the conflict. A key conflict has no recorded resolution: the source branch changes the key (or ends the object) and merges again (MKEY-3). |
| Result | FR-MODEL-013 met (both acceptance criteria). The PH-1 results record it as `met`, noting that it was not met at the PH-1 close; PH-1 has 13 gaps of 313 Must requirements again (UBS-VER-30). |
| Environment | development workstation (4 vCPU, linux x86_64), PostgreSQL 16, release build, 2026-10-19 |

## Scenarios (`verify/scenarios/tests/keys.yaml`)

| Test | Checks |
|---|---|
| SLUG-1 | a second `Supplier` with the slug is refused; a `Customer` and a `PreferredSupplier` may use it |
| SLUG-2 | a slug change onto a taken slug is refused; onto a free slug it commits |
| SLUG-3 | two objects of one commit with the same slug are refused; after an object is ended, its slug stays taken |
| MKEY-1 | FR-MODEL-013 acceptance 2: the same key on two draft branches commits on each; the first merge into `main` commits; `merge_preview` of the second lists the key conflict; the second merge fails `VER.MERGE_CONFLICT` and `main` is unchanged |
| MKEY-2 | the same slug created on two branches: the second merge reports a conflict of kind `key` naming the slug |
| MKEY-3 | after the key is changed on the second draft, its merge commits |

## Verification

| Check | Result |
|---|---|
| Scenarios (all, with SLUG-1…SLUG-3 and MKEY-1…MKEY-3) | 40 of 40 in memory, on PostgreSQL and over UBTP (WebSocket and HTTP) |
| Conformance (BPA 1.0-rc7), in memory, on PostgreSQL and with the prototype tag | 0 failures |
| Replay corpus | 10,000 of 10,000 outcomes and 1,000 of 1,000 replays identical |
| Simulation (with the contended keys of step 0b-3) | 2,000 seeds, 0 violations |
| Properties; L2 escape; storage; fault | 10,000 cases each, 0 failures; 382 of 382 blocked; 20 of 20; 40 SIGKILL events, 0 findings |
| Node | node suite 12 of 12; SUITE-NOD-EVENTS 15 of 15; SUITE-NOD-JOBS 13 of 13; SUITE-NOD-ISO 544 of 544; `cargo test` of node, forge, dvm, store, version and model on PostgreSQL |
| Benchmark tools | `ubos-bench` small dataset and `ubos-edge-bench --quick` run |
| fmt, clippy, `check_deps.py`, `regen_all.sh` | clean; PH-1 results 313 Must, 292 verified, 8 on the workstation only, 13 gaps |
