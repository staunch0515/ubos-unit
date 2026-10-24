"""Vector cases of BPA 1.0-rc8 for integration and addressing (PH-2 step 1f, DEC-094):
`call.connector` (STD-ISA-090), the compensation of pre-commit calls (STD-TXN-051),
`emit.notify` (STD-ISA-071), short forms and VFS paths (STD-ADDR-003, -004) and the REST facade
(STD-PROTO-021). The outside world is simulated by scripted connector responses. Not required
of the PH-0 prototype."""
from .common import NOW, cls, field, uid, create
from .common import commit as _commit
from .model import port
from .sheets import op
from .execution import asset, fn

BR = 'feature/int'
HOST = 'bank.example'
ALICE = 'did:ubos:acme:01900000-0000-7000-8000-0000000000a1'
SUPPORT = 'did:ubos:acme:01900000-0000-7000-8000-0000000000b1'
BANK_URI = 'ubos://acme/Connector/bank'
F = lambda name, type_, **kw: dict({'name': name, 'type': type_}, **kw)  # noqa: E731
OPS = {'reserve': {'now': True, 'compensation': 'release', 'redact': ['token'],
                   'request': [F('account', 'text', required=True), F('amount', 'decimal', required=True),
                               F('tax_id', 'text', classification='personal')],
                   'response': [F('reservation', 'text', required=True), F('token', 'text')]},
       'release': {'request': [F('account', 'text'), F('amount', 'decimal'), F('tax_id', 'text')], 'response': []},
       'check': {'now': True, 'compensation': 'none', 'compensation_reason': 'a read changes nothing',
                 'request': [F('account', 'text', required=True)], 'response': [F('status', 'text', required=True)]},
       'pay': {'request': [F('account', 'text', required=True), F('amount', 'decimal', required=True)],
               'response': [F('payment_id', 'text')]}}


def connector(n=60, slug='bank', host=HOST, **extra):
    body = {'name': slug, 'description': slug, 'endpoint': {'protocol': 'https', 'host': host, 'path': '/api'},
            'auth': {'method': 'oauth2', 'credential': 'secret://bank/api'}, 'operations': OPS,
            'retry': {'max_attempts': 2}, 'breaker': {'failures': 3, 'open_for': 'PT1M'},
            'rate_limit': {'per': 'minute', 'count': 3}, 'allowed_classifications': ['public', 'internal']}
    body.update(extra)
    return create(uid(n), 'Connector', slug, body)


PERSON = cls('Person', [field('name', 'text', True), field('did', 'text')], principal_field='did')
CUSTOMER = cls('Customer', [field('name', 'text', True), field('balance', 'decimal'),
                            field('tax_id', 'text', classification='personal'),
                            field('owner', 'reference', constraints={'target_class': 'Person'}, authz_relation='owner')],
               ports=[port('touch', 'acme.int#touch')])
CLASSES = [PERSON, CUSTOMER]
C1 = uid(1)
DATA = [create(uid(2), 'Person', 'alice', {'name': 'Alice', 'did': ALICE}),
        create(C1, 'Customer', 'c-1', {'name': 'Acme', 'balance': '100', 'tax_id': '123-45-6789',
                                       'owner': 'ubos://acme/Person/alice'}),
        create(uid(3), 'Customer', 'c-2', {'name': 'Globex', 'balance': '20'}),
        create(uid(70), 'NotificationTemplate', 'update', {'name': 'acme.update', 'description': 'update',
                                                           'title': 'Update for {{this.name}}',
                                                           'body': 'Tax ID {{this.tax_id}}, balance {{this.balance}}'})]
CALL = lambda op_, req, when='now': {'do': 'call.connector', 'args': {  # noqa: E731
    'connector': '"%s"' % BANK_URI, 'operation': '"%s"' % op_, 'request': req, 'when': '"%s"' % when}}
RESERVE = {'account': '"a-1"', 'amount': '"10.50"'}
NOTIFY = lambda recipients, **kw: {'do': 'emit.notify', 'args': dict({  # noqa: E731
    'template': '"ubos://acme/NotificationTemplate/update"', 'subject': '"ubos://acme/Customer/c-1"',
    'recipients': recipients}, **kw)}
