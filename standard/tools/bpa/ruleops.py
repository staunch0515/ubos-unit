"""Rule operations of BPA 1.0-rc11 (PH-2 step 5a, DEC-112): the end of a rule's attachment
(STD-LGS-070), the live impact count of a rule change (STD-LGS-071), rule tests and their
coverage (STD-LGS-072) and the test gate of change-set merges (STD-LGS-073). Mixed into the
reference store."""
import copy

from . import BpaError
from .sheets import registry, _normalise  # noqa: F401
from .tables import evaluate

RULE_TEST = 'RuleTest'
RULE_OBJECTS = ('GovernanceSheet', 'DecisionTable')
CANDIDATES = ('GovernanceSheet', 'PropertyDecl', 'DecisionTable')
TEST_KINDS = ('cascade', 'decision', 'table', 'validation')


def _incomplete(what):
    return BpaError('MODEL.DECLARATION_INCOMPLETE', what)


def _invalid(what):
    return BpaError('MODEL.INVALID_VALUE', what)


def check_test(b):
    """STD-LGS-072: the declaration of a rule test."""
    kind = b.get('kind')
    if kind not in TEST_KINDS:
        raise _invalid('a rule test kind is cascade, decision, table or validation')
    inp, exp = b.get('input'), b.get('expect')
    if not isinstance(inp, dict) or not isinstance(exp, dict):
        raise _incomplete('a rule test has an input and an expectation')
    if kind == 'table':
        if not isinstance(inp.get('table'), str) or not isinstance(inp.get('inputs'), dict):
            raise _incomplete('a table test names the table and its inputs')
    else:
        if not isinstance(inp.get('class'), str) or not isinstance(inp.get('body'), dict):
            raise _incomplete('a rule test names the class and the body of its input object')
        if kind == 'decision' and not isinstance(inp.get('port'), str):
            raise _incomplete('a decision test names the port')
    keys = {'cascade': ('properties',), 'decision': ('outcome',), 'table': ('outputs',),
            'validation': ('violations',)}[kind]
    if not any(k in exp for k in keys + ('error',)):
        raise _incomplete('the expectation of a %s test has %s or error' % (kind, ' or '.join(keys)))


