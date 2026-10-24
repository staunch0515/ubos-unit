"""Vector cases of BPA 1.0-rc8 for search and views (PH-2 step 1g, DEC-095): `search.aggregate`
(STD-ISA-081), `search.traverse` (STD-ISA-082), `search.text` (STD-ISA-083) and view
resolution (STD-CLS-023). Not required of the PH-0 prototype."""
from .common import NOW, cls, field, uid, create
from .common import commit as _commit
from .model import port
from .sheets import op, sheet
from .execution import asset, fn

BR = 'feature/search'
ALICE = 'did:ubos:acme:01900000-0000-7000-8000-0000000000a1'
BOB = 'did:ubos:acme:01900000-0000-7000-8000-0000000000a2'
P = lambda n: 'ubos://acme/Person/%s' % n  # noqa: E731
V = lambda n: 'ubos://acme/View/%s' % n  # noqa: E731
PERSON = cls('Person', [field('name', 'text', True), field('did', 'text'), field('region', 'text'),
                        field('manager', 'reference', constraints={'target_class': 'Person'})], principal_field='did')
TEAM = cls('Team', [field('name', 'text', True),
                    field('members', 'list', constraints={'item_type': {'type': 'reference',
                                                                        'constraints': {'target_class': 'Person'}}})])
INVOICE = cls('Invoice', [field('number', 'text', True), field('region', 'text'), field('amount', 'money'),
                          field('qty', 'integer'), field('rate', 'decimal'),
                          field('margin', 'decimal', classification='confidential'),
                          field('owner', 'reference', constraints={'target_class': 'Person'}, authz_relation='owner'),
                          field('title', 'text', text_index=True),
                          field('memo', 'text', text_index=True, classification='confidential')],
              ports=[port('approve', 'acme.srch#approve')],
              views=[{'mode': 'view', 'view': V('invoice'),
                      'variants': [{'device': 'phone', 'view': V('invoice-phone')},
                                   {'device': 'phone', 'role': 'auditor', 'view': V('invoice-audit-phone')},
                                   {'role': 'auditor', 'view': V('invoice-audit')}]},
                     {'mode': 'print', 'view': V('missing')}])
CREDIT = cls('CreditNote', [field('reason', 'text')], parent='Invoice')
CLASSES = [PERSON, TEAM, INVOICE, CREDIT]


def usd(a, c='USD'):
    return {'amount': a, 'currency': c}


def inv(n, num, region, amount, qty, rate, owner, title, memo=None, cur='USD'):
    body = {'number': num, 'region': region, 'amount': usd(amount, cur), 'qty': qty, 'rate': rate, 'margin': '0.2',
            'owner': P(owner), 'title': title}
    if memo:
        body['memo'] = memo
    return create(uid(n), 'Invoice', 'i-%d' % (n - 10), body)


def view(n, slug):
    return create(uid(n), 'View', slug, {'name': 'acme.%s' % slug, 'description': slug, 'layout': {'slug': slug}})


DATA = [create(uid(1), 'Person', 'alice', {'name': 'Alice', 'did': ALICE, 'region': 'east'}),
        create(uid(2), 'Person', 'bob', {'name': 'Bob', 'did': BOB, 'region': 'west', 'manager': P('alice')}),
        create(uid(3), 'Person', 'carol', {'name': 'Carol', 'region': 'west', 'manager': P('bob')}),
        create(uid(5), 'Team', 'ops', {'name': 'Ops', 'members': [P('alice'), P('bob')]}),
        inv(11, 'INV-1', 'east', '100.00', 2, '1.5', 'alice', 'Quarterly invoices for consulting services',
            'Payment reminder sent'),
        inv(12, 'INV-2', 'east', '50.50', 3, '2.25', 'alice', 'Consulting retainer'),
        inv(13, 'INV-3', 'west', '80.00', 5, '0.5', 'bob', 'Hardware invoice', cur='EUR'),
        inv(14, 'INV-4', 'west', '20.00', 1, '3', 'bob', 'Service subscription renewal'),
        view(41, 'invoice'), view(42, 'invoice-phone'), view(43, 'invoice-audit-phone'), view(44, 'invoice-audit')]
