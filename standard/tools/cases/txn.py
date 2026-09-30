"""Vector cases for processes and commit semantics (STD-TXN-*)."""
from .common import setup, commit, create, base, uid, stock, gl

C0 = create(uid(1), body={'name': 'Acme'}, valid={'from': '2026-01-01'})
C2 = create(uid(2), slug='b', body={'name': 'Beta'}, valid={'from': '2026-01-01'})


def upd(label, oid, body):
    return {'op': 'update', 'object_id': oid, 'base': base(label, oid), 'body': body}


def read(oid, **kw):
    return {'op': 'read', 'input': dict({'branch': 'main', 'object_id': oid}, **kw)}


def cases(g):
    V = g.vector
    # STD-TXN-001 process outcome
    V('STD-TXN-001', 'a committed process makes its changes visible', setup=setup(), steps=[commit([C0]), read(uid(1))])
    V('STD-TXN-001', 'an aborted process leaves no trace', setup=setup(), steps=[
        commit([C0, dict(C2, slug='Bad Slug')], error='MODEL.INVALID_NAME'), read(uid(1)) | {'error': 'IAM.NOT_FOUND'}])
    # STD-TXN-002 snapshot and read-your-writes
    V('STD-TXN-002', 'changes of one commit see the earlier changes of the same commit', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5'), stock(uid(21), 'bolt', '1')]),
        {'op': 'read_ledger', 'input': {'branch': 'main', 'class': 'StockMove', 'ledger_key': {'item': 'bolt'}}}])
    V('STD-TXN-002', 'a read at a fixed time is a stable snapshot', setup=setup(), steps=[
        commit([C0], label='c1'), read(uid(1), time=1) | {'label': 'r1'}, commit([upd('c1', uid(1), {'name': 'B'})]),
        read(uid(1), time=1) | {'same_as': 'r1/'}])
    # STD-TXN-003 atomic visibility
    V('STD-TXN-003', 'if one change fails, no change of the commit is visible', setup=setup(), steps=[
        commit([C0]), commit([C2, create(uid(1), slug='dup')], error='TXN.ALREADY_EXISTS'), read(uid(2)) | {'error': 'IAM.NOT_FOUND'}])
    V('STD-TXN-003', 'a failed commit does not advance the branch', setup=setup(), steps=[
        commit([C0]), commit([create(uid(1), slug='dup')], error='TXN.ALREADY_EXISTS'), commit([C2])])
    V('STD-TXN-003', 'a failed commit appends no entries', setup=setup(), steps=[
        commit([gl(uid(10), '1010', '100.00'), gl(uid(11), '4000', '-99.00')], error='LEDG.UNBALANCED'),
        {'op': 'read_ledger', 'input': {'branch': 'main', 'class': 'GlEntry', 'ledger_key': {'account': '1010'}}}])
    # STD-TXN-004 flush content
    V('STD-TXN-004', 'versions and entries of one commit become visible together', setup=setup(), steps=[
        commit([C0, stock(uid(20), 'bolt', '5')]),
        {'op': 'diff', 'input': {'from': {'branch': 'main', 'time': 0}, 'to': {'branch': 'main'}}, 'match': 'exact'}])
    # STD-TXN-020 every update names its base
    V('STD-TXN-020', 'an update with its base succeeds', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'B'})])])
    V('STD-TXN-020', 'an end without a base is rejected', setup=setup(), steps=[
        commit([C0]), commit([{'op': 'end', 'object_id': uid(1), 'valid': {'from': '2026-02-01'}}], error='MODEL.DECLARATION_INCOMPLETE')])
    V('STD-TXN-020', 'a create of an existing object fails', setup=setup(), steps=[
        commit([C0]), commit([dict(C0, slug='other')], error='TXN.ALREADY_EXISTS')])
    # STD-TXN-021 object-level compare-and-set
    V('STD-TXN-021', 'an update with a stale base fails and lists the object', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'B'})]),
        commit([upd('c1', uid(1), {'name': 'C'})], error='TXN.HEAD_MOVED', details=True)])
    V('STD-TXN-021', 'every stale object is listed', setup=setup(), steps=[
        commit([C0, C2], label='c1'), commit([upd('c1', uid(1), {'name': 'B'}), upd('c1', uid(2), {'name': 'B2'})]),
        commit([upd('c1', uid(1), {'name': 'C'}), upd('c1', uid(2), {'name': 'C2'})], error='TXN.HEAD_MOVED', details=True)])
    V('STD-TXN-021', 'the head moving for unrelated objects does not fail the commit', setup=setup(), steps=[
        commit([C0, C2], label='c1'), commit([upd('c1', uid(2), {'name': 'B2'})]), commit([upd('c1', uid(1), {'name': 'B'})])])
    V('STD-TXN-021', 'a new object on the branch does not fail an update', setup=setup(), steps=[
        commit([C0], label='c1'), commit([C2]), commit([upd('c1', uid(1), {'name': 'B'})]), read(uid(1))])
    V('STD-TXN-021', 'the base of the latest version succeeds after a failure', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'B'})], label='c2'),
        commit([upd('c1', uid(1), {'name': 'C'})], error='TXN.HEAD_MOVED'), commit([upd('c2', uid(1), {'name': 'C'})]), read(uid(1))])
    # STD-TXN-022 serializable processes
    reads = lambda label, oid: {'serializable': True, 'reads': {oid: {'$ref': '%s/versions/%s' % (label, oid)}}}
    V('STD-TXN-022', 'a serializable process fails when an object it read changed', setup=setup(), steps=[
        commit([C0, C2], label='c1'), commit([upd('c1', uid(2), {'name': 'B2'})]),
        commit([upd('c1', uid(1), {'name': 'B'})], process=reads('c1', uid(2)), error='TXN.READ_SET_CHANGED')])
    V('STD-TXN-022', 'a serializable process commits when its reads are unchanged', setup=setup(), steps=[
        commit([C0, C2], label='c1'), commit([upd('c1', uid(1), {'name': 'B'})], process=reads('c1', uid(2)))])
    V('STD-TXN-022', 'a process that is not serializable ignores its reads', setup=setup(), steps=[
        commit([C0, C2], label='c1'), commit([upd('c1', uid(2), {'name': 'B2'})]),
        commit([upd('c1', uid(1), {'name': 'B'})], process={'serializable': False, 'reads': {uid(2): {'$ref': 'c1/versions/%s' % uid(2)}}})])
    # STD-TXN-024 ledger appends and concurrency
    V('STD-TXN-024', 'appends carry no base and do not fail when the key changed', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5')]), commit([stock(uid(21), 'bolt', '1')]), commit([stock(uid(22), 'bolt', '2')])])
    V('STD-TXN-024', 'appends to one key from two branches never fail with HEAD_MOVED', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5')]), {'op': 'create_branch', 'input': {'name': 'feature/x', 'from': 'main'}},
        commit([stock(uid(21), 'bolt', '1')], branch='feature/x'), commit([stock(uid(22), 'bolt', '2')]),
        {'op': 'merge', 'input': {'source': 'feature/x', 'target': 'main'}}])
