"""Vector cases of BPA 1.0-rc13 (PH-2, DEC-157), the refinements the finance Buk asked for in
PH-2 step 10: the posting-date field of ledger classes (STD-KIND-025), explicit coordinates of
`read.get` and `read.projection` in a process (STD-ISA-030, -037), `read_only` matched on the
version before a change (STD-LGS-011), and the vectors of two kernel conformance fixes: accounts
of classes extending `LedgerAccount` (STD-KIND-029, DEC-155) and the re-run of a call that
committed nothing on its pinned snapshot (STD-LGS-044, DEC-153). Not required of the PH-0
prototype."""
from .common import CLASSES, PERIOD, cls, field, uid, commit, create, base, usd, setup
from .execution import asset, fn
from .sheets import op, sheet
from . import approvals as ap
from . import finance as fin
from . import rules as ru

MAR = '2026-03-15T10:00:00.000000Z'
APR = '2026-04-10T09:00:00.000000Z'
FEB = create(uid(52), 'FiscalPeriod', '2026-02', {'start': '2026-02-01', 'end': '2026-03-01', 'state': 'open'})
PER_MAR = create(uid(50), 'FiscalPeriod', '2026-03', {'start': '2026-03-01', 'end': '2026-04-01', 'state': 'open'})
PER_APR = create(uid(51), 'FiscalPeriod', '2026-04', {'start': '2026-04-01', 'end': '2026-05-01', 'state': 'open'})
JL = cls('JournalLine', [field('account', 'text', True), field('amount', 'money', True),
                         field('posting_date', 'date')],
         kind='ledger', ledger={'key': ['account'], 'amounts': ['amount'], 'double_entry': True,
                                'period_scope': 'FiscalPeriod', 'posting_date': 'posting_date'})
JL_CLASSES = CLASSES + [JL]


def jl(n, account, amount, date=None, group='g1', valid_at='2026-03-15'):
    body = {'account': account, 'amount': usd(amount)}
    if date:
        body['posting_date'] = date
    return {'op': 'append', 'class': 'JournalLine', 'entry_id': uid(n), 'ledger_key': {'account': account},
            'valid_at': valid_at, 'body': body, 'posting_group': group}


def pair(n, date, amount='100.00', group='g1'):
    return [jl(n, '1010', amount, date, group), jl(n + 1, '4000', '-' + amount, date, group)]


def upd(oid, label, body):
    return {'op': 'update', 'object_id': oid, 'base': base(label, oid), 'body': body}


def close(oid, label, artefact):
    return [commit([upd(oid, label, {'state': 'closing'})]),
            op('close_period', None, None, branch='main', period=oid, artefact_id=artefact)]


def cases(g):
    g.pure('STD-ISA-001', 'the descriptors of ABI 1.0 are normative (since 1.0-rc13)', 'instructions', {},
           prototype=False)
    posting(g)
    coordinates(g)
    frozen(g)
    accounts(g)
    simulation(g)


def posting(g):
    def V(desc, steps, now=MAR, classes=JL_CLASSES, also=()):
        g.vector('STD-KIND-025', desc, setup=setup(now, classes), steps=steps, prototype=False, also=also)

    V('an entry with a posting date in a closed period fails; in an open period it commits', [
        commit([FEB, PER_MAR], label='p')] + close(uid(52), 'p', uid(60)) + [
        commit(pair(10, '2026-02-20'), error='LEDG.PERIOD_CLOSED'),
        commit(pair(10, '2026-03-05')),
        op('read_ledger', branch='main', **{'class': 'JournalLine', 'ledger_key': {'account': '1010'}})])
    V('an entry dated in an open period commits while the period of the commit date is closed', [
        commit([PER_MAR, PER_APR], label='p')] + close(uid(51), 'p', uid(60)) + [
        commit(pair(10, '2026-03-31'))], now=APR)
    V('an entry without a posting date posts on the date of its commit', [
        commit([PER_MAR, PER_APR], label='p'), commit(pair(10, None))] + close(uid(50), 'p', uid(60)) + [
        op('read', None, None, branch='main', object_id=uid(60))])
    V('the close artefact counts the entries by their posting dates', [
        commit([PER_MAR, PER_APR], label='p'),
        commit(pair(10, '2026-03-31')), commit(pair(12, '2026-04-02', '7.00', 'g2')),
        {'op': 'set_now', 'input': {'now': APR}},
        commit(pair(14, '2026-03-30', '30.00', 'g3'))] + close(uid(50), 'p', uid(60)) + [
        op('read', None, None, branch='main', object_id=uid(60))], also=('STD-KIND-026',))
    V('a reversal posts on the date of its commit', [
        commit([PER_MAR, PER_APR], label='p'), commit(pair(10, '2026-03-20'))] + close(uid(50), 'p', uid(60)) + [
        {'op': 'set_now', 'input': {'now': APR}},
        commit([{'op': 'reverse', 'entry_id': uid(10), 'new_entry_id': uid(12)},
                {'op': 'reverse', 'entry_id': uid(11), 'new_entry_id': uid(13)}]),
        op('read_ledger', branch='main', **{'class': 'JournalLine', 'ledger_key': {'account': '1010'}})])
    g.vector('STD-KIND-025', 'the posting-date member names a date field of the class',
             setup=[{'op': 'set_now', 'input': {'now': MAR}}],
             steps=[{'op': 'load_genesis', 'error': 'MODEL.INVALID_VALUE', 'input': {'classes': [PERIOD, cls(
                 'Line', [field('account', 'text', True), field('amount', 'money', True), field('day', 'text')],
                 kind='ledger', ledger={'key': ['account'], 'amounts': ['amount'], 'double_entry': False,
                                        'period_scope': 'FiscalPeriod', 'posting_date': 'day'})]}}],
             prototype=False)