class RuleOps:

    # ---- the end of an attachment (STD-LGS-070) ----

    @staticmethod
    def _gaps(tl):
        """The ends of a timeline's segments where no segment starts."""
        starts = {f for f, _, _ in tl}
        return sorted({t for _, t, _ in tl if t is not None and t not in starts})

    def _schedule_detach(self, branch, st, result):
        if not hasattr(self, 'detach_schedule'):
            self.detach_schedule = []
        for oid in result['versions']:
            o = st['objects'].get(oid)
            if o is None or o['class'] not in RULE_OBJECTS:
                continue
            for t in self._gaps(o['timeline']):
                key = (branch, oid, t)
                if not self._due(t, o['class']) and key not in self.detach_schedule:
                    self.detach_schedule.append(key)

    def _tick_detach(self):
        keep = []
        for key in getattr(self, 'detach_schedule', []):
            branch, oid, t = key
            o = self._view(branch, self.branches[branch]['head'])[0]['objects'].get(oid)
            if o is None:
                continue
            if not self._due(t, o['class']):
                keep.append(key)
                continue
            if t in self._gaps(o['timeline']):
                self.outbox.append({'effect_id': 'rule-detached/%s/%s/%s' % (branch, oid, t),
                                    'type': 'RuleDetached', 'branch': branch, 'object_id': oid,
                                    'payload': {'class': o['class'], 'valid_to': t}})
        self.detach_schedule = keep

    # ---- live impact (STD-LGS-071) ----

    def _governed_objects(self, st):
        out = []
        for oid in sorted(st['objects']):
            o = st['objects'][oid]
            lin = self._cls(o['class'])['linearization']
            if o['kind'] != 'definition' or 'ConfigAsset' in lin or 'LogicAsset' in lin:
                continue
            if not self._live(st, oid):
                continue
            out.append((oid, o))
        return out

    def _outcome(self, st, branch, oid, o, ctx):
        """The effective declarations and the failing validations of an object."""
        from .rules import _rule_ok
        from .store import tl_at
        asof = self._default_valid(o['class'])
        seg = tl_at(o['timeline'], asof)
        body = self.versions[seg[2] if seg else o['head']]['body']
        eff, _ = self._effective(st, branch, oid, ctx, asof, body=body)
        classes = list(self.decls.values())
        rules = list(self._cls(o['class']).get('validations') or []) + list(eff.get('validations') or [])
        failing = sorted(r['name'] for r in rules if not _rule_ok(classes, o['class'], r['rule'], body, None))
        return eff, failing

    def rule_impact(self, i):
        """The current objects of the target whose effective declarations or validation
        outcomes would change under the rules of the source branch, per class."""
        target, source = i.get('branch', 'main'), i.get('source')
        self._branch(source)
        T = self._state(target)[0]
        S = self._state(source)[0]
        mixed = {'objects': {k: v for k, v in T['objects'].items() if v['class'] not in CANDIDATES},
                 'ledger': T['ledger']}
        for k, v in S['objects'].items():
            if v['class'] in CANDIDATES:
                mixed['objects'][k] = v
        from .rules import Rules  # noqa: F401
        ctx = dict(i.get('ctx') or {}, vae=self.TENANT)
        only = i.get('classes')
        by_class, n = {}, 0
        for oid, o in self._governed_objects(T):
            lin = self._cls(o['class'])['linearization']
            if only and not set(only) & set(lin):
                continue
            n += 1
            before = self._outcome(T, target, oid, o, ctx)
            after = self._outcome(mixed, target, oid, o, ctx)
            if before != after:
                by_class[o['class']] = by_class.get(o['class'], 0) + 1
        return {'branch': target, 'source': source, 'evaluated': n, 'changed': sum(by_class.values()),
                'by_class': by_class}

    # ---- rule tests (STD-LGS-072) ----

    def _provisional(self, st, cls, body):
        """A state with a provisional object of `cls` and `body` (not stored)."""
        oid = '00000000-0000-7000-8000-000000000000'
        vh = 'provisional:%d' % len(self.versions)
        lc = self._lifecycle(st, cls)
        self.versions[vh] = {'object_id': oid, 'class': cls, 'slug': '', 'body': body, 'tombstone': False,
                             'state': lc[0]['initial'] if lc else None, 'cseq': self._cseq,
                             'valid': {'from': self._default_valid(cls), 'to': None}}
        o = {'class': cls, 'kind': self._cls(cls)['kind'], 'slug': 'provisional', 'head': vh,
             'timeline': [[self._default_valid(cls), None, vh]]}
        return {'objects': dict(st['objects'], **{oid: o}), 'ledger': st['ledger']}, oid, o, vh

    def _rule_test_one(self, st, branch, tid, body, hits):
        from .rules import _rule_ok
        kind, inp, exp = body['kind'], body['input'], body['expect']
        try:
            if kind == 'table':
                oid, o = self._table(st, inp['table'])
                from .store import tl_at
                seg = tl_at(o['timeline'], inp.get('asof') or self._today())
                if seg is None:
                    raise BpaError('QRY.NOT_FOUND_AT_TIME')
                r = evaluate(self.versions[seg[2]]['body'], inp['inputs'])
                hits['rows'] |= {(oid, n) for n in r['rows']}
                actual = {'outputs': r['outputs']}
            else:
                self._cls(inp['class'])
                pst, oid, o, vh = self._provisional(st, inp['class'], inp['body'])
                try:
                    ctx = dict(inp.get('ctx') or {}, vae=self.TENANT)
                    asof = self._default_valid(inp['class'])
                    out, _ = self._cascade_in(pst, branch, oid, o, inp['body'], ctx, asof)
                    for _, (_, cands) in out.items():
                        hits['blocks'] |= {c['position'][:2] for c in cands}
                    eff = {p: v for p, (v, _) in out.items()}
                    if kind == 'cascade':
                        actual = {'properties': {p: eff.get(p) for p in sorted(exp.get('properties') or {})}}
                    elif kind == 'decision':
                        r = self._decide_in(pst, branch, oid, inp['port'], inp.get('params') or {}, ctx)
                        actual = {'outcome': r['outcome']}
                        if r.get('steps'):
                            actual['steps'] = r['steps']
                    else:
                        classes = list(self.decls.values())
                        rules = list(self._cls(inp['class']).get('validations') or []) + \
                            list(eff.get('validations') or [])
                        hits['validations'] |= {r['name'] for r in rules}
                        actual = {'violations': sorted(r['name'] for r in rules
                                                       if not _rule_ok(classes, inp['class'], r['rule'],
                                                                       inp['body'], None))}
                finally:
                    del self.versions[vh]
        except BpaError as e:
            actual = {'error': e.code}
        if 'error' in exp or 'error' in actual:
            passed = exp.get('error') == actual.get('error')
        elif kind == 'decision':
            passed = actual['outcome'] == exp['outcome'] and ('steps' not in exp or
                                                              exp['steps'] == actual.get('steps', []))
        elif kind == 'validation':
            passed = actual['violations'] == sorted(exp['violations'])
        else:
            key = 'properties' if kind == 'cascade' else 'outputs'
            passed = actual[key] == exp[key]
        return {'test_id': tid, 'name': body['name'], 'kind': kind, 'passed': passed, 'actual': actual}

    def _rule_tests_of(self, st):
        return [(oid, self.versions[o['head']]['body']) for oid, o in sorted(st['objects'].items())
                if o['class'] == RULE_TEST and self._live(st, oid)]

    def _rule_test_run(self, st, branch, ids=None):
        hits = {'blocks': set(), 'rows': set(), 'validations': set()}
        results = [self._rule_test_one(st, branch, tid, b, hits) for tid, b in self._rule_tests_of(st)
                   if ids is None or tid in ids]
        return results, hits

    def run_rule_tests(self, i):
        """STD-LGS-072: run the rule tests of a branch (all, or those named) and report
        their results and the coverage of the rules."""
        branch = i.get('branch', 'main')
        st = self._state(branch, i)[0]
        ids = i.get('tests')
        results, hits = self._rule_test_run(st, branch, set(ids) if ids is not None else None)
        blocks = set()
        for sid, (_, sbody) in self._sheet_objects(st).items():
            blocks |= {(sid, n) for n in range(len(sbody['ast']['blocks']))}
        rows = set()
        validations = set()
        for oid, o in st['objects'].items():
            if not self._live(st, oid):
                continue
            body = self.versions[o['head']]['body']
            if o['class'] == 'DecisionTable':
                rows |= {(oid, n) for n in range(len(body.get('rows') or []))}
            elif o['class'] == 'GovernanceSheet':
                for b in body['ast']['blocks']:
                    for d in b['declarations']:
                        if d['property'] == 'validations' and isinstance(d['value'], list):
                            validations |= {v['name'] for v in d['value'] if isinstance(v, dict) and 'name' in v}
        for d in self.decls.values():
            validations |= {v['name'] for v in d.get('validations') or []}

        def cov(hit, total):
            return {'hit': len(hit & total), 'total': len(total)}
        return {'results': results, 'passed': sum(1 for r in results if r['passed']),
                'failed': sum(1 for r in results if not r['passed']),
                'coverage': {'blocks': cov(hits['blocks'], blocks), 'rows': cov(hits['rows'], rows),
                             'validations': cov(hits['validations'], validations)}}

    # ---- the test gate (STD-LGS-073) ----

    def _test_gate(self, source, target):
        """Before a change set merges into a protected branch: the rule tests the change
        affects, run at the head of the source, must pass (`RULE.TESTS_FAILED`)."""
        from .store import PROTECTED
        if self.branches[target]['kind'] not in PROTECTED:
            return
        S, T = self._state(source)[0], self._state(target)[0]
        changed = {oid for oid, o in S['objects'].items()
                   if (T['objects'].get(oid) or {}).get('head') != o['head']}
        rules_changed = any(S['objects'][oid]['class'] in CANDIDATES + ('Logic', 'ClassExtension')
                            for oid in changed)
        ids = set()
        for tid, b in self._rule_tests_of(S):
            tgt = b.get('target')
            if tid in changed or (tgt is not None and tgt in changed) or (tgt is None and rules_changed):
                ids.add(tid)
        if not ids:
            return
        results, _ = self._rule_test_run(S, source, ids)
        failed = [r['test_id'] for r in results if not r['passed']]
        if failed:
            raise BpaError('RULE.TESTS_FAILED', {'failed': failed})
