# Verifier and formal models (`ubos-verify`)

Open verifier, formal models and verification environments (DEC-028: component
`ubos-verify`, folder `verify/`).

| Folder | Content |
|---|---|
| `ubos-verify/` | the verifier: command line and library over `ubos-proof` (inclusion and consistency proofs, signatures, archives; structured reports, DSN-DVM-938), PH-1 step 6c (DEC-059) |
| `models/` | TLA+ models `commit_cas.tla`, `merge.tla`, `timeline.tla` and model-checking reports (PH-0 step 2, EXIT-0-02) |
| `sandbox/` | L2 escape suite `l2-escape-cases.json` (VER-SEC-0001 PH-0 step 6; VER-SEC-0100 PH-1 step 5d) |
| `reports/` | evidence of PH-0 exits: `exit-0-03/` conformance, `exit-0-04/` properties, `exit-0-06/` sandbox spike; PH-1: `exit-1-02/` conformance (interim), `exit-1-03/` storage, `exit-1-10/` L2 escape suite, `exit-1-12/` export/import round trip |
| `envs/` | versioned definitions of the reference environments (UBS-VER-02): `ENV-REF-SERVER-ph0.md` for PH-0 step 5 |

Code here may depend only on `ubos-types`, `ubos-crypto` and `ubos-proof` (DEC-028,
`docs/tools/check_deps.py`). Licence: Apache-2.0 (`LICENSE`). Usage:

```
ubos-verify inclusion <proof.json> [--root <hash>] [--dids <dids.json>]
ubos-verify consistency <commits.json> [--from <commit id>] [--to <commit id>] [--dids <dids.json>]
ubos-verify signature <signature.json> --content-id <id> --did <did.json> [--at <instant>]
ubos-verify archive <archive.json>
```
