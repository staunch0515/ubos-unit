"""Vector cases of BPA 1.0-rc2 for the UBTP protocol (PH-1 step 10d, DEC-068): the message
envelope (STD-PROTO-001), the handshake (STD-PROTO-002), `mutate` (STD-PROTO-010), `query`
(STD-PROTO-011), `subscribe` (STD-PROTO-012), flow control and cancellation (STD-PROTO-013)
and the bindings (STD-PROTO-020), through the in-process binding `ubtp`. Not required of the
PH-0 prototype."""
from .common import uid, create, base, NOW
from .common import commit as _commit
from .sheets import sheet
from .rules import CLASSES as RULE_CLASSES, ACCT, CUSTOMER, ACCOUNT, MOVE

BR = 'feature/api'
FB = {'op': 'create_branch', 'input': {'name': BR, 'from': 'main'}}
CLASSES = RULE_CLASSES
SHEET = uid(900)
DID = 'did:ubos:acme:01900000-0000-7000-8000-00000000000a'


def commit(changes, branch=BR, **kw):
    return _commit(changes, branch=branch, **kw)


def env(i, type_, body=None, **extra):
    m = {'v': '1.0', 'id': str(i), 'type': type_, 'body': body if body is not None else {}}
    m.update(extra)
    return m


def hs(i='h', grants=None, **cred):
    c = {'kind': 'in_process'}
    if grants is not None:
        c['grants'] = list(grants)
    c.update(cred)
    return env(i, 'handshake', {'versions': ['1.0'], 'abi': {'min': '1.0', 'max': '1.0'}, 'encodings': ['json'],
                                'features': [], 'credentials': c})


def U(messages, label=None, session=None, error=None):
    inp = {'messages': messages}
    if session:
        inp['session'] = session
    s = {'op': 'ubtp', 'input': inp}
    if label:
        s['label'] = label
    if error:
        s['error'] = error
    return s


def cr(n, slug, body=None):
    b = {'name': slug.title()}
    b.update(body or {})
    return {'op': 'create', 'object_id': uid(n), 'class': 'Customer', 'slug': slug, 'body': b}


def mutate(i, changes=None, port=None, branch=BR, **extra):
    body = {'vae': 'acme', 'branch': branch}
    if changes is not None:
        body['commit'] = {'changes': changes}
    if port is not None:
        body['port'] = port
    body.update(extra)
    return env(i, 'mutate', body)


def query(i, branch=BR, **body):
    b = {'vae': 'acme', 'branch': branch}
    b.update(body)
    return env(i, 'query', b)


def sub(i, branch=BR, **body):
    b = {'vae': 'acme', 'branch': branch}
    b.update(body)
    return env(i, 'subscribe', b)


def cases(g):
    V = g.vector

    def S(clause, desc, steps, classes=None, now_=NOW, also=()):
        V(clause, desc, setup=[{'op': 'set_now', 'input': {'now': now_}},
                               {'op': 'load_genesis', 'input': {'classes': classes or CLASSES}}, FB],
          steps=steps, prototype=False, also=also)

    envelope(S)
    handshake(S)
    mutate_(S)
    query_(S)
    subscribe(S)
    flow(S)
    bindings(S)


