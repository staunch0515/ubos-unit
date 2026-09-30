# EXIT-0-04 — Property checks (SUITE-DVM-PROP)

| Item | Value |
|---|---|
| Criterion | EXIT-0-04: root history independence holds in ≥ 1,000,000 generated cases; UBS-PH-0 §7.4: ≥ 1,000,000 cases per property, 0 failures |
| Build | commit `1f4d9b0` (`cargo build --release -p ubos-dvm --bin ubos-props`) |
| Generator | SplitMix64, seed 20260930, 4 threads; a case is replayed from its index |
| Workstation | 4 vCPU, Linux, Rust 1.94 |

| Item | Property | Cases | Failures | Time |
|---|---|---|---|---|
| VER-PROP-0001 | root history independence (and distinct states give distinct roots) | 1,000,000 | 0 | 106 s |
| VER-PROP-0002 | timeline non-overlap after every accepted operation | 1,000,000 | 0 | 0.4 s |
| VER-PROP-0003 | merge determinism: same conflicts, or same bodies and periods, whichever branch is the source | 1,000,000 | 0 | 503 s |

VER-PROP-0003 outcomes: 561,759 conflicts and 438,241 merges that changed the target, so
both paths of the merge are exercised.

Sensitivity: each property detected a seeded defect before the run — an unsorted tree
input (VER-PROP-0001), `supersede_from` without removing the replaced interval
(VER-PROP-0002), and a merge that silently takes the target value on a conflict
(VER-PROP-0003).

Raw results: `properties.json`. Reproduce:

```sh
cargo build --release -p ubos-dvm --bin ubos-props
./target/release/ubos-props --cases 1000000 --report properties.json
```
