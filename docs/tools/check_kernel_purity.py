#!/usr/bin/env python3
"""VER-INSP-0002 procedure 1 (EXIT-2-04, MET-BIZ-001; PH-2 step 9e, DEC-145): scan the kernel
crates for finance identifiers and classify every hit.

Finance identifiers (account, journal, invoice, bill, vendor, customer, 1099, depreciation,
revaluation, sales tax) are looked for in `crates/*/src/**/*.rs`, outside tests (`tests/`,
`#[cfg(test)]`), test drivers and benchmarks (`src/bin/`) and examples. Every hit must match a
rule below naming a generic, standardised meaning; a hit no rule explains is a finding.

Usage: python3 docs/tools/check_kernel_purity.py [--list]   (exit 1 on findings)
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CRATES = os.path.join(ROOT, 'crates')
WORDS = re.compile(r'\b(account|journal|invoice|bill|vendor|customer|1099|depreciation|revaluation|'
                   r'sales[ _]tax)\w*', re.I)

# (identifier pattern, path pattern, reason): the first rule whose patterns match explains a hit
RULES = [
    (r'^journal', r'.*', 'the execution journal of processes (STD-CTX-011, -012)'),
    (r'^account(s|ing|ed)$', r'ubos-runtime/', 'memory accounting of executions (DSN-DVM-612)'),
    (r'^account', r'ubos-dvm/src/(finance|projections|matching|stateful)\.rs$',
     'the chart of accounts, statement lines and roll-up of BPA 1.0 (STD-KIND-029, -030, STD-ISA-037)'),
    (r'^accounts$', r'ubos-model/src/lib\.rs$', 'the genesis class LedgerAccount of BPA 1.0 (STD-KIND-029)'),
    (r'^account', r'ubos-(node/src/identity|sdk/src/session)\.rs$', 'user accounts of the identity system (DSN-NOD-101)'),
    (r'^accounting$', r'ubos-node/src/identity\.rs$', 'failure accounting of sign-in throttling (DSN-NOD-101)'),
    (r'^bill', r'ubos-(node/src/server|sdk/src/registry)\.rs$', 'the BILL error area, licensing (STD-ERR-002)'),
    (r'^vendor$', r'ubos-(model|rules)/src/lib\.rs$|ubos-rules/src/registry\.rs$',
     'the governance-sheet layer named vendor (STD-LGS-002)'),
    (r'^vendor$', r'ubos-forge/src/main\.rs$', 'the implementation vendor of a conformance report (STD-CONF-003)'),
    (r'^customer', r'ubos-forge/src/(project|package)\.rs$', 'the example class of the forge new skeleton (DSN-FRG-002)'),
    (r'^customer', r'ubos-sdk/src/lib\.rs$', 'a documentation example of the SDK'),
    (r'^customer$', r'ubos-sdk/src/registry\.rs$', 'the meaning of IAM.KEY_UNAVAILABLE: a customer-managed key'),
]
RULES = [(re.compile(w, re.I), re.compile(p), why) for w, p, why in RULES]


def hits():
    for root, dirs, files in os.walk(CRATES):
        rel_root = os.path.relpath(root, CRATES)
        parts = rel_root.split(os.sep)
        if 'target' in parts or 'tests' in parts or 'bin' in parts or 'examples' in parts:
            continue
        for f in sorted(files):
            if not f.endswith('.rs'):
                continue
            path = os.path.join(root, f)
            rel = os.path.relpath(path, CRATES).replace(os.sep, '/')
            in_test = False
            with open(path, encoding='utf-8', errors='replace') as fh:
                for n, line in enumerate(fh, 1):
                    if '#[cfg(test)]' in line:
                        in_test = True
                    if in_test:
                        continue
                    for m in WORDS.finditer(line):
                        yield rel, n, m.group(0), line.strip()


def main():
    listing = '--list' in sys.argv
    total, findings, reasons = 0, [], {}
    for rel, n, word, line in hits():
        total += 1
        why = next((r for w, p, r in RULES if w.search(word) and p.search(rel)), None)
        if why is None:
            findings.append('%s:%d %s | %s' % (rel, n, word, line[:100]))
        else:
            reasons[why] = reasons.get(why, 0) + 1
    if listing:
        for why, c in sorted(reasons.items(), key=lambda x: -x[1]):
            print('%5d  %s' % (c, why))
    for f in findings:
        print('finding: ' + f)
    print('kernel purity (VER-INSP-0002): %d identifier hits, %d classified, %d findings'
          % (total, total - len(findings), len(findings)))
    sys.exit(1 if findings else 0)


if __name__ == '__main__':
    main()
