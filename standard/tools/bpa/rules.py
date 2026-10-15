"""Validations, invariants, commit-enforced declarations and action decisions (BPA 1.0-rc2,
STD-LGS-030, -031; PH-1 step 10c, DEC-067).

The declarations come from the cascade of STD-LGS-022 (`validations`, `invariants` and
`decisions` are kernel properties, STD-LGS-011) and, for validations, from the class
declarations and extensions of the object (FR-RULE-031). `validate` previews a commit;
`decide` evaluates the decisions of a port call without running it."""
from . import BpaError
from . import expr as _expr

PROTECTED = ('main', 'release', 'overlay')
STEP_OF = {'approvals': 'approval', 'require_signature': 'signature', 'maker_checker': 'second_person'}


class DryRun(Exception):
    """Raised inside a dry-run commit with its result."""

    def __init__(self, result):
        self.result = result


def _rule_ok(classes, cls, rule, body, old, params_types=None, params=None, ctx=None):
    """`True` when an L1 rule holds; an evaluation failure counts as not holding."""
    # `old` is the base version; on a create its members are all null (DEC-067)
    i = {'expr': rule, 'classes': classes, 'class': cls, 'this': body, 'old': old if old is not None else {}}
    if params_types is not None:
        i['params_types'], i['params'] = params_types, params or {}
    if ctx is not None:
        i['ctx'] = ctx
    try:
        return _expr.eval_expr(i, typed_ref=lambda uri: None)['value'] is True
    except BpaError as e:
        if e.code in ('LOGIC.TYPE_ERROR', 'LOGIC.NULL_ACCESS'):
            return False
        raise


