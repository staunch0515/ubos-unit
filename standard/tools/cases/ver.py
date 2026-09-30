"""Vector cases for versioning (STD-VER-*)."""
import hashlib

from .common import setup, commit, create, base, uid, stock, cls, field

H = lambda s: 'sha256:' + hashlib.sha256(s.encode()).hexdigest()
COMMIT = {'vae': 'acme', 'branch': 'main', 'parents': [H('p1')], 'author': 'did:web:acme.example:alice',
          'process_id': '01890a5d-ac96-7abc-8def-0123456789ab', 'tx_time': '2026-03-15T10:00:00.000000Z',
          'changes': [{'op': 'update', 'target': uid(1), 'version': H('v2'), 'base': H('v1')}],
          'root': H('root'), 'rule_binding': H('rb'), 'abi': '1.0',
          'signatures': [{'alg': 'ed25519', 'key': 'k1', 'sig': 'AAAA'}]}


def upd(label, oid, body, **extra):
    ch = {'op': 'update', 'object_id': oid, 'base': base(label, oid), 'body': body}
    ch.update(extra)
    return ch


def read(oid, branch='main', **kw):
    i = {'branch': branch, 'object_id': oid}
    i.update(kw)
    return {'op': 'read', 'input': i}


def branch(name, frm='main', **kw):
    i = {'name': name, 'from': frm}
    i.update(kw)
    return {'op': 'create_branch', 'input': i}


def merge(source, target='main', error=None, details=False, **kw):
    s = {'op': 'merge', 'input': {'source': source, 'target': target}}
    if error:
        s['error'] = error
        s['details'] = details
    s.update(kw)
    return s


C0 = create(uid(1), body={'name': 'Acme', 'email': 'a@acme.example'}, valid={'from': '2026-01-01'})


def two_sided(src_change, tgt_change, extra_setup=()):
    """Base commit on main, a feature branch, one change on each side."""
    return [commit([C0], label='c1'), *extra_setup, branch('feature/x'),
            commit([src_change], branch='feature/x', label='s'), commit([tgt_change], label='t')]