def envelope(S):
    S('STD-PROTO-001', 'a well-formed request gets a result that replies to its id', [U([hs(), env(1, 'ping')])])
    S('STD-PROTO-001', 'server frames carry their own ids and reply_to the request', [U([hs(), env('a', 'ping'), env('b', 'ping')])])
    S('STD-PROTO-001', 'the trace member is carried into the reply', [U([hs(), env(1, 'ping', trace='00-abc-def-01')])])
    S('STD-PROTO-001', 'an envelope needs v, id, type and body', [U([hs(), {'v': '1.0', 'id': '1', 'type': 'ping'}])])
    S('STD-PROTO-001', 'an unknown protocol version is unsupported', [U([hs(), {'v': '2.0', 'id': '1', 'type': 'ping', 'body': {}}])])
    S('STD-PROTO-001', 'an unknown message type is a protocol error', [U([hs(), env(1, 'shout')])])
    S('STD-PROTO-001', 'a server message type cannot be sent by the client', [U([hs(), env(1, 'result')])])
    S('STD-PROTO-001', 'a request carries no reply_to', [U([hs(), env(1, 'ping', reply_to='h')])])
    S('STD-PROTO-001', 'ids are client-unique within the session', [U([hs(), env(1, 'ping'), env(1, 'ping')])])
    S('STD-PROTO-001', 'ids stay unique across calls of the same session', [U([hs(), env(1, 'ping')]), U([env(1, 'ping')])])
    S('STD-PROTO-001', 'the body is a map', [U([hs(), env(1, 'ping', body=[])])])
    S('STD-PROTO-001', 'an unknown envelope member is a protocol error', [U([hs(), env(1, 'ping', colour='red')])])
    S('STD-PROTO-001', 'an error frame carries the error object with code and details', [
        U([hs(), query(1, read='ubos://acme/Customer/none')])])
    S('STD-PROTO-001', 'a message that is not a map is a protocol error', [U([hs(), 'ping'])])
    S('STD-PROTO-001', 'the id of the error reply is the id of the faulty message when it has one', [
        U([hs(), {'v': '1.0', 'id': 'x', 'type': 'ping'}])])


def handshake(S):
    S('STD-PROTO-002', 'the first message must be the handshake', [U([env(1, 'ping'), hs()])])
    S('STD-PROTO-002', 'the handshake answers with the versions, the session, the principal and the limits', [U([hs()])])
    S('STD-PROTO-002', 'the in-process binding takes the principal from the credentials', [
        U([hs(principal=DID, principal_kind='service', role='cfo')])])
    S('STD-PROTO-002', 'without a principal the credentials name the default principal of the adapter', [
        U([hs(grants=('read',))])])
    S('STD-PROTO-002', 'a protocol version outside the supported range fails with the range', [
        U([env('h', 'handshake', {'versions': ['0.9'], 'credentials': {'kind': 'in_process'}})])])
    S('STD-PROTO-002', 'an ABI range that excludes 1.0 is unsupported', [
        U([env('h', 'handshake', {'versions': ['1.0'], 'abi': {'min': '2.0', 'max': '2.0'}, 'credentials': {'kind': 'in_process'}})])])
    S('STD-PROTO-002', 'the in-process binding negotiates JSON only', [
        U([env('h', 'handshake', {'versions': ['1.0'], 'encodings': ['cbor'], 'credentials': {'kind': 'in_process'}})])])
    S('STD-PROTO-002', 'bearer tokens are verified by the Server, not by the in-process binding', [
        U([env('h', 'handshake', {'versions': ['1.0'], 'credentials': {'bearer': 'eyJ...'}})])])
    S('STD-PROTO-002', 'a second handshake on an open session is a protocol error', [U([hs(), hs('h2')])])
    S('STD-PROTO-002', 'after a failed handshake the session stays closed', [
        U([env('h', 'handshake', {'versions': ['0.9'], 'credentials': {'kind': 'in_process'}}), env(1, 'ping')])])
    S('STD-PROTO-002', 'sessions are independent', [U([hs()], session='a'), U([env(1, 'ping')], session='b')])
    S('STD-PROTO-002', 'the handshake is remembered across calls of the session', [U([hs()]), U([env(1, 'ping')])])


