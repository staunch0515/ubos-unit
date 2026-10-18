#!/usr/bin/env python3
"""Generate the PH-1 verification results by requirement (docs/50-system/60-verification/
31-results-trace-ph1.md; VER-INSP-9500, EXIT-1-20).

It joins the traceability of `gen_traceability.py` (the suites that verify a requirement's design
items and standard clauses, and the verification items naming it) with the verification results
index `30-results-ph1.md` (the result of every suite and verification item in PH-1, with its
evidence, and requirement-level results where a suite's result is not the requirement's). A suite counts for a
requirement when it runs one of the verification methods the requirement states. A Must
requirement is **verified** when a requirement-level result says `met` or `met-workstation`, or,
without one, when at least one of its suites or direct items has the result `passed` or
`workstation`. The Must requirements without such a result are the EXIT-1-20 gaps; they are
listed with the results of their suites. Gaps are reported, not validation errors.

Usage: python3 docs/tools/gen_results.py [--json]
"""
import collections
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gen_traceability as gt  # noqa: E402

OUT = os.path.join(gt.ROOT, '60-verification', '31-results-trace-ph1.md')
INDEX = os.path.join(gt.ROOT, '60-verification', '30-results-ph1.md')
PASSING = {'passed', 'workstation'}
MET = {'met', 'met-workstation'}
ROW = re.compile(r'^\| `([a-z-]+)` \| ((?:SUITE|VER)-[A-Z0-9-]+|(?:FR|NR|CR)-[A-Z0-9]+-\d{3}) \| ([^|]*)\|', re.M)


def load():
    reqs, dsns, vers, stds = {}, {}, {}, {}
    suites = []
    for rel, text in gt.files():
        if rel.startswith('10-requirements'):
            for m in gt.REQ.finditer(text):
                b = m.group(4)
                pr = re.search(r'\*\*Priority:\*\* (\w+)', b)
                ph = re.search(r'\*\*Phase:\*\* (PH-\d)', b)
                methods = {x.strip() for x in gt.field(b, 'Verification').split(',') if x.strip()}
                reqs[m.group(1)] = (m.group(3).strip(), pr.group(1) if pr else '', ph.group(1) if ph else '',
                                    methods)
        if rel.startswith('40-systems'):
            for m in gt.DSN.finditer(text):
                b = m.group(4)
                dsns[m.group(1)] = (m.group(2), gt.expand(gt.field(b, 'Origin')),
                                    set(x.strip() for x in gt.field(b, 'Verification').split(',')))
        if rel.startswith('30-standard'):
            for m in gt.STD.finditer(text):
                stds[m.group(1)] = gt.expand(gt.field(m.group(3), 'Satisfies'))
        if rel.startswith('60-verification'):
            for m in gt.VER.finditer(text):
                vers[m.group(1)] = gt.expand(gt.field(m.group(3), 'Verifies'))
            if rel.endswith('04-suites.md'):
                for m in re.finditer(r'^\| ([A-Z]+) \| (SUITE-[A-Z0-9-]+) \| [^|]+\| ([^|]+)\|', text, re.M):
                    suites.append((m.group(1), m.group(2), set(x.strip() for x in m.group(3).split(','))))
    return reqs, dsns, vers, stds, suites


def coverage(reqs, dsns, vers, stds, suites):
    """Requirement → (suites, direct verification items)."""
    realised = collections.defaultdict(list)
    for d, (_, origin, _) in dsns.items():
        for r in origin:
            realised[r].append(d)
    by_std = collections.defaultdict(list)
    for c, sat in stds.items():
        for r in sat:
            by_std[r].append(c)
    direct = collections.defaultdict(list)
    for v, verifies in vers.items():
        for r in verifies:
            direct[r].append(v)
    by_suite = {s: m for (_, s, m) in suites}
    out = {}
    for r in reqs:
        ss = set()
        for d in realised.get(r, []):
            sysname, _, methods = dsns[d]
            ss |= {s for (s_sys, s, s_methods) in suites if s_sys == sysname and methods & s_methods}
        if by_std.get(r):
            ss.add('SUITE-DVM-CONF')
        # a suite verifies the requirement by one of the methods the requirement states
        # (a BENCH target is not verified by a CONF suite)
        wanted = reqs[r][3]
        if wanted:
            ss = {s for s in ss if by_suite.get(s, set()) & wanted}
        out[r] = (sorted(ss), sorted(direct.get(r, [])))
    return out


