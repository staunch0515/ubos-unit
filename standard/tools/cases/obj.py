"""Vector cases for the object model (STD-OBJ-*)."""
import hashlib

from .common import setup, commit, create, base, uid

ENV = {'object_id': uid(1), 'class': 'Customer', 'class_version': 'sha256:' + hashlib.sha256(b'Customer v1').hexdigest(),
       'schema_version': 1, 'kind': 'definition', 'slug': 'acme', 'valid': {'from': '2026-01-01', 'to': None}}
BODY = {'name': 'Acme Inc.', 'email': 'ap@acme.example'}


def cases(g):
    V = g.vector
    # STD-OBJ-001 object identity
    V('STD-OBJ-001', 'an object keeps its object_id across versions', setup=setup(), steps=[
        commit([create(uid(1))], label='c1'),
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('c1', uid(1)), 'body': {'email': 'a@x.example'}}]),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(1)}}])
    V('STD-OBJ-001', 'two objects with the same body are distinct objects', setup=setup(), steps=[
        commit([create(uid(1), slug='a'), create(uid(2), slug='b')]),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(2)}}])
    V('STD-OBJ-001', 'an object_id cannot be created twice', setup=setup(), steps=[
        commit([create(uid(1))]), commit([create(uid(1), slug='other')], error='TXN.ALREADY_EXISTS')])
    V('STD-OBJ-001', 'reading an object that never existed fails', setup=setup(), steps=[
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(9)}, 'error': 'IAM.NOT_FOUND'}])
    # STD-OBJ-002 version envelope and hash
    P = g.pure
    P('STD-OBJ-002', 'version hash over the canonical envelope with body_ref', 'version_hash', {'envelope': ENV, 'body': BODY})
    for member, value in [('slug', 'acme-inc'), ('schema_version', 2), ('valid', {'from': '2026-02-01', 'to': None}),
                          ('kind', 'raw_document'), ('state', 'active'),
                          ('predecessor', 'sha256:' + hashlib.sha256(b'v0').hexdigest()), ('tombstone', True)]:
        P('STD-OBJ-002', 'changing envelope member %s changes the version hash' % member, 'version_hash',
          {'envelope': dict(ENV, **{member: value}), 'body': BODY})
    P('STD-OBJ-002', 'changing the body changes body_ref and the version hash', 'version_hash',
      {'envelope': ENV, 'body': dict(BODY, name='Acme LLC')})
    P('STD-OBJ-002', 'member order in the envelope does not matter', 'version_hash', None,
      text='{"body": {"email": "ap@acme.example", "name": "Acme Inc."}, "envelope": {"valid": {"to": null, "from": "2026-01-01"}, '
           '"slug": "acme", "schema_version": 1, "object_id": "%s", "kind": "definition", "class_version": "%s", "class": "Customer"}}'
           % (ENV['object_id'], ENV['class_version']))
    for member in ('object_id', 'class', 'class_version', 'slug', 'valid'):
        P('STD-OBJ-002', 'an envelope without %s is rejected' % member, 'version_hash',
          {'envelope': {k: v for k, v in ENV.items() if k != member}, 'body': BODY}, error='MODEL.INVALID_VALUE')
    # STD-OBJ-003 own values only
    V('STD-OBJ-003', 'a default of the class is not stored in the body', setup=setup(), steps=[
        commit([create(uid(1), body={'name': 'Acme'})]),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(1)}, 'match': 'exact'}])
    V('STD-OBJ-003', 'an explicitly set value equal to the default is stored', setup=setup(), steps=[
        commit([create(uid(1), body={'name': 'Acme', 'country': 'US'})]),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(1)}, 'match': 'exact'}])
    V('STD-OBJ-003', 'an undeclared member is not stored', setup=setup(), steps=[
        commit([create(uid(1), body={'name': 'Acme', 'nickname': 'A'})], error='MODEL.INVALID_VALUE')])
    # STD-OBJ-004 slug changes
    V('STD-OBJ-004', 'a slug change produces a new version with the new slug', setup=setup(), steps=[
        commit([create(uid(1), slug='acme')], label='c1'),
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('c1', uid(1)), 'slug': 'acme-inc', 'body': {}}]),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(1)}}])
    V('STD-OBJ-004', 'the earlier version keeps the old slug', setup=setup(), steps=[
        commit([create(uid(1), slug='acme')], label='c1'),
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('c1', uid(1)), 'slug': 'acme-inc', 'body': {}}]),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(1), 'time': 1}}])
    V('STD-OBJ-004', 'a slug outside the grammar is rejected', setup=setup(), steps=[
        commit([create(uid(1), slug='Acme Inc')], error='MODEL.INVALID_NAME')])
    # STD-OBJ-005 ending
    end_steps = [commit([create(uid(1), valid={'from': '2026-01-01'})], label='c1'),
                 commit([{'op': 'end', 'object_id': uid(1), 'base': base('c1', uid(1)), 'valid': {'from': '2026-03-01'}}])]
    V('STD-OBJ-005', 'an ended object is not found at an asof after the end', setup=setup(), steps=end_steps + [
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(1), 'asof': '2026-03-10'}, 'error': 'QRY.NOT_FOUND_AT_TIME'}])
    V('STD-OBJ-005', 'an ended object remains readable at an earlier asof', setup=setup(), steps=end_steps + [
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(1), 'asof': '2026-02-15'}}])
    V('STD-OBJ-005', 'an ended object remains readable at an earlier time', setup=setup(), steps=end_steps + [
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(1), 'asof': '2026-03-10', 'time': 1}}])
    V('STD-OBJ-005', 'the valid period of the last segment ends at the end date', setup=setup(), steps=end_steps + [
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(1), 'asof': '2026-01-01'}}])
    V('STD-OBJ-005', 'an ended object cannot be created again', setup=setup(), steps=end_steps + [
        commit([create(uid(1), slug='again')], error='TXN.ALREADY_EXISTS')])
    # STD-OBJ-008 single-chunk bodies
    V('STD-OBJ-008', 'a body under 64 KiB is one chunk: body_ref is the chunk identifier of the body', steps=[
        {'op': 'version_hash', 'input': {'envelope': ENV, 'body': BODY}, 'label': 'v'},
        {'op': 'content_id', 'input': {'domain': 'ubos.chunk.v1', 'value': BODY}, 'same_as': 'v/body_ref'}])
    V('STD-OBJ-008', 'the body_ref does not depend on the envelope', steps=[
        {'op': 'version_hash', 'input': {'envelope': dict(ENV, slug='other'), 'body': BODY}, 'label': 'v'},
        {'op': 'content_id', 'input': {'domain': 'ubos.chunk.v1', 'value': BODY}, 'same_as': 'v/body_ref'}])

    # STD-OBJ-009 size limits (body size = bytes of the canonical body; limit set by set_limits)
    def sized(n):
        return {'name': 'x' * (n - len('{"name":""}'))}

    lim = [{'op': 'set_limits', 'input': {'max_body_bytes': 100}}]
    V('STD-OBJ-009', 'a body at the configured limit is accepted', setup=setup() + lim, prototype=False, steps=[
        commit([create(uid(1), body=sized(100))]),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(1)}}])
    V('STD-OBJ-009', 'a body one byte over the configured limit is rejected', setup=setup() + lim, prototype=False, steps=[
        commit([create(uid(1), body=sized(101))], error='MODEL.OBJECT_TOO_LARGE')])
    V('STD-OBJ-009', 'an update that grows the body over the limit is rejected', setup=setup() + lim, prototype=False, steps=[
        commit([create(uid(1), body=sized(60))], label='c1'),
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('c1', uid(1)), 'body': sized(120)}],
               error='MODEL.OBJECT_TOO_LARGE')])
    V('STD-OBJ-009', 'the limit also applies to raw documents', setup=setup() + lim, prototype=False, steps=[
        commit([create(uid(1), klass='ImportedDocument', body={'source': 'y' * 120})], error='MODEL.OBJECT_TOO_LARGE')])
    V('STD-OBJ-009', 'the size is measured on the canonical body, not on the input text', setup=setup() + lim,
      prototype=False, steps=[
        {'op': 'commit', 'input_text': '{"branch": "main", "changes": [{"op": "create", "object_id": "%s", '
         '"class": "Customer", "slug": "c-1", "body": {   "name":   "%s"   }}]}' % (uid(1), 'x' * 89)},
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(1)}}])