def mutate_(S):
    acct = create(uid(1), 'Account', 'a-1', {'name': 'Acme', 'balance': '100', 'status': 'open'})
    T = 'ubos://acme/Account/a-1'
    S('STD-PROTO-010', 'a commit body commits raw changes and answers with the commit and the versions', [
        U([hs(), mutate(1, [cr(1, 'acme')])]), {'op': 'read', 'input': {'branch': BR, 'object_id': uid(1)}}])
    S('STD-PROTO-010', 'a raw commit needs the commit.raw permission when the credentials carry grants', [
        U([hs(grants=('read',)), mutate(1, [cr(1, 'acme')])])])
    S('STD-PROTO-010', 'credentials with the commit.raw grant may commit raw changes', [
        U([hs(grants=('read', 'commit.raw')), mutate(1, [cr(1, 'acme')])])])
    S('STD-PROTO-010', 'a port body runs the port as one process', [
        commit([ACCT, acct]), U([hs(), mutate(1, port={'target': T, 'name': 'rename', 'params': {'name': 'Beta'}})]),
        {'op': 'read', 'input': {'branch': BR, 'object_id': uid(1)}}])
    S('STD-PROTO-010', 'a port body needs no raw permission', [
        commit([ACCT, acct]), U([hs(grants=('read', 'update', 'close')), mutate(1, port={'target': T, 'name': 'close'})])])
    S('STD-PROTO-010', 'mutate carries port or commit, not both', [
        commit([ACCT, acct]), U([hs(), mutate(1, [cr(1, 'x')], port={'target': T, 'name': 'close'})])])
    S('STD-PROTO-010', 'the errors of the process come back as error frames', [
        U([hs(), mutate(1, [cr(1, 'acme', {'rating': 'high'})])])])
    S('STD-PROTO-010', 'validate_only previews a commit without committing', [
        commit([sheet(SHEET, 'v', 'Customer { validations: [{name: "r", rule: "this.rating is null or this.rating <= 3", '
                      'severity: "error", message: "rating"}]; }')]),
        U([hs(), mutate(1, [cr(1, 'acme', {'rating': 5})], validate_only=True)]),
        {'op': 'read', 'input': {'branch': BR, 'object_id': uid(1)}, 'error': 'IAM.NOT_FOUND'}])
    S('STD-PROTO-010', 'validate_only on a port call comes with the simulation of PH-2', [
        commit([ACCT, acct]), U([hs(), mutate(1, port={'target': T, 'name': 'close'}, validate_only=True)])])
    S('STD-PROTO-010', 'base names the head the client saw; a moved head is rejected', [
        commit([cr(2, 'beta')]), U([hs(), mutate(1, [cr(1, 'acme')], base=0)])])
    S('STD-PROTO-010', 'base equal to the head is accepted', [U([hs(), mutate(1, [cr(1, 'acme')], base=0)])])
    S('STD-PROTO-010', 'an idempotency key returns the first result for a repeated request', [
        U([hs(), mutate(1, [cr(1, 'acme')], idempotency_key='k'), mutate(2, [cr(1, 'acme')], idempotency_key='k')]),
        {'op': 'commit_info', 'input': {'branch': BR}}])
    S('STD-PROTO-010', 'the same idempotency key with another payload fails', [
        U([hs(), mutate(1, [cr(1, 'acme')], idempotency_key='k'), mutate(2, [cr(2, 'beta')], idempotency_key='k')])])
    S('STD-PROTO-010', 'idempotency keys are per principal', [
        U([hs(principal=DID), mutate(1, [cr(1, 'acme')], idempotency_key='k')], session='a'),
        U([hs(), mutate(1, [cr(2, 'beta')], idempotency_key='k')], session='b')])
    S('STD-PROTO-010', 'a require decision answers with a pending result and the steps', [
        commit([ACCT, acct, sheet(SHEET, 'd', 'Account { approvals: 2; }')]),
        U([hs(), mutate(1, port={'target': T, 'name': 'close'})]), {'op': 'read', 'input': {'branch': BR, 'object_id': uid(1)}}])
    S('STD-PROTO-010', 'a deny decision is an error', [
        commit([ACCT, acct, sheet(SHEET, 'd', 'Account { decisions: [{name: "no", outcome: "deny"}]; }')]),
        U([hs(), mutate(1, port={'target': T, 'name': 'close'})])])
    S('STD-PROTO-010', 'the warnings of the commit are returned', [
        commit([sheet(SHEET, 'w', 'Customer { validations: [{name: "email", rule: "this.email is not null", '
                      'severity: "warning", message: "no email"}]; }')]),
        U([hs(), mutate(1, [cr(1, 'acme')])])])
    S('STD-PROTO-010', 'audit_level strict takes message and reason from the mutate body', [
        commit([sheet(SHEET, 's', 'Customer { audit_level: "strict"; }')]),
        U([hs(), mutate(1, [cr(1, 'acme')]), mutate(2, [cr(1, 'acme')], message='new customer', reason='onboarding')])])
    S('STD-PROTO-010', 'the principal of the session initiates the process', [
        U([hs(principal=DID), mutate(1, [cr(1, 'acme')])]), {'op': 'commit_info', 'input': {'branch': BR}}])


