"""Vector cases for bitemporal semantics (STD-TIME-*)."""
from .common import setup, commit, create, base, uid, stock, cls, field, CLASSES

C0 = create(uid(1), body={'name': 'Acme'}, valid={'from': '2026-01-01'})
EVENT = cls('Meter', [field('reading', 'integer', True)], valid_time='instant')


def upd(label, oid, body, kind=None, frm=None, to=None):
    ch = {'op': 'update', 'object_id': oid, 'base': base(label, oid), 'body': body}
    if kind:
        ch['valid'] = {'kind': kind, 'from': frm}
        if to:
            ch['valid']['to'] = to
    return ch


def read(asof=None, time=None, oid=None, error=None):
    i = {'branch': 'main', 'object_id': oid or uid(1)}
    if asof:
        i['asof'] = asof
    if time is not None:
        i['time'] = time
    s = {'op': 'read', 'input': i}
    if error:
        s['error'] = error
    return s


def cases(g):
    V = g.vector
    # STD-TIME-001 transaction time
    V('STD-TIME-001', 'a client-supplied transaction time is rejected', setup=setup(), steps=[
        {'op': 'commit', 'input': {'branch': 'main', 'changes': [C0], 'tx_time': '2026-03-15T10:00:00.000000Z'},
         'error': 'TIME.RESERVED_FIELD'}])
    V('STD-TIME-001', 'a transaction time inside a change is rejected', setup=setup(), steps=[
        commit([dict(C0, tx_time='2026-03-15T10:00:00.000000Z')], error='TIME.RESERVED_FIELD')])
    V('STD-TIME-001', 'transaction time orders reads: an earlier time does not see a later commit', setup=setup(), steps=[
        commit([C0]), commit([create(uid(2), slug='b', valid={'from': '2026-01-01'})]),
        read(asof='2026-02-01', time=1, oid=uid(2), error='IAM.NOT_FOUND')])
    # STD-TIME-010 valid-time periods
    V('STD-TIME-010', 'a date-granularity version is valid from a date', setup=setup(), steps=[commit([C0]), read('2026-01-01')])
    V('STD-TIME-010', 'a date-granularity version is not valid before its start', setup=setup(), steps=[
        commit([C0]), read('2025-12-31', error='QRY.NOT_FOUND_AT_TIME')])
    V('STD-TIME-010', 'an instant-granularity class uses instants', setup=setup(classes=CLASSES + [EVENT]), steps=[
        commit([create(uid(8), klass='Meter', slug='m', body={'reading': 1}, valid={'from': '2026-03-01T08:00:00.000000Z'})]),
        read('2026-03-01T08:00:00.000001Z', oid=uid(8))])
    # STD-TIME-011 timeline operations
    T = lambda desc, changes, check: V('STD-TIME-011', desc, setup=setup(), steps=[commit([C0], label='c1'), commit(changes), *check])
    T('supersede_from splits the segment that contains the date', [upd('c1', uid(1), {'name': 'B'}, 'supersede_from', '2026-02-01')],
      [read('2026-01-15'), read('2026-02-15')])
    V('STD-TIME-011', 'supersede_from stops at the start of the next segment', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'B'}, 'supersede_from', '2026-03-01')], label='c2'),
        commit([upd('c2', uid(1), {'name': 'C'}, 'supersede_from', '2026-02-01')]), read('2026-02-15'), read('2026-03-15')])
    T('set_period fills a gap before the first segment', [upd('c1', uid(1), {'name': 'Early'}, 'set_period', '2025-10-01', '2026-01-01')],
      [read('2025-11-01'), read('2026-02-01')])
    V('STD-TIME-011', 'set_period that partly overlaps a segment is rejected', setup=setup(), steps=[
        commit([C0], label='c1'),
        commit([upd('c1', uid(1), {'name': 'X'}, 'set_period', '2026-02-01', '2026-03-01')], error='TIME.OVERLAPPING_VALIDITY')])
    V('STD-TIME-011', 'set_period replaces a whole segment it covers', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'B'}, 'supersede_from', '2026-02-01')], label='c2'),
        commit([upd('c2', uid(1), {'name': 'A2'}, 'set_period', '2026-01-01', '2026-02-01')]), read('2026-01-15'), read('2026-02-15')])
    V('STD-TIME-011', 'set_period with an empty period is invalid', setup=setup(), steps=[
        commit([C0], label='c1'),
        commit([upd('c1', uid(1), {'name': 'X'}, 'set_period', '2026-02-01', '2026-02-01')], error='MODEL.INVALID_VALUE')])
    V('STD-TIME-011', 'replace_future removes all later segments', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'B'}, 'supersede_from', '2026-04-01')], label='c2'),
        commit([upd('c2', uid(1), {'name': 'C'}, 'replace_future', '2026-02-01')]), read('2026-05-01'), read('2026-01-15')])
    V('STD-TIME-011', 'end truncates the timeline at the date', setup=setup(), steps=[
        commit([C0], label='c1'), commit([{'op': 'end', 'object_id': uid(1), 'base': base('c1', uid(1)), 'valid': {'from': '2026-02-01'}}]),
        read('2026-01-31'), read('2026-02-01', error='QRY.NOT_FOUND_AT_TIME')])
    V('STD-TIME-011', 'end removes later segments', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'B'}, 'supersede_from', '2026-04-01')], label='c2'),
        commit([{'op': 'end', 'object_id': uid(1), 'base': base('c2', uid(1)), 'valid': {'from': '2026-02-01'}}]),
        read('2026-05-01', error='QRY.NOT_FOUND_AT_TIME')])
    V('STD-TIME-011', 'a plain update supersedes from today', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'B'})]), read('2026-03-14'), read('2026-03-15')])
    V('STD-TIME-011', 'a future-dated supersede leaves today unchanged', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'B'}, 'supersede_from', '2026-06-01')]),
        read('2026-03-15'), read('2026-06-01')])
    V('STD-TIME-011', 'creating an object requires an empty timeline', setup=setup(), steps=[
        commit([C0]), commit([C0], error='TXN.ALREADY_EXISTS')])
    V('STD-TIME-011', 'segments stay sorted and non-overlapping after several operations', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'B'}, 'supersede_from', '2026-02-01')], label='c2'),
        commit([upd('c2', uid(1), {'name': 'C'}, 'supersede_from', '2026-03-01')], label='c3'),
        commit([upd('c3', uid(1), {'name': 'D'}, 'replace_future', '2026-02-15')]),
        read('2026-01-15'), read('2026-02-10'), read('2026-02-20'), read('2026-04-01')])
    # STD-TIME-012 as-of resolution
    V('STD-TIME-012', 'the default asof is today', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'Later'}, 'supersede_from', '2026-04-01')]), read()])
    V('STD-TIME-012', 'a future-dated version becomes effective by resolution', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'Later'}, 'supersede_from', '2026-04-01')]),
        {'op': 'set_now', 'input': {'now': '2026-04-02T00:00:00.000000Z'}}, read()])
    V('STD-TIME-012', 'an asof outside every segment is not found at those coordinates', setup=setup(), steps=[
        commit([C0]), read('2025-06-01', error='QRY.NOT_FOUND_AT_TIME')])
    V('STD-TIME-012', 'reading at a past time uses the timeline known then', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'Fixed'}, 'supersede_from', '2026-01-01')]),
        read('2026-02-01', time=1)])
    V('STD-TIME-012', 'reading at the head time sees the correction', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'Fixed'}, 'supersede_from', '2026-01-01')]),
        read('2026-02-01', time=2)])
    V('STD-TIME-012', 'the read reports the segment that contains asof', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'B'}, 'set_period', '2025-06-01', '2026-01-01')]),
        read('2025-07-01')])
    # STD-TIME-020 bitemporal inclusion of entries
    L = lambda **kw: {'op': 'read_ledger', 'input': dict({'branch': 'main', 'class': 'StockMove', 'ledger_key': {'item': 'bolt'}}, **kw)}
    V('STD-TIME-020', 'an entry is included from its valid_at on', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5', valid_at='2026-03-10'), stock(uid(21), 'bolt', '2', valid_at='2026-03-20')]),
        L(asof='2026-03-15')])
    V('STD-TIME-020', 'an entry is included only at times from its commit on', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5', valid_at='2026-03-10')]), commit([stock(uid(21), 'bolt', '2', valid_at='2026-03-01')]),
        L(asof='2026-03-31', time=1)])
    V('STD-TIME-020', 'a backdated entry appears in past asof reads after its commit', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5', valid_at='2026-03-10')]), commit([stock(uid(21), 'bolt', '2', valid_at='2026-03-01')]),
        L(asof='2026-03-05')])
    V('STD-TIME-020', 'an entry valid on the asof date is included', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5', valid_at='2026-03-10')]), L(asof='2026-03-10')])
    V('STD-TIME-020', 'no entry is included before the first valid_at', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5', valid_at='2026-03-10')]), L(asof='2026-03-09')])
    # STD-TIME-030 retroactive changes
    V('STD-TIME-030', 'a change before today records its affected valid range', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'Fixed'}, 'supersede_from', '2026-02-01')])])
    V('STD-TIME-030', 'a change from today on is not retroactive', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'B'})])])
    V('STD-TIME-030', 'a future change is not retroactive', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'B'}, 'supersede_from', '2026-06-01')])])
    V('STD-TIME-030', 'a set_period in the past records its period', setup=setup(), steps=[
        commit([C0], label='c1'), commit([upd('c1', uid(1), {'name': 'E'}, 'set_period', '2025-06-01', '2025-09-01')])])
    V('STD-TIME-030', 'a retroactive end records its range', setup=setup(), steps=[
        commit([C0], label='c1'), commit([{'op': 'end', 'object_id': uid(1), 'base': base('c1', uid(1)), 'valid': {'from': '2026-02-01'}}])])
