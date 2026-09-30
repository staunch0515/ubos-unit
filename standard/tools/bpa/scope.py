"""Scope rule of a BPA release (DEC-039).

A release in development states its rule in scope.json:
  {"base": "<released version>", "gate": "1a", "profiles": [...]}
Its scope is the scope of the base release plus every clause of the listed profiles whose
earliest satisfied requirement lies in PH-0 or in the named gate of PH-1 (the gate of a PH-1
requirement is read from the generated feature list UBS-PHF-1). A clause without a
**Satisfies** line is definitional and belongs to the scope when its profile is listed.
"""
import json
import os
import re

from .spec import REPO

SYS = os.path.join(REPO, 'docs', '50-system')
REQ = re.compile(r'^### ((?:FR|NR|CR)-[A-Z0-9]+-\d{3}) — .*?\n(.*?)(?=^### |\Z)', re.M | re.S)
GATE_ROW = re.compile(r'\| ((?:FR|NR|CR)-[A-Z0-9]+-\d{3}) \| [A-Za-z]+ \| (1[ab]) \|')
REF = re.compile(r'(?:FR|NR|CR)-[A-Z0-9]+-\d{3}')


def requirement_phases():
    """{requirement: 'PH-n' or 'PH-1/1a' | 'PH-1/1b'}."""
    phase = {}
    for dirpath, _, names in os.walk(os.path.join(SYS, '10-requirements')):
        for n in names:
            if n.endswith('.md'):
                text = open(os.path.join(dirpath, n), encoding='utf-8').read()
                for m in REQ.finditer(text):
                    p = re.search(r'\*\*Phase:\*\* (PH-\d)', m.group(2))
                    phase[m.group(1)] = p.group(1) if p else '?'
    feats = open(os.path.join(SYS, '50-phases', 'PH-1', '01-features.md'), encoding='utf-8').read()
    for m in GATE_ROW.finditer(feats):
        if phase.get(m.group(1)) == 'PH-1':
            phase[m.group(1)] = 'PH-1/' + m.group(2)
    return phase


def rule_scope(cat, doc, std_dir):
    """The sorted clause list the rule of scope document `doc` gives, or None without a rule."""
    rule = doc.get('rule')
    if not rule:
        return None
    base = json.load(open(os.path.join(std_dir, 'release', rule['base'], 'scope.json')))
    out = {'STD-%s-%s' % (a, n) for a, ns in base['clauses'].items() for n in ns}
    wanted = {'PH-0', 'PH-1/' + rule['gate']}
    phase = requirement_phases()
    for cid, c in cat.items():
        if c['profile'] not in rule['profiles']:
            continue
        sat = re.search(r'\*\*Satisfies:\*\*(.*)', c['block'])
        refs = REF.findall(sat.group(1)) if sat else []
        if not refs or any(phase.get(r) in wanted for r in refs):
            out.add(cid)
    return sorted(out, key=lambda c: (c.split('-')[1], c))


def scope_doc_clauses(clause_ids, order):
    """Group clause IDs by area in the order of `order` (area list), for scope.json."""
    groups = {}
    for c in clause_ids:
        _, area, n = c.split('-')
        groups.setdefault(area, []).append(n)
    return {a: sorted(groups[a]) for a in order if a in groups} | {
        a: sorted(v) for a, v in groups.items() if a not in order}
