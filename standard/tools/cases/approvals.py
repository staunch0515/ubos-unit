"""Vector cases of BPA 1.0-rc8 for approvals and rules (PH-2 step 1c, DEC-091): deferred
commits (STD-TXN-060), decision tables (STD-LGS-040, STD-ISA-053), approval requirements
(STD-LGS-041), separation of duties (STD-LGS-042), approval evidence (STD-LGS-043) and
simulation (STD-LGS-044). Not required of the PH-0 prototype."""
from .common import cls, field, uid, create, NOW
from .common import commit as _commit
from .sheets import sheet, op
from .execution import asset, fn, SEED
from .history import cs
from .rules import ACCOUNT, ACCT, CLASSES as RULE_CLASSES, BR, FB, SHEET, APPROVER

MAKER = 'did:ubos:acme:01900000-0000-7000-8000-000000000020'
CHECKER = 'did:ubos:acme:01900000-0000-7000-8000-000000000021'
CHECKER2 = 'did:ubos:acme:01900000-0000-7000-8000-000000000022'
T = 'ubos://acme/Account/a-1'
ACC = create(uid(1), 'Account', 'a-1', {'name': 'Acme', 'balance': '100', 'status': 'open'})
PC = uid(501)


def commit(changes, branch=BR, **kw):
    return _commit(changes, branch=branch, **kw)


def ex(port_='close', pid=1, defer=True, initiator=MAKER, label=None, error=None, branch=BR, **kw):
    inp = {'branch': branch, 'target': T, 'port': port_, 'params': kw.pop('params', {}),
           'process': {'process_id': uid(500 + pid), 'initiator': initiator}, 'entropy_seed': SEED}
    if defer:
        inp['defer'] = True
    inp.update(kw)
    s = {'op': 'execute', 'input': inp}
    if label:
        s['label'] = label
    if error:
        s['error'] = error
    return s


def approve(approver=CHECKER, pc=PC, error=None, details=False, **kw):
    s = op('approve', None, error, pending_change_id=pc, approver=approver, **kw)
    if details:
        s['details'] = True
    return s


def pending(pc=PC, error=None):
    return op('pending_change', None, error, pending_change_id=pc)


def read(oid=uid(1)):
    return op('read', None, None, branch=BR, object_id=oid)


def table(oid, slug, inputs, outputs, rows, hit_policy='unique', **extra):
    body = {'name': slug, 'inputs': inputs, 'outputs': outputs, 'rows': rows, 'hit_policy': hit_policy,
            'description': slug}
    body.update({k: v for k, v in extra.items() if k != 'valid'})
    ch = create(oid, 'DecisionTable', slug, body)
    if extra.get('valid'):
        ch['valid'] = extra['valid']
    return ch


def f(name, type_, **c):
    d = {'name': name, 'type': type_}
    if c:
        d['constraints'] = c
    return d


FEE_ROWS = [{'when': {'amount': 'params.amount < 1000'}, 'then': {'tier': 'small', 'fee': '5.00'}},
            {'when': {'amount': 'params.amount < 10000'}, 'then': {'tier': 'medium', 'fee': '20.00'}},
            {'when': {'amount': '-'}, 'then': {'tier': 'large', 'fee': '50.00'}}]
FEE = table(uid(70), 'fee-tiers', [f('amount', 'decimal')], [f('tier', 'text'), f('fee', 'decimal')], FEE_ROWS,
            hit_policy='first')


