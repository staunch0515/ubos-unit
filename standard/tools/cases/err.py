"""Vector cases for the error object and registry (STD-ERR-*)."""
from .common import setup, commit, create, base, uid


def cases(g):
    V = g.vector
    C0 = create(uid(1), body={'name': 'Acme'})
    V('STD-ERR-001', 'a stale base error carries structured details', setup=setup(), steps=[
        commit([C0], label='c1'), commit([{'op': 'update', 'object_id': uid(1), 'base': base('c1', uid(1)), 'body': {'name': 'B'}}]),
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('c1', uid(1)), 'body': {'name': 'C'}}],
               error='TXN.HEAD_MOVED', details=True)])
    V('STD-ERR-001', 'a merge conflict error carries the conflict list', setup=setup(), steps=[
        commit([C0], label='c1'), {'op': 'create_branch', 'input': {'name': 'feature/x', 'from': 'main'}},
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('c1', uid(1)), 'body': {'name': 'S'}}], branch='feature/x'),
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('c1', uid(1)), 'body': {'name': 'T'}}]),
        {'op': 'merge', 'input': {'source': 'feature/x', 'target': 'main'}, 'error': 'VER.MERGE_CONFLICT', 'details': True}])
    P = g.pure
    for code, op, inp in [
        ('MODEL.CURRENCY_MISMATCH', 'money', {'op': 'add', 'a': {'amount': '1', 'currency': 'USD'}, 'b': {'amount': '1', 'currency': 'GBP'}}),
        ('MODEL.UNKNOWN_TYPE', 'validate_value', {'type': 'uuid', 'value': 'x'}),
        ('MODEL.INVALID_NAME', 'check_name', {'grammar': 'class-name', 'text': 'lower'}),
        ('VER.INVALID_BRANCH_NAME', 'check_name', {'grammar': 'branch-name', 'text': 'main/x'}),
        ('MODEL.CONSTRAINT_VIOLATED', 'validate_value', {'type': 'integer', 'constraints': {'max': 1}, 'value': 2}),
        ('MODEL.INVALID_VALUE', 'validate_value', {'type': 'boolean', 'value': 0}),
        ('LOGIC.DIVISION_BY_ZERO', 'decimal', {'op': 'div', 'a': '1', 'b': '0', 'scale': 0, 'rounding': 'half_even'}),
        ('MODEL.INHERITANCE_CYCLE', 'linearize', {'classes': [{'name': 'A', 'parent': 'A'}], 'class': 'A'})]:
        P('STD-ERR-002', 'the registered code %s is used for its condition' % code, op, inp, error=code,
          prototype=op != 'linearize')