def coordinates(g):
    BR = fin.BR
    GET = asset(82, 'acme.coords', {
        'get': fn([{'do': 'read.get', 'args': {'target': 'params.u'}, 'as': 'r'}, {'return': 'r.fields.name'}], ['u']),
        'proj': fn([{'do': 'read.projection', 'args': {'projection': 'params.p', 'time': 'params.t'}, 'as': 'r'},
                    {'return': 'r'}], ['p', 't'])}, caps=('read',))

    def L(clause, desc, steps, data=()):
        g.vector(clause, desc, setup=setup(fin.NOW, fin.FIN_CLASSES) + [
            op('create_branch', None, None, name=BR, **{'from': 'main'}), commit(list(data) + [GET], branch=BR,
                                                                                  label='d')],
            steps=steps, prototype=False)

    def ex(function, args, pid, error=None):
        return op('execute', None, error, branch=BR, asset='acme.coords', function=function, args=args,
                  entropy_seed=fin.SEED, process={'process_id': uid(pid)})

    beta = create(uid(5), 'Customer', 'c-9', {'name': 'Beta'}, valid={'from': '2026-01-01'})
    U = 'ubos://acme/Customer/c-9'

    # ---- STD-ISA-030 explicit coordinates in a process ----
    L('STD-ISA-030', 'a target with selectors is resolved on the process branch', [
        commit([beta], branch=BR), ex('get', [U + '?asof=2026-03-01'], 600)])
    L('STD-ISA-030', 'asof alone reads the version valid at that date in the process snapshot', [
        commit([beta], branch=BR, label='b'),
        commit([{'op': 'update', 'object_id': uid(5), 'base': base('b', uid(5)), 'body': {'name': 'Beta Two'},
                 'valid': {'kind': 'supersede_from', 'from': '2026-03-10'}}], branch=BR),
        ex('get', [U + '?asof=2026-03-01'], 600), ex('get', [U + '?asof=2026-03-12'], 601)])
    L('STD-ISA-030', 'a branch selector reads the named branch', [
        commit([create(uid(6), 'Customer', 'c-9', {'name': 'Main'})]), commit([beta], branch=BR),
        ex('get', [U + '?branch=main'], 600), ex('get', [U + '?asof=2026-03-15'], 601)])

    # ---- STD-ISA-037 read.projection as known at ----
    bal = fin.proj(70, 'balances', 'balances')
    two = [fin.gl(uid(10), '1010', '100.00'), fin.gl(uid(11), '4000', '-100.00')]
    more = [fin.gl(uid(12), '1010', '30.00', group='g2'), fin.gl(uid(13), '5000', '-30.00', group='g2')]
    later = [commit(two, branch=BR), {'op': 'set_now', 'input': {'now': '2026-03-16T10:00:00.000000Z'}},
             commit(more, branch=BR), {'op': 'set_now', 'input': {'now': '2026-03-17T10:00:00.000000Z'}}]
    L('STD-ISA-037', 'time reads the projection as known at an earlier transaction time',
      later + [ex('proj', ['balances', '2026-03-15T12:00:00.000000Z'], 600)], data=[bal])
    L('STD-ISA-037', 'a time after the snapshot reads at the snapshot',
      later + [ex('proj', ['balances', '2026-03-18T00:00:00.000000Z'], 600)], data=[bal])
    L('STD-ISA-037', 'time is an instant', [ex('proj', ['balances', '2026-03-15'], 600, error='MODEL.INVALID_VALUE')],
      data=[bal])


