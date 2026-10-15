"""Lifecycles, transitions, guards and available actions (BPA 1.0-rc3, STD-CLS-004,
STD-ISA-017, STD-ISA-036, STD-ISA-050; PH-1 step 12a, DEC-071).

A class names its lifecycle object (`DAT-Lifecycle`, genesis class `Lifecycle`); the
lifecycle state is the `state` member of the object's versions, set to the initial state on
create and changed only by `commit.transition`."""
from . import BpaError
from . import expr as _expr
from .values import GRAMMARS

EFFECT_KINDS = ('set', 'logic')
FIELD_RULES = ('read_only', 'required', 'hidden')


def _member(name):
    return isinstance(name, str) and GRAMMARS['member-name'].match(name)


def check_lifecycle(body):
    """STD-CLS-004: the shape of a `DAT-Lifecycle` body; returns the transitions by name."""
    states = body.get('states')
    if not isinstance(states, list) or not states or not all(_member(s) for s in states) or len(set(states)) != len(states):
        raise BpaError('MODEL.INVALID_VALUE', 'states is a non-empty list of distinct state names')
    if body.get('initial') not in states:
        raise BpaError('MODEL.INVALID_VALUE', 'initial names a state')
    final = body.get('final') or []
    if not isinstance(final, list) or any(f not in states for f in final):
        raise BpaError('MODEL.INVALID_VALUE', 'final names states')
    trans = body.get('transitions')
    if not isinstance(trans, list):
        raise BpaError('MODEL.INVALID_VALUE', 'transitions is a list')
    by_name = {}
    for t in trans:
        if not isinstance(t, dict) or not isinstance(t.get('name'), str) or not isinstance(t.get('from'), list) \
                or not isinstance(t.get('to'), str):
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a transition has name, from and to')
        if not _member(t['name']):
            raise BpaError('MODEL.INVALID_NAME', 'transition %s' % t['name'])
        if t['name'] in by_name:
            raise BpaError('MODEL.INVALID_VALUE', 'duplicate transition %s' % t['name'])
        if not t['from'] or any(f not in states for f in t['from']) or t['to'] not in states:
            raise BpaError('MODEL.INVALID_VALUE', 'a transition leaves and enters declared states')
        for g in t.get('guards') or []:
            if not isinstance(g, dict) or not isinstance(g.get('name'), str) or not isinstance(g.get('rule'), str) \
                    or ('message' in g and not isinstance(g['message'], str)):
                raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a guard has name, rule and message')
            _expr.parse(g['rule'])
        by_name[t['name']] = t
    effects = body.get('effects') or {}
    if not isinstance(effects, dict) or any(s not in states for s in effects):
        raise BpaError('MODEL.INVALID_VALUE', 'effects maps declared states')
    for s, e in effects.items():
        if not isinstance(e, dict) or any(k not in ('entry', 'exit') for k in e):
            raise BpaError('MODEL.INVALID_VALUE', 'effects of a state are {entry, exit}')
        for items in e.values():
            if not isinstance(items, list):
                raise BpaError('MODEL.INVALID_VALUE', 'effects are lists')
            for ef in items:
                kinds = [k for k in EFFECT_KINDS if isinstance(ef, dict) and k in ef]
                if len(kinds) != 1 or len(ef) != 1:
                    raise BpaError('MODEL.INVALID_VALUE', 'an effect is {set} or {logic}')
                if kinds[0] == 'set':
                    if not isinstance(ef['set'], dict) or not ef['set']:
                        raise BpaError('MODEL.INVALID_VALUE', 'set maps fields to expressions')
                    for k, v in ef['set'].items():
                        if not _member(k) or not isinstance(v, str):
                            raise BpaError('MODEL.INVALID_VALUE', 'set maps fields to expressions')
                        _expr.parse(v)
                elif not isinstance(ef['logic'], str) or '#' not in ef['logic']:
                    raise BpaError('MODEL.INVALID_VALUE', 'logic names <asset>#<function>')
    rules = body.get('field_rules') or {}
    if not isinstance(rules, dict) or any(s not in states for s in rules):
        raise BpaError('MODEL.INVALID_VALUE', 'field_rules maps declared states')
    for r in rules.values():
        if not isinstance(r, dict) or any(k not in FIELD_RULES for k in r) \
                or any(not isinstance(v, list) or not all(_member(f) for f in v) for v in r.values()):
            raise BpaError('MODEL.INVALID_VALUE', 'field rules are lists of field names')
    return by_name


def extends(parent, child):
    """FR-FLOW-015: `child` keeps the initial state, every state and every transition of
    `parent` (a transition keeps its name and target and may leave more states)."""
    if child.get('initial') != parent.get('initial') or any(s not in child['states'] for s in parent['states']):
        return False
    ct = {t['name']: t for t in child.get('transitions') or []}
    for t in parent.get('transitions') or []:
        c = ct.get(t['name'])
        if c is None or c['to'] != t['to'] or any(f not in c['from'] for f in t['from']):
            return False
    return True


def findings(body):
    """STD-BPU-011 item 2: unreachable states and dead ends other than final states."""
    out = []
    reach, todo = {body['initial']}, [body['initial']]
    while todo:
        s = todo.pop()
        for t in body.get('transitions') or []:
            if s in t['from'] and t['to'] not in reach:
                reach.add(t['to'])
                todo.append(t['to'])
    for s in body['states']:
        if s not in reach:
            out.append({'state': s, 'finding': 'unreachable'})
        elif s not in (body.get('final') or []) and not any(s in t['from'] for t in body.get('transitions') or []):
            out.append({'state': s, 'finding': 'dead_end'})
    return out