def query_(S):
    c1 = cr(1, 'acme', {'country': 'US', 'rating': 3})
    c2 = cr(2, 'beta', {'country': 'CA', 'rating': 1})
    S('STD-PROTO-011', 'read returns the object the URI names', [commit([c1]), U([hs(), query(1, read='ubos://acme/Customer/acme')])])
    S('STD-PROTO-011', 'read of an unknown object fails', [U([hs(), query(1, read='ubos://acme/Customer/none')])])
    S('STD-PROTO-011', 'query returns the matching objects as items with the evaluation commits', [
        commit([c1, c2]), U([hs(), query(1, query={'from': 'Customer', 'where': 'this.rating > 2'})])])
    S('STD-PROTO-011', 'query takes the textual form', [
        commit([c1, c2]), U([hs(), query(1, query='from Customer where this.rating > 2')])])
    S('STD-PROTO-011', 'query pages with a cursor', [
        commit([c1, c2]), U([hs(), query(1, query={'from': 'Customer', 'limit': 1, 'order_by': [{'path': 'this.name', 'dir': 'asc'}]})])])
    S('STD-PROTO-011', 'a malformed query is rejected', [U([hs(), query(1, query='select *')])])
    S('STD-PROTO-011', 'coordinates select the commit', [
        commit([c1], label='a'), commit([c2]), U([hs(), query(1, query={'from': 'Customer'}, time=1), query(2, query={'from': 'Customer'})])])
    S('STD-PROTO-011', 'asof selects the valid time', [
        commit([c1], label='a'),
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('a', uid(1)), 'body': {'rating': 5}, 'valid': {'from': '2026-04-01'}}]),
        U([hs(), query(1, read='ubos://acme/Customer/acme?asof=2026-03-20'), query(2, read='ubos://acme/Customer/acme?asof=2026-04-02')])])
    S('STD-PROTO-011', 'explain returns the cascade explanation of a property', [
        commit([c1]), commit([sheet(SHEET, 's', 'Customer { audit_level: "strict"; }')]),
        U([hs(), query(1, explain={'target': 'ubos://acme/Customer/acme', 'kind': 'cascade', 'property': 'audit_level'})])])
    S('STD-PROTO-011', 'explain returns the decision of a port', [
        commit([ACCT, create(uid(5), 'Account', 'a-1', {'name': 'A'}), sheet(SHEET, 'd', 'Account { approvals: 1; }')]),
        U([hs(), query(1, explain={'target': 'ubos://acme/Account/a-1', 'kind': 'decision', 'port': 'close'})])])
    S('STD-PROTO-011', 'history returns the commits that changed the object', [
        commit([c1], label='a'), commit([{'op': 'update', 'object_id': uid(1), 'base': base('a', uid(1)), 'body': {'rating': 4}}]),
        U([hs(), query(1, history={'target': 'ubos://acme/Customer/acme'})])])
    S('STD-PROTO-011', 'timeline returns the valid-time segments of the object', [
        commit([c1], label='a'),
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('a', uid(1)), 'body': {'rating': 4}, 'valid': {'from': '2026-04-01'}}]),
        U([hs(), query(1, timeline={'target': 'ubos://acme/Customer/acme', 'from': '2026-01-01'})])])
    S('STD-PROTO-011', 'diff compares two coordinates', [
        commit([c1]), commit([c2]), U([hs(), query(1, diff={'from': {'branch': BR, 'time': 1}, 'to': {'branch': BR, 'time': 2}})])])
    S('STD-PROTO-011', 'view payloads come with the views of step 13', [
        commit([c1]), U([hs(), query(1, view={'target': 'ubos://acme/Customer/acme', 'mode': 'edit'})])])
    S('STD-PROTO-011', 'query carries exactly one request kind', [
        commit([c1]), U([hs(), query(1, read='ubos://acme/Customer/acme', query={'from': 'Customer'})])])


