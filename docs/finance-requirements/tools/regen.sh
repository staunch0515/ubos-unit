#!/usr/bin/env sh
# Regenerate the dataset, expected results and traceability of this folder, then check it.
# Run from anywhere; exits non-zero if the check fails.
set -e
DIR="$(cd "$(dirname "$0")" && pwd)"
python3 "$DIR/gen_finance_expected.py"
python3 "$DIR/check_finance.py"
