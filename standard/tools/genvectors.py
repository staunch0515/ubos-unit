#!/usr/bin/env python3
"""Generate the conformance vectors of a BPA release (DEC-032).

Vector cases are written in cases/*.py as operations with inputs and, for negative steps,
the expected error code. The expected results are computed by the reference oracle in bpa/;
a case whose declared error does not occur, or that fails unexpectedly, stops generation.
Each vector gets the next free ID in the VER-CONF range of its first clause and is written
as canonical JSON to standard/vectors/<profile>/VER-CONF-nnnn.json.

Usage: python3 standard/tools/genvectors.py
"""
import importlib
import json
import os
import shutil
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from bpa.canon import canonical                          # noqa: E402
from bpa.ops import run_step, pointer, run_vector, strip, UBTP_STRIPPED  # noqa: E402
from bpa.spec import clauses, REPO                       # noqa: E402
from bpa.store import Store                              # noqa: E402

OUT = os.path.join(REPO, 'standard', 'vectors')
AREAS = ['fnd', 'obj', 'cls', 'kind', 'ver', 'time', 'txn', 'proof', 'err', 'model', 'history', 'proofs', 'language', 'execution',
         'sheets', 'rules', 'core1b', 'ubtp', 'flow', 'periods', 'approvals', 'workflows', 'evolution',
         'protection', 'integration', 'search', 'finance', 'work', 'ruleops', 'durable']
# Stateful operations whose expected results are stripped of implementation-dependent members
# and compared as subsets by default (STD-CONF-005, rule 6); the operations of step 1b keep
# exact matching.
STRIPPED = ('commit', 'read', 'read_ledger', 'actions', 'merge', 'diff', 'export_state', 'create_branch',
            'branch_info', 'commit_info', 'read_range', 'history', 'tree_diff', 'merge_preview', 'change_set',
            'tag', 'tag_info', 'release', 'revert', 'process_info', 'lineage', 'derivation', 'impact', 'outbox',
            'bulk', 'commit_record', 'prove', 'consistency_proof', 'tenant_root', 'export_archive',
            'import_archive', 'resolve', 'query', 'execute', 'journal', 'metering', 'metering_total', 'replay',
            'verify_asset', 'bpu_descriptor', 'bpu_state', 'verify_bpu', 'cascade', 'explain_cascade', 'decide', 'ubtp',
            'close_period', 'reopen_period', 'match', 'unmatch', 'restate',  # 1.0-rc8 (DEC-090)
            'decide_table', 'pending_change', 'approve', 'simulate',       # 1.0-rc8 (DEC-091)
            'start_workflow', 'deliver_event', 'complete_task', 'migrate_workflow', 'run_saga',
            'quarantine_policy', 'release_quarantine', 'change_class', 'upstream_changes',
            'resolve_upstream',                                             # 1.0-rc8 (DEC-092)
            'subject_key', 'key_availability', 'erase_subject', 'stored_form', 'disclose', 'lookup_key',
            'relations', 'check_access',                                    # 1.0-rc8 (DEC-093)
            'egress_allow', 'connector_stub', 'exchanges', 'principal', 'resolve_path',
            'rest',                                                         # 1.0-rc8 (DEC-094)
            'resolve_view', 'index_lag',                                    # 1.0-rc8 (DEC-095)
            'currency', 'fx_rate', 'ingest_statement', 'read_projection', 'projection_lag',
            'rebuild_projection',                                           # 1.0-rc9 (DEC-098)
            'delegate', 'revoke_delegation', 'delegations', 'task', 'inbox',
            'sla_report',                                                   # 1.0-rc10 (DEC-105)
            'rule_impact', 'run_rule_tests',                                # 1.0-rc11 (DEC-112)
            'sagas', 'saga_action')                                         # 1.0-rc12 (DEC-119)


# The time-zone vectors use only transitions that are stable in every tz database release
# since 2024; this is the release they were generated and checked with.
TZDB = '2025b'


def released_expect(vec, phase, i):
    """The expectation of step `i` of a vector as released (the file in `vectors/`, DEC-071)."""
    path = os.path.join(OUT, vec['profile'].lower(), vec['id'] + '.json')
    with open(path, encoding='utf-8') as fh:
        return json.load(fh)[phase][i - 1]['expect']