def subscribe(S):
    c1 = cr(1, 'acme', {'country': 'US', 'rating': 3})
    c2 = cr(2, 'beta', {'country': 'CA', 'rating': 1})
    upd = lambda label, n, body: {'op': 'update', 'object_id': uid(n), 'base': base(label, uid(n)), 'body': body}  # noqa: E731
    S('STD-PROTO-012', 'subscribe to an object answers with the subscription and the cursor at the head', [
        commit([c1]), U([hs(), sub(1, target='ubos://acme/Customer/acme')])])
    S('STD-PROTO-012', 'a change of the object is emitted with its cursor', [
        commit([c1], label='a'), U([hs(), sub(1, target='ubos://acme/Customer/acme')]),
        commit([upd('a', 1, {'rating': 4})]), U([env(2, 'ping')])])
    S('STD-PROTO-012', 'changes of other objects are not emitted for an object subscription', [
        commit([c1], label='a'), U([hs(), sub(1, target='ubos://acme/Customer/acme')]), commit([c2]), U([env(2, 'ping')])])
    S('STD-PROTO-012', 'a mutate of the session emits its own change in the same call', [
        commit([c1], label='a'), U([hs(), sub(1, target='ubos://acme/Customer/acme'), mutate(2, [upd('a', 1, {'rating': 4})])])])
    S('STD-PROTO-012', 'changes are emitted in commit order, one frame per commit', [
        commit([c1], label='a'), U([hs(), sub(1, target='ubos://acme/Customer/acme')]),
        commit([upd('a', 1, {'rating': 4})], label='b'), commit([upd('b', 1, {'rating': 5})]), U([env(2, 'ping')])])
    S('STD-PROTO-012', 'a query subscription emits the changed objects that match', [
        commit([c1, c2], label='a'), U([hs(), sub(1, query={'from': 'Customer', 'where': 'this.country == "US"'})]),
        commit([upd('a', 1, {'rating': 4}), upd('a', 2, {'rating': 2})]), U([env(2, 'ping')])])
    S('STD-PROTO-012', 'a query subscription emits a newly created matching object', [
        U([hs(), sub(1, query={'from': 'Customer'})]), commit([c1]), U([env(2, 'ping')])])
    S('STD-PROTO-012', 'an events subscription emits the released effects', [
        commit([ACCT, create(uid(5), 'Account', 'a-1', {'name': 'A', 'status': 'open'})], label='d'),
        U([hs(), sub(1, events={'types': ['AccountClosed']})]),
        commit([{'op': 'update', 'object_id': uid(5), 'base': {'$ref': 'd/versions/' + uid(5)}, 'body': {'status': 'closed'}}],
               process={'effects': [{'type': 'AccountClosed', 'object_id': uid(5)}]}),
        U([env(2, 'ping')])])
    S('STD-PROTO-012', 'from_cursor replays the commits missed since the cursor', [
        commit([c1], label='a'), U([hs(), sub(1, target='ubos://acme/Customer/acme')]),
        commit([upd('a', 1, {'rating': 4})], label='b'), commit([upd('b', 1, {'rating': 5})]),
        U([hs('h2'), sub(2, target='ubos://acme/Customer/acme', from_cursor='%s:1' % BR)], session='reconnect')])
    S('STD-PROTO-012', 'a cursor beyond the retention window has expired', [
        commit([c1], label='a')] + [commit([upd('a' if i == 0 else 'u%d' % (i - 1), 1, {'rating': i % 5 + 1})], label='u%d' % i)
                                    for i in range(101)] + [
        U([hs(), sub(1, target='ubos://acme/Customer/acme', from_cursor='%s:1' % BR)])])
    S('STD-PROTO-012', 'a cursor ahead of the branch is invalid', [
        commit([c1]), U([hs(), sub(1, target='ubos://acme/Customer/acme', from_cursor='%s:9' % BR)])])
    S('STD-PROTO-012', 'subscribing to an unknown object fails', [U([hs(), sub(1, target='ubos://acme/Customer/none')])])
    S('STD-PROTO-012', 'unsubscribe stops the emits', [
        commit([c1], label='a'), U([hs(), sub(1, target='ubos://acme/Customer/acme'), env(2, 'unsubscribe', {'subscription_id': 'sub-1'})]),
        commit([upd('a', 1, {'rating': 4})]), U([env(3, 'ping')])])
    S('STD-PROTO-012', 'unsubscribing an unknown subscription fails', [U([hs(), env(1, 'unsubscribe', {'subscription_id': 'sub-9'})])])
    S('STD-PROTO-012', 'subscribe carries exactly one of target, query and events', [
        commit([c1]), U([hs(), sub(1, target='ubos://acme/Customer/acme', query={'from': 'Customer'})])])


