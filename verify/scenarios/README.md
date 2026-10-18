# Gate-1a scenarios (VER-SCN-1000, EXIT-1-13)

A Forge project (DSN-FRG-002) holding the ten gate-1a scenarios of UBS-PH-1 §3 as `forge test`
scenario tests: SCN-002, -003, -004, -005, -006, -007, -008, -010, -011, -018, main and
alternate flows, one file per scenario under `tests/`; since PH-1 step 18d-3b also SCN-009 of
gate 1b. `forge test … --backend ubtp` runs the same tests over UBTP on a Server, and
`server/scn_server.py` runs the operator scenarios SCN-001 and SCN-017 on an installed Server
(EXIT-1-19, `verify/reports/exit-1-19/`).

The project is written by `tools/gen_scenarios.py` (run it from the repository root after a
change; the generated YAML is committed so that the tests read without the generator).
`forge check verify/scenarios` checks it; `forge test verify/scenarios` runs it in memory,
`--backend postgres --database …` on a fresh VAE of a PostgreSQL database. The report of
the run is `verify/reports/exit-1-13/`.
