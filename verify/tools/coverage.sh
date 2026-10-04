#!/bin/sh
# Line coverage of the kernel crates by the verification suites (VER-INSP-0001, EXIT-1-11,
# DEC-061). Runs the suites as instrumented release binaries (cargo-llvm-cov) in memory and
# on PostgreSQL, then writes the summary and the JSON report.
#
#   sh verify/tools/coverage.sh [database-url] [out-dir]
#
# Needs `cargo install cargo-llvm-cov` and `rustup component add llvm-tools-preview`.
set -e
cd "$(dirname "$0")/../.."
DB="${1:-host=127.0.0.1 user=postgres dbname=bpa}"
OUT="${2:-verify/reports/exit-1-11}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target/cov}"
cargo llvm-cov clean --workspace
R="cargo llvm-cov --no-report --release --locked run"
$R -p ubos-forge --bin forge -- conform --vectors standard/vectors
$R -p ubos-forge --bin forge -- conform --vectors standard/vectors --backend postgres --database "$DB"
$R -p ubos-forge --bin forge -- check verify/scenarios
$R -p ubos-forge --bin forge -- types verify/scenarios --json > /dev/null
$R -p ubos-forge --bin forge -- test verify/scenarios
$R -p ubos-forge --bin forge -- test verify/scenarios --backend postgres --database "$DB"
$R -p ubos-dvm --bin ubos-props -- --cases 1000 --threads 2
$R -p ubos-dvm --bin ubos-l2-suite -- --cases verify/sandbox/l2-escape-cases.json
$R -p ubos-dvm --bin ubos-store-suite -- --appends 500 --database "$DB" > /dev/null
$R -p ubos-node --bin ubos-node-suite -- --database "$DB"
$R -p ubos-dvm --bin ubos-roundtrip -- --database "$DB" --vaes 1 --objects 40 --entries 40 --months 2
mkdir -p "$OUT"
cargo llvm-cov report --release --summary-only | tee "$OUT/coverage-summary.txt"
cargo llvm-cov report --release --json --output-path "$CARGO_TARGET_DIR/coverage.json"
python3 verify/tools/coverage_by_crate.py "$CARGO_TARGET_DIR/coverage.json" "$OUT/coverage-by-crate.json"