class Gen:
    def __init__(self):
        self.catalogue = clauses()
        self.next = {}
        self.vectors = []
        self.frozen = {}      # vector id -> reason, for vectors a later release supersedes

    def vector(self, clause, description, steps, setup=(), also=(), prototype=True, determinism=None):
        cl = self.catalogue[clause]
        lo, hi = cl['range']
        n = self.next.get(clause, lo)
        if n > hi:
            raise SystemExit('%s: VER-CONF range exhausted' % clause)
        self.next[clause] = n + 1
        store, results = Store(), {}
        vec = {'id': 'VER-CONF-%04d' % n, 'clauses': [clause] + list(also), 'profile': cl['profile'],
               'description': description, 'setup': [], 'steps': []}
        for phase, items in (('setup', setup), ('steps', steps)):
            for i, s in enumerate(items, 1):
                label = s.get('label', '%s%d' % (phase[0], i))
                step = {'op': s['op']}
                if 'input_text' in s:
                    step['input_text'] = s['input_text']
                else:
                    step['input'] = s.get('input', {})
                if s.get('frozen'):
                    # A released vector that a later release supersedes (DEC-067): its
                    # expectation is kept as released, the oracle is not consulted. A positive
                    # step keeps the expectation of the released file (DEC-071).
                    step['expect'] = {'error_code': s['error']} if s.get('error') else released_expect(vec, phase, i)
                    vec['steps'].append(step)
                    self.frozen[vec['id']] = s['frozen']
                    continue
                out = run_step(store, step, results)
                if 'label' in s:
                    step['label'] = s['label']
                if out[0] == 'ok':
                    results[label] = out[1]
                if s.get('error'):
                    if out[0] != 'error' or out[1] != s['error']:
                        raise SystemExit('%s %s: expected %s, oracle gave %r' % (vec['id'], label, s['error'], out))
                    step['expect'] = {'error_code': s['error']}
                    if s.get('details'):
                        step['expect']['details'] = out[2]
                elif out[0] == 'error':
                    raise SystemExit('%s %s: oracle error %s %s' % (vec['id'], label, out[1], out[2]))
                elif 'same_as' in s:
                    l, _, p = s['same_as'].partition('/')
                    if out[1] != pointer(results[l], p):
                        raise SystemExit('%s %s: not the same as %s' % (vec['id'], label, s['same_as']))
                    step['expect'] = {'same_as': s['same_as']}
                elif phase == 'steps' and s.get('check', True):
                    stateful = s['op'] in STRIPPED
                    res = strip(out[1], UBTP_STRIPPED if s['op'] in ('ubtp', 'rest', 'exchanges') else ()) if stateful else out[1]
                    step['expect'] = {'result': res, 'match': s.get('match', 'subset' if stateful else 'exact')}
                vec[phase].append(step)
        last = vec['steps'][-1].get('expect', {})
        vec['polarity'] = 'negative' if 'error_code' in last else 'positive'
        tags = ['prototype'] if prototype else []
        if tags:
            vec['tags'] = tags
        if determinism:
            vec['determinism'] = determinism
        if vec['id'] not in self.frozen:
            problems = run_vector(vec)
            if problems:
                raise SystemExit('%s does not replay on the oracle: %r' % (vec['id'], problems))
        self.vectors.append(vec)
        return vec

    def pure(self, clause, description, op, inp, error=None, text=None, **kw):
        step = {'op': op, 'input': inp} if text is None else {'op': op, 'input_text': text}
        if error:
            step['error'] = error
        return self.vector(clause, description, [step], **kw)


def main():
    g = Gen()
    g.tzdb = TZDB
    for area in AREAS:
        try:
            mod = importlib.import_module('cases.' + area)
        except ModuleNotFoundError:
            continue
        mod.cases(g)
    if os.path.isdir(OUT):
        for d in os.listdir(OUT):
            if os.path.isdir(os.path.join(OUT, d)):
                shutil.rmtree(os.path.join(OUT, d))
    for v in g.vectors:
        d = os.path.join(OUT, v['profile'].lower())
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, v['id'] + '.json'), 'w', encoding='utf-8') as fh:
            fh.write(canonical(v) + '\n')
    with open(os.path.join(OUT, 'superseded.txt'), 'w', encoding='utf-8') as fh:
        fh.write('# Vectors of a released BPA version that the release in development supersedes\n'
                 '# (DEC-067): kept as released, excluded from the release in development.\n')
        for vid in sorted(g.frozen):
            fh.write('%s %s\n' % (vid, g.frozen[vid]))
    per = {}
    for v in g.vectors:
        per[v['profile']] = per.get(v['profile'], 0) + 1
    print('vectors: %d (%s)' % (len(g.vectors), ', '.join('%s %d' % kv for kv in sorted(per.items()))))


if __name__ == '__main__':
    main()