def frozen(g):
    def S(desc, steps):
        g.vector('STD-LGS-011', desc, setup=[{'op': 'set_now', 'input': {'now': ru.NOW}},
                                             {'op': 'load_genesis', 'input': {'classes': ru.CLASSES}}, ru.FB],
                 steps=steps, prototype=False)

    ro = ru.commit([sheet(ru.SHEET, 'ro', 'Customer[this.rating == 5] { read_only: true; }')])
    cust = ru.commit([ru.c(1, 'acme', {'rating': 1})], label='c')

    def edit(after, body, **kw):
        return ru.commit([{'op': 'update', 'object_id': uid(1), 'base': base(after, uid(1)), 'body': body}], **kw)
    S('read_only is matched on the version before the change: the change into the match commits', [
        ro, cust, edit('c', {'rating': 5}, label='r'), op('read', None, None, branch=ru.BR, object_id=uid(1))])
    S('an object that matched before the change is frozen', [
        ro, cust, edit('c', {'rating': 5}, label='r'),
        edit('r', {'email': 'b@x.com'}, error='RULE.DECLARATION_UNSATISFIED', details=True)])
    S('a change out of the match is rejected', [
        ro, cust, edit('c', {'rating': 5}, label='r'),
        edit('r', {'rating': 2}, error='RULE.DECLARATION_UNSATISFIED', details=True)])
    S('ending an object that matches is rejected', [
        ro, cust, edit('c', {'rating': 5}, label='r'),
        ru.commit([{'op': 'end', 'object_id': uid(1), 'base': base('r', uid(1))}],
                  error='RULE.DECLARATION_UNSATISFIED', details=True)])


def accounts(g):
    GLA = cls('GlAccount', [field('statement_line', 'text')], parent='LedgerAccount')
    acc = lambda slug: 'ubos://acme/GlAccount/%s' % slug  # noqa: E731

    def account(n, slug, type_='asset', parent=None):
        body = {'number': slug, 'name': slug, 'type': type_}
        if parent:
            body['parent'] = acc(parent)
        return create(uid(n), 'GlAccount', slug, body, valid={'from': '2026-01-01'})

    def post(n, a, amount, group='g1'):
        return {'op': 'append', 'class': 'Posting', 'entry_id': uid(n), 'ledger_key': {'account': acc(a)},
                'valid_at': '2026-03-15', 'body': {'account': acc(a), 'amount': usd(amount)}, 'posting_group': group}
    chart = [account(30, 'assets'), account(31, 'cash', parent='assets'), account(32, 'bank', parent='assets'),
             account(33, 'revenue', type_='income'), account(34, 'sales', type_='income', parent='revenue')]

    def V(desc, steps, also=()):
        g.vector('STD-KIND-029', desc, setup=setup(fin.NOW, fin.FIN_CLASSES + [GLA]) + [
            op('create_branch', None, None, name=fin.BR, **{'from': 'main'})],
            steps=fin.on_branch(steps), prototype=False, also=also)
    V('the accounts of a class extending LedgerAccount are checked: a posting to one with children fails', [
        commit(chart), commit([post(10, 'assets', '100.00'), post(11, 'sales', '-100.00')],
                              error='MODEL.CONSTRAINT_VIOLATED', details=True)])
    V('rollup sums the leaves of accounts of a class extending LedgerAccount', [
        commit(chart + [fin.proj(71, 'accounts', 'accounts', source='Posting')]),
        commit([post(10, 'cash', '100.00'), post(11, 'sales', '-100.00')]),
        commit([post(12, 'bank', '40.00', 'g2'), post(13, 'sales', '-40.00', 'g2')]),
        fin.rp('accounts', rollup=True)], also=('STD-ISA-037',))


def simulation(g):
    hist = [ap.ex('peek', pid=1, defer=False),
            op('set_now', None, None, now='2026-03-16T10:00:00.000000Z'),
            ap.ex('close', pid=2, defer=False),
            op('set_now', None, None, now='2026-03-17T10:00:00.000000Z'),
            {'op': 'create_branch', 'input': {'name': 'feature/candidate', 'from': ru.BR}}]
    g.vector('STD-LGS-044', 'a call that committed nothing is re-run on the snapshot its journal pinned',
             setup=[{'op': 'set_now', 'input': {'now': ru.NOW}},
                    {'op': 'load_genesis', 'input': {'classes': ru.CLASSES}}, ru.FB],
             steps=[ap.commit([ru.ACCT, ap.ACC])] + hist + [
                 op('simulate', None, None, source='feature/candidate', target=ru.BR, budget={'processes': 10},
                    run_id=uid(700))], prototype=False)
