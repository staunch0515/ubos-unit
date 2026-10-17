# `forge dev` — the developer loop (DSN-FRG-501, NR-PERF-016; PH-1 step 17e)

`forge dev` (DEC-078, part 17e) measured by `verify/tools/dev_loop.py` on the development
workstation (4 vCPUs, Linux; release builds of `forge` and `ubos-server`).
Reports: `memory.json` and `postgres.json`.

| Item | Value |
|---|---|
| Reference project (NR-PERF-016) | 100 classes, 200 rules (two validations per class), 50 tests (25 scenarios, 25 unit tests), one logic asset behind a port of every class, 100 seed objects |
| The node | `ubos-server` in the `dev` preset, started by `forge dev`, on PostgreSQL 16 (`--database`) or in memory; the project installed over UBTP on `feature/dev` |
| Target (NR-PERF-016) | ≤ 10 s for the edit–check–test loop |
| **Edit–check–test loop** | **p50 2142 ms, max 2252 ms on PostgreSQL; p50 2102 ms, max 2230 ms in memory** |
| **Save → the running VAE uses the script** (DSN-FRG-501 acceptance 1) | **p50 285 ms, max 312 ms on PostgreSQL; p50 239 ms, max 266 ms in memory** |
| Start (bootstrap, install, tests) | 6.63 s on PostgreSQL, 2.13 s in memory (FR-DEV-021 asks 30 s from PH-4) |
| A model change (the development VAE rebuilt) | 3.08 s on PostgreSQL, 2.22 s in memory |
| `forge check` of a 500-class project after a script change | 299 ms (DSN-FRG-003 acceptance asks NR-PERF-016) |

## Method

The tool generates the reference project and starts `forge dev --test --json`, which
1. checks the project;
2. starts the node;
3. installs the seed and the logic on `feature/dev`;
4. runs the tests and reports `ready`.

A UBTP client (HTTP, `dev` credentials) then calls the port `describe` of a seeded object
on `feature/dev`. Ten times, the tool saves the logic asset with a new result and calls the
port every 20 ms until the new result comes back. That time is "save → use". It covers:
- the 150 ms scan of the sources;
- the check of the whole project;
- the update of the `Logic` object over UBTP;
- the next call.

`forge dev` reports its own check, install and test times for each edit; their sum is the
edit–check–test loop. The tests run after the install, so a script is in use before its
tests finish. Last, a field is added to a class. Classes are declared at genesis in PH-1,
so `forge dev` starts the node on a fresh tenant named after the model, installs the
project again and writes the generated types; the tool waits until the port answers again.

## Edits on PostgreSQL

| Edit | Save → use (ms) | Check (ms) | Tests (ms) | Install (ms) | Loop (ms) |
|---|---|---|---|---|---|
| 2 | 204 | 39 | 1930 | 36 | 2015 |
| 3 | 280 | 41 | 2081 | 57 | 2190 |
| 4 | 304 | 47 | 2049 | 60 | 2169 |
| 5 | 300 | 59 | 1875 | 48 | 1996 |
| 6 | 261 | 34 | 2066 | 35 | 2145 |
| 7 | 312 | 65 | 2121 | 51 | 2252 |
| 8 | 274 | 40 | 2036 | 50 | 2138 |
| 9 | 308 | 49 | 2056 | 46 | 2162 |
| 10 | 271 | 50 | 1601 | 37 | 1702 |
| 11 | 290 | 50 | 1879 | 55 | 1999 |

## Where the time goes

The tests take most of the loop. Every test runs on its own fixture, a VAE built from the
model and the seed (DSN-FRG-203), so the 50 tests build 50 fixtures of 100 classes and 100
objects. The check takes about 50 ms at the reference size and about 300 ms at 500
classes. The content-addressed build cache of DSN-FRG-003 is therefore not needed to meet
NR-PERF-016 at these sizes, and it has not been built; a cache of the fixture would cut
the test time first.

## Reproduce

```
cargo build --release --locked -p ubos-forge -p ubos-node --bin forge --bin ubos-server
python3 verify/tools/dev_loop.py --forge target/release/forge --server target/release/ubos-server \
    --database "host=127.0.0.1 user=postgres dbname=bpa" --edits 10 --report verify/reports/forge-dev/postgres.json
python3 verify/tools/dev_loop.py --forge target/release/forge --server target/release/ubos-server \
    --edits 10 --report verify/reports/forge-dev/memory.json
```

CI runs the same tool on a debug build with two edits and a 60 s bound (job `sdk suites`).