def cases(g):
    def S(clause, desc, steps, classes=None, also=()):
        g.vector(clause, desc, setup=[{'op': 'set_now', 'input': {'now': NOW}},
                                      {'op': 'load_genesis', 'input': {'classes': classes or RULE_CLASSES}}, FB],
                 steps=steps, prototype=False, also=also)

    deploy = [commit([ACCT, ACC])]

    def needs(src):
        return deploy + [commit([sheet(SHEET, 'd', src)])]

    # ---- STD-TXN-060 deferred commits ----
    S('STD-TXN-060', 'with defer a port call that requires approval is stored as a pending change; the target is unchanged',
      needs('Account { approvals: 1; }') + [ex(), read(), pending()])
    S('STD-TXN-060', 'the final approval applies the changes in one commit', needs('Account { approvals: 1; }') + [
        ex(), approve(), read()])
    S('STD-TXN-060', 'with two approvals required one approval leaves the change pending', needs('Account { approvals: 2; }') + [
        ex(), approve(), read(), approve(CHECKER2), read()])
    S('STD-TXN-060', 'a rejection discards the pending change', needs('Account { approvals: 1; }') + [
        ex(), approve(decision='reject', comment='not now'), approve(CHECKER2), read()])
    S('STD-TXN-060', 'an expired pending change is not applied', needs('Account { approvals: {count: 1, expires_in: "P1D"}; }') + [
        ex(), op('set_now', None, None, now='2026-03-17T10:00:00.000000Z'), approve(), read()])
    S('STD-TXN-060', 'a change of the subject invalidates the pending change and voids its approvals',
      needs('Account { approvals: 2; }') + [
          ex(), approve(), op('read', 'r', None, branch=BR, object_id=uid(1)),
          commit([{'op': 'update', 'object_id': uid(1), 'base': {'$ref': 'r/version_hash'}, 'body': {'balance': '90'}}]),
          approve(CHECKER2), read()])
    S('STD-TXN-060', 'an unknown pending change is not found', deploy + [pending(uid(999), error='IAM.NOT_FOUND')])
    S('STD-TXN-060', 'without defer a required approval still stops the call', needs('Account { approvals: 1; }') + [
        ex(defer=False, error='RULE.DECISION_REQUIRES')])

    # ---- STD-LGS-041 approval requirements ----
    S('STD-LGS-041', 'the requirements of a pending change carry the approval specification',
      needs('Account { approvals: {count: 2, roles: ["controller"], order: "any"}; }') + [ex(), pending()])
    S('STD-LGS-041', 'an approver without an eligible role is denied',
      needs('Account { approvals: {count: 1, roles: ["controller"]}; }') + [
          ex(), approve(roles=['clerk'], error='IAM.DENIED', details=True)])
    S('STD-LGS-041', 'an approver with an eligible role approves',
      needs('Account { approvals: {count: 1, roles: ["controller"]}; }') + [ex(), approve(roles=['controller']), read()])
    S('STD-LGS-041', 'named approvers in sequence approve in their order',
      needs('Account { approvals: {count: 2, principals: ["%s", "%s"], order: "sequence"}; }' % (CHECKER, CHECKER2)) + [
          ex(), approve(CHECKER2, error='IAM.DENIED', details=True), approve(CHECKER), approve(CHECKER2), read()])
    S('STD-LGS-041', 'a require decision with an approval step needs one approval',
      needs('Account { decisions: [{name: "review", outcome: "require", steps: ["approval"]}]; }') + [
          ex(), pending(), approve(), read()])

    # ---- STD-LGS-042 separation of duties ----
    S('STD-LGS-042', 'the initiator cannot approve (maker-checker)', needs('Account { approvals: 1; }') + [
        ex(), approve(MAKER, error='IAM.DENIED', details=True)])
    S('STD-LGS-042', 'the initiator cannot approve through a delegation', needs('Account { approvals: 1; }') + [
        ex(), approve(CHECKER, on_behalf_of=MAKER, error='IAM.DENIED', details=True)])
    S('STD-LGS-042', 'a principal decides once per pending change', needs('Account { approvals: 2; }') + [
        ex(), approve(), approve(error='IAM.DENIED', details=True)])
    S('STD-LGS-042', 'a principal who performed an incompatible duty on the object cannot approve',
      deploy + [ex('rename', pid=2, defer=False, initiator=CHECKER, params={'name': 'Acme Ltd'}),
                commit([sheet(SHEET, 'd', 'Account { approvals: {count: 1, incompatible: ["rename"]}; }')]),
                ex(), approve(CHECKER, error='IAM.DENIED', details=True), approve(CHECKER2), read()])

    # ---- STD-LGS-043 approval evidence ----
    S('STD-LGS-043', 'an approval records the approver, time, decision, comment, preview and authentication',
      needs('Account { approvals: 2; }') + [
          ex(), approve(comment='checked', auth_level='strong', on_behalf_of=CHECKER2)])
    S('STD-LGS-043', 'the applied change records its commit', needs('Account { approvals: 1; }') + [
        ex(label='e'), approve(), pending()])
    S('STD-LGS-043', 'a signature step is met by a signed approval', needs('Account { require_signature: true; }') + [
        ex(), approve(), read()], also=('STD-TXN-060',))
    S('STD-LGS-043', 'a signed approval applies a change that requires a signature', needs('Account { require_signature: true; }') + [
        ex(), approve(signature={'alg': 'ed25519', 'kid': CHECKER + '#k1', 'sig': 'AAAA'}), read()])
    S('STD-LGS-043', 'change-set approvals take the same evidence members', [
        cs('open', uid(40), source=BR, target='main', title='promote'), cs('submit', uid(40)),
        cs('approve', uid(40), approver=APPROVER, comment='looks right', auth_level='strong')])

    # ---- STD-LGS-040 decision tables ----
    def D(desc, tbl, inputs, error=None, asof=None, extra=(), also=()):
        inp = {'branch': BR, 'table': tbl['object_id'], 'inputs': inputs}
        if asof:
            inp['asof'] = asof
        S('STD-LGS-040', desc, [commit([tbl])] + list(extra) + [op('decide_table', None, error, **inp)], also=also)

    for amount, tier in (('999.99', 'small'), ('1000', 'medium'), ('9999.99', 'medium'), ('10000', 'large')):
        D('first: the tier at %s is %s' % (amount, tier), FEE, {'amount': amount})
    uq = table(uid(71), 'risk', [f('rating', 'integer'), f('region', 'enum', allowed_values=['EU', 'US'])],
               [f('limit', 'decimal')],
               [{'when': {'rating': 'params.rating >= 4', 'region': 'params.region == "EU"'}, 'then': {'limit': '5000'}},
                {'when': {'rating': 'params.rating >= 4', 'region': 'params.region == "US"'}, 'then': {'limit': '8000'}},
                {'when': {'rating': 'params.rating < 4'}, 'then': {'limit': '1000'}}])
    D('unique: exactly one row matches', uq, {'rating': 5, 'region': 'US'})
    D('a dash matches any value', uq, {'rating': 2, 'region': 'EU'})
    over = table(uid(72), 'overlap', [f('rating', 'integer')], [f('limit', 'decimal')],
                 [{'when': {'rating': 'params.rating >= 3'}, 'then': {'limit': '1'}},
                  {'when': {'rating': 'params.rating >= 4'}, 'then': {'limit': '2'}}])
    D('unique: two matching rows fail', over, {'rating': 5}, error='RULE.MULTIPLE_MATCH')
    D('unique: no matching row fails', over, {'rating': 1}, error='RULE.NO_MATCH')
    prio = table(uid(73), 'prio', [f('rating', 'integer')], [f('limit', 'decimal')],
                 [{'when': {'rating': 'params.rating >= 3'}, 'then': {'limit': '1'}, 'priority': 1},
                  {'when': {'rating': 'params.rating >= 4'}, 'then': {'limit': '2'}, 'priority': 5}],
                 hit_policy='priority')
    D('priority: the matching row of the highest priority wins', prio, {'rating': 5})
    coll = [f('code', 'text')]
    bonus = [{'when': {'code': 'starts_with(params.code, "A")'}, 'then': {'points': 10}},
             {'when': {'code': 'ends_with(params.code, "9")'}, 'then': {'points': 5}},
             {'when': {'code': '-'}, 'then': {'points': 1}}]
    D('collect: the outputs of all matching rows', table(uid(74), 'bonus', coll, [f('points', 'integer')], bonus,
                                                         hit_policy='collect'), {'code': 'A19'})
    D('collect with sum adds the outputs', table(uid(74), 'bonus', coll, [f('points', 'integer')], bonus,
                                                 hit_policy='collect', aggregator='sum'), {'code': 'A19'})
    D('collect with count counts the matching rows', table(uid(74), 'bonus', coll, [f('points', 'integer')], bonus,
                                                           hit_policy='collect', aggregator='count'), {'code': 'B29'})
    D('collect with max takes the largest output', table(uid(74), 'bonus', coll, [f('points', 'integer')], bonus,
                                                         hit_policy='collect', aggregator='max'), {'code': 'A10'})
    D('an input of the wrong type fails', FEE, {'amount': 'lots'}, error='MODEL.INVALID_VALUE')
    S('STD-LGS-040', 'a table with an undeclared hit policy is not committed', [
        commit([table(uid(75), 'bad', [f('a', 'integer')], [f('b', 'integer')], [{'when': {}, 'then': {'b': 1}}],
                      hit_policy='any')], error='MODEL.CONSTRAINT_VIOLATED')])
    old = table(uid(76), 'rate', [f('amount', 'decimal')], [f('rate', 'decimal')],
                [{'when': {}, 'then': {'rate': '0.10'}}], valid={'from': '2026-01-01'})
    D('a table is evaluated in its version valid at asof', old, {'amount': '1'}, asof='2026-02-01', extra=[
        op('read', 'v', None, branch=BR, object_id=uid(76)),
        commit([{'op': 'update', 'object_id': uid(76), 'base': {'$ref': 'v/version_hash'},
                 'body': {'rows': [{'when': {}, 'then': {'rate': '0.12'}}]}, 'valid': {'kind': 'supersede_from', 'from': '2026-03-01'}}])])

    def C(desc, t, error=None):
        g.pure('STD-LGS-040', desc, 'check_table', {'table': t['body']}, error=error, prototype=False)

    gap = table(uid(77), 'credit', [f('exposure', 'decimal')], [f('approver', 'text')],
                [{'when': {'exposure': 'params.exposure < 1000000'}, 'then': {'approver': 'manager'}},
                 {'when': {'exposure': 'params.exposure >= 2000000'}, 'then': {'approver': 'board'}}])
    C('the authoring check reports a missing range as a gap', gap)
    C('the authoring check reports overlaps a unique table forbids', over)
    C('the authoring check of a complete table without overlaps finds nothing', uq)
    C('the authoring check reports equal priorities as overlaps', table(uid(78), 'tie', [f('rating', 'integer')],
                                                                      [f('limit', 'decimal')],
                                                                      [{'when': {'rating': 'params.rating >= 3'}, 'then': {'limit': '1'}, 'priority': 2},
                                                                       {'when': {'rating': 'params.rating >= 4'}, 'then': {'limit': '2'}, 'priority': 2}],
                                                                      hit_policy='priority'))
    C('inputs that are neither enumerable nor ranges are listed as unchecked', table(uid(74), 'bonus', coll,
                                                                                    [f('points', 'integer')], bonus,
                                                                                    hit_policy='collect'))
    C('an aggregator goes with collect only', table(uid(79), 'agg', [f('a', 'integer')], [f('b', 'integer')],
                                                    [{'when': {}, 'then': {'b': 1}}], hit_policy='first', aggregator='sum'),
      error='MODEL.INVALID_VALUE')

    # ---- STD-ISA-053 run.decision ----
    prog = asset(85, 'acme.fees', {'main': fn([
        {'do': 'run.decision', 'args': {'table': '"ubos://acme/DecisionTable/fee-tiers"', 'inputs': {'amount': '"2500"'}},
         'as': 'o'}, {'return': 'o'}], [])}, caps=('run', 'read'))
    miss = asset(86, 'acme.miss', {'main': fn([
        {'do': 'run.decision', 'args': {'table': '"ubos://acme/DecisionTable/overlap"', 'inputs': {'rating': '1'}}},
        {'return': '1'}], [])}, caps=('run', 'read'))

    def R(desc, a, name, tables, error=None):
        g.vector('STD-ISA-053', desc, setup=[{'op': 'set_now', 'input': {'now': NOW}},
                                             {'op': 'load_genesis', 'input': {'classes': RULE_CLASSES}}, FB],
                 steps=[commit([a] + tables),
                        op('execute', None, error, branch=BR, asset=name, function='main', args=[],
                           process={'process_id': uid(600)}, entropy_seed=SEED)], prototype=False)
    R('run.decision returns the outputs of the table', prog, 'acme.fees', [FEE])
    R('run.decision fails without a matching row', miss, 'acme.miss', [over], error='RULE.NO_MATCH')
    R('run.decision of an unknown table is not found', prog, 'acme.fees', [], error='IAM.NOT_FOUND')

    # ---- STD-LGS-044 simulation ----
    CAND = 'feature/candidate'
    hist = [ex(defer=False, pid=1),
            op('set_now', None, None, now='2026-03-16T10:00:00.000000Z'),
            ex('rename', pid=2, defer=False, params={'name': 'Acme Two'}),
            op('set_now', None, None, now='2026-03-17T10:00:00.000000Z'),
            {'op': 'create_branch', 'input': {'name': CAND, 'from': BR}},
            commit([sheet(SHEET, 'd', 'Account { decisions: [{name: "no_close", outcome: "deny"}]; }')], branch=CAND)]

    def sim(budget=10, error=None, **kw):
        return op('simulate', None, error, source=CAND, target=BR, budget={'processes': budget},
                  run_id=uid(700), **kw)
    S('STD-LGS-044', 'a simulation re-runs historical port calls with the candidate rules and reports changed outcomes',
      deploy + hist + [sim(**{'from': '2026-03-01', 'to': '2026-04-01'})])
    S('STD-LGS-044', 'a budget smaller than the window gives partial results with coverage', deploy + hist + [
        sim(budget=1)])
    S('STD-LGS-044', 'a selection limits the simulated port calls', deploy + hist + [sim(select={'port': 'rename'})])
    S('STD-LGS-044', 'the run is stored as an object', deploy + hist + [sim(), read(uid(700))])
    S('STD-LGS-044', 'a simulation releases no effects', deploy + hist + [
        op('outbox', 'o', None), sim(), {'op': 'outbox', 'input': {}, 'same_as': 'o'}])
    S('STD-LGS-044', 'a simulation has a budget', deploy + hist + [
        op('simulate', None, 'MODEL.DECLARATION_INCOMPLETE', source=CAND, target=BR, run_id=uid(700))])
