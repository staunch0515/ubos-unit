"""Vector cases of BPA 1.0-rc9 for money, accounts, statements and projections (PH-2 step 3a,
DEC-098): currency reference data (STD-FND-028), FX rates and conversion (STD-FND-029), the
chart of accounts (STD-KIND-029), statement lines (STD-KIND-030), projections (STD-TIME-021),
`read.projection` (STD-ISA-037) and compensation proposals (STD-TIME-043). Not required of the
PH-0 prototype."""
from bpa.finance import statement_id

from .common import CLASSES, CUSTOMER, cls, field, uid, commit, create, base, gl, stock, setup
from .execution import asset, fn
from .sheets import op

BR = 'feature/fin'
NOW = '2026-03-15T10:00:00.000000Z'
SEED = '22' * 32
ACC = lambda slug: 'ubos://acme/LedgerAccount/%s' % slug  # noqa: E731
EUR = lambda a: {'amount': a, 'currency': 'EUR'}  # noqa: E731
GET = lambda instr, args: [{'do': instr, 'args': args, 'as': 'r'}, {'return': 'r'}]  # noqa: E731

POSTING = cls('Posting', [field('account', 'reference', True, constraints={'target_class': 'LedgerAccount'}),
                          field('amount', 'money', True), field('memo', 'text')],
              kind='ledger', ledger={'key': ['account'], 'amounts': ['amount'], 'double_entry': True,
                                     'account': 'account'})
STMT = cls('BankStatementLine', [field('source', 'text', True), field('bank_account', 'text', True),
                                 field('statement', 'text', True), field('line', 'integer', True),
                                 field('amount', 'money', True), field('reference', 'text')],
           kind='ledger', ledger={'key': ['bank_account'], 'amounts': ['amount'], 'double_entry': False,
                                  'statement': {'source': 'source', 'account': 'bank_account',
                                                'statement': 'statement', 'line': 'line'}})
FIN_CLASSES = CLASSES + [POSTING, STMT]

FX = asset(81, 'acme.fx', {
    'conv': fn([{'return': 'fx_convert(money(params.a, params.c), params.q, params.t)'}], ['a', 'c', 'q', 't']),
    'conv_src': fn([{'return': 'fx_convert(money(params.a, params.c), params.q, params.t, params.s)'}],
                   ['a', 'c', 'q', 't', 's']),
    'conv_ref': fn([{'return': 'convert(money(params.a, params.c), params.q, params.r)'}], ['a', 'c', 'q', 'r']),
    'proj': fn(GET('read.projection', {'projection': 'params.p', 'group': 'params.g', 'asof': 'params.d',
                                       'rollup': 'params.r', 'at_least': 'params.l'}), ['p', 'g', 'd', 'r', 'l'])},
    caps=('read',))


def fx(n, slug, base_, quote, rate, rate_type='spot', source='ecb', valid_from='2026-01-01'):
    return create(uid(n), 'FxRate', slug, {'source': source, 'base': base_, 'quote': quote, 'rate_type': rate_type,
                                           'rate': rate}, valid={'from': valid_from})


def currency(n, code, units=2, numeric=None, name='Test currency'):
    body = {'code': code, 'name': name, 'minor_units': units}
    if numeric is not None:
        body['numeric'] = numeric
    return create(uid(n), 'Currency', code.lower(), body)


def account(n, slug, type_='asset', parent=None, currency_=None, valid_from='2026-01-01'):
    body = {'number': slug, 'name': slug, 'type': type_}
    if parent:
        body['parent'] = ACC(parent)
    if currency_:
        body['currency'] = currency_
    return create(uid(n), 'LedgerAccount', slug, body, valid={'from': valid_from})


def post(n, acct, amount, group='g1', valid_at='2026-03-15', cur='USD'):
    return {'op': 'append', 'class': 'Posting', 'entry_id': uid(n), 'ledger_key': {'account': ACC(acct)},
            'valid_at': valid_at, 'body': {'account': ACC(acct), 'amount': {'amount': amount, 'currency': cur}},
            'posting_group': group}