def eval_value(classes, cls, text, body, params_types=None, params=None, ctx=None):
    """The value of an L1 expression over `this`, `params` and `ctx`; raises on failure."""
    i = {'expr': text, 'classes': classes, 'class': cls, 'this': body, 'old': {}}
    if params_types is not None:
        i['params_types'], i['params'] = params_types, params or {}
    if ctx is not None:
        i['ctx'] = ctx
    return _expr.eval_expr(i, typed_ref=lambda uri: None)['value']


def guard_ok(classes, cls, rule, body, params_types, params, ctx):
    """A guard holds when its rule evaluates to true; a failure to evaluate is a failed guard."""
    try:
        return eval_value(classes, cls, rule, body, params_types, params, ctx) is True
    except BpaError as e:
        if e.code in ('LOGIC.TYPE_ERROR', 'LOGIC.NULL_ACCESS'):
            return False
        raise


class Flow:
    """Store mixin: the lifecycle in force for a class, its checks and the available actions."""

    def _lifecycle(self, st, cls):
        """`(body, object_id, version_hash)` of the lifecycle in force for a class on a state,
        or None; the URI must resolve (`MODEL.REFERENCE_INVALID`)."""
        uri = self._cls(cls).get('lifecycle')
        if uri is None:
            return None
        oid = self._resolve(st, uri)
        if oid is None or not self._live(st, oid) or st['objects'][oid]['class'] != 'Lifecycle':
            raise BpaError('MODEL.REFERENCE_INVALID', 'lifecycle %s' % uri)
        vh = st['objects'][oid]['head']
        return self.versions[vh]['body'], oid, vh

    def _check_lifecycle_objects(self, st):
        """Checks of the lifecycle objects written by a commit: their shape and the extension
        rule against the lifecycles of the classes naming them (STD-CLS-004)."""
        written = [oid for oid, _ in self._written if st['objects'][oid]['class'] == 'Lifecycle']
        for oid in written:
            body = self.versions[st['objects'][oid]['head']]['body']
            check_lifecycle(body)
            uri = self._uri(st['objects'][oid])
            for name, eff in self.classes.items():
                if self._decl(name).get('lifecycle') != uri:
                    continue
                for anc in eff['linearization'][1:]:
                    puri = self._decl(anc).get('lifecycle')
                    if puri and puri != uri:
                        poid = self._resolve(st, puri)
                        if poid is not None and self._live(st, poid) and \
                                not extends(self.versions[st['objects'][poid]['head']]['body'], body):
                            raise BpaError('MODEL.NARROWING_VIOLATION', 'lifecycle of %s does not extend %s' % (name, anc))
                for other, oeff in self.classes.items():
                    if other != name and name in oeff['linearization']:
                        curi = self._decl(other).get('lifecycle')
                        if curi and curi != uri:
                            coid = self._resolve(st, curi)
                            if coid is not None and self._live(st, coid) and \
                                    not extends(body, self.versions[st['objects'][coid]['head']]['body']):
                                raise BpaError('MODEL.NARROWING_VIOLATION', 'lifecycle of %s does not extend %s' % (other, name))

    def _object_state(self, st, oid):
        o = st['objects'][oid]
        return self.versions[o['head']].get('state')

    def _actions_in(self, st, branch, oid, ctx, permit=None, params_of=None):
        """STD-ISA-036: the ports of an object with their availability and reasons."""
        o = st['objects'].get(oid)
        if o is None or not self._live(st, oid):
            raise BpaError('IAM.NOT_FOUND', 'unknown object')
        cls = o['class']
        eff = self._cls(cls)
        lc = self._lifecycle(st, cls)
        state = self._object_state(st, oid)
        body = self.versions[o['head']]['body']
        classes = list(self.decls.values())
        transitions = {t['name']: t for t in (lc[0].get('transitions') if lc else [])}
        out = []
        for name, p in sorted((eff.get('_ports') or {}).items()):
            a = {'port': name, 'kind': p.get('kind')}
            if p.get('transition'):
                a['transition'] = p['transition']
                t = transitions.get(p['transition'])
                if t:
                    a['to'] = t['to']
            reason = None
            if p.get('states') and state not in p['states']:
                reason = 'state'
            elif p.get('transition') and (p['transition'] not in transitions or state not in transitions[p['transition']]['from']):
                reason = 'state'
            elif permit is not None and not permit(p.get('permission', 'none'), cls):
                reason = 'permission'
            elif p.get('kind') == 'command':
                r = self._decide_in(st, branch, oid, name, {}, ctx)
                if r['outcome'] != 'allow':
                    reason = 'decision'
                    a['decision'] = r['outcome']
                else:
                    ptypes = {q['name']: q for q in p.get('params') or []}
                    failed = [g for g in p.get('guards') or [] if not guard_ok(classes, cls, g, body, ptypes, {}, ctx)]
                    if p.get('transition') and p['transition'] in transitions:
                        failed += ['%s/%s' % (p['transition'], g['name']) for g in transitions[p['transition']].get('guards') or []
                                   if not guard_ok(classes, cls, g['rule'], body, ptypes, {}, ctx)]
                    if failed:
                        reason = 'guard'
                        a['guards'] = failed
            a['available'] = reason is None
            if reason:
                a['reason'] = reason
            out.append(a)
        return {'state': state, 'actions': out}

    def actions(self, branch, object_id, ctx=None, sel=None):
        """The adapter operation `actions` (STD-CONF-005): every port permitted."""
        from .store import branch_kind
        st, _, ev, _ = self._state(branch, sel or {})
        ctx = dict(ctx or {})
        ctx.setdefault('principal_kind', 'human')
        ctx.setdefault('branch_kind', branch_kind(branch))
        ctx.setdefault('vae', self.TENANT)
        ctx.setdefault('channel', 'api')
        out = self._actions_in(st, branch, object_id, ctx)
        out['evaluated_at'] = ev
        return out
