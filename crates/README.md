# Rust crates (`ubos`)

Kernel, node and Forge crates of the reference implementation (DEC-028: component `ubos`,
folder `crates/`). The crate list, layers and allowed dependencies are normative in
UBS-ARC-07 §2 and enforced by `docs/tools/check_deps.py` (AR-031).

PH-0 prototype crates (UBS-PH-0 §2, DEC-033): `ubos-types`, `ubos-model` (minimal class
registry), `ubos-store` (memory and PostgreSQL), `ubos-tree`, `ubos-version`, `ubos-time`,
`ubos-dvm` (adapter of STD-CONF-005) and `ubos-forge` (runner). All 453 BPA 0.1 vectors
tagged `prototype` pass on both backends (EXIT-0-03).

PH-0 step 6 (DEC-034) adds `ubos-runtime` with a prototype L2 (Rhai) host and the escape
suite runner `ubos-sandbox-spike`; all 100 initial escape cases are blocked (EXIT-0-06):

```sh
cargo build --release -p ubos-runtime
./target/release/ubos-sandbox-spike --cases verify/sandbox/l2-escape-cases.json
```

PH-0 step 5 preparation (`verify/reports/exit-0-05/`): `ubos-tree` updates stored nodes in
batches (DSN-DVM-007), and `ubos-bench tree` measures it. The stores keep commits as deltas
with branch ordinals: an object index with `cseq` intervals, ledger entries, stored tree
nodes and a state root per commit, written with the head compare-and-set in one
transaction (DSN-DVM-004, -005, -007, -008). A branch reads its own rows and falls back to
its parent at the fork point. Tables of the step-4 layout must be dropped before use.
`ubos-bench load|check|run` generates and loads WL-GENERIC and drives VER-BENCH-0001;
commits can be grouped (`Engine::commit_group`, DSN-DVM-006).

PH-1 step 11 (DEC-070) adds `ubos-rules`, the pure engine of governance sheets (STD-LGS):
grammar, AST, property registry, authoring checks, specificity, the rank and overlap
analysis of the cascade and the cascade itself; the sheet objects, their checks at commit
and the evaluation of qualifiers over a state are the kernel's (`ubos-dvm/src/sheets.rs`).
Commit enforcement, validations, invariants, the validation preview and the decisions before
command ports are `ubos-dvm/src/rules.rs` (STD-LGS-030, -031).
Sheets are compiled once per version hash and indexed by class (`ubos-rules::Cache`);
`ubos-bench sheets` and `ubos-bench invariants` measure NR-PERF-009 and DSN-DVM-708 in
memory (`verify/reports/exit-1-14/workstation/`).

PH-1 step 12 (DEC-071) adds `ubos-flow`, the pure engine of lifecycles (STD-CLS-004,
STD-ISA-017, STD-ISA-036, STD-ISA-050): the shape of a lifecycle declaration, the extension
rule for subclass lifecycles, the reachability findings of the BPU class verification, guards
and `set` effects as L1 expressions, port availability by state and the action list of an
object. The lifecycle objects, the state in the version envelope, `commit.transition`,
`run.port` availability, `read.actions` and the operation `actions` are the kernel's
(`ubos-dvm/src/flow.rs`, with the commit path in `stateful.rs` and the instructions in
`execution.rs`).

Run the BPA vectors tagged `prototype`:

```sh
cargo build --release -p ubos-forge
./target/release/forge conform --vectors standard/vectors --tag prototype --report report.json
./target/release/forge conform --vectors standard/vectors --tag prototype \
    --backend postgres --database "host=127.0.0.1 user=postgres dbname=bpa"
```

Toolchain: Rust stable pinned in `rust-toolchain.toml` (DEC-007). Before a commit:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 docs/tools/check_deps.py
```

Licence: AGPL-3.0-only (`LICENSE`), with a commercial licence available (DEC-019).
