#!/usr/bin/env bash
# Build the single-binary Server package (DSN-NOD-601, FR-OPS-011; PH-1 step 16f):
#   ubos-server-<version>-<arch>-linux.tar.gz  a static (musl) binary, the systemd units,
#                                              the example configuration and INSTALL.md
#   SHA256SUMS                                 its checksum
# Usage: crates/ubos-node/packaging/build.sh [--target x86_64-unknown-linux-musl] [--out dist]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
target="$(uname -m)-unknown-linux-musl"
out="$root/dist"
while [ $# -gt 0 ]; do
  case "$1" in
    --target) target="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
arch="${target%%-*}"
# the C parts of the dependencies build with musl-gcc (musl-tools)
export "CC_${target//-/_}=${CC_MUSL:-musl-gcc}"
cargo build --release --locked --target "$target" -p ubos-node --bin ubos-server
bin="$root/target/$target/release/ubos-server"
version="$("$bin" version | awk '{print $2}')"
name="ubos-server-$version-$arch-linux"
stage="$(mktemp -d)"
mkdir -p "$stage/$name/bin" "$stage/$name/systemd"
cp "$bin" "$stage/$name/bin/ubos-server"
strip "$stage/$name/bin/ubos-server" 2>/dev/null || true
cp "$here"/systemd/*.service "$here"/systemd/*.timer "$stage/$name/systemd/"
cp "$here/ubos.toml.example" "$here/INSTALL.md" "$stage/$name/"
mkdir -p "$out"
tar -C "$stage" -czf "$out/$name.tar.gz" "$name"
rm -rf "$stage"
(cd "$out" && sha256sum "$name.tar.gz" >> SHA256SUMS && sort -u -k2 SHA256SUMS -o SHA256SUMS)
echo "$out/$name.tar.gz"