def cases(g):
    V, P = g.vector, g.pure
    # STD-VER-001 commit record and commit_id
    P('STD-VER-001', 'commit_id is the content identifier of the commit without signatures', 'commit_id', {'commit': COMMIT})
    V('STD-VER-001', 'signatures do not change the commit_id', steps=[
        {'op': 'commit_id', 'input': {'commit': COMMIT}, 'label': 'a'},
        {'op': 'commit_id', 'input': {'commit': dict(COMMIT, signatures=[])}, 'same_as': 'a/'}])
    for member, value in [('parents', [H('p1'), H('p2')]), ('tx_time', '2026-03-15T10:00:00.000001Z'),
                          ('root', H('root2')), ('message', 'fix'), ('author', 'did:web:acme.example:bob'),
                          ('changes', [])]:
        P('STD-VER-001', 'changing %s changes the commit_id' % member, 'commit_id', {'commit': dict(COMMIT, **{member: value})})
    V('STD-VER-001', 'a commit reports its cseq', setup=setup(), steps=[commit([C0]), commit([create(uid(2), slug='b')])])
    # STD-VER-002 immutability
    V('STD-VER-002', 'a later commit does not alter an earlier version', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'email': 'x@acme.example'})]), read(uid(1), time=1)])
    V('STD-VER-002', 'a later end does not remove an earlier version', setup=setup(), steps=[
        commit([C0], label='c1'), commit([{'op': 'end', 'object_id': uid(1), 'base': base('c1', uid(1)),
                                           'valid': {'from': '2026-02-01'}}]),
        read(uid(1), time=1, asof='2026-03-01')])
    V('STD-VER-002', 'the state at a past cseq is stable', setup=setup(), steps=[
        commit([C0], label='c1'), {'op': 'export_state', 'input': {'branch': 'main', 'time': 1}, 'label': 'e1'},
        commit([upd('c1', uid(1), {'email': 'x@acme.example'})]),
        {'op': 'export_state', 'input': {'branch': 'main', 'time': 1}, 'same_as': 'e1/'}])
    # STD-VER-003 first-parent history
    V('STD-VER-003', 'commits on a branch get increasing cseq numbers', setup=setup(), steps=[
        commit([C0]), commit([create(uid(2), slug='b')]), commit([create(uid(3), slug='c')])])
    V('STD-VER-003', 'the history of a branch includes its fork point', setup=setup(), steps=[
        commit([C0]), branch('feature/x'), commit([create(uid(2), slug='b')], branch='feature/x'),
        read(uid(1), branch='feature/x', time=1)])
    V('STD-VER-003', 'a cseq that is not on the branch is not found', setup=setup(), steps=[
        commit([C0]), {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(1), 'time': 5}, 'error': 'IAM.NOT_FOUND'}])
    # STD-VER-010 branches
    V('STD-VER-010', 'a new branch sees the state of its source', setup=setup(), steps=[
        commit([C0]), branch('feature/x'), read(uid(1), branch='feature/x')])
    V('STD-VER-010', 'a new branch starts at the cseq of its source head', setup=setup(), steps=[
        commit([C0]), commit([create(uid(2), slug='b')]), branch('feature/x')])
    V('STD-VER-010', 'commits on a branch do not change the source', setup=setup(), steps=[
        commit([C0], label='c1'), branch('feature/x'),
        commit([upd('c1', uid(1), {'email': 'x@acme.example'})], branch='feature/x'), read(uid(1))])
    V('STD-VER-010', 'a branch created from a past cseq', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'email': 'x@acme.example'})]),
        branch('correction/jan', time=1), read(uid(1), branch='correction/jan')])
    V('STD-VER-010', 'a branch name cannot be reused', setup=setup(), steps=[
        branch('feature/x'), {'op': 'create_branch', 'input': {'name': 'feature/x', 'from': 'main'},
                              'error': 'TXN.ALREADY_EXISTS'}])
    V('STD-VER-010', 'the branch state tree equals the source state at creation', setup=setup(), steps=[
        commit([C0]), {'op': 'export_state', 'input': {'branch': 'main'}, 'label': 'm'}, branch('feature/x'),
        {'op': 'export_state', 'input': {'branch': 'feature/x'}, 'same_as': 'm/'}])
    # STD-VER-013 branch names
    for name, ok in [('main', True), ('feature/new-invoice', True), ('release/2026-q1', True), ('draft/a/b', True),
                     ('sandbox/try', True), ('work/x', False), ('feature', False), ('Feature/x', False),
                     ('feature/', False)]:
        P('STD-VER-013', 'branch name %r is %s' % (name, 'valid' if ok else 'invalid'), 'check_name',
          {'grammar': 'branch-name', 'text': name}, error=None if ok else 'VER.INVALID_BRANCH_NAME')
    V('STD-VER-013', 'creating a branch with an invalid name fails', setup=setup(), steps=[
        {'op': 'create_branch', 'input': {'name': 'tmp/x', 'from': 'main'}, 'error': 'VER.INVALID_BRANCH_NAME'}])
    # STD-VER-030 diff
    D = lambda a, b: {'op': 'diff', 'input': {'from': a, 'to': b}, 'match': 'exact'}
    V('STD-VER-030', 'an added object', setup=setup(), steps=[commit([C0]), D({'branch': 'main', 'time': 0}, {'branch': 'main'})])
    V('STD-VER-030', 'a changed field is listed with old and new values', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'email': 'x@acme.example'})]),
        D({'branch': 'main', 'time': 1}, {'branch': 'main'})])
    V('STD-VER-030', 'a cleared field has the new value null', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'email': None})]),
        D({'branch': 'main', 'time': 1}, {'branch': 'main'})])
    V('STD-VER-030', 'an ended object', setup=setup(), steps=[
        commit([C0], label='c1'), commit([{'op': 'end', 'object_id': uid(1), 'base': base('c1', uid(1)), 'valid': {'from': '2026-02-01'}}]),
        D({'branch': 'main', 'time': 1}, {'branch': 'main'})])
    V('STD-VER-030', 'a slug change without field changes is a move', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {}, slug='acme-inc')]),
        D({'branch': 'main', 'time': 1}, {'branch': 'main'})])
    V('STD-VER-030', 'unchanged objects are omitted', setup=setup(), steps=[
        commit([C0, create(uid(2), slug='b')], label='c1'), commit([upd('c1', uid(2), {'email': 'b@b.example'})]),
        D({'branch': 'main', 'time': 1}, {'branch': 'main'})])
    V('STD-VER-030', 'appended ledger entries', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5')]), commit([stock(uid(21), 'bolt', '1')]),
        D({'branch': 'main', 'time': 1}, {'branch': 'main'})])
    V('STD-VER-030', 'the diff between two branches', setup=setup(), steps=[
        commit([C0], label='c1'), branch('feature/x'),
        commit([upd('c1', uid(1), {'name': 'Acme Corp'}), create(uid(2), slug='b')], branch='feature/x'),
        D({'branch': 'main'}, {'branch': 'feature/x'})])
    V('STD-VER-030', 'the diff of equal states is empty', setup=setup(), steps=[
        commit([C0]), branch('feature/x'), D({'branch': 'main'}, {'branch': 'feature/x'})])
    # STD-VER-040 merge base
    V('STD-VER-040', 'a second merge uses the first merge as base and does not conflict again', setup=setup(), steps=[
        *two_sided(upd('c1', uid(1), {'email': 's@acme.example'}), upd('c1', uid(1), {'name': 'T Name'})),
        merge('feature/x'), commit([create(uid(5), slug='later')], branch='feature/x'),
        merge('feature/x'), read(uid(5))])
    V('STD-VER-040', 'merging back after a merge into the branch uses the latest common ancestor', setup=setup(), steps=[
        *two_sided(upd('c1', uid(1), {'email': 's@acme.example'}), upd('c1', uid(1), {'name': 'T Name'})),
        merge('main', 'feature/x'), merge('feature/x', 'main'), read(uid(1))])
    V('STD-VER-040', 'a source already contained in the target needs no merge', setup=setup(), steps=[
        commit([C0]), branch('feature/x'), commit([create(uid(2), slug='b')]), merge('feature/x')])
    V('STD-VER-040', 'criss-cross merges resolve with a virtual base', setup=setup(), steps=[
        commit([C0], label='c1'), branch('feature/x'),
        commit([create(uid(2), slug='b')], branch='feature/x'), commit([create(uid(3), slug='c')]),
        merge('feature/x', 'main'), merge('main', 'feature/x'),
        commit([create(uid(4), slug='d')], branch='feature/x'), commit([create(uid(6), slug='f')]),
        merge('feature/x', 'main'), read(uid(4))])
    V('STD-VER-040', 'merging in the other direction reaches the same field values', setup=setup(), steps=[
        *two_sided(upd('c1', uid(1), {'email': 's@acme.example'}), upd('c1', uid(1), {'name': 'T Name'})),
        branch('draft/copy', 'main'), merge('feature/x', 'main'), merge('draft/copy', 'feature/x'),
        {'op': 'read', 'input': {'branch': 'feature/x', 'object_id': uid(1)}}])
    # STD-VER-050 three-way merge
    M = lambda desc, s, t, err=None, check=None: V('STD-VER-050', desc, setup=setup(), steps=[
        *two_sided(s, t), merge('feature/x', error=err, details=bool(err)), *([check] if check and not err else [])])
    M('changed only on the source: take the source', upd('c1', uid(1), {'email': 's@acme.example'}),
      create(uid(2), slug='b'), check=read(uid(1)))
    M('changed only on the target: keep the target', create(uid(2), slug='b'),
      upd('c1', uid(1), {'email': 't@acme.example'}), check=read(uid(1)))
    M('different fields changed on both sides: both kept', upd('c1', uid(1), {'email': 's@acme.example'}),
      upd('c1', uid(1), {'name': 'T Name'}), check=read(uid(1)))
    M('the same field changed to the same value: no conflict', upd('c1', uid(1), {'email': 'same@acme.example'}),
      upd('c1', uid(1), {'email': 'same@acme.example'}), check=read(uid(1)))
    M('the same field changed to different values: conflict', upd('c1', uid(1), {'email': 's@acme.example'}),
      upd('c1', uid(1), {'email': 't@acme.example'}), err='VER.MERGE_CONFLICT')
    M('a field set on one side and cleared on the other: conflict', upd('c1', uid(1), {'email': 's@acme.example'}),
      upd('c1', uid(1), {'email': None}), err='VER.MERGE_CONFLICT')
    M('a field added on one side only: taken', upd('c1', uid(1), {'country': 'DE'}), create(uid(2), slug='b'),
      check=read(uid(1)))
    M('an object created on the source only: taken', create(uid(2), slug='b'), create(uid(3), slug='c'),
      check=read(uid(2)))
    M('objects created on both sides with different ids: both kept', create(uid(2), slug='b'), create(uid(3), slug='c'),
      check={'op': 'diff', 'input': {'from': {'branch': 'main', 'time': 2}, 'to': {'branch': 'main'}}, 'match': 'exact'})
    M('ended on one side and changed on the other: conflict',
      {'op': 'end', 'object_id': uid(1), 'base': base('c1', uid(1)), 'valid': {'from': '2026-02-01'}},
      upd('c1', uid(1), {'email': 't@acme.example'}), err='VER.MERGE_CONFLICT')
    M('ended on both sides: ended', {'op': 'end', 'object_id': uid(1), 'base': base('c1', uid(1)), 'valid': {'from': '2026-02-01'}},
      {'op': 'end', 'object_id': uid(1), 'base': base('c1', uid(1)), 'valid': {'from': '2026-02-01'}},
      check={'op': 'read', 'input': {'branch': 'main', 'object_id': uid(1), 'asof': '2026-03-01'}, 'error': 'QRY.NOT_FOUND_AT_TIME'})
    M('ended on the source, unchanged on the target: ended', {'op': 'end', 'object_id': uid(1), 'base': base('c1', uid(1)),
                                                             'valid': {'from': '2026-02-01'}}, create(uid(2), slug='b'),
      check={'op': 'read', 'input': {'branch': 'main', 'object_id': uid(1), 'asof': '2026-03-01'}, 'error': 'QRY.NOT_FOUND_AT_TIME'})
    M('changes in different valid-time periods merge per segment',
      upd('c1', uid(1), {'email': 's@acme.example'}, valid={'kind': 'supersede_from', 'from': '2026-02-01'}),
      upd('c1', uid(1), {'name': 'T Name'}, valid={'kind': 'supersede_from', 'from': '2026-03-01'}),
      check=read(uid(1), asof='2026-02-15'))
    M('per-segment merge: the later segment holds both changes',
      upd('c1', uid(1), {'email': 's@acme.example'}, valid={'kind': 'supersede_from', 'from': '2026-02-01'}),
      upd('c1', uid(1), {'name': 'T Name'}, valid={'kind': 'supersede_from', 'from': '2026-03-01'}),
      check=read(uid(1), asof='2026-03-15'))
    M('per-segment merge: the earliest segment is unchanged',
      upd('c1', uid(1), {'email': 's@acme.example'}, valid={'kind': 'supersede_from', 'from': '2026-02-01'}),
      upd('c1', uid(1), {'name': 'T Name'}, valid={'kind': 'supersede_from', 'from': '2026-03-01'}),
      check=read(uid(1), asof='2026-01-15'))
    # STD-VER-051 field strategies (three_way default)
    V('STD-VER-051', 'the default strategy three_way reports a conflict when both sides changed a field', setup=setup(), steps=[
        *two_sided(upd('c1', uid(1), {'name': 'S'}), upd('c1', uid(1), {'name': 'T'})),
        merge('feature/x', error='VER.MERGE_CONFLICT', details=True)])
    V('STD-VER-051', 'three_way takes the one changed side without conflict', setup=setup(), steps=[
        *two_sided(upd('c1', uid(1), {'name': 'S'}), create(uid(2), slug='b')), merge('feature/x'), read(uid(1))])
    # STD-VER-052 complete conflict report
    V('STD-VER-052', 'every conflicting field is reported', setup=setup(), steps=[
        *two_sided(upd('c1', uid(1), {'name': 'S', 'email': 's@x.example'}), upd('c1', uid(1), {'name': 'T', 'email': 't@x.example'})),
        merge('feature/x', error='VER.MERGE_CONFLICT', details=True)])
    V('STD-VER-052', 'conflicts on several objects are all reported', setup=setup(), steps=[
        commit([C0, create(uid(2), slug='b')], label='c1'), branch('feature/x'),
        commit([upd('c1', uid(1), {'name': 'S'}), upd('c1', uid(2), {'name': 'S2'})], branch='feature/x'),
        commit([upd('c1', uid(1), {'name': 'T'}), upd('c1', uid(2), {'name': 'T2'})]),
        merge('feature/x', error='VER.MERGE_CONFLICT', details=True)])
    V('STD-VER-052', 'a failed merge changes nothing on the target', setup=setup(), steps=[
        *two_sided(upd('c1', uid(1), {'name': 'S'}), upd('c1', uid(1), {'name': 'T'})),
        merge('feature/x', error='VER.MERGE_CONFLICT'), read(uid(1))])
    V('STD-VER-052', 'object-level and field-level conflicts are reported together', setup=setup(), steps=[
        commit([C0, create(uid(2), slug='b')], label='c1'), branch('feature/x'),
        commit([upd('c1', uid(1), {'name': 'S'}), {'op': 'end', 'object_id': uid(2), 'base': base('c1', uid(2)),
                                                    'valid': {'from': '2026-02-01'}}], branch='feature/x'),
        commit([upd('c1', uid(1), {'name': 'T'}), upd('c1', uid(2), {'name': 'T2'})]),
        merge('feature/x', error='VER.MERGE_CONFLICT', details=True)])
    # STD-VER-053 kind-specific merge
    RD = create(uid(70), klass='ImportedDocument', slug='doc', body={'source': 'bank', 'payload': {'n': 1}},
                valid={'from': '2026-01-01'})
    V('STD-VER-053', 'a RawDocument changed on both sides conflicts as a whole', setup=setup(), steps=[
        commit([RD], label='c1'), branch('feature/x'),
        commit([upd('c1', uid(70), {'payload': {'n': 2}})], branch='feature/x'),
        commit([upd('c1', uid(70), {'source': 'erp'})]), merge('feature/x', error='VER.MERGE_CONFLICT', details=True)])
    V('STD-VER-053', 'a RawDocument changed on one side is taken whole', setup=setup(), steps=[
        commit([RD], label='c1'), branch('feature/x'), commit([upd('c1', uid(70), {'payload': {'n': 2}})], branch='feature/x'),
        merge('feature/x'), {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(70)}, 'match': 'exact'}])
    # STD-VER-054 ledger append on merge
    L = lambda k='bolt': {'op': 'read_ledger', 'input': {'branch': 'main', 'class': 'StockMove', 'ledger_key': {'item': k}}}
    V('STD-VER-054', 'entries of the source are appended to the target with new sequence numbers', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5')]), branch('feature/x'),
        commit([stock(uid(21), 'bolt', '2')], branch='feature/x'), commit([stock(uid(22), 'bolt', '7')]),
        merge('feature/x'), L()])
    V('STD-VER-054', 'the merge reports the source sequence of each appended entry', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5')]), branch('feature/x'),
        commit([stock(uid(21), 'bolt', '2')], branch='feature/x'), commit([stock(uid(23), 'bolt', '3')], branch='feature/x'),
        commit([stock(uid(22), 'bolt', '7')]), merge('feature/x')])
    V('STD-VER-054', 'entries already on the target are not appended again', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5')]), branch('feature/x'),
        commit([stock(uid(21), 'bolt', '2')], branch='feature/x'), merge('feature/x'),
        commit([stock(uid(23), 'bolt', '1')], branch='feature/x'), merge('feature/x'), L()])
    V('STD-VER-054', 'entries on both sides never conflict', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5')]), branch('feature/x'),
        commit([stock(uid(21), 'bolt', '2')], branch='feature/x'), commit([stock(uid(22), 'bolt', '-1')]), merge('feature/x')])
    V('STD-VER-054', 'a reversal whose original is on the target merges', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5')]), branch('feature/x'),
        commit([{'op': 'reverse', 'entry_id': uid(20), 'new_entry_id': uid(21)}], branch='feature/x'), merge('feature/x'), L()])
    V('STD-VER-054', 'an original and its reversal both from the source merge in order', setup=setup(), steps=[
        branch('feature/x'), commit([stock(uid(20), 'bolt', '5')], branch='feature/x'),
        commit([{'op': 'reverse', 'entry_id': uid(20), 'new_entry_id': uid(21)}], branch='feature/x'), merge('feature/x'), L()])
    V('STD-VER-054', 'each ledger key keeps its own sequence after a merge', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5')]), branch('feature/x'),
        commit([stock(uid(21), 'nut', '2'), stock(uid(22), 'bolt', '1')], branch='feature/x'), merge('feature/x'), L('nut')])
    # STD-VER-056 merge commit
    V('STD-VER-056', 'a merge produces one commit on the target', setup=setup(), steps=[
        *two_sided(upd('c1', uid(1), {'email': 's@acme.example'}), create(uid(2), slug='b')), merge('feature/x')])
    V('STD-VER-056', 'the state before the merge commit is unchanged', setup=setup(), steps=[
        *two_sided(upd('c1', uid(1), {'email': 's@acme.example'}), create(uid(2), slug='b')), merge('feature/x'),
        read(uid(1), time=2)])
    V('STD-VER-056', 'a merge into a branch whose head is the base still creates a merge commit', setup=setup(), steps=[
        commit([C0]), branch('feature/x'), commit([create(uid(2), slug='b')], branch='feature/x'), merge('feature/x')])
    V('STD-VER-056', 'the merge commit contains the merged state', setup=setup(), steps=[
        *two_sided(upd('c1', uid(1), {'email': 's@acme.example'}), upd('c1', uid(1), {'name': 'T Name'})),
        merge('feature/x'), {'op': 'diff', 'input': {'from': {'branch': 'main', 'time': 2}, 'to': {'branch': 'main'}}, 'match': 'exact'}])
