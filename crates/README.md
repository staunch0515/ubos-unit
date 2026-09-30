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

The PostgreSQL store keeps each commit's full branch state as one record; the layout with
kernel indexes and stored tree nodes that the benchmarks need (DSN-DVM-001…011) is built in
PH-0 step 5.

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