GET = lambda instr, args: [{'do': instr, 'args': args, 'as': 'r'}, {'return': 'r'}]  # noqa: E731
SRCH = asset(80, 'acme.srch', {
    'agg': fn(GET('search.aggregate', {'query': 'params.q', 'group_by': 'params.g', 'aggregates': 'params.a',
                                       'convert': 'params.c'}), ['q', 'g', 'a', 'c']),
    'walk': fn(GET('search.traverse', {'start': 'params.s', 'fields': 'params.f', 'direction': 'params.d',
                                       'depth': 'params.n', 'classes': 'params.k', 'limit': 'params.l'}),
               ['s', 'f', 'd', 'n', 'k', 'l']),
    'find': fn(GET('search.text', {'text': 'params.t', 'classes': 'params.k', 'limit': 'params.l'}), ['t', 'k', 'l']),
    'approve': fn([{'return': '"ok"'}])}, caps=('search', 'read'))
SEED = '11' * 32
Q = {'from': 'Invoice'}


def commit(changes, branch=BR, **kw):
    return _commit(changes, branch=branch, **kw)


def ex(function, args, error=None, label=None, principal=None, pid=None, details=False):
    inp = {'branch': BR, 'asset': 'acme.srch', 'function': function, 'args': args, 'entropy_seed': SEED}
    proc = {}
    if principal:
        proc['initiator'] = principal
    if pid:
        proc['process_id'] = uid(500 + pid)
    if proc:
        inp['process'] = proc
    s = op('execute', label, error, **inp)
    if details:
        s['details'] = True
    return s


def agg(aggs, group_by=None, q=Q, convert=None, **kw):
    return ex('agg', [q, group_by, aggs, convert], **kw)


def walk(start, fields=None, direction=None, depth=None, classes=None, limit=None, **kw):
    return ex('walk', [start, fields, direction, depth, classes, limit], **kw)


def find(text, classes=None, limit=None, **kw):
    return ex('find', [text, classes, limit], **kw)


def a(fn_, field_=None, as_=None, **kw):
    d = {'fn': fn_, 'as': as_ or fn_}
    if field_:
        d['field'] = field_
    d.update(kw)
    return d


def rules(source, n=900):
    return commit([sheet(uid(n), 'rules-%d' % n, source)])


OWNER_ONLY = 'Invoice { access: [{action: "read", allow_if: "principal in this.owner"}]; }'


def vw(mode, label=None, error=None, **kw):
    return op('resolve_view', label, error, branch=BR, mode=mode, **kw)