def flow(S):
    c1 = cr(1, 'acme', {'country': 'US', 'rating': 3})
    upd = lambda label, n, body: {'op': 'update', 'object_id': uid(n), 'base': base(label, uid(n)), 'body': body}  # noqa: E731
    S('STD-PROTO-013', 'at most the negotiated window of emits is outstanding per subscription', [
        commit([c1], label='a'), U([hs(), sub(1, target='ubos://acme/Customer/acme')])]
      + [commit([upd('a' if i == 0 else 'w%d' % (i - 1), 1, {'rating': i % 5 + 1})], label='w%d' % i) for i in range(10)]
      + [U([env(2, 'ping')])])
    S('STD-PROTO-013', 'an ack releases the held emits', [
        commit([c1], label='a'), U([hs(), sub(1, target='ubos://acme/Customer/acme')])]
      + [commit([upd('a' if i == 0 else 'w%d' % (i - 1), 1, {'rating': i % 5 + 1})], label='w%d' % i) for i in range(10)]
      + [U([env(2, 'ping')]), U([env(3, 'ack', {'subscription_id': 'sub-1', 'cursor': '%s:9' % BR})])])
    S('STD-PROTO-013', 'an ack of an unknown subscription fails', [U([hs(), env(1, 'ack', {'subscription_id': 'sub-9', 'cursor': '%s:1' % BR})])])
    S('STD-PROTO-013', 'an ack needs a cursor', [
        commit([c1]), U([hs(), sub(1, target='ubos://acme/Customer/acme'), env(2, 'ack', {'subscription_id': 'sub-1'})])])
    S('STD-PROTO-013', 'cancel of a completed request reports it was not cancelled', [
        U([hs(), env(1, 'ping'), env(2, 'cancel', {'id': '1'})])])
    S('STD-PROTO-013', 'ping answers pong', [U([hs(), env(1, 'ping')])])
    S('STD-PROTO-013', 'the window is announced by the handshake', [U([hs()])])


def bindings(S):
    c1 = cr(1, 'acme', {'country': 'US', 'rating': 3})
    S('STD-PROTO-020', 'the in-process binding gives the same read as the adapter operation', [
        commit([c1]), {'op': 'resolve', 'label': 'r', 'input': {'uri': 'ubos://acme/Customer/acme?branch=%s' % BR}},
        U([hs(), query(1, read='ubos://acme/Customer/acme?branch=%s' % BR)], label='u'),
        {'op': 'normalize_text', 'input': {'text': {'$ref': 'u/responses/1/body/items/0/version_hash'}}, 'same_as': 'r/version_hash'}])
    S('STD-PROTO-020', 'the in-process binding gives the same query as the adapter operation', [
        commit([c1]), {'op': 'query', 'label': 'q', 'input': {'branch': BR, 'from': 'Customer'}},
        U([hs(), query(1, query={'from': 'Customer'})], label='u'),
        {'op': 'normalize_text', 'input': {'text': {'$ref': 'u/responses/1/body/items/0/object_id'}}, 'same_as': 'q/objects/0'}])
    S('STD-PROTO-020', 'a mutate through the binding is the same commit as the adapter operation would make', [
        U([hs(), mutate(1, [c1])], label='u'), {'op': 'commit_info', 'label': 'c', 'input': {'branch': BR}},
        {'op': 'normalize_text', 'input': {'text': {'$ref': 'u/responses/1/body/commits/0'}}, 'same_as': 'c/commit_id'}])
    S('STD-PROTO-020', 'sync frames come with PH-3', [U([hs(), env(1, 'sync')])])
    S('STD-PROTO-020', 'the frames of one call are the frames the message sequence produces, in order', [
        commit([c1]), U([hs(), env(1, 'ping'), query(2, read='ubos://acme/Customer/acme'), env(3, 'ping')])])
