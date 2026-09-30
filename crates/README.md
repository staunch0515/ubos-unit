# Rust crates (`ubos`)

Kernel, node and Forge crates of the reference implementation (DEC-028: component `ubos`,
folder `crates/`). The crate list, layers and allowed dependencies are normative in
UBS-ARC-07 §2 and enforced by `docs/tools/check_deps.py` (AR-031).

PH-0 prototype crates (UBS-PH-0 §2, DEC-033): `ubos-types`, `ubos-store`, `ubos-tree`,
`ubos-version`, `ubos-time`, `ubos-dvm` (adapter of STD-CONF-005) and `ubos-forge` (runner).
Step 4a implements the pure operations; stores, versioning and time follow in step 4b.

Run the BPA vectors tagged `prototype`:

```sh
cargo build --release -p ubos-forge
./target/release/forge conform --vectors standard/vectors --tag prototype --report report.json
```

Toolchain: Rust stable pinned in `rust-toolchain.toml` (DEC-007). Before a commit:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 docs/tools/check_deps.py
```

Licence: AGPL-3.0-only (`LICENSE`), with a commercial licence available (DEC-019).
