"""Vector cases for the state kinds (STD-KIND-*)."""
import hashlib

from .common import setup, commit, create, base, uid, gl, stock, usd, CLASSES, PERIOD, cls, field

CV = 'sha256:' + hashlib.sha256(b'GlEntry v1').hexdigest()
ENTRY = {'entry_id': uid(100), 'class': 'GlEntry', 'class_version': CV, 'ledger_key': {'account': '1010'},
         'valid_at': '2026-03-15', 'posting_group': 'g1'}
EBODY = {'account': '1010', 'amount': usd('100.00')}


def ledger(branch='main', cls_='GlEntry', key=None, **extra):
    i = {'branch': branch, 'class': cls_, 'ledger_key': key or {'account': '1010'}}
    i.update(extra)
    return {'op': 'read_ledger', 'input': i}


def cases(g):
    V, P = g.vector, g.pure
    # STD-KIND-001 Definition semantics
    V('STD-KIND-001', 'a Definition version carries a valid-time period', setup=setup(), steps=[
        commit([create(uid(1), valid={'from': '2026-01-01'})]),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(1), 'asof': '2026-02-01'}}])
    V('STD-KIND-001', 'an update without a base is rejected', setup=setup(), steps=[
        commit([create(uid(1))]),
        commit([{'op': 'update', 'object_id': uid(1), 'body': {'email': 'x@y.example'}}],
               error='MODEL.DECLARATION_INCOMPLETE')])
    V('STD-KIND-001', 'an update with the current base succeeds', setup=setup(), steps=[
        commit([create(uid(1))], label='c1'),
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('c1', uid(1)), 'body': {'email': 'x@y.example'}}]),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(1)}}])
    V('STD-KIND-001', 'a required field cannot be cleared', setup=setup(), steps=[
        commit([create(uid(1))], label='c1'),
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('c1', uid(1)), 'body': {'name': None}}],
               error='MODEL.CONSTRAINT_VIOLATED')])
    # STD-KIND-020 entry schema and hash
    P('STD-KIND-020', 'entry hash over the canonical entry with body_ref', 'entry_hash', {'entry': ENTRY, 'body': EBODY})
    V('STD-KIND-020', 'seq and source_seq are excluded from the entry hash', steps=[
        {'op': 'entry_hash', 'input': {'entry': ENTRY, 'body': EBODY}, 'label': 'a'},
        {'op': 'entry_hash', 'input': {'entry': dict(ENTRY, seq=7, source_seq={'branch': 'feature/x', 'seq': 3}),
                                        'body': EBODY}, 'same_as': 'a/'}])
    for member, value in [('valid_at', '2026-03-16'), ('ledger_key', {'account': '1020'}), ('posting_group', 'g2'),
                          ('corrects', [uid(99)]), ('correction_type', 'reversal')]:
        P('STD-KIND-020', 'changing %s changes the entry hash' % member, 'entry_hash',
          {'entry': dict(ENTRY, **{member: value}), 'body': EBODY})
    P('STD-KIND-020', 'changing the body changes the entry hash', 'entry_hash',
      {'entry': ENTRY, 'body': dict(EBODY, amount=usd('100.01'))})
    for member in ('entry_id', 'class', 'ledger_key', 'valid_at'):
        P('STD-KIND-020', 'an entry without %s is rejected' % member, 'entry_hash',
          {'entry': {k: v for k, v in ENTRY.items() if k != member}, 'body': EBODY}, error='MODEL.INVALID_VALUE')
    V('STD-KIND-020', 'an appended entry is read back with its fields', setup=setup(), steps=[
        commit([gl(uid(10), '1010', '100.00'), gl(uid(11), '4000', '-100.00')]), ledger()])
    # STD-KIND-021 append-only
    two = [gl(uid(10), '1010', '100.00'), gl(uid(11), '4000', '-100.00')]
    V('STD-KIND-021', 'an update targeting an entry fails', setup=setup(), steps=[
        commit(two), commit([{'op': 'update', 'object_id': uid(10), 'base': 'sha256:' + '0' * 64, 'body': {}}],
                            error='LEDG.IMMUTABLE_ENTRY')])
    V('STD-KIND-021', 'an end targeting an entry fails', setup=setup(), steps=[
        commit(two), commit([{'op': 'end', 'object_id': uid(10), 'base': 'sha256:' + '0' * 64}],
                            error='LEDG.IMMUTABLE_ENTRY')])
    V('STD-KIND-021', 'a create of a ledger-class object fails', setup=setup(), steps=[
        commit([create(uid(1), klass='GlEntry', body={'account': '1010', 'amount': usd('1.00')})],
               error='LOGIC.FORBIDDEN_TARGET')])
    V('STD-KIND-021', 'an entry_id cannot be appended twice', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5')]), commit([stock(uid(20), 'bolt', '5')], error='TXN.ALREADY_EXISTS')])
    # STD-KIND-022 gap-free sequences
    V('STD-KIND-022', 'entries of one ledger key are numbered 1, 2, 3 in commit order', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5')]), commit([stock(uid(21), 'bolt', '-2')]),
        commit([stock(uid(22), 'bolt', '1')]), ledger(cls_='StockMove', key={'item': 'bolt'})])
    V('STD-KIND-022', 'entries appended in one commit are numbered in change order', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5'), stock(uid(21), 'bolt', '1')]),
        ledger(cls_='StockMove', key={'item': 'bolt'})])
    V('STD-KIND-022', 'each ledger key has its own sequence', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5'), stock(uid(21), 'nut', '7'), stock(uid(22), 'bolt', '1')]),
        ledger(cls_='StockMove', key={'item': 'nut'})])
    V('STD-KIND-022', 'a failed commit does not consume a sequence number', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5')]),
        commit([stock(uid(21), 'bolt', '1'), stock(uid(20), 'bolt', '5')], error='TXN.ALREADY_EXISTS'),
        commit([stock(uid(22), 'bolt', '3')]), ledger(cls_='StockMove', key={'item': 'bolt'})])
    V('STD-KIND-022', 'the commit result reports the assigned sequence numbers', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5')]), commit([stock(uid(21), 'bolt', '1'), stock(uid(22), 'nut', '1')])])
    V('STD-KIND-022', 'a sequence number is visible only from its commit on', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5')]), commit([stock(uid(21), 'bolt', '1')]),
        ledger(cls_='StockMove', key={'item': 'bolt'}, time=1)])
    # STD-KIND-023 reversal
    rev = {'op': 'reverse', 'entry_id': uid(10), 'new_entry_id': uid(12)}
    rev2 = {'op': 'reverse', 'entry_id': uid(11), 'new_entry_id': uid(13)}
    V('STD-KIND-023', 'a reversal negates every amount and references the original', setup=setup(), steps=[
        commit(two), commit([rev, rev2]), ledger()])
    V('STD-KIND-023', 'a reversal keeps the valid_at of the original', setup=setup(), steps=[
        commit(two), commit([rev, rev2]), ledger(asof='2026-03-15')])
    V('STD-KIND-023', 'a second reversal of the same entry fails', setup=setup(), steps=[
        commit(two), commit([rev, rev2]),
        commit([{'op': 'reverse', 'entry_id': uid(10), 'new_entry_id': uid(14)},
                {'op': 'reverse', 'entry_id': uid(11), 'new_entry_id': uid(15)}], error='LEDG.ALREADY_REVERSED')])
    V('STD-KIND-023', 'a reversal entry cannot be reversed', setup=setup(), steps=[
        commit(two), commit([rev, rev2]),
        commit([{'op': 'reverse', 'entry_id': uid(12), 'new_entry_id': uid(14)},
                {'op': 'reverse', 'entry_id': uid(13), 'new_entry_id': uid(15)}], error='LEDG.REVERSAL_NOT_REVERSIBLE')])
    V('STD-KIND-023', 'a reversal of a single-sided ledger entry', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5')]),
        commit([{'op': 'reverse', 'entry_id': uid(20), 'new_entry_id': uid(21)}]),
        ledger(cls_='StockMove', key={'item': 'bolt'})])
    # STD-KIND-024 double entry
    V('STD-KIND-024', 'a balanced posting group commits', setup=setup(), steps=[commit(two), ledger(key={'account': '4000'})])
    V('STD-KIND-024', 'an unbalanced posting group fails', setup=setup(), steps=[
        commit([gl(uid(10), '1010', '100.00'), gl(uid(11), '4000', '-90.00')], error='LEDG.UNBALANCED')])
    V('STD-KIND-024', 'each posting group balances on its own', setup=setup(), steps=[
        commit([gl(uid(10), '1010', '100.00'), gl(uid(11), '4000', '-50.00'), gl(uid(12), '4000', '-50.00'),
                gl(uid(13), '1010', '20.00', group='g2'), gl(uid(14), '4000', '-10.00', group='g2')],
               error='LEDG.UNBALANCED')])
    V('STD-KIND-024', 'a group balances per currency', setup=setup(), steps=[
        commit([gl(uid(10), '1010', '100.00'),
                dict(gl(uid(11), '4000', '-100.00'), body={'account': '4000', 'amount': {'amount': '-100.00', 'currency': 'EUR'}})],
               error='LEDG.UNBALANCED')])
    V('STD-KIND-024', 'a class without double_entry needs no balance', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5')]), ledger(cls_='StockMove', key={'item': 'bolt'})])
    # STD-KIND-025 periods
    closed = create(uid(50), klass='FiscalPeriod', slug='2026-03',
                    body={'start': '2026-03-01', 'end': '2026-04-01', 'state': 'closed'})
    opened = create(uid(51), klass='FiscalPeriod', slug='2026-04',
                    body={'start': '2026-04-01', 'end': '2026-05-01', 'state': 'open'})
    V('STD-KIND-025', 'an append with a posting date in a closed period fails', setup=setup(), steps=[
        commit([closed]), commit(two, error='LEDG.PERIOD_CLOSED')])
    V('STD-KIND-025', 'an append in an open period succeeds', setup=setup(now='2026-04-10T09:00:00.000000Z'), steps=[
        commit([closed, opened]), commit(two), ledger()])
    V('STD-KIND-025', 'the posting date is the date of the commit, not valid_at', setup=setup(now='2026-04-10T09:00:00.000000Z'),
      steps=[commit([closed, opened]),
             commit([gl(uid(10), '1010', '100.00', valid_at='2026-03-20'), gl(uid(11), '4000', '-100.00', valid_at='2026-03-20')]),
             ledger()])
    V('STD-KIND-025', 'a ledger class without a period scope ignores periods', setup=setup(), steps=[
        commit([closed]), commit([stock(uid(20), 'bolt', '5')]), ledger(cls_='StockMove', key={'item': 'bolt'})])
    V('STD-KIND-025', 'the end of a period is exclusive', setup=setup(now='2026-04-01T00:00:00.000000Z'), steps=[
        commit([closed]), commit(two), ledger()])
    # STD-KIND-040 System kind
    V('STD-KIND-040', 'a generic create of a System-kind object fails', setup=setup(), steps=[
        commit([create(uid(60), klass='DeliveryJob', slug='job-1', body={'target': 'x'})], error='LOGIC.FORBIDDEN_TARGET')])
    V('STD-KIND-040', 'a generic create of a System-kind object with an empty body also fails', setup=setup(), steps=[
        commit([create(uid(61), klass='DeliveryJob', slug='job-2', body={})], error='LOGIC.FORBIDDEN_TARGET')])
    # STD-KIND-060 RawDocument
    V('STD-KIND-060', 'a RawDocument body may hold undeclared content', setup=setup(), steps=[
        commit([create(uid(70), klass='ImportedDocument', slug='doc-1',
                       body={'source': 'bank', 'payload': {'lines': [{'amount': '1.00'}]}})]),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(70)}, 'match': 'exact'}])
    V('STD-KIND-060', 'declared fields of a RawDocument are validated', setup=setup(), steps=[
        commit([create(uid(70), klass='ImportedDocument', slug='doc-1', body={'source': 7, 'payload': {}})],
               error='MODEL.INVALID_VALUE')])
    V('STD-KIND-060', 'required declared fields of a RawDocument are enforced', setup=setup(), steps=[
        commit([create(uid(70), klass='ImportedDocument', slug='doc-1', body={'payload': {}})],
               error='MODEL.CONSTRAINT_VIOLATED')])
    # STD-KIND-080 immutable kind
    V('STD-KIND-080', 'the kind of a class with instances cannot change', setup=setup(), steps=[
        commit([create(uid(1))]),
        {'op': 'load_genesis', 'input': {'classes': [cls('Customer', [field('name', 'text', True)], kind='raw_document')]},
         'error': 'MODEL.KIND_IMMUTABLE'}])
    V('STD-KIND-080', 'the kind of a class without instances may change', setup=setup(), steps=[
        {'op': 'load_genesis', 'input': {'classes': [cls('Customer', [field('name', 'text', True)], kind='raw_document')]}}])