def index():
    """The results index: id → (result, evidence)."""
    text = open(INDEX, encoding='utf-8').read()
    return {m.group(2): (m.group(1), m.group(3).strip()) for m in ROW.finditer(text)}


def main():
    reqs, dsns, vers, stds, suites = load()
    cov = coverage(reqs, dsns, vers, stds, suites)
    res = index()
    must = sorted(r for r, v in reqs.items() if v[2] == 'PH-1' and v[1] == 'Must')
    rows, gaps, unknown = [], [], set()
    counts = collections.Counter()
    for r in must:
        ss, vs = cov[r]
        own = res.get(r)
        for x in ss + vs:
            if x not in res:
                unknown.add(x)
        passing = [x for x in ss + vs if res.get(x, ('', ''))[0] in PASSING]
        if own:
            verified = own[0] in MET
            how = '%s (requirement result)' % own[0]
        else:
            verified = bool(passing)
            how = ', '.join(passing[:4]) + (' …' if len(passing) > 4 else '') if passing else '—'
        kind = 'verified' if verified else 'gap'
        if verified and (own and own[0] == 'met-workstation' or not own and not any(
                res.get(x, ('', ''))[0] == 'passed' for x in passing)):
            kind = 'verified on the workstation'
        counts[kind] += 1
        if not verified:
            gaps.append(r)
        rows.append('| %s | %s | %s | %s |' % (reqs[r][0], r, kind, how))
    lines = ['---', 'id: UBS-VER-31', 'title: Verification results by requirement — PH-1 (generated)',
             'status: draft', 'phase: PH-1', 'depends_on: [UBS-VER-22, UBS-VER-30]', '---', '',
             '# Verification results by requirement — PH-1', '',
             '> Generated by `docs/tools/gen_results.py` from the traceability (UBS-VER-22) and the results',
             '> index (UBS-VER-30). Do not edit by hand.', '',
             'A Must requirement is **verified** when its requirement-level result is `met` (or',
             '`met-workstation`), or, without one, when a suite verifying it or a verification item naming it',
             'has the result `passed` (or `workstation`: passed on the workstation, the reference-environment',
             'run pending). A suite counts when it runs one of the verification methods the requirement states',
             '(a BENCH target is not verified by a CONF suite). The last column names the passing suites and items.', '',
             '| Must requirements | Verified | Verified on the workstation only | Gaps |', '|---|---|---|---|',
             '| %d | %d | %d | %d |' % (len(must), counts['verified'], counts['verified on the workstation'],
                                       counts['gap']), '',
             '| Title | Requirement | Result | By |', '|---|---|---|---|'] + rows + ['']
    lines += ['## Gaps (EXIT-1-20)', '']
    if gaps:
        lines += ['Must requirements without a passing result, with the results of their suites and items.', '',
                  '| Title | Requirement | Requirement result | Suites and items (result) |', '|---|---|---|---|']
        for r in gaps:
            ss, vs = cov[r]
            parts = ['%s (%s)' % (x, res.get(x, ('unknown', ''))[0]) for x in ss + vs]
            own = res.get(r)
            lines.append('| %s | %s | %s | %s |' % (reqs[r][0], r, own[0] if own else '—', ', '.join(parts) or '—'))
        lines.append('')
    else:
        lines += ['None.', '']
    if unknown:
        lines += ['## Suites and items without a result in the index', '',
                  ', '.join(sorted(unknown)), '']
    with open(OUT, 'w', encoding='utf-8') as fh:
        fh.write('\n'.join(lines))
    summary = {'must': len(must), 'verified': counts['verified'],
               'workstation': counts['verified on the workstation'], 'gaps': gaps, 'unknown': sorted(unknown)}
    if '--json' in sys.argv:
        print(json.dumps(summary, indent=1))
    else:
        print('PH-1 results: %d Must, %d verified, %d on the workstation only, %d gaps, %d without a result' % (
            len(must), counts['verified'], counts['verified on the workstation'], len(gaps), len(unknown)))


if __name__ == '__main__':
    main()
