# Verifier and formal models (`ubos-verify`)

Open verifier, formal models and verification environments (DEC-028: component
`ubos-verify`, folder `verify/`).

| Folder | Content |
|---|---|
| `models/` | TLA+ models `commit_cas.tla`, `merge.tla`, `timeline.tla` and model-checking reports (PH-0 step 2, EXIT-0-02) |
| `sandbox/` | L2 escape suite `l2-escape-cases.json` (VER-SEC-0001 PH-0 step 6; VER-SEC-0100 PH-1 step 5d) |
| `reports/` | evidence of PH-0 exits: `exit-0-03/` conformance, `exit-0-04/` properties, `exit-0-06/` sandbox spike; PH-1: `exit-1-02/` conformance (interim), `exit-1-03/` storage, `exit-1-10/` L2 escape suite |
| `envs/` | versioned definitions of the reference environments (UBS-VER-02): `ENV-REF-SERVER-ph0.md` for PH-0 step 5 |

The verifier CLI and library follow in later steps. Code here may depend only on
`ubos-types`, `ubos-crypto` and `ubos-proof` (DEC-028). Licence: Apache-2.0 (`LICENSE`).
