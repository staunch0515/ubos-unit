# EXIT-1-11 — Coverage, mutation score and dependency graph (VER-INSP-0001)

| Item | Value |
|---|---|
| Criterion | EXIT-1-11: line coverage ≥ 85% and mutation score ≥ 70% for the core crates; 0 dependency-graph violations (NR-MAINT-002, NR-MAINT-003) |
| Core crates | the eleven kernel crates `ubos-types`, `ubos-crypto`, `ubos-tree`, `ubos-time`, `ubos-model`, `ubos-expr`, `ubos-version`, `ubos-store`, `ubos-runtime`, `ubos-proof`, `ubos-dvm` (DEC-061) |
| Coverage | **90.1%** of 22,770 kernel lines, by the verification suites run as instrumented release binaries in memory and on PostgreSQL (`coverage-summary.txt`, `coverage-by-crate.json`; `verify/tools/coverage.sh`) |
| Mutation score | **89.9%** of a sample of 237 viable mutants (213 detected: 212 caught, 1 timed out; 24 missed; 16 unviable excluded) — shards 1/8 of the small crates and 1/32 of the large ones, with the vectors, scenarios (in memory and on PostgreSQL), property sample and host suite as the test bodies (`mutants-sample.json`; `verify/tools/mutants.sh`); nightly CI (step 8c) runs every shard and accumulates the full score |
| Dependency graph | `python3 docs/tools/check_deps.py`: errors: 0 (AR-031, UBS-ARC-07) |
| Build | PH-1 step 8a, DEC-061; cargo-llvm-cov 0.9, cargo-mutants, Rust 1.94; workstation 4 vCPU, Linux x86-64; PostgreSQL 16 |

## 1. Line coverage by crate

| Crate | Lines | Covered | Coverage |
|---|---|---|---|
| ubos-types | 1,117 | 1,060 | 94.9% |
| ubos-crypto | 301 | 268 | 89.0% |
| ubos-tree | 445 | 436 | 98.0% |
| ubos-time | 92 | 89 | 96.7% |
| ubos-model | 943 | 903 | 95.8% |
| ubos-expr | 2,515 | 2,126 | 84.5% |
| ubos-version | 2,493 | 2,324 | 93.2% |
| ubos-store | 3,760 | 3,233 | 86.0% |
| ubos-runtime | 1,697 | 1,508 | 88.9% |
| ubos-proof | 352 | 318 | 90.3% |
| ubos-dvm | 9,055 | 8,249 | 91.1% |
| **kernel crates** | **22,770** | **20,514** | **90.1%** |

The suites that produced the coverage: `forge conform` (1,227 vectors, in memory and on
PostgreSQL), `forge check`, `types` and `test` of `verify/scenarios` (both backends),
`ubos-props --cases 1000`, the L2 escape suite, the storage suite (`--appends 500`), the host
suite and the export/import round trip. Coverage of the tool and suite binaries themselves
(`ubos-forge`, `src/bin/`) is in `coverage-summary.txt` but outside the criterion.

## 2. Mutation sample by crate

| Crate | Viable in sample | Caught | Timed out | Missed | Unviable | Detected |
|---|---|---|---|---|---|---|
| ubos-types | 0 | 0 | 0 | 0 | 0 | — |
| ubos-crypto | 92 | 77 | 0 | 15 | 7 | 84% |
| ubos-tree | 0 | 0 | 0 | 0 | 0 | — |
| ubos-time | 0 | 0 | 0 | 0 | 0 | — |
| ubos-model | 33 | 30 | 0 | 3 | 1 | 91% |
| ubos-proof | 0 | 0 | 0 | 0 | 0 | — |
| ubos-expr | 0 | 0 | 0 | 0 | 0 | — |
| ubos-version | 0 | 0 | 0 | 0 | 0 | — |
| ubos-store | 0 | 0 | 0 | 0 | 0 | — |
| ubos-runtime | 0 | 0 | 0 | 0 | 0 | — |
| ubos-dvm | 112 | 105 | 1 | 6 | 8 | 95% |

