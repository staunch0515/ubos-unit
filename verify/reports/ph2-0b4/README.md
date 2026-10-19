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

<!-- rows filled after the run -->