def ex(function, args, error=None, label=None, asof=None, branch=BR, pid=600):
    inp = {'branch': branch, 'asset': 'acme.fx', 'function': function, 'args': args, 'entropy_seed': SEED,
           'process': {'process_id': uid(pid)}}
    if asof:
        inp['ctx'] = {'asof': asof}
    return op('execute', label, error, **inp)


def proj(n, slug, name, source='GlEntry', group_by=('account',), aggregates=None, mode=None, **extra):
    body = {'name': name, 'source': source, 'group_by': list(group_by),
            'aggregates': aggregates or [{'fn': 'sum', 'field': 'amount', 'as': 'balance'},
                                         {'fn': 'count', 'as': 'entries'}]}
    if mode:
        body['mode'] = mode
    body.update(extra)
    return create(uid(n), 'Projection', slug, body)


def rp(name='balances', label=None, error=None, branch='main', **kw):
    return op('read_projection', label, error, branch=branch, projection=name, **kw)


def line(source, acct, stmt, n, amount, ref=None, valid_at='2026-03-10'):
    body = {'source': source, 'bank_account': acct, 'statement': stmt, 'line': n,
            'amount': {'amount': amount, 'currency': 'USD'}}
    if ref:
        body['reference'] = ref
    return {'ledger_key': {'bank_account': acct}, 'valid_at': valid_at, 'body': body}


def ingest(lines, label=None, error=None, klass='BankStatementLine'):
    return op('ingest_statement', label, error, branch='main', **{'class': klass, 'lines': lines})


def stmt_ledger(acct='1010-bank'):
    return op('read_ledger', branch='main', **{'class': 'BankStatementLine', 'ledger_key': {'bank_account': acct}})


def on_branch(steps):
    """The steps on the feature branch: projections are configuration assets, which a protected
    branch takes through change sets only (STD-VER-012)."""
    out = []
    for s in steps:
        s = dict(s, input=dict(s.get('input', {})))
        for k in ('branch', 'from'):
            if s['input'].get(k) == 'main':
                s['input'][k] = BR
        out.append(s)
    return out


