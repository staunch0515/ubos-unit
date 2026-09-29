#!/usr/bin/env sh
# Regenerate all generated files of docs/50-system and validate the volume.
# Run from anywhere; exits non-zero if validation fails.
set -e
DIR="$(cd "$(dirname "$0")" && pwd)"
python3 "$DIR/gen_phase_features.py"
python3 "$DIR/gen_traceability.py"
python3 "$DIR/gen_indexes.py"
python3 "$DIR/check_system.py"
python3 "$DIR/check_system.py" --json | python3 -c "import json,sys; r=json.load(sys.stdin); sys.exit(1 if r['errors'] else 0)"
# Finance requirements (self-contained folder docs/finance-requirements, own tools)
sh "$DIR/../finance-requirements/tools/regen.sh"