class Rules:
    """Store mixin."""

    MAX_VIOLATIONS = 100

    def _rule_ctx(self, branch, p):
        from .store import branch_kind
        ctx = {'branch_kind': branch_kind(branch), 'channel': p.get('channel', 'api'),
               'principal_kind': p.get('principal_kind', 'human'), 'vae': self.TENANT}
        if p.get('role') is not None:
            ctx['role'] = p['role']
        return ctx

    # -- defaults before a create (STD-LGS-011 `field_default`) --

    def _apply_defaults(self, st, cls, oid, body):
        branch, p = getattr(self, '_cur_branch', None), getattr(self, '_cur_p', None)
        eff_cls = self._cls(cls)
        if branch is None or eff_cls['kind'] != 'definition' or 'ConfigAsset' in eff_cls['linearization'] \
                or 'LogicAsset' in eff_cls['linearization']:
            return body
        eff, _ = self._effective(st, branch, oid, self._rule_ctx(branch, p), self._default_valid(cls),
                                 body=body, cls=cls)
        out = dict(body)
        for path, v in eff.items():
            if path.startswith('field_default.') and out.get(path[len('field_default.'):]) is None:
                out[path[len('field_default.'):]] = v
        return out

    # -- commit enforcement (STD-LGS-011 commit properties, STD-LGS-030) --

    def _enforce(self, branch, st, view, result, p, via_change_set):
        """Commit-enforced declarations and the validations and invariants of the objects
        written; raises `RULE.DECLARATION_UNSATISFIED`, `VER.CHANGE_SET_REQUIRED` or
        `RULE.VALIDATION_FAILED`; returns the warnings and infos."""
        ctx = self._rule_ctx(branch, p)
        classes = list(self.decls.values())
        violations, warnings = [], []
        invariants = {}
        # Business objects only: ConfigAsset and LogicAsset objects (sheets, extensions, logic)
        # are governed by write policy and change sets, not by the declarations they carry.
        def governed(oid):
            lin = self._cls(st['objects'][oid]['class'])['linearization']
            return st['objects'][oid]['kind'] == 'definition' and 'ConfigAsset' not in lin and 'LogicAsset' not in lin
        written = [(oid, vh) for oid, vh in self._written if governed(oid)]
        ended = [oid for oid, _ in self._ended if governed(oid)]
        for oid in ended:
            if oid in dict(written):
                continue
            eff, _ = self._effective(st, branch, oid, ctx, self._default_valid(st['objects'][oid]['class']))
            if eff.get('read_only') is True:
                raise BpaError('RULE.DECLARATION_UNSATISFIED', {'property': 'read_only', 'object_id': oid})
        for oid, vh in written:
            o = st['objects'][oid]
            cls = o['class']
            body = self.versions[vh]['body']
            existed = oid in view['objects']
            old = self.versions[view['objects'][oid]['head']]['body'] if existed else None
            eff, _ = self._effective(st, branch, oid, ctx, self._default_valid(cls), body=body)
            if eff.get('read_only') is True and existed:
                raise BpaError('RULE.DECLARATION_UNSATISFIED', {'property': 'read_only', 'object_id': oid})
            for f in eff.get('field_required') or []:
                if body.get(f) is None:
                    raise BpaError('RULE.DECLARATION_UNSATISFIED', {'property': 'field_required', 'object_id': oid,
                                                                     'field': f})
            if eff.get('audit_level') == 'strict' and (not p.get('message') or not p.get('reason')):
                raise BpaError('RULE.DECLARATION_UNSATISFIED', {'property': 'audit_level', 'object_id': oid,
                                                                 'message': 'a strict commit carries message and reason'})
            if eff.get('write_policy') == 'change_set' and not via_change_set \
                    and self.branches[branch]['kind'] in PROTECTED:
                raise BpaError('VER.CHANGE_SET_REQUIRED')
            rules = list(self._cls(cls).get('validations') or []) + list(eff.get('validations') or [])
            for r in rules:
                if not _rule_ok(classes, cls, r['rule'], body, old):
                    item = {'code': 'RULE.VALIDATION_FAILED', 'object_id': oid, 'name': r['name'],
                            'severity': r['severity'], 'message': r.get('message', r['name'])}
                    (violations if r['severity'] == 'error' else warnings).append(item)
            for inv in eff.get('invariants') or []:
                invariants.setdefault(inv['name'], inv)
        for name in sorted(invariants):
            inv = invariants[name]
            for oid, vh in written:
                cls = st['objects'][oid]['class']
                if not set(inv['scope']) & set(self._cls(cls)['linearization']):
                    continue
                if not _rule_ok(classes, cls, inv['rule'], self.versions[vh]['body'], None):
                    item = {'code': 'RULE.INVARIANT_VIOLATED', 'object_id': oid, 'name': name,
                            'severity': inv['severity'], 'message': inv.get('message', name)}
                    (violations if inv['severity'] == 'error' else warnings).append(item)
        if violations:
            raise BpaError('RULE.VALIDATION_FAILED', {'violations': violations[:self.max_violations]})
        return warnings

    # -- validation preview (FR-RULE-035, STD-ISA-054) --

    def validate(self, branch, changes, process=None):
        """The violations and warnings a commit of `changes` would produce, without committing."""
        p = self._process(process)
        self._check_active(branch)
        versions, derivations = set(self.versions), set(getattr(self, 'derivations', {}) or {})
        self._dry = True
        try:
            self._do_commit(branch, changes, p)
        except DryRun as d:
            return {'violations': [], 'warnings': d.result.get('warnings', [])}
        except BpaError as e:
            if e.code == 'RULE.VALIDATION_FAILED':
                return {'violations': e.detail['violations'], 'warnings': []}
            raise
        finally:
            self._dry = False
            for h in [h for h in self.versions if h not in versions]:
                del self.versions[h]
            if hasattr(self, 'derivations'):
                for k in [k for k in self.derivations if k not in derivations]:
                    del self.derivations[k]
        raise BpaError('MODEL.INVALID_VALUE', 'unreachable')

    # -- decisions (STD-LGS-031) --

    def _decide_in(self, st, branch, oid, port, params, ctx):
        o = st['objects'].get(oid)
        if o is None or not self._live(st, oid):
            raise BpaError('IAM.NOT_FOUND', 'unknown object')
        cls = o['class']
        eff_cls = self._cls(cls)
        decl = (eff_cls.get('_ports') or eff_cls.get('ports') or {}).get(port)
        if decl is None:
            raise BpaError('MODEL.PORT_NOT_IMPLEMENTED', 'no port %s' % port)
        body = self.versions[o['head']]['body']
        ctx = dict(ctx or {})
        ctx.setdefault('principal_kind', 'human')
        from .store import branch_kind
        ctx.setdefault('branch_kind', branch_kind(branch))
        ctx.setdefault('vae', self.TENANT)
        ctx.setdefault('channel', 'api')
        eff, bindings = self._effective(st, branch, oid, ctx, self._default_valid(cls), body=body)
        if decl.get('kind') == 'query':
            return {'outcome': 'allow', 'explanations': [], 'bindings': bindings}
        denies, steps, explanations = [], [], []
        ptypes = {q['name']: q for q in decl.get('params') or []}
        classes = list(self.decls.values())
        for d in eff.get('decisions') or []:
            when = d.get('when')
            if when and not _rule_ok(classes, cls, when, body, None, ptypes, params, ctx):
                continue
            if d['outcome'] == 'allow':
                continue
            explanations.append({'name': d['name'], 'outcome': d['outcome'], 'message': d.get('message', d['name']),
                                 'steps': list(d.get('steps') or [])})
            if d['outcome'] == 'deny':
                denies.append(d['name'])
            else:
                steps += [s for s in d.get('steps') or [] if s not in steps]
        for prop, step in STEP_OF.items():
            v = eff.get(prop)
            if v is True or (isinstance(v, dict) and v.get('count', 0) > 0):
                explanations.append({'name': prop, 'outcome': 'require', 'message': prop, 'steps': [step],
                                     'value': v})
                if step not in steps:
                    steps.append(step)
        out = {'outcome': 'deny' if denies else 'require' if steps else 'allow',
               'explanations': explanations, 'bindings': bindings}
        if steps and not denies:
            out['steps'] = steps
        return out

    def decide(self, branch, object_id, port, params=None, ctx=None, sel=None):
        """`DAT-DecisionResult` of a port call on an object (STD-LGS-031)."""
        st = self._state(branch, sel or {})[0]
        return self._decide_in(st, branch, object_id, port, params or {}, ctx)

    def _check_decision(self, st, branch, oid, port, params, ctx):
        """Before a command port runs: deny and require stop the call (DSN-DVM-720)."""
        r = self._decide_in(st, branch, oid, port, params, ctx)
        if r['outcome'] == 'deny':
            raise BpaError('RULE.DECISION_DENIED', r)
        if r['outcome'] == 'require':
            raise BpaError('RULE.DECISION_REQUIRES', r)
        return r