FAIL = {'fail': 'RULE.INVARIANT_VIOLATED'}
FUNCTIONS = {
    'reserve': fn([dict(CALL('reserve', RESERVE), **{'as': 'r'}), {'return': 'r.reservation'}]),
    'reserve_fail': fn([CALL('reserve', RESERVE), FAIL]),
    'reserve_twice_fail': fn([CALL('reserve', RESERVE), CALL('reserve', {'account': '"a-2"', 'amount': '"5.00"'}), FAIL]),
    'check_fail': fn([CALL('check', {'account': '"a-1"'}), FAIL]),
    'check': fn([dict(CALL('check', {'account': '"a-1"'}), **{'as': 'r'}), {'return': 'r.status'}]),
    'personal': fn([CALL('reserve', dict(RESERVE, tax_id='"123-45-6789"'))]),
    'unknown_op': fn([CALL('refund', RESERVE)]),
    'bad_request': fn([CALL('reserve', {'account': '"a-1"', 'amount': '"ten"'})]),
    'pay_later': fn([dict(CALL('pay', {'account': '"a-1"', 'amount': '"10.50"'}, 'after_commit'), **{'as': 'e'}),
                     {'return': 'e.effect_id'}]),
    'pay_now': fn([CALL('pay', {'account': '"a-1"', 'amount': '"10.50"'})]),
    'notify': fn([NOTIFY(['"%s"' % ALICE, '"role:support"', '"relation:owner"'], channels=['"in_app"', '"email"'],
                         priority='"high"', action='"touch"')]),
    'notify_fail': fn([NOTIFY(['"%s"' % ALICE]), FAIL]),
    'notify_bad': fn([NOTIFY(['"bob"'])]),
    'notify_unknown': fn([NOTIFY(['"%s"' % ALICE], template='"ubos://acme/NotificationTemplate/none"')]),
    'touch': fn([{'return': '"touched"'}])}
INT = asset(80, 'acme.int', FUNCTIONS, caps=('commit', 'read', 'run', 'emit', 'call', 'connector:' + BANK_URI))
PLAIN = asset(81, 'acme.plain', {'reserve': FUNCTIONS['reserve']}, caps=('call',))
SEED = '11' * 32


def commit(changes, branch=BR, **kw):
    return _commit(changes, branch=branch, **kw)


def ex(function, error=None, label=None, a='acme.int', pid=None, details=False, **kw):
    inp = {'branch': BR, 'asset': a, 'function': function, 'args': [], 'entropy_seed': SEED}
    if pid is not None:
        inp['process'] = {'process_id': uid(500 + pid)}
    inp.update(kw)
    s = op('execute', label, error, **inp)
    if details:
        s['details'] = True
    return s


def stub(operation, *responses):
    return op('connector_stub', None, None, branch=BR, connector=BANK_URI, operation=operation, responses=list(responses))


def ok(**body):
    return {'status': 200, 'body': body}


DOWN = {'unavailable': True}


def rest(method, path, label=None, principal=ALICE, **kw):
    return op('rest', label, None, method=method, path=path, principal=principal, **kw)


