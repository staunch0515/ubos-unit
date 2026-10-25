"""Vector cases of BPA 1.0-rc11 for rule operations (PH-2 step 5a, DEC-112): the end of a rule's
attachment (STD-LGS-070), the live impact count (STD-LGS-071), rule tests and coverage
(STD-LGS-072), the test gate of change-set merges (STD-LGS-073) and the simulation of logic
changes (STD-LGS-074). Not required of the PH-0 prototype."""
from .common import uid, create, base, NOW
from .sheets import sheet, op
from .rules import CLASSES as RULE_CLASSES, BR, FB, SHEET, APPROVER
from .history import cs
from .execution import asset, fn
from .approvals import ACC, ACCT, ex, table, f, FEE, commit, read

CUST = create(uid(2), 'Customer', 'c-1', {'name': 'Acme', 'rating': 3})
CUST2 = create(uid(3), 'Customer', 'c-2', {'name': 'Beta', 'rating': 5, 'country': 'DE'})
CAND = 'feature/candidate'
T1, T2, T3, T4 = uid(810), uid(811), uid(812), uid(813)


def rtest(oid, kind, inp, expect, target=None, name=None):
    body = {'name': name or 'acme.test_%s' % oid[-3:], 'kind': kind, 'input': inp, 'expect': expect}
    if target is not None:
        body['target'] = target
    return create(oid, 'RuleTest', 'test-%s' % oid[-4:], body)


def upd(oid, label, body=None, **kw):
    ch = {'op': 'update', 'object_id': oid, 'base': {'$ref': '%s/version_hash' % label}}
    if body is not None:
        ch['body'] = body
    ch.update(kw)
    return ch


def tests(**kw):
    return op('run_rule_tests', None, None, branch=BR, **kw)


