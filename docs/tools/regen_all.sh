#!/usr/bin/env sh
# Regenerate all generated files of docs/50-system and validate the volume.
# Run from anywhere; exits non-zero if validation fails.
set -e
DIR="$(cd "$(dirname "$0")" && pwd)"
python3 "$DIR/gen_phase_features.py"
python3 "$DIR/gen_traceability.py"
python3 "$DIR/gen_results.py"
python3 "$DIR/gen_indexes.py"
python3 "$DIR/check_system.py"
python3 "$DIR/check_system.py" --json | python3 -c "import json,sys; r=json.load(sys.stdin); sys.exit(1 if r['errors'] else 0)"
# BPA release build (standard/, DEC-032): vectors from the oracle, clauses, schemas, coverage, manifest
python3 "$DIR/../../standard/tools/genvectors.py"
python3 "$DIR/../../standard/tools/build_release.py"
# Instruction catalogue, error registry and profiles embedded in ubos-runtime (DEC-056)
python3 "$DIR/../../standard/tools/gen_isa_json.py"
# Finance requirements (self-contained folder docs/finance-requirements, own tools)
sh "$DIR/../finance-requirements/tools/regen.sh"
