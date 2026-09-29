"""Vector cases for the foundations area (STD-FND-*)."""
import hashlib


def cases(g):
    P = g.pure
    # STD-FND-001 canonical JSON
    for desc, value in [
        ('object members are sorted by key', {'b': 1, 'a': 2}),
        ('nested objects and arrays keep array order', {'z': [3, 1, 2], 'a': {'y': True, 'x': None}}),
        ('no insignificant whitespace in a list of scalars', [1, 'two', False, None]),
        ('keys sort by UTF-16 code units: supplementary characters before U+FFFF', {'￿': 1, '\U0001F600': 2}),
        ('keys sort by UTF-16 code units: upper case before lower case', {'b': 1, 'B': 2, 'a': 3, 'A': 4}),
        ('non-ASCII text is written as UTF-8, not escaped', {'name': 'Zürich €'}),
        ('control characters use the short escapes', {'t': 'a\nb\tc\rd\be\ff'}),
        ('other control characters use lowercase \\u escapes', {'t': '\x01\x1f'}),
        ('quote and backslash are escaped', {'t': 'say "hi" \\ bye'}),
        ('the solidus is not escaped', {'url': 'a/b'}),
        ('empty object and empty array', {'o': {}, 'a': []}),
        ('negative integer and zero', {'n': -42, 'z': 0}),
        ('largest safe integer', {'n': 9007199254740991}),
        ('smallest safe integer', {'n': -9007199254740991}),
        ('deep nesting', {'a': {'b': {'c': {'d': [1, {'e': 'f'}]}}}}),
        ('numeric-looking keys sort as strings', {'10': 1, '9': 2, '1': 3}),
    ]:
        P('STD-FND-001', 'canonical JSON: ' + desc, 'canonical_json', {'value': value})
    P('STD-FND-001', 'canonical JSON: -0 is written as 0', 'canonical_json', None, text='{"value": -0}')
    P('STD-FND-001', 'canonical JSON: duplicate member names are rejected', 'canonical_json', None,
      text='{"value": {"a": 1, "a": 2}}', error='MODEL.INVALID_VALUE')
    P('STD-FND-001', 'canonical JSON: whitespace in the input does not change the output', 'canonical_json', None,
      text='{ "value" : { "b" : [ 1 , 2 ] ,\n "a" : "x" } }')
    P('STD-FND-001', 'canonical JSON: escaped input characters are written literally', 'canonical_json', None,
      text='{"value": "caf\\u00e9 \\u20ac"}')
    # STD-FND-002 no floating point
    for desc, text in [('a fraction', '1.5'), ('an exponent', '1e3'), ('a fraction with zero', '2.0'),
                       ('a negative fraction', '-0.25'), ('an exponent inside an array', '[1, 2E-2]'),
                       ('a fraction inside an object', '{"amount": 10.10}'),
                       ('an integer above 2^53-1', '9007199254740992'),
                       ('an integer below -(2^53-1)', '-9007199254740993')]:
        P('STD-FND-002', 'canonical data rejects a number with ' + desc, 'canonical_json', None,
          text='{"value": %s}' % text, error='MODEL.INVALID_VALUE')
    P('STD-FND-002', 'a large integer is carried as a decimal string', 'validate_value',
      {'type': 'integer', 'value': '9007199254740993'})
    P('STD-FND-002', 'a decimal is carried as a string', 'validate_value', {'type': 'decimal', 'value': '10.10'})
    # STD-FND-003 NFC
    P('STD-FND-003', 'decomposed e + combining acute becomes precomposed', 'normalize_text', {'text': 'é'})
    P('STD-FND-003', 'Hangul jamo compose to a syllable', 'normalize_text', {'text': '가'})
    P('STD-FND-003', 'text already in NFC is unchanged', 'normalize_text', {'text': 'Straße'})
    P('STD-FND-003', 'canonical JSON normalises strings to NFC before serialising', 'canonical_json',
      {'value': {'k': 'Å'}})
    P('STD-FND-003', 'visually identical strings get the same content identifier', 'content_id',
      {'domain': 'ubos.chunk.v1', 'value': 'Amélie'})
    # STD-FND-010 content identifiers
    for desc, value in [('an empty object', {}), ('a string', 'hello'), ('an integer', 7), ('null', None),
                        ('a nested value', {'a': [1, {'b': 'c'}]}), ('true', True)]:
        P('STD-FND-010', 'content identifier of ' + desc + ' is sha256:<lowercase hex>', 'content_id',
          {'domain': 'ubos.chunk.v1', 'value': value})
    P('STD-FND-010', 'content identifier is independent of member order in the input', 'content_id', None,
      text='{"domain": "ubos.chunk.v1", "value": {"y": 2, "x": 1}}')
    P('STD-FND-010', 'content identifier syntax is accepted in a blob value', 'validate_value',
      {'type': 'blob', 'value': {'content_id': 'sha256:' + hashlib.sha256(b'x').hexdigest(), 'size': 1,
                                 'media_type': 'text/plain', 'name': 'x.txt'}})
    P('STD-FND-010', 'uppercase hex is not a content identifier', 'validate_value',
      {'type': 'blob', 'value': {'content_id': 'sha256:' + hashlib.sha256(b'x').hexdigest().upper(), 'size': 1,
                                 'media_type': 'text/plain', 'name': 'x.txt'}}, error='MODEL.INVALID_VALUE')
    P('STD-FND-010', 'an unregistered algorithm prefix is not a content identifier', 'validate_value',
      {'type': 'blob', 'value': {'content_id': 'md5:' + hashlib.md5(b'x').hexdigest(), 'size': 1,
                                 'media_type': 'text/plain', 'name': 'x.txt'}}, error='MODEL.INVALID_VALUE')
    # STD-FND-011 domain separation
    for dom in ('ubos.chunk.v1', 'ubos.version.v1', 'ubos.entry.v1', 'ubos.commit.v1', 'ubos.tree.leaf.v1',
                'ubos.tree.node.v1', 'ubos.seal.v1', 'ubos.pack.v1', 'ubos.field-commit.v1'):
        P('STD-FND-011', 'the same value hashes differently under domain tag ' + dom, 'content_id',
          {'domain': dom, 'value': {'k': 'v'}})
    P('STD-FND-011', 'an unregistered domain tag is rejected', 'content_id',
      {'domain': 'ubos.other.v1', 'value': {'k': 'v'}}, error='MODEL.INVALID_VALUE')
    # STD-FND-015 object identifiers
    P('STD-FND-015', 'a UUID version 7 is accepted', 'check_name',
      {'grammar': 'uuid-v7', 'text': '01890a5d-ac96-774b-bcce-b302099a8057'})
    P('STD-FND-015', 'a UUID version 4 is accepted on import', 'check_name',
      {'grammar': 'uuid', 'text': '4b1c8f0e-9d2a-4e6f-8a3b-5c7d9e1f2a3b'})
    P('STD-FND-015', 'a UUID version 4 is not a version 7 identifier', 'check_name',
      {'grammar': 'uuid-v7', 'text': '4b1c8f0e-9d2a-4e6f-8a3b-5c7d9e1f2a3b'}, error='MODEL.INVALID_NAME')
    P('STD-FND-015', 'uppercase UUID text is not canonical', 'check_name',
      {'grammar': 'uuid', 'text': '01890A5D-AC96-774B-BCCE-B302099A8057'}, error='MODEL.INVALID_NAME')
    P('STD-FND-015', 'a UUID without hyphens is rejected', 'check_name',
      {'grammar': 'uuid', 'text': '01890a5dac96774bbcceb302099a8057'}, error='MODEL.INVALID_NAME')
    # STD-FND-016 names
    for grammar, text, ok in [
        ('slug', 'invoice-2026_01', True), ('slug', 'a', True), ('slug', 'x' * 63, True),
        ('slug', 'x' * 64, False), ('slug', 'Invoice', False), ('slug', '-start', False),
        ('class-name', 'SalesInvoice', True), ('class-name', 'salesInvoice', False),
        ('class-name', 'Sales_Invoice', False), ('member-name', 'due_date', True),
        ('member-name', 'dueDate', False), ('member-name', '1st', False),
        ('namespace', 'acme.finance', True), ('namespace', 'acme..finance', False),
        ('namespace', 'Acme', False)]:
        P('STD-FND-016', '%s %r is %s' % (grammar, text if len(text) < 20 else text[:5] + '...(%d)' % len(text),
                                          'valid' if ok else 'invalid'),
          'check_name', {'grammar': grammar, 'text': text}, error=None if ok else 'MODEL.INVALID_NAME')
    # STD-FND-020 value types
    V = lambda desc, t, v, c=None, err=None: P('STD-FND-020', desc, 'validate_value',
                                               dict({'type': t, 'value': v}, **({'constraints': c} if c else {})),
                                               error=err)
    V('text is a string', 'text', 'hello')
    V('text min_length counts code points', 'text', 'äö', {'min_length': 2})
    V('text shorter than min_length violates the constraint', 'text', 'a', {'min_length': 2}, 'MODEL.CONSTRAINT_VIOLATED')
    V('text longer than max_length violates the constraint', 'text', 'abcd', {'max_length': 3}, 'MODEL.CONSTRAINT_VIOLATED')
    V('text pattern must match the whole value', 'text', 'AB12', {'pattern': '[A-Z]{2}[0-9]{2}'})
    V('text not matching the pattern violates it', 'text', 'AB123', {'pattern': '[A-Z]{2}[0-9]{2}'}, 'MODEL.CONSTRAINT_VIOLATED')
    V('a number is not text', 'text', 5, None, 'MODEL.INVALID_VALUE')
    V('integer within the safe range is a JSON number', 'integer', -12)
    V('integer below min violates the constraint', 'integer', 3, {'min': 5}, 'MODEL.CONSTRAINT_VIOLATED')
    V('integer above max violates the constraint', 'integer', 9, {'max': 5}, 'MODEL.CONSTRAINT_VIOLATED')
    V('a safe integer written as a string is not canonical', 'integer', '12', None, 'MODEL.INVALID_VALUE')
    V('a boolean is not an integer', 'integer', True, None, 'MODEL.INVALID_VALUE')
    V('decimal canonical string', 'decimal', '-12.5')
    V('decimal with a declared scale is written with that many digits', 'decimal', '12.5', {'scale': 2})
    V('decimal above max violates the constraint', 'decimal', '100.01', {'max': '100'}, 'MODEL.CONSTRAINT_VIOLATED')
    V('decimal with more digits than precision violates it', 'decimal', '12345.6', {'precision': 5}, 'MODEL.CONSTRAINT_VIOLATED')
    V('money with the currency minor unit', 'money', {'amount': '10.5', 'currency': 'USD'})
    V('money in a zero-decimal currency', 'money', {'amount': '1500', 'currency': 'JPY'})
    V('money in a three-decimal currency', 'money', {'amount': '1.2', 'currency': 'BHD'})
    V('money with an unknown currency code is invalid', 'money', {'amount': '1', 'currency': 'XYZ'}, None, 'MODEL.INVALID_VALUE')
    V('money outside the allowed currencies violates the constraint', 'money', {'amount': '1', 'currency': 'EUR'},
      {'currencies': ['USD']}, 'MODEL.CONSTRAINT_VIOLATED')
    V('money with a numeric amount is invalid', 'money', {'amount': 1, 'currency': 'USD'}, None, 'MODEL.INVALID_VALUE')
    V('quantity with a unit', 'quantity', {'value': '2.50', 'unit': 'kg'})
    V('quantity with a unit outside the allowed units violates the constraint', 'quantity',
      {'value': '1', 'unit': 'lb'}, {'units': ['kg', 'g']}, 'MODEL.CONSTRAINT_VIOLATED')
    V('boolean true', 'boolean', True)
    V('the string "true" is not a boolean', 'boolean', 'true', None, 'MODEL.INVALID_VALUE')
    V('date in the proleptic Gregorian calendar', 'date', '2024-02-29')
    V('February 30 is not a date', 'date', '2026-02-30', None, 'MODEL.INVALID_VALUE')
    V('a date with a time is not a date', 'date', '2026-01-01T00:00:00Z', None, 'MODEL.INVALID_VALUE')
    V('date before min violates the constraint', 'date', '2025-12-31', {'min': '2026-01-01'}, 'MODEL.CONSTRAINT_VIOLATED')
    V('time with seconds', 'time', '23:59:59')
    V('time with microseconds', 'time', '08:00:00.000001')
    V('hour 24 is not a time', 'time', '24:00:00', None, 'MODEL.INVALID_VALUE')
    V('instant in UTC with Z and microseconds', 'instant', '2026-03-01T12:00:00.000000Z')
    V('an instant with an offset is not canonical', 'instant', '2026-03-01T12:00:00.000000+01:00', None, 'MODEL.INVALID_VALUE')
    V('an instant without microseconds is not canonical', 'instant', '2026-03-01T12:00:00Z', None, 'MODEL.INVALID_VALUE')
    V('open period', 'period', {'from': '2026-01-01', 'to': None})
    V('closed half-open period', 'period', {'from': '2026-01-01', 'to': '2026-02-01'})
    V('an empty period is invalid', 'period', {'from': '2026-01-01', 'to': '2026-01-01'}, None, 'MODEL.INVALID_VALUE')
    V('period of instants', 'period', {'from': '2026-01-01T00:00:00.000000Z', 'to': None}, {'granularity': 'instant'})
    V('ISO 8601 duration', 'duration', 'P1Y2M3DT4H5M6S')
    V('duration in weeks', 'duration', 'P2W')
    V('P alone is not a duration', 'duration', 'P', None, 'MODEL.INVALID_VALUE')
    V('a duration ending in T is invalid', 'duration', 'P1DT', None, 'MODEL.INVALID_VALUE')
    V('enum value among the allowed values', 'enum', 'open', {'allowed_values': ['open', 'closed']})
    V('enum value outside the allowed values violates the constraint', 'enum', 'draft',
      {'allowed_values': ['open', 'closed']}, 'MODEL.CONSTRAINT_VIOLATED')
    V('composite with declared members', 'composite', {'street': 'Main St 1', 'zip': '12345'},
      {'members': {'street': {'type': 'text'}, 'zip': {'type': 'text', 'required': True}}})
    V('composite with an undeclared member is invalid', 'composite', {'street': 'x', 'city': 'y'},
      {'members': {'street': {'type': 'text'}}}, 'MODEL.INVALID_VALUE')
    V('composite without a required member violates the constraint', 'composite', {'street': 'x'},
      {'members': {'street': {'type': 'text'}, 'zip': {'type': 'text', 'required': True}}}, 'MODEL.CONSTRAINT_VIOLATED')
    V('list of integers', 'list', [1, 2, 3], {'item_type': {'type': 'integer'}})
    V('list with too many items violates max_items', 'list', [1, 2, 3], {'max_items': 2}, 'MODEL.CONSTRAINT_VIOLATED')
    V('keyed list with unique keys', 'list', [{'sku': 'a', 'qty': 1}, {'sku': 'b', 'qty': 2}], {'key': 'sku'})
    V('keyed list with a duplicate key violates the constraint', 'list', [{'sku': 'a'}, {'sku': 'a'}], {'key': 'sku'},
      'MODEL.CONSTRAINT_VIOLATED')
    V('map with typed values', 'map', {'en': 'Invoice', 'de': 'Rechnung'}, {'value_type': {'type': 'text'}})
    V('map key outside the key pattern violates the constraint', 'map', {'EN': 'x'}, {'key_pattern': '[a-z]{2}'},
      'MODEL.CONSTRAINT_VIOLATED')
    V('blob descriptor', 'blob', {'content_id': 'sha256:' + hashlib.sha256(b'pdf').hexdigest(), 'size': 3,
                                  'media_type': 'application/pdf', 'name': 'a.pdf'})
    V('blob larger than max_size violates the constraint', 'blob',
      {'content_id': 'sha256:' + hashlib.sha256(b'pdf').hexdigest(), 'size': 3000, 'media_type': 'application/pdf',
       'name': 'a.pdf'}, {'max_size': 1024}, 'MODEL.CONSTRAINT_VIOLATED')
    V('json value of any shape', 'json', {'a': [1, 'b', None]})
    V('json value larger than max_size violates the constraint', 'json', {'a': 'x' * 20}, {'max_size': 10},
      'MODEL.CONSTRAINT_VIOLATED')
    V('an unknown value type is rejected', 'float', 1, None, 'MODEL.UNKNOWN_TYPE')
    V('a type name in another case is unknown', 'Text', 'x', None, 'MODEL.UNKNOWN_TYPE')
    # STD-FND-021 absent versus null
    base_env = {'object_id': '01890a5d-ac96-774b-bcce-b302099a8057', 'class': 'Customer',
                'class_version': 'sha256:' + hashlib.sha256(b'Customer').hexdigest(), 'schema_version': 1,
                'kind': 'definition', 'slug': 'c-1', 'valid': {'from': '2026-01-01', 'to': None}}
    P('STD-FND-021', 'a body with a member set to null (explicitly cleared)', 'version_hash',
      {'envelope': base_env, 'body': {'name': 'Acme', 'email': None}})
    P('STD-FND-021', 'the same body with the member absent (not set) hashes differently', 'version_hash',
      {'envelope': base_env, 'body': {'name': 'Acme'}})
    P('STD-FND-021', 'null is accepted for any value type as "cleared"', 'validate_value', {'type': 'date', 'value': None})
    P('STD-FND-021', 'canonical JSON keeps null members', 'canonical_json', {'value': {'a': None, 'b': 1}})
    # STD-FND-022 decimals
    D = lambda desc, op, a, b=None, scale=None, rounding=None, err=None: P(
        'STD-FND-022', desc, 'decimal', {k: v for k, v in (('op', op), ('a', a), ('b', b), ('scale', scale),
                                                           ('rounding', rounding)) if v is not None}, error=err)
    D('canonical form drops trailing fraction zeros', 'canonical', '12.500')
    D('canonical form of an integer has no point', 'canonical', '12.0')
    D('negative zero is written as 0', 'canonical', '-0.00')
    D('with scale 2 the canonical form has two digits', 'canonical', '12.5', scale=2)
    D('more fraction digits than the scale is not canonical', 'canonical', '1.234', scale=2, err='MODEL.INVALID_VALUE')
    D('a leading plus sign is invalid', 'canonical', '+1', err='MODEL.INVALID_VALUE')
    D('leading zeros are invalid', 'canonical', '007', err='MODEL.INVALID_VALUE')
    D('an exponent is invalid', 'canonical', '1E3', err='MODEL.INVALID_VALUE')
    D('a point without digits is invalid', 'canonical', '1.', err='MODEL.INVALID_VALUE')
    D('addition is exact', 'add', '0.1', '0.2')
    D('subtraction is exact', 'sub', '1', '0.999999999999999999999')
    D('multiplication is exact', 'mul', '1.25', '1.25')
    D('addition of large values is exact', 'add', '99999999999999999999.99', '0.01')
    D('division rounds half_even to the scale', 'div', '1', '3', 4, 'half_even')
    D('division without a scale is rejected at type-check time', 'div', '1', '3', err='LOGIC.TYPE_ERROR')
    D('division by zero', 'div', '1', '0', 2, 'half_even', err='LOGIC.DIVISION_BY_ZERO')
    for v, mode in [('2.5', 'half_even'), ('3.5', 'half_even'), ('-2.5', 'half_even'), ('2.5', 'half_up'),
                    ('-2.5', 'half_up'), ('2.1', 'up'), ('-2.1', 'up'), ('2.9', 'down'), ('-2.9', 'down'),
                    ('2.1', 'ceiling'), ('-2.1', 'ceiling'), ('2.9', 'floor'), ('-2.9', 'floor')]:
        D('round %s to scale 0 with %s' % (v, mode), 'round', v, scale=0, rounding=mode)
    D('round to scale 2 with half_even', 'round', '1.005', scale=2, rounding='half_even')
    D('round to scale 2 with half_up', 'round', '1.005', scale=2, rounding='half_up')
    D('rounding a small negative value to zero gives 0', 'round', '-0.004', scale=2, rounding='half_up')
    D('an unknown rounding mode is invalid', 'round', '1.5', scale=0, rounding='nearest', err='MODEL.INVALID_VALUE')
    P('STD-FND-022', 'assignment to a field with scale rounds with the field rounding mode', 'validate_value',
      {'type': 'decimal', 'constraints': {'scale': 2, 'rounding': 'half_up'}, 'value': '2.345'})
    # STD-FND-023 money
    M = lambda a, c: {'amount': a, 'currency': c}
    P('STD-FND-023', 'adding money of one currency', 'money', {'op': 'add', 'a': M('10.25', 'USD'), 'b': M('0.75', 'USD')})
    P('STD-FND-023', 'subtracting money of one currency', 'money', {'op': 'sub', 'a': M('10', 'EUR'), 'b': M('12.5', 'EUR')})
    P('STD-FND-023', 'adding money of different currencies fails', 'money',
      {'op': 'add', 'a': M('1', 'USD'), 'b': M('1', 'EUR')}, error='MODEL.CURRENCY_MISMATCH')
    P('STD-FND-023', 'subtracting money of different currencies fails', 'money',
      {'op': 'sub', 'a': M('1', 'JPY'), 'b': M('1', 'USD')}, error='MODEL.CURRENCY_MISMATCH')
    P('STD-FND-023', 'yen amounts have no minor unit', 'money', {'op': 'add', 'a': M('100', 'JPY'), 'b': M('1', 'JPY')})
    P('STD-FND-023', 'the default scale of a currency is its minor unit', 'validate_value',
      {'type': 'money', 'value': M('7', 'KWD')})
    P('STD-FND-023', 'an amount with more digits than the minor unit is rounded half_even', 'validate_value',
      {'type': 'money', 'value': M('1.005', 'USD')})
    P('STD-FND-023', 'an explicit scale overrides the minor unit', 'validate_value',
      {'type': 'money', 'constraints': {'scale': 4}, 'value': M('1.23456', 'USD')})
    # STD-FND-024 time values
    det = {'tzdb': g.tzdb}
    for desc, inp in [
        ('a local time in New York in winter', ('2026-01-15', '09:30:00', 'America/New_York', None)),
        ('a local time in New York in summer', ('2026-07-15', '09:30:00', 'America/New_York', None)),
        ('a local time in Berlin', ('2026-03-01', '00:00:00', 'Europe/Berlin', None)),
        ('a local time in Tokyo (no DST)', ('2026-06-01', '12:00:00', 'Asia/Tokyo', None)),
        ('a local time in Kolkata (half-hour offset)', ('2026-06-01', '12:00:00', 'Asia/Kolkata', None)),
        ('a local time with microseconds', ('2026-06-01', '12:00:00.250000', 'UTC', None)),
        ('a non-existent local time resolves to the end of the gap (New York)', ('2024-03-10', '02:30:00', 'America/New_York', None)),
        ('a non-existent local time resolves to the end of the gap (Berlin)', ('2024-03-31', '02:15:00', 'Europe/Berlin', None)),
        ('a repeated local time resolves to the earlier instant', ('2024-11-03', '01:30:00', 'America/New_York', None)),
        ('a repeated local time with disambiguation later', ('2024-11-03', '01:30:00', 'America/New_York', 'later')),
        ('a repeated local time in Berlin resolves to the earlier instant', ('2024-10-27', '02:30:00', 'Europe/Berlin', None)),
        ('midnight in Auckland crosses the date line', ('2026-01-01', '00:00:00', 'Pacific/Auckland', None))]:
        date, time, tz, dis = inp
        i = {'date': date, 'time': time, 'tz': tz}
        if dis:
            i['disambiguation'] = dis
        P('STD-FND-024', desc, 'local_to_instant', i, determinism=det)
    P('STD-FND-024', 'an unknown time zone is rejected', 'local_to_instant',
      {'date': '2026-01-01', 'time': '00:00:00', 'tz': 'Mars/Olympus'}, error='MODEL.INVALID_VALUE', determinism=det)
    P('STD-FND-024', 'a date carries no time zone', 'validate_value', {'type': 'date', 'value': '2026-01-01+01:00'},
      error='MODEL.INVALID_VALUE')
    # STD-FND-025 documents
    doc = {'kind': 'section', 'children': [
        {'kind': 'heading', 'attrs': {'level': 1}, 'children': [{'kind': 'text', 'text': 'Terms'}]},
        {'kind': 'paragraph', 'children': [{'kind': 'text', 'text': 'Pay within ', 'marks': []},
                                           {'kind': 'text', 'text': '30 days', 'marks': ['bold']}]}]}
    P('STD-FND-025', 'a document tree of blocks with marked text', 'validate_value', {'type': 'document', 'value': doc})
    P('STD-FND-025', 'a table with rows and cells', 'validate_value', {'type': 'document', 'value': {
        'kind': 'table', 'children': [{'kind': 'row', 'children': [{'kind': 'cell', 'children': [
            {'kind': 'text', 'text': 'a'}]}]}]}})
    P('STD-FND-025', 'a clause reference and a binding', 'validate_value', {'type': 'document', 'value': {
        'kind': 'paragraph', 'children': [{'kind': 'clause_ref', 'attrs': {'clause': 'x'}},
                                          {'kind': 'binding', 'attrs': {'path': '/amount'}}]}})
    P('STD-FND-025', 'an unknown block kind is rejected', 'validate_value',
      {'type': 'document', 'value': {'kind': 'div'}}, error='MODEL.INVALID_VALUE')
    P('STD-FND-025', 'an unknown inline mark is rejected', 'validate_value',
      {'type': 'document', 'value': {'kind': 'text', 'text': 'x', 'marks': ['blink']}}, error='MODEL.INVALID_VALUE')
    P('STD-FND-025', 'a text leaf has no children', 'validate_value',
      {'type': 'document', 'value': {'kind': 'text', 'text': 'x', 'children': []}}, error='MODEL.INVALID_VALUE')
    P('STD-FND-025', 'a node without a kind is rejected', 'validate_value',
      {'type': 'document', 'value': {'text': 'x'}}, error='MODEL.INVALID_VALUE')
    P('STD-FND-025', 'text in documents is normalised to NFC', 'validate_value',
      {'type': 'document', 'value': {'kind': 'text', 'text': 'Café'}})
    # STD-FND-027 classification
    for clear, level, desc in [
        (['confidential'], 'internal', 'a confidential clearance covers internal'),
        (['confidential'], 'public', 'a confidential clearance covers public'),
        (['internal'], 'confidential', 'an internal clearance does not cover confidential'),
        (['sensitive_personal'], 'personal', 'personal must be granted explicitly, even with sensitive_personal'),
        (['personal'], 'personal', 'an explicit personal clearance covers personal'),
        (['personal'], 'sensitive_personal', 'sensitive_personal must be granted explicitly'),
        (['personal'], 'confidential', 'a personal clearance covers the lower non-personal levels')]:
        P('STD-FND-027', desc, 'classification_covers', {'clearances': clear, 'level': level})
    P('STD-FND-027', 'an unknown classification level is rejected', 'classification_covers',
      {'clearances': ['secret'], 'level': 'public'}, error='MODEL.INVALID_VALUE')