def cases(g):
    def S(clause, desc, steps):
        g.vector(clause, desc, setup=[{'op': 'set_now', 'input': {'now': NOW}},
                                      {'op': 'load_genesis', 'input': {'classes': CLASSES}},
                                      {'op': 'create_branch', 'input': {'name': BR, 'from': 'main'}},
                                      commit(DATA + [SRCH], label='d')], steps=steps, prototype=False)

    # ---- STD-ISA-081: search.aggregate ----
    def A(desc, steps):
        S('STD-ISA-081', desc, steps)

    A('money is aggregated per currency within each group', [agg([a('count'), a('sum', 'amount', 'total')], ['region'])])
    A('avg, min and max of decimals and integers', [agg([a('avg', 'rate'), a('min', 'qty'), a('max', 'qty'),
                                                         a('avg', 'qty', 'avg_qty'), a('sum', 'qty', 'sum_qty')])])
    A('percentile takes the nearest rank', [agg([a('percentile', 'qty', 'p50', p=50), a('percentile', 'qty', 'p90', p=90)])])
    A('an explicit conversion with rates aggregates money in one currency',
      [agg([a('sum', 'amount', 'total'), a('avg', 'amount', 'mean')], convert={'currency': 'USD', 'rates': {'EUR': '1.10'}})])
    A('a conversion without the rate of a currency fails',
      [agg([a('sum', 'amount', 'total')], convert={'currency': 'USD', 'rates': {}}, error='MODEL.CURRENCY_MISMATCH')])
    A('the query selects the rows', [agg([a('count'), a('sum', 'qty')], q={'from': 'Invoice', 'where': 'this.qty > 2'})])
    A('only the rows the principal may read count',
      [rules(OWNER_ONLY), agg([a('count'), a('sum', 'qty')], principal=ALICE), agg([a('count')], principal=BOB)])
    A('a field masked for the principal is not aggregated',
      [agg([a('sum', 'margin')], error='QRY.INVALID_QUERY', details=True),
       op('principal', None, None, did=ALICE, clearances=['confidential']), agg([a('sum', 'margin')], principal=ALICE)])
    A('the functions are count, sum, avg, min, max and percentile', [agg([a('median', 'qty')], error='MODEL.INVALID_VALUE')])
    A('a percentile is from 1 to 100', [agg([a('percentile', 'qty', p=0)], error='MODEL.INVALID_VALUE')])
    A('an unknown field is an invalid query', [agg([a('sum', 'weight')], error='QRY.INVALID_QUERY')])
    A('sum and avg need a numeric field', [agg([a('sum', 'region')], error='QRY.INVALID_QUERY')])
    A('subclass rows are included', [commit([create(uid(15), 'CreditNote', 'cn-1', {'number': 'CN-1', 'region': 'east',
                                                                                  'qty': 4, 'reason': 'return'})]),
                                     agg([a('count'), a('sum', 'qty')], ['region'])])

    # ---- STD-ISA-082: search.traverse ----
    def T(desc, steps):
        S('STD-ISA-082', desc, steps)

    T('one step out follows the references of the start', [walk('ubos://acme/Invoice/i-1')])
    T('one step in finds the objects that refer to the start', [walk(P('alice'), direction='in')])
    T('both directions to depth two', [walk(P('bob'), direction='both', depth=2)])
    T('fields restrict the references followed', [walk(P('alice'), fields=['owner'], direction='in')])
    T('classes filter the nodes returned, not the walk', [walk(P('carol'), direction='both', depth=3, classes=['Team'])])
    T('the depth is at most eight', [walk(P('alice'), depth=9, error='MODEL.INVALID_VALUE')])
    T('a limit truncates the result', [walk(P('alice'), direction='in', limit=1)])
    T('nodes the principal may not read are neither returned nor expanded',
      [rules(OWNER_ONLY), walk(P('bob'), direction='in', depth=2, principal=ALICE)])
    T('an unknown start is not found', [walk(P('dave'), error='IAM.NOT_FOUND')])

    # ---- STD-ISA-083: search.text ----
    def X(desc, steps):
        S('STD-ISA-083', desc, steps)

    X('results are ranked by matched words, then by frequency, with highlighted snippets',
      [find('consulting invoices')])
    X('plural forms match the singular', [find('invoice')])
    X('classes and limit restrict the results', [find('invoice', classes=['CreditNote']), find('invoice', limit=1)])
    X('a field masked for the principal is not searched',
      [find('reminder'), op('principal', None, None, did=ALICE, clearances=['confidential']),
       find('reminder', principal=ALICE)])
    X('only objects the principal may read are found', [rules(OWNER_ONLY), find('invoice', principal=BOB)])
    X('a lagging index omits newer changes and says so',
      [op('index_lag', None, None, branch=BR, cseq=1),
       commit([inv(16, 'INV-6', 'east', '5.00', 1, '1', 'alice', 'Late invoice')]), find('invoice'),
       op('index_lag', None, None, branch=BR, cseq=None), find('invoice')])
    X('the result is journaled: a replay returns it although the index changed',
      [find('invoice', pid=1), commit([inv(16, 'INV-6', 'east', '5.00', 1, '1', 'alice', 'Late invoice')]),
       op('replay', None, None, process_id=uid(501))])
    X('a search has at least one word', [find('  ', error='MODEL.INVALID_VALUE')])

    # ---- STD-CLS-023: views ----
    def W(desc, steps):
        S('STD-CLS-023', desc, steps)

    W('the view declared for the mode is selected', [vw('view', object_id=uid(11))])
    W('a subclass without a view takes the view of its parent',
      [commit([create(uid(15), 'CreditNote', 'cn-1', {'number': 'CN-1', 'reason': 'return'})]),
       vw('view', object_id=uid(15))])
    W('the variant matching most context keys refines the view',
      [vw('view', **{'class': 'Invoice', 'ctx': {'device': 'phone'}}),
       vw('view', **{'class': 'Invoice', 'ctx': {'device': 'phone', 'role': 'auditor'}}),
       vw('view', **{'class': 'Invoice', 'ctx': {'role': 'auditor', 'locale': 'de-DE'}})])
    W('without a declared view the view is generated from the effective definition',
      [vw('edit', object_id=uid(11)), vw('view', **{'class': 'Person'})])
    W('field visibility declared by sheets shapes the generated view',
      [rules('Invoice { field_visibility.margin: "hidden"; field_visibility.rate: "read_only"; }'),
       vw('edit', object_id=uid(11))])
    W('a declared view must resolve to a view object', [vw('print', error='MODEL.REFERENCE_INVALID', **{'class': 'Invoice'})])
    W('the modes are view, edit, create, list, card and print', [vw('table', error='MODEL.INVALID_VALUE', **{'class': 'Invoice'})])