def cases(g):
    def S(clause, desc, steps, also=()):
        g.vector(clause, desc, setup=[{'op': 'set_now', 'input': {'now': NOW}},
                                      {'op': 'load_genesis', 'input': {'classes': RULE_CLASSES}}, FB],
                 steps=steps, prototype=False, also=also)

    deploy = [commit([ACCT, ACC])]
    ro = sheet(SHEET, 'd', 'Customer { read_only: true; }')

    # ---- STD-LGS-070 the end of an attachment ----
    def end(oid, label, d):
        return {'op': 'end', 'object_id': oid, 'base': {'$ref': '%s/version_hash' % label}, 'valid': {'from': d}}
    S('STD-LGS-070', 'a sheet attached until a date stops applying then and RuleDetached is emitted', [
        commit([CUST, ro]), op('read', 's', None, branch=BR, object_id=SHEET), commit([end(SHEET, 's', '2026-03-20')]),
        op('cascade', None, None, branch=BR, object_id=uid(2)), op('outbox', None, None),
        op('set_now', None, None, now='2026-03-20T00:00:00.000000Z'),
        op('cascade', None, None, branch=BR, object_id=uid(2)), op('outbox', None, None)])
    S('STD-LGS-070', 'RuleDetached is not emitted before the end date', [
        commit([CUST, ro]), op('read', 's', None, branch=BR, object_id=SHEET), commit([end(SHEET, 's', '2026-03-18')]),
        op('set_now', None, None, now='2026-03-17T12:00:00.000000Z'), op('outbox', None, None),
        op('set_now', None, None, now='2026-03-18T00:00:00.000000Z'), op('outbox', None, None)])
    S('STD-LGS-070', 'moving the end before it is reached emits only the new end', [
        commit([CUST, ro]), op('read', 's', None, branch=BR, object_id=SHEET),
        commit([end(SHEET, 's', '2026-03-18')], label='e'),
        commit([{'op': 'update', 'object_id': SHEET, 'base': base('e', SHEET), 'body': {'description': 'extended'},
                 'valid': {'kind': 'set_period', 'from': '2026-03-15', 'to': '2026-03-25'}}]),
        op('set_now', None, None, now='2026-03-19T00:00:00.000000Z'), op('outbox', None, None),
        op('set_now', None, None, now='2026-03-26T00:00:00.000000Z'), op('outbox', None, None)])
    S('STD-LGS-070', 'a decision table attached until a date emits RuleDetached at its end', [
        commit([FEE]), op('read', 't', None, branch=BR, object_id=FEE['object_id']),
        commit([end(FEE['object_id'], 't', '2026-03-16')]),
        op('set_now', None, None, now='2026-03-16T08:00:00.000000Z'), op('outbox', None, None),
        op('decide_table', None, 'QRY.NOT_FOUND_AT_TIME', branch=BR, table=FEE['object_id'], inputs={'amount': '10'})])

    # ---- STD-LGS-071 live impact ----
    cand = [{'op': 'create_branch', 'input': {'name': CAND, 'from': BR}}]
    S('STD-LGS-071', 'the impact of a candidate sheet counts the objects whose declarations change, by class', [
        commit([CUST, CUST2, ACC, ACCT])] + cand + [
        commit([sheet(SHEET, 'd', 'Customer[this.country == "DE"] { read_only: true; }')], branch=CAND),
        op('rule_impact', None, None, branch=BR, source=CAND)])
    S('STD-LGS-071', 'a candidate validation counts the objects whose validation outcome changes', [
        commit([CUST, CUST2])] + cand + [
        commit([sheet(SHEET, 'd', 'Customer { validations: [{name: "top_rated", rule: "this.rating >= 4", '
                                  'severity: "warning", message: "rating below 4"}]; }')], branch=CAND),
        op('rule_impact', None, None, branch=BR, source=CAND)])
    S('STD-LGS-071', 'a candidate equal to the target changes nothing', [
        commit([CUST, ro])] + cand + [op('rule_impact', None, None, branch=BR, source=CAND)])
    S('STD-LGS-071', 'the impact can be limited to classes', [
        commit([CUST, CUST2, ACC, ACCT])] + cand + [
        commit([sheet(SHEET, 'd', '* { read_only: true; }')], branch=CAND),
        op('rule_impact', None, None, branch=BR, source=CAND, classes=['Account'])])
    S('STD-LGS-071', 'removing a sheet on the candidate is an impact too', [
        commit([CUST, ro])] + cand + [op('read', 's', None, branch=CAND, object_id=SHEET),
                                      commit([{'op': 'end', 'object_id': SHEET, 'base': {'$ref': 's/version_hash'}}],
                                             branch=CAND),
                                      op('rule_impact', None, None, branch=BR, source=CAND)])

    # ---- STD-LGS-072 rule tests ----
    cust = {'class': 'Customer', 'body': {'name': 'Gamma', 'rating': 2, 'country': 'DE'}}
    S('STD-LGS-072', 'a cascade test passes when the effective declarations are as expected', [
        commit([sheet(SHEET, 'd', 'Customer[this.country == "DE"] { read_only: true; }'),
                rtest(T1, 'cascade', cust, {'properties': {'read_only': True}}, target=SHEET)]), tests()])
    S('STD-LGS-072', 'a failing cascade test reports what it found', [
        commit([sheet(SHEET, 'd', 'Customer[this.country == "US"] { read_only: true; }'),
                rtest(T1, 'cascade', cust, {'properties': {'read_only': True}}, target=SHEET)]), tests()])
    S('STD-LGS-072', 'a decision test checks the outcome and the steps', [
        commit([ACCT, sheet(SHEET, 'd', 'Account { approvals: 2; }'),
                rtest(T1, 'decision', {'class': 'Account', 'body': {'name': 'X', 'status': 'open'}, 'port': 'close'},
                      {'outcome': 'require', 'steps': ['approval']}),
                rtest(T2, 'decision', {'class': 'Account', 'body': {'name': 'X', 'status': 'open'}, 'port': 'close'},
                      {'outcome': 'allow'})]), tests()])
    S('STD-LGS-072', 'a table test checks the outputs; the rows it hits count as covered', [
        commit([FEE, rtest(T1, 'table', {'table': FEE['object_id'], 'inputs': {'amount': '500'}},
                           {'outputs': {'tier': 'small', 'fee': '5.00'}}, target=FEE['object_id']),
                rtest(T2, 'table', {'table': FEE['object_id'], 'inputs': {'amount': '50000'}},
                      {'outputs': {'tier': 'large', 'fee': '50.00'}}, target=FEE['object_id'])]), tests()])
    S('STD-LGS-072', 'a test may expect an error', [
        commit([FEE, rtest(T1, 'table', {'table': FEE['object_id'], 'inputs': {'amount': 'lots'}},
                           {'error': 'MODEL.INVALID_VALUE'})]), tests()])
    S('STD-LGS-072', 'a validation test checks the failing validations by name', [
        commit([sheet(SHEET, 'd', 'Customer { validations: [{name: "has_email", rule: "this.email is not null", '
                                  'severity: "error", message: "email required"}]; }'),
                rtest(T1, 'validation', {'class': 'Customer', 'body': {'name': 'A', 'rating': 9}},
                      {'violations': ['has_email', 'rating_range']}),
                rtest(T2, 'validation', {'class': 'Customer', 'body': {'name': 'A', 'rating': 3, 'email': 'a@x'}},
                      {'violations': []})]), tests()])
    S('STD-LGS-072', 'named tests run alone', [
        commit([FEE, rtest(T1, 'table', {'table': FEE['object_id'], 'inputs': {'amount': '500'}},
                           {'outputs': {'tier': 'small', 'fee': '5.00'}}),
                rtest(T2, 'table', {'table': FEE['object_id'], 'inputs': {'amount': '500'}},
                      {'outputs': {'tier': 'large', 'fee': '50.00'}})]), tests(tests=[T1])])
    S('STD-LGS-072', 'a test of an unknown kind is not committed', [
        commit([rtest(T1, 'oracle', {'class': 'Customer', 'body': {}}, {'properties': {}})],
               error='MODEL.CONSTRAINT_VIOLATED')])
    S('STD-LGS-072', 'a test without an expectation is not committed', [
        commit([rtest(T1, 'cascade', {'class': 'Customer', 'body': {}}, {})], error='MODEL.DECLARATION_INCOMPLETE')])
    S('STD-LGS-072', 'coverage counts the sheet blocks a test matched', [
        commit([sheet(SHEET, 'd', 'Customer[this.country == "DE"] { read_only: true; } Account { approvals: 1; }'),
                rtest(T1, 'cascade', cust, {'properties': {'read_only': True}})]), tests()])

    # ---- STD-LGS-073 the test gate ----
    def promote(n=40):
        return [cs('open', uid(n), source=BR, target='main', title='promote'), cs('submit', uid(n)),
                cs('approve', uid(n), approver=APPROVER)]
    S('STD-LGS-073', 'a change set whose rule tests fail does not merge into main', [
        commit([sheet(SHEET, 'd', 'Customer[this.country == "US"] { read_only: true; }'),
                rtest(T1, 'cascade', cust, {'properties': {'read_only': True}}, target=SHEET)])] + promote() + [
        {'op': 'change_set', 'input': {'action': 'merge', 'id': uid(40)}, 'error': 'RULE.TESTS_FAILED',
         'details': True}])
    S('STD-LGS-073', 'a change set whose rule tests pass merges', [
        commit([sheet(SHEET, 'd', 'Customer[this.country == "DE"] { read_only: true; }'),
                rtest(T1, 'cascade', cust, {'properties': {'read_only': True}}, target=SHEET)])] + promote() + [
        cs('merge', uid(40))])
    S('STD-LGS-073', 'changing the target of a merged test runs the test again and blocks a breaking change', [
        commit([sheet(SHEET, 'd', 'Customer[this.country == "DE"] { read_only: true; }'),
                rtest(T1, 'cascade', cust, {'properties': {'read_only': True}}, target=SHEET)])] + promote() + [
        cs('merge', uid(40)),
        op('create_branch', None, None, name='feature/other', **{'from': 'main'}),
        op('read', 's', None, branch='feature/other', object_id=SHEET),
        commit([upd(SHEET, 's', {'source': 'Customer[this.country == "US"] { read_only: true; }'})],
               branch='feature/other'),
        cs('open', uid(41), source='feature/other', target='main', title='narrow'), cs('submit', uid(41)),
        cs('approve', uid(41), approver=APPROVER),
        {'op': 'change_set', 'input': {'action': 'merge', 'id': uid(41)}, 'error': 'RULE.TESTS_FAILED',
         'details': True}])
    S('STD-LGS-073', 'a merge into a feature branch is not gated', [
        commit([sheet(SHEET, 'd', 'Customer[this.country == "US"] { read_only: true; }'),
                rtest(T1, 'cascade', cust, {'properties': {'read_only': True}}, target=SHEET)]),
        op('create_branch', None, None, name='feature/team', **{'from': 'main'}),
        cs('open', uid(42), source=BR, target='feature/team', title='to team'), cs('submit', uid(42)),
        cs('approve', uid(42), approver=APPROVER), cs('merge', uid(42))])

    # ---- STD-LGS-074 simulation of logic changes ----
    hist = [ex('rename', pid=1, defer=False, params={'name': 'Acme One'}),
            op('set_now', None, None, now='2026-03-16T10:00:00.000000Z'),
            ex('rename', pid=2, defer=False, params={'name': 'Acme Two'}),
            op('set_now', None, None, now='2026-03-17T10:00:00.000000Z')] + cand
    upper = asset(80, 'acme.acct', {
        'rename': fn([{'do': 'read.get', 'args': {'target': 'self_id'}},
                      {'do': 'commit.update', 'args': {'target': 'self_id', 'set': {'name': 'upper(params.name)'}}},
                      {'return': 'upper(params.name)'}], ['name']),
        'close': fn([{'do': 'read.get', 'args': {'target': 'self_id'}},
                     {'do': 'commit.update', 'args': {'target': 'self_id', 'set': {'status': '"closed"'}}},
                     {'return': '"closed"'}]),
        'peek': fn([{'do': 'read.get', 'args': {'target': 'self_id'}, 'as': 'o'}, {'return': 'o.fields.status'}])})
    logic_change = [op('read', 'l', None, branch=CAND, object_id=ACCT['object_id']),
                    commit([upd(ACCT['object_id'], 'l', upper['body'])], branch=CAND)]

    def sim(**kw):
        return op('simulate', None, None, source=CAND, target=BR, budget={'processes': 10}, run_id=uid(700), **kw)
    S('STD-LGS-074', 'with include logic a simulation re-runs historical calls with the candidate logic', deploy + hist +
      logic_change + [sim(include=['logic'])])
    S('STD-LGS-074', 'without include the candidate logic is not used', deploy + hist + logic_change + [sim()])
    S('STD-LGS-074', 'a logic simulation releases no effects and leaves the target unchanged', deploy + hist +
      logic_change + [op('outbox', 'o', None), sim(include=['logic']), {'op': 'outbox', 'input': {}, 'same_as': 'o'},
                      read()])
    S('STD-LGS-074', 'include names logic only', deploy + hist + [
        op('simulate', None, 'MODEL.INVALID_VALUE', source=CAND, target=BR, budget={'processes': 10}, run_id=uid(700),
           include=['classes'])])
