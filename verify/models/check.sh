#!/usr/bin/env sh
# Model-check the PH-0 formal models with TLC (VER-FORM-0001..0003, EXIT-0-02).
#
# Usage: sh verify/models/check.sh [ci|full] [model ...]
#   ci   - reduced bounds from <model>.ci.cfg, used by CI (a few minutes)
#   full - the bounds of the verification items from <model>.cfg (see reports/)
# Models default to: commit_cas merge timeline.
# Needs Java 11 or later. TLC is downloaded once into verify/models/.tools/ and its
# SHA-256 is verified. Exits non-zero if any property is violated.
set -e
DIR="$(cd "$(dirname "$0")" && pwd)"
MODE="${1:-ci}"
[ $# -gt 0 ] && shift
MODELS="${*:-commit_cas merge timeline}"
TLA_VERSION="1.8.0"
TLA_SHA256="ab4694601923fd5ac06452abbf847c366a5054a3d739552085edd6ed986c29ec"
JAR="$DIR/.tools/tla2tools-$TLA_VERSION.jar"

if [ ! -f "$JAR" ]; then
    mkdir -p "$DIR/.tools"
    curl -sSfL -o "$JAR.tmp" \
        "https://github.com/tlaplus/tlaplus/releases/download/v$TLA_VERSION/tla2tools.jar"
    echo "$TLA_SHA256  $JAR.tmp" | sha256sum -c - >/dev/null
    mv "$JAR.tmp" "$JAR"
fi

WORKERS="${TLC_WORKERS:-auto}"
STATUS=0
for m in $MODELS; do
    case "$MODE" in
        ci) CFG="$m.ci.cfg" ;;
        full) CFG="$m.cfg" ;;
        *) echo "unknown mode $MODE"; exit 2 ;;
    esac
    OUT="$(mktemp -d)"
    echo "== $m ($CFG)"
    if (cd "$DIR" && java -XX:+UseParallelGC -cp "$JAR" tlc2.TLC -workers "$WORKERS" \
            -metadir "$OUT" -config "$CFG" "$m.tla") > "$OUT/tlc.txt" 2>&1 \
        && grep -q "No error has been found" "$OUT/tlc.txt"; then
        grep -E "states generated|depth of the complete" "$OUT/tlc.txt"
        echo "   ok"
    else
        grep -v "JAVA_TOOL_OPTIONS" "$OUT/tlc.txt" | tail -40
        echo "   FAILED"
        STATUS=1
    fi
    rm -rf "$OUT"
done
exit $STATUS