A timed-out mutant counts as detected: the tests stop a loop the mutation made endless (hard
timeout 300 s). The suite and tool binaries (`src/bin/`) are excluded from mutation. The
sample is one shuffled shard, so the per-crate numbers are indicative; the full score is the
sum of the nightly shards.

### Missed mutants

| Site | Mutation |
|---|---|
| `crates/ubos-crypto/src/lib.rs:35` | replace <impl std::fmt::Debug for DataKey>::fmt -> std::fmt::Result with Ok(Default::default()) |
| `crates/ubos-crypto/src/lib.rs:70` | replace != with == in LocalKeys::from_env |
| `crates/ubos-crypto/src/lib.rs:74` | replace * with + in LocalKeys::from_env |
| `crates/ubos-crypto/src/lib.rs:74` | replace * with / in LocalKeys::from_env |
| `crates/ubos-crypto/src/lib.rs:74` | replace + with - in LocalKeys::from_env |
| `crates/ubos-crypto/src/lib.rs:74` | replace + with * in LocalKeys::from_env |
| `crates/ubos-crypto/src/lib.rs:74` | replace * with + in LocalKeys::from_env |
| `crates/ubos-crypto/src/lib.rs:74` | replace * with / in LocalKeys::from_env |
| `crates/ubos-crypto/src/lib.rs:98` | replace || with && in <impl KeyService for LocalKeys>::by_id |
| `crates/ubos-crypto/src/lib.rs:98` | replace > with < in <impl KeyService for LocalKeys>::by_id |
| `crates/ubos-crypto/src/lib.rs:151` | replace > with == in hmac_sha256 |
| `crates/ubos-crypto/src/lib.rs:151` | replace > with >= in hmac_sha256 |
| `crates/ubos-crypto/src/lib.rs:171` | replace && with || in unhex |
| `crates/ubos-crypto/src/lib.rs:284` | replace || with && in unhex32 |
| `crates/ubos-crypto/src/lib.rs:286` | replace && with || in unhex32 |
| `crates/ubos-model/src/lib.rs:65` | replace * with + |
| `crates/ubos-model/src/lib.rs:251` | replace && with || in is_reference_list |
| `crates/ubos-model/src/lib.rs:258` | replace || with && in check_relationship |
| `crates/ubos-dvm/src/adapter.rs:195` | replace Adapter::enable_l2 with () |
| `crates/ubos-dvm/src/adapter.rs:250` | replace Adapter::backend -> &'static str with "" |
| `crates/ubos-dvm/src/adapter.rs:250` | replace Adapter::backend -> &'static str with "xyzzy" |
| `crates/ubos-dvm/src/adapter.rs:397` | replace && with || in Adapter::execute |
| `crates/ubos-dvm/src/adapter.rs:418` | delete ! in Adapter::execute |
| `crates/ubos-dvm/src/adapter.rs:419` | replace && with || in Adapter::execute |

What the misses show: 15 of the 24 are in `ubos-crypto`'s development key service read from
the environment (`LocalKeys::from_env`, `by_id`, the hex helpers, the HMAC length guard, a
`Debug` impl), which no suite exercises since the node key hierarchy of step 7a replaced it in
the host; three are malformed-input checks of `ubos-model` (kind resolution, relationship
declarations); six are in the adapter: `enable_l2`, the backend name and L2-only branches of
`execute`, because the in-memory conformance test runs the vectors without L2 (the one L2
vector expecting `LOGIC.NOT_AVAILABLE`) and the escape suite is a binary, not a test body.
The nightly shards of step 8c add the L2 vectors and the escape suite to the test bodies.

## 3. Reproduce

```sh
cargo install cargo-llvm-cov cargo-mutants && rustup component add llvm-tools-preview
sh verify/tools/coverage.sh "host=127.0.0.1 user=postgres dbname=bpa" verify/reports/exit-1-11
UBOS_TEST_DATABASE="host=127.0.0.1 user=postgres dbname=bpa" sh verify/tools/mutants.sh /tmp/mutants 0/8 \
    ubos-types ubos-crypto ubos-tree ubos-model ubos-proof
python3 docs/tools/check_deps.py
```