def cases(g):
    def S(clause, desc, steps, extra=(), egress=True, data=True):
        setup = [{'op': 'set_now', 'input': {'now': NOW}}, {'op': 'load_genesis', 'input': {'classes': CLASSES}},
                 {'op': 'create_branch', 'input': {'name': BR, 'from': 'main'}}]
        if egress:
            setup.append(op('egress_allow', None, None, hosts=[HOST]))
        if data:
            setup.append(commit(DATA + [INT, PLAIN, connector()], label='d'))
        g.vector(clause, desc, setup=setup + list(extra), steps=steps, prototype=False)

    def X(desc, steps, **kw):
        S('STD-ISA-090', desc, steps, **kw)

    # ---- STD-ISA-090: call.connector ----
    X('a call before commit returns the validated response; the exchange is recorded with redactions',
      [stub('reserve', ok(reservation='R-1', token='t0k3n')), ex('reserve'), op('exchanges')])
    X('a connector endpoint outside the egress allow-list is refused at commit',
      [commit([connector(61, 'other', host='evil.example')], error='LOGIC.EGRESS_NOT_ALLOWED')])
    X('a credential is a secret handle, never the secret',
      [commit([connector(61, 'other', auth={'method': 'basic', 'credential': 'hunter2'})], error='MODEL.INVALID_VALUE')])
    X('an operation called before commit declares its compensation',
      [commit([connector(61, 'other', operations={'hold': {'now': True, 'request': [], 'response': []}})],
              error='MODEL.DECLARATION_INCOMPLETE')])
    X('a personal field is not sent to a connector that allows public and internal data only',
      [stub('reserve', ok(reservation='R-1')), ex('personal', 'LOGIC.CLASSIFICATION_EGRESS_DENIED', details=True),
       op('exchanges')])
    X('an asset calls only the connectors it lists',
      [commit([create(uid(82), 'Logic', 'acme-bare', {
          'name': 'acme.bare', 'tier': 'host', 'abi': '1.0', 'source': {'functions': {'reserve': FUNCTIONS['reserve']}},
          'exports': ['reserve'], 'capabilities': ['call'], 'profile': 'standard', 'state': 'committed'})]),
       ex('reserve', 'LOGIC.FORBIDDEN_CAPABILITY', a='acme.bare')])
    X('an operation the connector does not declare is refused', [ex('unknown_op', 'MODEL.INVALID_VALUE')])
    X('a request is validated against the operation schema', [ex('bad_request', 'MODEL.INVALID_VALUE')])
    X('a call after commit returns an effect identifier and leaves through the outbox',
      [ex('pay_later'), op('outbox'), op('exchanges')])
    X('an operation not declared for calls before commit is not called now',
      [ex('pay_now', 'LOGIC.FORBIDDEN_CAPABILITY')])
    X('an unavailable connector is retried by its policy', [stub('reserve', DOWN, ok(reservation='R-2')),
                                                           ex('reserve'), op('exchanges')])
    X('a connector unavailable through every attempt fails the call',
      [stub('reserve', DOWN, DOWN), ex('reserve', 'LOGIC.CONNECTOR_UNAVAILABLE', details=True)])
    X('an error response fails with the external status',
      [stub('reserve', {'status': 502, 'body': {}}), ex('reserve', 'LOGIC.CONNECTOR_ERROR', details=True)])
    X('a response outside the operation schema is an error',
      [stub('reserve', ok(unexpected='x')), ex('reserve', 'LOGIC.CONNECTOR_ERROR', details=True)])
    X('consecutive failures open the breaker; calls then fail fast until it closes',
      [ex('check', 'LOGIC.CONNECTOR_UNAVAILABLE'), ex('check', 'LOGIC.CONNECTOR_UNAVAILABLE'),
       ex('check', 'LOGIC.CONNECTOR_UNAVAILABLE'), stub('check', ok(status='open')),
       ex('check', 'LOGIC.CONNECTOR_UNAVAILABLE', details=True),
       {'op': 'set_now', 'input': {'now': '2026-03-15T10:01:00.000000Z'}}, ex('check')])
    X('the rate limit of a connector is enforced',
      [stub('check', ok(status='a'), ok(status='b'), ok(status='c'), ok(status='d')), ex('check'), ex('check'),
       ex('check'), ex('check', 'LOGIC.CONNECTOR_UNAVAILABLE', details=True)])
    X('the response is journaled and a replay uses it without calling the connector',
      [stub('reserve', ok(reservation='R-1')), ex('reserve', pid=1),
       op('journal', None, None, process_id=uid(501)), op('replay', None, None, process_id=uid(501)),
       op('exchanges')])
    X('a host removed from the allow-list cannot be called',
      [op('egress_allow', None, None, hosts=['other.example']), ex('reserve', 'LOGIC.EGRESS_NOT_ALLOWED')])

    # ---- STD-TXN-051: compensation ----
    def T(desc, steps):
        S('STD-TXN-051', desc, steps)

    T('a process that fails after a call before commit invokes the compensation and records it',
      [stub('reserve', ok(reservation='R-1')), stub('release', ok()),
       ex('reserve_fail', 'RULE.INVARIANT_VIOLATED', pid=1), op('process_info', None, None, process_id=uid(501)),
       op('exchanges')])
    T('compensations run in reverse order of the calls',
      [stub('reserve', ok(reservation='R-1'), ok(reservation='R-2')), stub('release', ok(), ok()),
       ex('reserve_twice_fail', 'RULE.INVARIANT_VIOLATED', pid=1), op('process_info', None, None, process_id=uid(501))])
    T('a compensation that cannot be made now is queued in the outbox',
      [stub('reserve', ok(reservation='R-1')), ex('reserve_fail', 'RULE.INVARIANT_VIOLATED', pid=1),
       op('process_info', None, None, process_id=uid(501)), op('outbox')])
    T('an operation declared without compensation is recorded as such',
      [stub('check', ok(status='open')), ex('check_fail', 'RULE.INVARIANT_VIOLATED', pid=1),
       op('process_info', None, None, process_id=uid(501))])
    T('a process that commits compensates nothing',
      [stub('reserve', ok(reservation='R-1')), ex('reserve', pid=1), op('process_info', None, None, process_id=uid(501)),
       op('exchanges')])

    # ---- STD-ISA-071: emit.notify ----
    people = [op('principal', None, None, did=ALICE, clearances=['personal']),
              op('principal', None, None, did=SUPPORT, roles=['support'])]
    S('STD-ISA-071', 'a notification goes to each recipient (a DID, a role, a relation) and is rendered with the '
                     'recipient\'s masking',
      people + [ex('notify'), op('outbox')])
    S('STD-ISA-071', 'a process that fails sends no notification', people + [ex('notify_fail', 'RULE.INVARIANT_VIOLATED'),
                                                                               op('outbox')])
    S('STD-ISA-071', 'a recipient is a DID, a role or a relation', [ex('notify_bad', 'MODEL.INVALID_VALUE')])
    S('STD-ISA-071', 'the template is a NotificationTemplate object', [ex('notify_unknown', 'IAM.NOT_FOUND')])

    # ---- STD-ADDR-003: short forms ----
    def A(clause, desc, steps):
        S(clause, desc, steps)

    A('STD-ADDR-003', 'a short form is resolved relative to the VAE',
      [op('resolve', None, None, uri='Customer/c-1?branch=%s' % BR)])
    A('STD-ADDR-003', 'a short form takes the selectors of a URI',
      [op('resolve', None, None, uri='Customer/c-1?branch=%s&asof=2026-03-15' % BR)])
    A('STD-ADDR-003', 'a short form is normalised to the canonical URI',
      [op('resolve_path', None, None, branch=BR, path='Customer/c-1')])
    A('STD-ADDR-003', 'a short form that names no object is not found',
      [op('resolve_path', None, 'IAM.NOT_FOUND', branch=BR, path='Customer/c-9')])
    A('STD-ADDR-003', 'text that is not a short form is refused',
      [op('resolve_path', None, 'MODEL.INVALID_VALUE', branch=BR, path='customer c-1')])

    # ---- STD-ADDR-004: VFS paths ----
    def V(desc, path, error=None):
        A('STD-ADDR-004', desc, [op('resolve_path', None, error, branch=BR, path=path)])

    V('a /data path names a data object', '/data/acme.finance/Customer/c-1')
    V('a /usr/logic path names a logic asset by name', '/usr/logic/acme/int')
    V('an /etc path names a configuration asset by name', '/etc/acme/update')
    V('a /data path in another namespace is not found', '/data/acme.sales/Customer/c-1', 'IAM.NOT_FOUND')
    V('a /data path names data classes only', '/data/ubos/Connector/bank', 'IAM.NOT_FOUND')
    V('a /view path without a view asset is not found', '/view/acme/dashboard', 'IAM.NOT_FOUND')
    V('the roots are data, usr, etc, view and lib', '/tmp/acme/x', 'MODEL.INVALID_VALUE')
    V('a logic path is /usr/logic/<namespace>/<name>', '/usr/acme/int', 'MODEL.INVALID_VALUE')
    A('STD-ADDR-004', 'resolve accepts a VFS path',
      [op('resolve', None, None, uri='/data/acme.finance/Customer/c-1?branch=%s' % BR)])

    # ---- STD-PROTO-021: REST facade ----
    def R(desc, steps):
        S('STD-PROTO-021', desc, steps)

    base = '/api/v1/acme/Customer'
    R('GET of an object maps to a read', [rest('GET', base + '/c-1', query={'branch': BR})])
    R('GET of an unknown object is an RFC 9457 problem with the code and its status',
      [rest('GET', base + '/c-9', query={'branch': BR})])
    R('GET of a class with q maps to a query', [rest('GET', base, query={'branch': BR, 'q': 'this.balance > 50'})])
    R('POST to a port maps to a mutate', [rest('POST', base + '/c-1/touch', query={'branch': BR}, body={})])
    R('an Idempotency-Key is carried: the same request returns the first response',
      [rest('POST', base + '/c-1/touch', 'a', query={'branch': BR}, body={}, headers={'Idempotency-Key': 'k-1'}),
       {'op': 'rest', 'input': {'method': 'POST', 'path': base + '/c-1/touch', 'principal': ALICE,
                                'query': {'branch': BR}, 'body': {}, 'headers': {'Idempotency-Key': 'k-1'}},
        'same_as': 'a'}])
    R('another request under the same Idempotency-Key is a problem',
      [rest('POST', base + '/c-1/touch', query={'branch': BR}, body={}, headers={'Idempotency-Key': 'k-1'}),
       rest('POST', base + '/c-1/touch', query={'branch': BR}, body={'x': 1}, headers={'Idempotency-Key': 'k-1'})])
    R('POST to a class creates an object', [rest('POST', base, query={'branch': BR},
                                                 body={'object_id': uid(9), 'slug': 'c-9', 'fields': {'name': 'Initech'}}),
                                            rest('GET', base + '/c-9', query={'branch': BR})])
    R('a method the facade does not map is a problem with status 405', [rest('DELETE', base + '/c-1')])
    R('another VAE is not found', [rest('GET', '/api/v1/other/Customer/c-1')])
    R('a validation error keeps the status of its code', [rest('POST', base, query={'branch': BR},
                                                              body={'object_id': uid(9), 'slug': 'c-9', 'fields': {}})])