def cases(g):
    def V(clause, desc, steps, classes=FIN_CLASSES, now=NOW, also=()):
        g.vector(clause, desc, setup=setup(now, classes), steps=steps, prototype=False, also=also)

    def B(clause, desc, steps):
        """A vector on the feature branch."""
        g.vector(clause, desc, setup=setup(NOW, FIN_CLASSES) + [op('create_branch', name=BR, **{'from': 'main'})],
                 steps=on_branch(steps), prototype=False)

    def L(clause, desc, steps, data=(), also=()):
        """A vector with the FX logic deployed on a feature branch."""
        g.vector(clause, desc, setup=setup(NOW, FIN_CLASSES) + [
            op('create_branch', name=BR, **{'from': 'main'}), commit(list(data) + [FX], branch=BR, label='d')],
            steps=steps, prototype=False, also=also)

    # ---- STD-FND-028 currency reference data ----
    V('STD-FND-028', 'the table gives the minor units of ISO 4217 codes', [
        op('currency', branch='main', code='JPY'), op('currency', branch='main', code='OMR'),
        op('currency', branch='main', code='MXN'), op('currency', branch='main', code='CLF')])
    V('STD-FND-028', 'a code outside the table is unknown', [op('currency', branch='main', code='XAB',
                                                                error='MODEL.INVALID_VALUE')])
    V('STD-FND-028', 'money of a code outside the table fails', [
        commit([create(uid(1), body={'name': 'Acme', 'credit_limit': {'amount': '10', 'currency': 'XAB'}})],
               error='MODEL.INVALID_VALUE')])
    V('STD-FND-028', 'a Currency object adds a code with its minor units', [
        commit([currency(10, 'XAB', units=3, numeric='901')]),
        commit([create(uid(1), body={'name': 'Acme', 'credit_limit': {'amount': '1.2345', 'currency': 'XAB'}})]),
        op('read', branch='main', object_id=uid(1)), op('currency', branch='main', code='XAB')])
    V('STD-FND-028', 'a Currency object does not redefine a code of the table', [
        commit([currency(10, 'USD', units=3)], error='MODEL.CONSTRAINT_VIOLATED', details=True)])
    V('STD-FND-028', 'two live Currency objects do not share a code', [
        commit([currency(10, 'XAB')]),
        commit([create(uid(11), 'Currency', 'xab-2', {'code': 'XAB', 'name': 'Again', 'minor_units': 2})],
               error='MODEL.CONSTRAINT_VIOLATED')])
    V('STD-FND-028', 'minor units are 0 to 4', [commit([currency(10, 'XAB', units=5)],
                                                     error='MODEL.CONSTRAINT_VIOLATED', details=True)])
    V('STD-FND-028', 'a numeric code has three digits', [commit([currency(10, 'XAB', numeric='9O1')],
                                                              error='MODEL.CONSTRAINT_VIOLATED', details=True)])
    V('STD-FND-028', 'an ended Currency object no longer adds its code', [
        commit([currency(10, 'XAB')], label='c'),
        commit([{'op': 'end', 'object_id': uid(10), 'base': base('c', uid(10))}]),
        commit([create(uid(1), body={'name': 'Acme', 'credit_limit': {'amount': '1', 'currency': 'XAB'}})],
               error='MODEL.INVALID_VALUE')])
    V('STD-FND-028', 'money of a table code validates at its minor units', [
        op('validate_value', type='money', value={'amount': '12.345', 'currency': 'TND'}),
        op('validate_value', type='money', value={'amount': '12.5', 'currency': 'ISK'})])

    # ---- STD-FND-029 FX rates and conversion ----
    rates = [fx(20, 'ecb-eur-usd-spot', 'EUR', 'USD', '1.0850'), fx(21, 'ecb-usd-jpy-spot', 'USD', 'JPY', '149.37')]
    L('STD-FND-029', 'fx_convert uses the rate of the key and rounds to the minor units of the quote', [
        ex('conv', ['100.00', 'EUR', 'USD', 'spot'], asof='2026-03-10'),
        ex('conv', ['10.00', 'USD', 'JPY', 'spot'], asof='2026-03-10', pid=601),
        op('journal', process_id=uid(600))], data=rates)
    L('STD-FND-029', 'the inverse of a rate is used when only the opposite key exists', [
        ex('conv', ['108.50', 'USD', 'EUR', 'spot'], asof='2026-03-10'),
        ex('conv', ['1000', 'JPY', 'USD', 'spot'], asof='2026-03-10', pid=601)], data=rates)
    L('STD-FND-029', 'the rate is the version valid at the evaluation date', [
        op('read', branch=BR, object_id=uid(20), label='r'),
        commit([{'op': 'update', 'object_id': uid(20), 'base': {'$ref': 'r/version_hash'}, 'body': {'rate': '1.0900'},
                 'valid': {'kind': 'supersede_from', 'from': '2026-03-12'}}], branch=BR),
        ex('conv', ['100.00', 'EUR', 'USD', 'spot'], asof='2026-03-11'),
        ex('conv', ['100.00', 'EUR', 'USD', 'spot'], asof='2026-03-12', pid=601),
        op('fx_rate', branch=BR, base='EUR', quote='USD', rate_type='spot', date='2026-03-12')], data=rates)
    L('STD-FND-029', 'no rate for the key and date fails with MODEL.CURRENCY_MISMATCH', [
        ex('conv', ['100.00', 'EUR', 'USD', 'closing'], asof='2026-03-10', error='MODEL.CURRENCY_MISMATCH'),
        ex('conv', ['100.00', 'EUR', 'USD', 'spot'], asof='2025-12-31', error='MODEL.CURRENCY_MISMATCH', pid=601)],
      data=rates)
    L('STD-FND-029', 'two sources need the source argument', [
        ex('conv', ['100.00', 'EUR', 'USD', 'spot'], asof='2026-03-10', error='MODEL.INVALID_VALUE'),
        ex('conv_src', ['100.00', 'EUR', 'USD', 'spot', 'fed'], asof='2026-03-10', pid=601)],
      data=rates + [fx(22, 'fed-eur-usd-spot', 'EUR', 'USD', '1.0861', source='fed')])
    L('STD-FND-029', 'money already in the quote currency is returned unchanged', [
        ex('conv', ['12.34', 'USD', 'USD', 'spot'], asof='2026-03-10')])
    V('STD-FND-029', 'a rate converts between two currencies with a positive rate', [
        commit([fx(20, 'x-usd-usd', 'USD', 'USD', '1')], error='MODEL.CONSTRAINT_VIOLATED', details=True),
        commit([fx(20, 'x-eur-usd', 'EUR', 'USD', '0')], error='MODEL.CONSTRAINT_VIOLATED', details=True),
        commit([fx(20, 'x-eur-xab', 'EUR', 'XAB', '1.2')], error='MODEL.INVALID_VALUE')])
    V('STD-FND-029', 'two live rates do not share a key', [
        commit([fx(20, 'ecb-eur-usd-spot', 'EUR', 'USD', '1.0850')]),
        commit([fx(21, 'ecb-eur-usd-spot-2', 'EUR', 'USD', '1.0851')], error='MODEL.CONSTRAINT_VIOLATED')])
    L('STD-FND-029', 'convert reads its rate reference as an FxRate object and the journal records it', [
        ex('conv_ref', ['100.00', 'EUR', 'USD', 'ubos://acme/FxRate/ecb-eur-usd-spot'], asof='2026-03-10'),
        op('journal', process_id=uid(600))], data=rates)
    g.pure('STD-FND-029', 'eval_expr converts with a list of rates, directly and by the inverse', 'eval_expr',
           {'expr': '[fx_convert(100.00EUR, "USD", "spot"), fx_convert(108.50USD, "EUR", "spot")]',
            'fx_rates': [{'source': 'ecb', 'base': 'EUR', 'quote': 'USD', 'rate_type': 'spot', 'rate': '1.0850'}]},
           prototype=False)

    # ---- STD-KIND-029 chart of accounts ----
    chart = [account(30, 'assets'), account(31, 'cash', parent='assets'), account(32, 'bank', parent='assets'),
             account(33, 'revenue', type_='income'), account(34, 'sales', type_='income', parent='revenue'),
             account(35, 'eur-bank', parent='assets', currency_='EUR')]
    sale = [post(10, 'cash', '100.00'), post(11, 'sales', '-100.00')]
    V('STD-KIND-029', 'postings to leaf accounts commit', [
        commit(chart), commit(sale),
        op('read_ledger', branch='main', **{'class': 'Posting', 'ledger_key': {'account': ACC('cash')}})])
    V('STD-KIND-029', 'a posting to an account that has children fails', [
        commit(chart), commit([post(10, 'assets', '100.00'), post(11, 'sales', '-100.00')],
                              error='MODEL.CONSTRAINT_VIOLATED', details=True)])
    V('STD-KIND-029', 'a posting outside the valid time of its account fails', [
        commit(chart + [account(36, 'petty', parent='assets', valid_from='2026-04-01')]),
        commit([post(10, 'petty', '5.00'), post(11, 'sales', '-5.00')], error='MODEL.CONSTRAINT_VIOLATED',
               details=True)])
    V('STD-KIND-029', 'an account with a currency takes amounts in that currency only', [
        commit(chart), commit([post(10, 'eur-bank', '100.00'), post(11, 'sales', '-100.00')],
                              error='MODEL.CONSTRAINT_VIOLATED', details=True)])
    V('STD-KIND-029', 'an account is not its own ancestor', [
        commit([account(30, 'a', parent='b'), account(31, 'b', parent='a')], error='MODEL.CONSTRAINT_VIOLATED',
               details=True)])
    V('STD-KIND-029', 'an account that receives a child keeps its entries; later postings use a leaf', [
        commit(chart), commit(sale),
        commit([account(37, 'cash-us', parent='cash', valid_from='2026-03-20')]),
        commit([post(12, 'cash', '20.00', group='g2', valid_at='2026-03-16'),
                post(13, 'sales', '-20.00', group='g2', valid_at='2026-03-16')]),
        commit([post(14, 'cash', '30.00', group='g3', valid_at='2026-03-21'),
                post(15, 'sales', '-30.00', group='g3', valid_at='2026-03-21')], error='MODEL.CONSTRAINT_VIOLATED',
               details=True)])
    V('STD-KIND-029', 'an account names a known currency', [
        commit([account(35, 'x-bank', currency_='XAB')], error='MODEL.INVALID_VALUE')])
    V('STD-KIND-029', 'a reversal is checked at the valid time of the original', [
        commit(chart), commit(sale), commit([account(37, 'cash-us', parent='cash', valid_from='2026-03-20')]),
        commit([{'op': 'reverse', 'entry_id': uid(10), 'new_entry_id': uid(16)},
                {'op': 'reverse', 'entry_id': uid(11), 'new_entry_id': uid(17)}])])

    # ---- STD-KIND-030 statement lines ----
    s1 = [line('bank-a', '1010-bank', 'S-2026-03', 1, '250.00', 'INV-1'),
          line('bank-a', '1010-bank', 'S-2026-03', 2, '-40.00', 'FEE')]
    s2 = s1 + [line('bank-a', '1010-bank', 'S-2026-03', 3, '75.00', 'INV-2')]
    V('STD-KIND-030', 'an ingestion appends the lines of a statement in one commit', [ingest(s1), stmt_ledger()])
    V('STD-KIND-030', 'a repeated ingestion creates no entries', [ingest(s1), ingest(s1), stmt_ledger()])
    V('STD-KIND-030', 'a redelivery with an added line appends only that line', [ingest(s1), ingest(s2), stmt_ledger()])
    changed = [line('bank-a', '1010-bank', 'S-2026-03', 1, '205.00', 'INV-1')] + s2[1:]
    V('STD-KIND-030', 'a changed line fails the whole ingestion', [
        ingest(s1), ingest(changed, error='TXN.IDEMPOTENCY_MISMATCH', label='x'), stmt_ledger()])
    V('STD-KIND-030', 'a line repeated within one ingestion is a duplicate', [ingest(s1 + s1[:1]), stmt_ledger()])
    direct = s1[0]
    eid = statement_id({'source': 'source', 'account': 'bank_account', 'statement': 'statement', 'line': 'line'},
                       direct['body'])
    V('STD-KIND-030', 'the entry id of a statement line is derived from the line', [
        commit([{'op': 'append', 'class': 'BankStatementLine', 'entry_id': uid(40), 'ledger_key': direct['ledger_key'],
                 'valid_at': direct['valid_at'], 'body': direct['body']}], error='MODEL.INVALID_VALUE')])
    V('STD-KIND-030', 'a second append of the same line fails', [
        commit([{'op': 'append', 'class': 'BankStatementLine', 'entry_id': eid, 'ledger_key': direct['ledger_key'],
                 'valid_at': direct['valid_at'], 'body': direct['body']}]),
        ingest(s1),
        commit([{'op': 'append', 'class': 'BankStatementLine', 'entry_id': eid, 'ledger_key': direct['ledger_key'],
                 'valid_at': direct['valid_at'], 'body': direct['body']}], error='TXN.ALREADY_EXISTS')])
    V('STD-KIND-030', 'ingestion needs a class that declares statement lines', [
        ingest(s1, klass='StockMove', error='MODEL.INVALID_VALUE')])

    # ---- STD-TIME-021 projections ----
    bal = proj(70, 'balances', 'balances')
    two = [gl(uid(10), '1010', '100.00'), gl(uid(11), '4000', '-100.00')]
    more = [gl(uid(12), '1010', '30.00', group='g2', valid_at='2026-03-20'),
            gl(uid(13), '5000', '-30.00', group='g2', valid_at='2026-03-20')]
    P = lambda desc, steps: B('STD-TIME-021', desc, steps)  # noqa: E731
    P('a projection gives the balance per account with the watermark', [
        commit([bal]), commit(two), commit(more), rp()])
    P('asof counts the entries valid at or before the date', [commit([bal]), commit(two), commit(more),
                                                              rp(asof='2026-03-15'), rp(asof='2026-03-14')])
    P('a read at a past commit folds the entries of that commit', [
        commit([bal]), commit(two, label='c1'), commit(more), rp(time={'$ref': 'c1/cseq'})])
    P('money is aggregated per currency', [
        commit([bal]), commit(two),
        commit([gl(uid(14), '1010', '50.00', group='g3', body={'account': '1010', 'amount': EUR('50.00')}),
                gl(uid(15), '4000', '-50.00', group='g3', body={'account': '4000', 'amount': EUR('-50.00')})]),
        rp(group={'account': '1010'})])
    P('min, max and last', [
        commit([proj(70, 'stats', 'stats', aggregates=[{'fn': 'min', 'field': 'amount', 'as': 'low'},
                                                       {'fn': 'max', 'field': 'amount', 'as': 'high'},
                                                       {'fn': 'last', 'field': 'memo', 'as': 'memo'}])]),
        commit([dict(gl(uid(10), '1010', '100.00'), body={'account': '1010', 'amount': {'amount': '100.00', 'currency': 'USD'},
                                                          'memo': 'first'}),
                gl(uid(11), '4000', '-100.00')]),
        commit([dict(gl(uid(12), '1010', '-30.00', group='g2', valid_at='2026-03-20'),
                     body={'account': '1010', 'amount': {'amount': '-30.00', 'currency': 'USD'}, 'memo': 'second'}),
                gl(uid(13), '5000', '30.00', group='g2', valid_at='2026-03-20')]),
        rp('stats', group={'account': '1010'})])
    P('a decimal sum keeps the largest scale; no group_by gives one group', [
        commit([proj(70, 'stock', 'stock', source='StockMove', group_by=(),
                     aggregates=[{'fn': 'sum', 'field': 'quantity', 'as': 'quantity'}, {'fn': 'count', 'as': 'n'}])]),
        commit([stock(uid(20), 'bolt', '10.5'), stock(uid(21), 'nut', '2.25'), stock(uid(22), 'bolt', '-3')]),
        rp('stock')])
    P('group keeps the groups with the given key values', [commit([bal]), commit(two), commit(more),
                                                            rp(group={'account': '4000'})])
    P('group names fields of group_by', [commit([bal]), commit(two), rp(group={'memo': 'x'},
                                                                         error='QRY.INVALID_QUERY')])
    P('an unknown projection is not found', [commit(two), rp('missing', error='IAM.NOT_FOUND')])
    lagged = [commit([proj(70, 'balances', 'balances', mode='async')]), commit(two, label='c1'),
              op('projection_lag', branch='main', projection='balances', cseq={'$ref': 'c1/cseq'}), commit(more,
                                                                                                         label='c2')]
    P('an async projection reads the fold at its watermark', lagged + [rp()])
    P('at_least above the watermark fails with QRY.WATERMARK_TIMEOUT', lagged + [
        rp(at_least={'$ref': 'c2/cseq'}, error='QRY.WATERMARK_TIMEOUT', details=True), rp(at_least={'$ref': 'c1/cseq'})])
    P('an async projection catches up when maintenance resumes', lagged + [
        op('projection_lag', branch='main', projection='balances', cseq=None), rp(at_least={'$ref': 'c2/cseq'})])
    P('a rebuild switches to the fold at the head', lagged + [
        op('rebuild_projection', branch='main', projection='balances'), rp()])
    P('a rebuild of a sync projection gives the same result', [
        commit([bal]), commit(two), commit(more), rp(label='before'),
        op('rebuild_projection', branch='main', projection='balances'), dict(rp(), same_as='before')])
    P('a sync projection does not lag', [commit([bal]), op('projection_lag', branch='main', projection='balances',
                                                            cseq=1, error='MODEL.INVALID_VALUE')])
    P('a changed declaration applies to the reads after it', [
        commit([bal], label='p'), commit(two), commit(more),
        commit([{'op': 'update', 'object_id': uid(70), 'base': base('p', uid(70)), 'body': {'group_by': []}}]),
        rp()])
    P('a projection folds the entries of its branch', [
        commit([bal]), commit(two), op('create_branch', name='feature/q1', **{'from': 'main'}),
        commit(more, branch='feature/q1'), rp(branch='feature/q1'), rp()])
    for desc, body in (
            ('the source of a projection is a ledger class', {'source': 'Customer'}),
            ('group_by names fields of the source', {'group_by': ['region']}),
            ('sum takes a numeric or money field', {'aggregates': [{'fn': 'sum', 'field': 'memo', 'as': 'm'}]}),
            ('an aggregate has a known function', {'aggregates': [{'fn': 'avg', 'field': 'amount', 'as': 'm'}]}),
            ('aggregate names are unique', {'aggregates': [{'fn': 'count', 'as': 'n'}, {'fn': 'count', 'as': 'n'}]})):
        P(desc, [commit([proj(70, 'bad', 'bad', **{k: v for k, v in body.items() if k != 'aggregates'},
                              aggregates=body.get('aggregates'))], error='MODEL.INVALID_VALUE')])
    P('two live projections do not share a name', [
        commit([bal]), commit([proj(71, 'balances-2', 'balances')], error='MODEL.CONSTRAINT_VIOLATED')])

    # ---- STD-ISA-037 read.projection ----
    R = lambda desc, steps, data=(): L('STD-ISA-037', desc, steps, data=data)  # noqa: E731
    R('read.projection reads at the process snapshot', [
        commit(two, branch=BR), ex('proj', ['balances', None, None, None, None])], data=[bal])
    R('read.projection filters by group and asof', [
        commit(two, branch=BR), commit(more, branch=BR),
        ex('proj', ['balances', {'account': '1010'}, '2026-03-15', None, None])], data=[bal])
    R('an unknown projection is not found', [ex('proj', ['missing', None, None, None, None], error='IAM.NOT_FOUND')])
    R('at_least above the watermark of an async projection fails', [
        commit(two, branch=BR, label='c1'),
        op('projection_lag', branch=BR, projection='balances', cseq={'$ref': 'c1/cseq'}),
        commit(more, branch=BR, label='c2'),
        ex('proj', ['balances', None, None, None, {'$ref': 'c2/cseq'}], error='QRY.WATERMARK_TIMEOUT')],
      data=[proj(70, 'balances', 'balances', mode='async')])
    pbal = proj(71, 'accounts', 'accounts', source='Posting')
    sales2 = [post(12, 'bank', '40.00', group='g2'), post(13, 'sales', '-40.00', group='g2')]
    B('STD-ISA-037', 'rollup sums the leaves into every account above them', [
        commit(chart + [pbal]), commit(sale), commit(sales2), rp('accounts', rollup=True)])
    B('STD-ISA-037', 'rollup follows the hierarchy at the asof date', [
        commit(chart + [pbal]), commit(sale),
        op('read', branch='main', object_id=uid(32), label='b'),
        commit([{'op': 'update', 'object_id': uid(32), 'base': {'$ref': 'b/version_hash'},
                 'body': {'parent': ACC('revenue')}, 'valid': {'kind': 'supersede_from', 'from': '2026-03-20'}}]),
        commit(sales2), rp('accounts', rollup=True, asof='2026-03-19'), rp('accounts', rollup=True, asof='2026-03-25')])
    B('STD-ISA-037', 'rollup needs count and sum aggregates over the account field', [
        commit(chart + [proj(71, 'accounts', 'accounts', source='Posting',
                             aggregates=[{'fn': 'max', 'field': 'amount', 'as': 'm'}])]),
        commit(sale), rp('accounts', rollup=True, error='QRY.INVALID_QUERY')])

    # ---- STD-TIME-043 compensation proposals ----
    proposals(g)


