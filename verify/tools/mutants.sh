#!/bin/sh
# Mutation score of the kernel crates (VER-INSP-0001, EXIT-1-11, DEC-061) with cargo-mutants.
# The test bodies are the integration tests of ubos-forge (every BPA vector in memory, the
# gate-1a scenarios) and the property sample of ubos-props.
#
#   UBOS_TEST_DATABASE="host=127.0.0.1 user=postgres dbname=bpa" \
#   sh verify/tools/mutants.sh <out-dir> [shard k/n] [crates…]
#
# Without crates, every kernel crate is mutated; the suite and tool binaries (`src/bin/`) are
# not. A shard runs one of n equal, shuffled slices of the mutants (nightly CI runs the
# slices in a matrix). With `UBOS_TEST_DATABASE` the scenarios on PostgreSQL and the host suite
# are part of the test bodies (one job at a time: they share the database). Needs `cargo install cargo-mutants`.
set -e
cd "$(dirname "$0")/../.."
OUT="$1"; shift
SHARD=""
case "$1" in */*) SHARD="--shard $1"; shift;; esac
if [ $# -eq 0 ]; then
  set -- ubos-types ubos-crypto ubos-tree ubos-time ubos-model ubos-expr ubos-version \
         ubos-store ubos-runtime ubos-proof ubos-dvm
fi
PKGS=""
for c in "$@"; do PKGS="$PKGS -p $c"; done
cargo mutants $PKGS $SHARD -e '**/bin/**' --test-package ubos-forge --test-package ubos-dvm --test-package ubos-node \
  --timeout 300 --output "$OUT"
