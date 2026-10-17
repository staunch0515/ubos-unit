# Verifier and formal models (`ubos-verify`)

Open verifier, formal models and verification environments (DEC-028: component
`ubos-verify`, folder `verify/`).

| Folder | Content |
|---|---|
| `ubos-verify/` | the verifier: command line and library over `ubos-proof` (inclusion and consistency proofs, signatures, archives; structured reports, DSN-DVM-938), PH-1 step 6c (DEC-059) |
| `models/` | TLA+ models `commit_cas.tla`, `merge.tla`, `timeline.tla` and model-checking reports (PH-0 step 2, EXIT-0-02) |
| `tools/` | `coverage.sh`, `coverage_by_crate.py`, `mutants.sh` and `night.py`: coverage and mutation score of the kernel crates by the suites (VER-INSP-0001, EXIT-1-11; PH-1 step 8a); `dev_loop.py`: the developer loop of `forge dev` (NR-PERF-016; PH-1 step 17e) |
| `replay/` | the replay corpus's recorded outcome hashes (VER-CONF-8500, EXIT-1-08; PH-1 step 8c) |
| `scenarios/` | the ten gate-1a scenarios as a Forge project (VER-SCN-1000, EXIT-1-13; PH-1 step 7c) |
| `sdk/` | SUITE-SDK-PROTO, SUITE-SDK-NET and SUITE-SDK-TYPES (PH-1 step 17d): the shared cases `cases.json`, the fixture project `fixture/` and the driver `sdk_suite.py` with its fault proxy; each SDK has its runner (`crates/ubos-sdk/examples/suite.rs`, `packages/sdk-ts/suite/suite.ts`) |
| `sandbox/` | L2 escape suite `l2-escape-cases.json` (VER-SEC-0001 PH-0 step 6; VER-SEC-0100 PH-1 step 5d) |
| `reports/` | evidence of PH-0 exits: `exit-0-03/` conformance, `exit-0-04/` properties, `exit-0-06/` sandbox spike; PH-1:  `exit-1-03/` storage, `exit-1-10/` L2 escape suite, `exit-1-12/` export/import round trip, `suite-nod-host/` host services (PH-1 step 7a), `forge-7b/` Forge check and test evidence (PH-1 step 7b), `exit-1-13/` gate-1a scenarios, `exit-1-04/` kernel properties at a million cases, `exit-1-05/` simulation seeds, `exit-1-06/` durability under crashes, `exit-1-08/` replay corpus on three platforms, `exit-1-05/nights.md` the night log, `exit-1-11/` coverage and mutation score, `exit-1-02/` final engine vectors on PostgreSQL, `exit-1-18/` SUITE-NOD-OPS on the workstation (PH-1 step 16f), `exit-1-15/sdk-suite.json` the SDK suites (PH-1 step 17d), `forge-dev/` the developer loop (PH-1 step 17e) |
| `envs/` | versioned definitions of the reference environments (UBS-VER-02): `ENV-REF-SERVER-ph0.md` for PH-0 step 5 |

Code here may depend only on `ubos-types`, `ubos-crypto` and `ubos-proof` (DEC-028,
`docs/tools/check_deps.py`). Licence: Apache-2.0 (`LICENSE`). Usage:

```
ubos-verify inclusion <proof.json> [--root <hash>] [--dids <dids.json>]
ubos-verify consistency <commits.json> [--from <commit id>] [--to <commit id>] [--dids <dids.json>]
ubos-verify signature <signature.json> --content-id <id> --did <did.json> [--at <instant>]
ubos-verify archive <archive.json>
```