RATE = cls('Rate', [field('value', 'decimal', True)])
PROPOSAL_CLASSES = CLASSES + [RATE]


def poster(method):
    amount = 'money(r.fields.value, "USD") * 1000'
    return {'name': 'acme.post', 'tier': 'host', 'abi': '1.0', 'profile': 'standard', 'state': 'committed',
            'capabilities': ['read', 'commit'], 'exports': ['post'],
            'source': {'functions': {'post': {'params': ['d'], 'body': [
                {'do': 'read.get', 'args': {'target': '"ubos://acme/Rate/r-1"'}, 'as': 'r'},
                {'do': 'commit.append', 'args': {'class': '"GlEntry"', 'ledger_key': {'account': '"6100"'},
                                                 'fields': {'account': '"6100"', 'amount': amount,
                                                            'memo': '"%s"' % method},
                                                 'posting_group': '"fee"', 'valid_at': 'params.d'}},
                {'do': 'commit.append', 'args': {'class': '"GlEntry"', 'ledger_key': {'account': '"2100"'},
                                                 'fields': {'account': '"2100"', 'amount': '0.00USD - ' + amount,
                                                            'memo': '"%s"' % method},
                                                 'posting_group': '"fee"', 'valid_at': 'params.d'}},
                {'return': '1'}]}}}}


