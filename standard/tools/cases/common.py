"""Shared class declarations and helpers for stateful vector cases."""

NOW = '2026-03-15T10:00:00.000000Z'


def field(name, type_, required=False, **extra):
    f = {'name': name, 'type': type_, 'label': name.replace('_', ' '), 'description': name}
    if required:
        f['required'] = True
    f.update(extra)
    return f


def cls(name, fields, parent='DataObject', kind='definition', **extra):
    d = {'name': name, 'namespace': 'acme.finance', 'kind': kind, 'parent': parent, 'label': name,
         'description': name, 'schema_version': 1, 'fields': fields}
    d.update(extra)
    return d


CUSTOMER = cls('Customer', [field('name', 'text', True), field('email', 'text'),
                            field('credit_limit', 'money'), field('country', 'text', default='US')])
PERIOD = cls('FiscalPeriod', [field('start', 'date', True), field('end', 'date', True),
                              field('state', 'enum', True, constraints={'allowed_values': ['open', 'closing', 'closed']})])
GL = cls('GlEntry', [field('account', 'text', True), field('amount', 'money', True), field('memo', 'text')],
         kind='ledger', ledger={'key': ['account'], 'amounts': ['amount'], 'double_entry': True,
                                'period_scope': 'FiscalPeriod'})
STOCK = cls('StockMove', [field('item', 'text', True), field('quantity', 'decimal', True)],
            kind='ledger', ledger={'key': ['item'], 'amounts': ['quantity'], 'double_entry': False})
DOC = cls('ImportedDocument', [field('source', 'text', True)], kind='raw_document')
JOB = cls('DeliveryJob', [field('target', 'text')], parent='SystemObject', kind='system')

CLASSES = [CUSTOMER, PERIOD, GL, STOCK, DOC, JOB]


def uid(n):
    return '01890a5d-ac96-7%03x-8000-%012x' % (n % 4096, n)


def setup(now=NOW, classes=None):
    return [{'op': 'set_now', 'input': {'now': now}},
            {'op': 'load_genesis', 'input': {'classes': classes or CLASSES}}]


def usd(a):
    return {'amount': a, 'currency': 'USD'}


def create(oid, klass='Customer', slug='c-1', body=None, **extra):
    ch = {'op': 'create', 'object_id': oid, 'class': klass, 'slug': slug,
          'body': body if body is not None else {'name': 'Acme'}}
    ch.update(extra)
    return ch


def commit(changes, branch='main', label=None, error=None, process=None, **kw):
    s = {'op': 'commit', 'input': {'branch': branch, 'changes': changes}}
    if process:
        s['input']['process'] = process
    if label:
        s['label'] = label
    if error:
        s['error'] = error
    s.update(kw)
    return s


def base(label, oid):
    return {'$ref': '%s/versions/%s' % (label, oid)}


def gl(eid, account, amount, group='g1', **extra):
    ch = {'op': 'append', 'class': 'GlEntry', 'entry_id': eid, 'ledger_key': {'account': account},
          'valid_at': '2026-03-15', 'body': {'account': account, 'amount': usd(amount)}, 'posting_group': group}
    ch.update(extra)
    return ch


def stock(eid, item, qty, valid_at='2026-03-15'):
    return {'op': 'append', 'class': 'StockMove', 'entry_id': eid, 'ledger_key': {'item': item},
            'valid_at': valid_at, 'body': {'item': item, 'quantity': qty}}
