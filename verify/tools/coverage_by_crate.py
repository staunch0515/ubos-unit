#!/usr/bin/env python3
"""Line coverage per crate from a cargo-llvm-cov JSON export (VER-INSP-0001, EXIT-1-11).

    coverage_by_crate.py <llvm-cov json> <out json>

Prints one line per crate and the total of the kernel crates; exits 1 when the kernel total
is below 85% (UBS-PH-1 EXIT-1-11)."""
import collections
import json
import re
import sys

CORE = ['ubos-types', 'ubos-crypto', 'ubos-tree', 'ubos-time', 'ubos-model', 'ubos-expr',
        'ubos-version', 'ubos-store', 'ubos-runtime', 'ubos-proof', 'ubos-dvm']
THRESHOLD = 85.0


def main(src, dst):
    data = json.load(open(src))
    per = collections.defaultdict(lambda: [0, 0])
    for f in data['data'][0]['files']:
        m = re.search(r'/crates/(ubos-[a-z]+)/', f['filename'])
        if not m:
            continue
        lines = f['summary']['lines']
        per[m.group(1)][0] += lines['count']
        per[m.group(1)][1] += lines['covered']
    out = {'threshold_percent': THRESHOLD, 'kernel_crates': CORE, 'crates': {}}
    total = [0, 0]
    for crate in CORE + sorted(k for k in per if k not in CORE):
        count, covered = per[crate]
        pct = round(100.0 * covered / count, 1) if count else 0.0
        out['crates'][crate] = {'lines': count, 'covered': covered, 'percent': pct,
                                'kernel': crate in CORE}
        if crate in CORE:
            total[0] += count
            total[1] += covered
        print(f'{crate:14} {covered:6}/{count:6} {pct:6.1f}%')
    pct = round(100.0 * total[1] / total[0], 1) if total[0] else 0.0
    out['kernel'] = {'lines': total[0], 'covered': total[1], 'percent': pct}
    print(f'kernel crates  {total[1]:6}/{total[0]:6} {pct:6.1f}% (threshold {THRESHOLD}%)')
    json.dump(out, open(dst, 'w'), indent=2)
    open(dst, 'a').write('\n')
    return 0 if pct >= THRESHOLD else 1


if __name__ == '__main__':
    sys.exit(main(sys.argv[1], sys.argv[2]))