def proposals(g):
    def V(desc, steps):
        g.vector('STD-TIME-043', desc, setup=setup('2026-01-05T09:00:00.000000Z', PROPOSAL_CLASSES), steps=steps,
                 prototype=False)

    deploy = [
        commit([create(uid(1), 'Rate', 'r-1', {'value': '0.10'}, valid={'from': '2026-01-01'}),
                create(uid(80), 'Logic', 'acme-post', poster('v1'))], label='d'),
        op('verify_asset', branch='main', object_id=uid(80)),
        {'op': 'set_now', 'input': {'now': '2026-02-16T09:00:00.000000Z'}},
        op('execute', branch='main', asset='acme.post', function='post', args=['2026-02-15'],
           ctx={'asof': '2026-02-15'}, process={'process_id': uid(501)}, entropy_seed='11' * 32),
        {'op': 'set_now', 'input': {'now': '2026-03-05T09:00:00.000000Z'}},
        op('read', branch='main', object_id=uid(1), label='r'),
        commit([{'op': 'update', 'object_id': uid(1), 'base': {'$ref': 'r/version_hash'}, 'body': {'value': '0.12'},
                 'valid': {'kind': 'supersede_from', 'from': '2026-01-01'}}], label='fix')]

    def restate(label=None, error=None, **kw):
        return op('restate', label, error, branch='main', time={'$ref': 'fix/cseq'}, name='correction/rate-fix', **kw)

    def ledger(account, branch='correction/rate-fix'):
        return op('read_ledger', branch=branch, **{'class': 'GlEntry', 'ledger_key': {'account': account}})

    V('a restatement proposes the difference per ledger key, dated in the open period', deploy + [
        restate(proposals={})])
    V('date_policy original dates the proposals at the valid time of the original entries', deploy + [
        restate(proposals={'date_policy': 'original'})])
    V('the appends of a re-run are not committed on the correction branch', deploy + [
        restate(proposals={}), ledger('6100'), ledger('6100', branch='main')])
    V('without proposals the result has none', deploy + [restate()])
    V('an unchanged result gives no proposals', [
        commit([create(uid(1), 'Rate', 'r-1', {'value': '0.10'}, valid={'from': '2026-01-01'}),
                create(uid(80), 'Logic', 'acme-post', poster('v1'))], label='d'),
        op('verify_asset', branch='main', object_id=uid(80)),
        {'op': 'set_now', 'input': {'now': '2026-02-16T09:00:00.000000Z'}},
        op('execute', branch='main', asset='acme.post', function='post', args=['2026-02-15'],
           ctx={'asof': '2026-02-15'}, process={'process_id': uid(501)}, entropy_seed='11' * 32),
        op('read', branch='main', object_id=uid(1), label='r'),
        commit([{'op': 'update', 'object_id': uid(1), 'base': {'$ref': 'r/version_hash'}, 'body': {'value': '0.10'},
                 'valid': {'kind': 'supersede_from', 'from': '2026-01-01'}}], label='fix'),
        restate(proposals={})])
    V('date_policy is open_period or original', deploy + [restate(proposals={'date_policy': 'later'},
                                                                  error='MODEL.INVALID_VALUE')])
