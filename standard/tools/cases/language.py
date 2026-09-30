"""Vector cases of BPA 1.0-rc1 for L1 expressions, queries, addressing and custom value types
(PH-1 step 1e-1, DEC-043): STD-EXPR-001…005, STD-EXPR-011, STD-ADDR-001, -002, -010 and
STD-FND-026. Not required of the PH-0 prototype."""
from bpa import crypto

from .common import CLASSES, NOW, cls, field, uid, commit, create, base

NOWI = '2026-03-15T10:00:00.000000Z'
CTX = {'now': NOWI, 'tenant': 'acme', 'branch': 'main', 'locale': 'en-US', 'time_zone': 'America/New_York'}
INVOICE = cls('Invoice', [field('number', 'text', True), field('total', 'money'), field('due', 'date'),
                          field('status', 'enum', constraints={'allowed_values': ['open', 'paid']}),
                          field('lines', 'list', constraints={'item_type': {'type': 'decimal'}}),
                          field('customer', 'reference', constraints={'target_class': 'Customer'}),
                          field('note', 'text'), field('issued', 'instant')])
THIS = {'number': 'INV-7', 'total': {'amount': '1200.00', 'currency': 'USD'}, 'due': '2026-04-30', 'status': 'open',
        'lines': ['500', '700.00'], 'issued': '2026-03-01T09:30:00.000000Z'}
CAL = {'us': {'weekend': [6, 7], 'holidays': ['2026-04-03', '2026-12-25']}}
RATES = {'usd-eur-2026-03-15': {'from': 'USD', 'to': 'EUR', 'rate': '0.92'}}
ISIN = {'name': 'Isin', 'base': 'text', 'normalize': 'upper(trim(value))', 'validate': 'isin_valid(value)',
        'constraints': {'min_length': 12, 'max_length': 12}, 'description': 'ISO 6166 security identifier'}


def cases(g):
    V = g.vector

    def E(clause, desc, e, error=None, this=True, **kw):
        inp = {'expr': e, 'ctx': CTX, 'calendars': CAL, 'rates': RATES}
        if this:
            inp.update(classes=[INVOICE] + CLASSES, this=this if isinstance(this, dict) else THIS,
                       **{'class': 'Invoice'})
        inp.update(kw)
        g.pure(clause, desc, 'eval_expr', inp, error=error, prototype=False)

    def T(clause, desc, e, error=None, **kw):
        inp = {'expr': e, 'classes': [INVOICE] + CLASSES, 'class': 'Invoice'}
        inp.update(kw)
        g.pure(clause, desc, 'check_expr', inp, error=error, prototype=False)

    def P(clause, desc, op, inp, error=None):
        g.pure(clause, desc, op, inp, error=error, prototype=False)

    def S(clause, desc, steps, classes=None, value_types=None):
        gen = {'classes': classes or CLASSES}
        if value_types is not None:
            gen['value_types'] = value_types
        V(clause, desc, setup=[{'op': 'set_now', 'input': {'now': NOW}}, {'op': 'load_genesis', 'input': gen}],
          steps=steps, prototype=False)

    # ---- STD-EXPR-001 grammar ----
    E('STD-EXPR-001', 'integer and decimal literals; multiplication binds tighter than addition', '1 + 2 * 3.5',
      this=False)
    E('STD-EXPR-001', 'parentheses group', '(1 + 2) * 3', this=False)
    E('STD-EXPR-001', 'whitespace between tokens is insignificant', ' ( 1+2 )*\n3 ', this=False)
    E('STD-EXPR-001', 'string literals with escapes are NFC text', '"Jos\\u0065\\u0301 \\"B\\""', this=False)
    E('STD-EXPR-001', 'date, instant and money literals', '[d\'2026-03-15\', t\'2026-03-15T10:00:00.000000Z\', 12.5EUR]',
      this=False)
    E('STD-EXPR-001', 'a conditional expression', 'this.status == "open" ? this.total : 0.00USD')
    E('STD-EXPR-001', 'member paths from this', 'this.total.currency')
    E('STD-EXPR-001', 'null-safe member access', '[params.addr?.city, params.home?.city]',
      params_types={'addr': {'type': 'composite', 'constraints': {'members': [
          {'name': 'city', 'type': 'text', 'label': 'city', 'description': 'city'}]}}} | {'home': {'type': 'composite', 'constraints': {'members': [
          {'name': 'city', 'type': 'text', 'label': 'city', 'description': 'city'}]}}},
      params={'home': {'city': 'Boston'}})
    E('STD-EXPR-001', 'index into a list', 'this.lines[1]')
    E('STD-EXPR-001', 'in a list literal', 'this.status in ["open", "overdue"]')
    E('STD-EXPR-001', 'in a path', '500 in this.lines')
    E('STD-EXPR-001', 'is null and is not null', '[this.note is null, this.number is not null]')
    E('STD-EXPR-001', 'matches a pattern over the whole text', '[this.number matches "INV-[0-9]+", this.number matches "INV"]')
    E('STD-EXPR-001', 'a collection function with a lambda', 'any(this.lines, x -> x > 600)')
    E('STD-EXPR-001', 'params and ctx are paths', 'params.days + 1 > 0 and ctx.tenant == "acme"',
      params_types={'days': 'integer'}, params={'days': 3})
    E('STD-EXPR-001', 'old is the base version in commit-time rules', 'this.status != old.status',
      old=dict(THIS, status='paid'), with_old=True)
    E('STD-EXPR-001', 'unbalanced parentheses are a syntax error', '(1 + 2', error='LOGIC.TYPE_ERROR', this=False)
    E('STD-EXPR-001', 'an unknown character is a syntax error', '1 # 2', error='LOGIC.TYPE_ERROR', this=False)
    E('STD-EXPR-001', 'not applies once', 'not not true', error='LOGIC.TYPE_ERROR', this=False)
    E('STD-EXPR-001', 'in takes a list literal or a path', '1 in 1 + 2', error='LOGIC.TYPE_ERROR', this=False)
    E('STD-EXPR-001', 'a money literal names an ISO 4217 currency', '1.00XYZ', error='LOGIC.TYPE_ERROR', this=False)
    E('STD-EXPR-001', 'a date literal is a calendar date', "d'2026-02-30'", error='LOGIC.TYPE_ERROR', this=False)
    E('STD-EXPR-001', 'sort_by takes a lambda', 'sort_by(this.lines, x -> -x)')

    # ---- STD-EXPR-002 typing ----
    T('STD-EXPR-002', 'integer arithmetic stays integer', '1 + 2 * 3')
    T('STD-EXPR-002', 'integer and decimal promote to decimal', '1 + 2.5')
    T('STD-EXPR-002', 'money plus money is money', 'this.total + 1.00USD')
    T('STD-EXPR-002', 'money times decimal is money', 'this.total * 0.1')
    T('STD-EXPR-002', 'date minus date is an integer', "this.due - d'2026-03-15'")
    T('STD-EXPR-002', 'instant minus instant is a duration', "t'2026-03-15T10:00:00.000000Z' - this.issued")
    T('STD-EXPR-002', 'comparisons are boolean', 'this.total > 1000.00USD')
    T('STD-EXPR-002', 'enum fields are text', 'this.status')
    T('STD-EXPR-002', 'list fields have typed items', 'this.lines')
    T('STD-EXPR-002', 'money plus a number is a type error with its position', 'this.total + 1',
      error='LOGIC.TYPE_ERROR')
    V('STD-EXPR-002', 'the type error reports the position of the operator', prototype=False, steps=[
        {'op': 'check_expr', 'input': {'expr': 'this.total + 1', 'classes': [INVOICE] + CLASSES, 'class': 'Invoice'},
         'error': 'LOGIC.TYPE_ERROR', 'details': True}])
    T('STD-EXPR-002', 'text and numbers do not compare', 'this.number < 5', error='LOGIC.TYPE_ERROR')
    T('STD-EXPR-002', 'booleans are not ordered', 'true < false', error='LOGIC.TYPE_ERROR')
    T('STD-EXPR-002', 'an unknown member is a type error', 'this.amount', error='LOGIC.TYPE_ERROR')
    T('STD-EXPR-002', 'an unknown function is a type error', 'sqrt(4)', error='LOGIC.TYPE_ERROR')
    T('STD-EXPR-002', 'a condition is boolean', 'this.number ? 1 : 2', error='LOGIC.TYPE_ERROR')
    T('STD-EXPR-002', 'branches of a condition have compatible types', 'true ? 1 : "one"', error='LOGIC.TYPE_ERROR')
    T('STD-EXPR-002', 'function arguments are typed', 'upper(this.total)', error='LOGIC.TYPE_ERROR')
    T('STD-EXPR-002', 'and needs boolean operands', 'this.number and true', error='LOGIC.TYPE_ERROR')
    E('STD-EXPR-002', 'money of different currencies fails at run time', 'this.total + 1.00EUR',
      error='MODEL.CURRENCY_MISMATCH')
    T('STD-EXPR-002', 'ctx members are typed', 'ctx.now')

    # ---- STD-EXPR-003 null semantics ----
    E('STD-EXPR-003', 'comparisons with null are null', '[this.note == "x", null == null, this.note != "x"]')
    E('STD-EXPR-003', 'and follows the SQL truth table', '[null and true, null and false, false and null, null and null]',
      this=False)
    E('STD-EXPR-003', 'or follows the SQL truth table', '[null or true, null or false, true or null, null or null]',
      this=False)
    E('STD-EXPR-003', 'not null is null', 'not (this.note == "x")')
    E('STD-EXPR-003', 'arithmetic with null is null', '[1 + null, null * 2.5, -(null)]', this=False)
    E('STD-EXPR-003', '?. returns null for a null receiver', 'params.addr?.city', params_types={'addr': {'type': 'composite', 'constraints': {'members': [
          {'name': 'city', 'type': 'text', 'label': 'city', 'description': 'city'}]}}}, params={})
    E('STD-EXPR-003', '. on a null receiver is an error', 'params.addr.city', params_types={'addr': {'type': 'composite', 'constraints': {'members': [
          {'name': 'city', 'type': 'text', 'label': 'city', 'description': 'city'}]}}}, params={},
      error='LOGIC.TYPE_ERROR')
    E('STD-EXPR-003', 'member access on null through a map needs ?.', 'ref("ubos://acme/Customer/none").name',
      error='IAM.NOT_FOUND')
    E('STD-EXPR-003', 'a condition that is null takes the else branch', 'this.note == "x" ? 1 : 2')
    E('STD-EXPR-003', 'in with a null value is null', 'this.note in ["a", "b"]')
    E('STD-EXPR-003', 'is null tests explicitly', 'this.note is null')
    E('STD-EXPR-003', 'functions of null are null', '[upper(this.note), len(this.note)]')
    E('STD-EXPR-003', 'aggregates skip nulls', 'sum([1, null, 2])', this=False)
    E('STD-EXPR-003', 'any is null when no item is true and one is null', '[any([false, null]), any([true, null]), '
      'all([true, null]), all([false, null])]', this=False)
    E('STD-EXPR-003', 'a map member of null is null with ?.', 'params.extra?.code',
      params_types={'extra': 'json'}, params={})
    P('STD-EXPR-003', 'a validation that is null is not violated', 'validate_value',
      {'type': 'Code', 'value': 'abc', 'value_types': [{'name': 'Code', 'base': 'text', 'description': 'code',
                                                        'validate': 'value == null'}]})

    # ---- STD-EXPR-004 totality and bounds ----
    E('STD-EXPR-004', 'integer division by zero fails', '7 % 0', error='LOGIC.DIVISION_BY_ZERO', this=False)
    E('STD-EXPR-004', 'decimal division by zero fails', 'div(1, 0, 2, "half_even")', error='LOGIC.DIVISION_BY_ZERO',
      this=False)
    E('STD-EXPR-004', 'division uses the scale and rounding of the target field', '10 / 3', this=False,
      target={'type': 'decimal', 'constraints': {'scale': 2, 'rounding': 'up'}})
    E('STD-EXPR-004', 'division without a target scale is rejected at type check', '10 / 3',
      error='LOGIC.TYPE_ERROR', this=False)
    E('STD-EXPR-004', 'the explicit div function names scale and rounding', 'div(2, 3, 4, "half_up")', this=False)
    E('STD-EXPR-004', 'evaluation is charged to the operation budget', 'sum(this.lines, x -> x * 2 + 1)',
      error='LOGIC.LIMIT_EXCEEDED', max_units=5)
    E('STD-EXPR-004', 'a small expression fits a small budget', '1 + 2', this=False, max_units=50)
    E('STD-EXPR-004', 'collections are bounded by max_items', 'count([1, 2, 3, 4])', error='LOGIC.LIMIT_EXCEEDED',
      this=False, max_items=3)
    E('STD-EXPR-004', 'there are no user-defined functions', 'f(1)', error='LOGIC.TYPE_ERROR', this=False)
    E('STD-EXPR-004', 'a lambda variable does not hide a root', 'map(this.lines, this -> this)',
      error='LOGIC.TYPE_ERROR')
    E('STD-EXPR-004', 'the remainder has the sign of the dividend', '[7 % 3, -7 % 3, 7 % -3]', this=False)

    # ---- STD-EXPR-005 function library ----
    lib = [
        ('text length counts code points', 'len("Jos\\u00e9")'),
        ('lower and upper', '[lower("ÄBC"), upper("straße")]'),
        ('trim removes surrounding white space', 'trim("  a b \\n")'),
        ('substr from a zero-based start with a length', '[substr("invoice", 2), substr("invoice", 2, 3), substr("ab", 5)]'),
        ('concat joins texts', 'concat("INV", "-", this.number)'),
        ('contains, starts_with and ends_with', '[contains(this.number, "V-"), starts_with(this.number, "INV"), '
                                                'ends_with(this.number, "8")]'),
        ('replace replaces every occurrence', 'replace("a-b-c", "-", "/")'),
        ('pad_left pads to a width', '[pad_left("42", 5, "0"), pad_left("123456", 5, "0")]'),
        ('format substitutes numbered placeholders', 'format("{0} of {1} due {2}", this.number, this.total, this.due)'),
        ('abs of numbers and money', '[abs(-2.5), abs(-3), abs(-12.34USD)]'),
        ('round with a scale and a mode', '[round(2.345, 2), round(2.345, 2, "half_up"), round(-2.5, 0), '
                                          'round(2.5, 0, "half_up")]'),
        ('round of money keeps the currency', 'round(10.555EUR, 1, "down")'),
        ('floor and ceil give integers', '[floor(2.7), floor(-2.1), ceil(2.1), ceil(-2.7)]'),
        ('min, max and clamp', '[min(3, 2.5), max(d\'2026-01-01\', d\'2026-02-01\'), clamp(15, 0, 10), clamp(-1, 0, 10)]'),
        ('amount, currency and money', '[amount(this.total), currency(this.total), money(12.3, "JPY")]'),
        ('convert applies a rate object', 'convert(this.total, "EUR", "usd-eur-2026-03-15")'),
        ('allocate splits money by weights without losing cents', 'allocate(100.00USD, [1, 1, 1])'),
        ('allocate by uneven weights', 'allocate(10.00EUR, [3, 7, 0])'),
        ('today is the business date of ctx.now', 'today()'),
        ('now is ctx.now', 'now()'),
        ('date parts and weekday (ISO, Monday = 1)', '[year(this.due), month(this.due), day(this.due), weekday(this.due)]'),
        ('add_days and days_between', '[add_days(this.due, 5), days_between(d\'2026-03-15\', this.due)]'),
        ('add_months clamps to the end of the month', '[add_months(d\'2026-01-31\', 1), add_months(d\'2024-01-31\', 1), '
                                                      'add_months(d\'2026-03-31\', -13)]'),
        ('start and end of month', '[start_of_month(this.due), end_of_month(d\'2024-02-10\')]'),
        ('to_zone shows an instant in a time zone', 'to_zone(this.issued, "America/New_York")'),
        ('business days skip weekends and holidays', '[is_business_day(d\'2026-04-03\', "us"), '
                                                     'is_business_day(d\'2026-04-06\', "us")]'),
        ('add_business_days counts business days', '[add_business_days(d\'2026-04-02\', 2, "us"), '
                                                   'add_business_days(d\'2026-04-07\', -2, "us")]'),
        ('next and previous business day', '[next_business_day(d\'2026-04-02\', "us"), '
                                           'previous_business_day(d\'2026-04-06\', "us")]'),
        ('cutoff falls on the next business day', 'cutoff(d\'2026-04-03\', "17:00:00", "America/New_York", "us")'),
        ('periods: contains and overlaps', '[contains(period(d\'2026-01-01\', d\'2026-04-01\'), d\'2026-03-31\'), '
                                           'contains(period(d\'2026-01-01\', d\'2026-04-01\'), d\'2026-04-01\'), '
                                           'overlaps(period(d\'2026-01-01\', d\'2026-04-01\'), period(d\'2026-03-01\', null))]'),
        ('collections: count, sum, min, max and first', '[count(this.lines), sum(this.lines), min(this.lines), '
                                                         'max(this.lines), first(this.lines)]'),
        ('collections with lambdas', '[count(this.lines, x -> x > 600), sum(this.lines, x -> x * 2), '
                                     'first(this.lines, x -> x > 600), min(this.lines, x -> -x)]'),
        ('filter and map', '[filter(this.lines, x -> x > 600), map(this.lines, x -> x + 1)]'),
        ('distinct keeps the first occurrence', 'distinct([3, 1, 3, 2, 1])'),
        ('sort_by is stable and puts nulls first', 'sort_by(["bb", "a", "ccc", null], x -> len(x))'),
        ('all and any over a lambda', '[all(this.lines, x -> x > 100), any(this.lines, x -> x > 1000)]'),
        ('Luhn check digits', '[luhn_valid("79927398713"), luhn_valid("79927398710")]'),
        ('ISO 7064 MOD 97-10', '[iso7064_mod97_10("5493001KJTIIGC8Y1R12"), iso7064_mod97_10("5493001KJTIIGC8Y1R13")]'),
        ('ISIN check digit', '[isin_valid("US0378331005"), isin_valid("US0378331006"), isin_valid("us0378331005")]'),
        ('LEI check digits', '[lei_valid("5493001KJTIIGC8Y1R12"), lei_valid("5493001KJTIIGC8Y1R1")]'),
        ('IBAN check digits', '[iban_valid("GB82WEST12345698765432"), iban_valid("GB82WEST12345698765431"), '
                              'iban_valid("DE89370400440532013000")]'),
    ]
    for desc, e in lib:
        E('STD-EXPR-005', desc, e)
    E('STD-EXPR-005', 'convert rejects a rate for other currencies', 'convert(1.00EUR, "USD", "usd-eur-2026-03-15")',
      error='MODEL.CURRENCY_MISMATCH', this=False)
    E('STD-EXPR-005', 'convert needs a known rate', 'convert(1.00USD, "EUR", "none")',
      error='MODEL.REFERENCE_INVALID', this=False)
    E('STD-EXPR-005', 'an unknown calendar is rejected', 'is_business_day(d\'2026-04-03\', "cn")',
      error='MODEL.REFERENCE_INVALID', this=False)
    E('STD-EXPR-005', 'date() rejects an impossible date', 'date(2026, 2, 29)', error='MODEL.INVALID_VALUE',
      this=False)
    E('STD-EXPR-005', 'a period runs forward', "period(d'2026-04-01', d'2026-01-01')", error='MODEL.INVALID_VALUE',
      this=False)
    E('STD-EXPR-005', 'ref reads a referenced object and exists tests one', '[ref(this.customer).name, '
      'exists(this.customer), exists("ubos://acme/Customer/none")]',
      this=dict(THIS, customer='ubos://acme/Customer/c-1'),
      objects={'ubos://acme/Customer/c-1': {'name': 'Acme'}})
    E('STD-EXPR-005', 'instant differences are durations', '[this.issued - t\'2026-02-28T08:00:00.000000Z\', '
      't\'2026-03-01T09:30:00.000000Z\' - this.issued]')

    # ---- STD-EXPR-011 query evaluation ----
    cust = [create(uid(1), 'Customer', 'c-1', {'name': 'Acme', 'country': 'US'}),
            create(uid(2), 'Customer', 'c-2', {'name': 'Beta', 'country': 'CA'}),
            create(uid(3), 'Customer', 'c-3', {'name': 'Gamma'})]
    vip = cls('VipCustomer', [field('tier', 'text')], parent='Customer')
    order = cls('SalesOrder', [field('customer', 'reference', constraints={'target_class': 'Customer'}),
                               field('amount', 'decimal')])
    QC = CLASSES + [vip, order]
    q = lambda label=None, error=None, **kw: dict({'op': 'query', 'input': dict({'branch': 'main', 'from': 'Customer'},
                                                                              **kw)},
                                                  **({'label': label} if label else {}),
                                                  **({'error': error} if error else {}))
    S('STD-EXPR-011', 'where filters on the fields of this', [commit(cust), q(where='this.country == "US"')])
    S('STD-EXPR-011', 'a where that is null excludes the object', [commit(cust), q(where='this.country != "US"')])
    S('STD-EXPR-011', 'a class includes its subclasses unless only is given', [
        commit(cust + [create(uid(4), 'VipCustomer', 'v-1', {'name': 'Delta', 'tier': 'gold'})]),
        q(), q(only=True)], classes=QC)
    S('STD-EXPR-011', 'order by a path, descending, with the object id as tie-breaker', [
        commit(cust + [create(uid(4), 'Customer', 'c-4', {'name': 'Acme'})]),
        q(order_by=[{'path': 'this.name', 'dir': 'desc'}]), q(order_by=[{'path': 'this.country'}])])
    S('STD-EXPR-011', 'pages of a query read the snapshot bound into the cursor', [
        commit(cust), q('p1', limit=2, order_by=[{'path': 'this.name'}]),
        commit([create(uid(5), 'Customer', 'c-0', {'name': 'Aardvark'})]),
        q(limit=2, order_by=[{'path': 'this.name'}], cursor={'$ref': 'p1/cursor'}),
        q(limit=2, order_by=[{'path': 'this.name'}])])
    S('STD-EXPR-011', 'a cursor belongs to its query', [
        commit(cust), q('p1', limit=2),
        q(error='QRY.INVALID_QUERY', limit=2, where='true', cursor={'$ref': 'p1/cursor'})])
    S('STD-EXPR-011', 'paths through references resolve at the query coordinates', [
        commit([create(uid(1), 'Customer', 'c-1', {'name': 'Acme'}, valid={'from': '2026-01-01'})], label='m'),
        commit([create(uid(6), 'SalesOrder', 'o-1', {'customer': 'ubos://acme/Customer/c-1', 'amount': '10'},
                       valid={'from': '2026-01-01'})]),
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('m', uid(1)), 'body': {'name': 'Acme Corp'},
                 'valid': {'kind': 'supersede_from', 'from': '2026-06-01'}}]),
        q(**{'from': 'SalesOrder'}, where='ref(this.customer).name == "Acme"'),
        q(**{'from': 'SalesOrder'}, where='ref(this.customer).name == "Acme"', asof='2026-07-01')], classes=QC)
    S('STD-EXPR-011', 'a query at an earlier commit sees that snapshot', [
        commit(cust[:1]), commit(cust[1:]), q(time=1), q()])
    S('STD-EXPR-011', 'where is a boolean expression', [commit(cust), q(error='LOGIC.TYPE_ERROR', where='this.name')])
    S('STD-EXPR-011', 'the page size is at most 1000', [commit(cust), q(error='QRY.INVALID_QUERY', limit=1001)])
    S('STD-EXPR-011', 'a query names a known class', [commit(cust), q(error='QRY.INVALID_QUERY', **{'from': 'Vendor'})])

    # ---- STD-ADDR-001 canonical URI ----
    for desc, uri in [('a canonical URI', 'ubos://acme/Customer/c-1'),
                      ('an authority with a VAE', 'ubos://acme.eu/Customer/c-1'),
                      ('selectors and a version suffix', 'ubos://acme/Customer/c-1?branch=feature/x&asof=2026-03-01@v2'),
                      ('a time and an asof selector', 'ubos://acme/Customer/c-1?time=2026-03-15T10:00:00.000000Z&asof=2026-03-15T10:00:00.000000Z')]:
        P('STD-ADDR-001', 'parse %s' % desc, 'parse_uri', {'uri': uri})
    for desc, uri in [('a missing scheme', 'acme/Customer/c-1'), ('a class that is not a class name', 'ubos://acme/customer/c-1'),
                      ('a slug with capitals', 'ubos://acme/Customer/C-1'), ('an unknown selector', 'ubos://acme/Customer/c-1?color=red'),
                      ('a version zero', 'ubos://acme/Customer/c-1@v0'), ('an authority with three parts', 'ubos://a.b.c/Customer/c-1')]:
        P('STD-ADDR-001', 'reject %s' % desc, 'parse_uri', {'uri': uri}, error='MODEL.INVALID_VALUE')
    P('STD-ADDR-001', 'stored references carry no selectors', 'validate_value',
      {'type': 'reference', 'value': 'ubos://acme/Customer/c-1?branch=main'}, error='MODEL.INVALID_VALUE')
    ren = [commit([create(uid(1), 'Customer', 'c-1', {'name': 'Acme'})], label='m'),
           commit([{'op': 'update', 'object_id': uid(1), 'base': base('m', uid(1)), 'slug': 'acme',
                    'body': {'email': 'a@x.com'}}], label='u')]
    S('STD-ADDR-001', 'a URI resolves the object by class and current slug', ren + [
        {'op': 'resolve', 'input': {'uri': 'ubos://acme/Customer/acme'}}])
    S('STD-ADDR-001', 'readers follow slug redirects', ren + [{'op': 'resolve', 'input': {'uri': 'ubos://acme/Customer/c-1'}}])
    S('STD-ADDR-001', 'a version suffix addresses the n-th version on the branch line', ren + [
        {'op': 'resolve', 'input': {'uri': 'ubos://acme/Customer/acme@v1'}},
        {'op': 'resolve', 'input': {'uri': 'ubos://acme/Customer/acme@v2'}},
        {'op': 'resolve', 'input': {'uri': 'ubos://acme/Customer/acme@v3'}, 'error': 'IAM.NOT_FOUND'}])
    S('STD-ADDR-001', 'another tenant is not visible', ren + [
        {'op': 'resolve', 'input': {'uri': 'ubos://globex/Customer/acme'}, 'error': 'IAM.NOT_FOUND'}])
    S('STD-ADDR-001', 'the class is the class of the object', ren + [
        {'op': 'resolve', 'input': {'uri': 'ubos://acme/DataObject/acme'}, 'error': 'IAM.NOT_FOUND'}])

    # ---- STD-ADDR-002 selector precedence ----
    P('STD-ADDR-002', 'commit and tag together conflict', 'parse_uri',
      {'uri': 'ubos://acme/Customer/c-1?tag=q1&commit=sha256:' + '0' * 64}, error='ADDR.CONFLICTING_SELECTORS')
    P('STD-ADDR-002', 'tag and time together conflict', 'parse_uri',
      {'uri': 'ubos://acme/Customer/c-1?tag=q1&time=2026-03-15T10:00:00.000000Z'}, error='ADDR.CONFLICTING_SELECTORS')
    P('STD-ADDR-002', 'a selector is given once', 'parse_uri',
      {'uri': 'ubos://acme/Customer/c-1?branch=main&branch=main'}, error='ADDR.CONFLICTING_SELECTORS')
    tagged = ren[:1] + [{'op': 'tag', 'input': {'name': 'q1', 'branch': 'main'}}] + ren[1:]
    S('STD-ADDR-002', 'a tag implies its branch', tagged + [
        {'op': 'create_branch', 'input': {'name': 'feature/x', 'from': 'main'}},
        {'op': 'resolve', 'input': {'uri': 'ubos://acme/Customer/c-1?tag=q1'}},
        {'op': 'resolve', 'input': {'uri': 'ubos://acme/Customer/c-1?tag=q1&branch=feature/x'},
         'error': 'ADDR.CONFLICTING_SELECTORS'}])
    def uri_of(label, suffix, commit_label):
        return {'op': 'eval_expr', 'label': label,
                'input': {'expr': 'concat("ubos://acme/Customer/c-1?commit=", params.c, "%s")' % suffix,
                          'params_types': {'c': 'text'}, 'params': {'c': {'$ref': commit_label + '/commit_id'}}}}
    S('STD-ADDR-002', 'a commit implies its branch', ren + [
        {'op': 'create_branch', 'input': {'name': 'feature/x', 'from': 'main'}},
        {'op': 'commit_info', 'input': {'branch': 'main', 'time': 1}, 'label': 'c'},
        uri_of('u1', '', 'c'), {'op': 'resolve', 'input': {'uri': {'$ref': 'u1/value'}}},
        uri_of('u2', '&branch=feature/x', 'c'),
        {'op': 'resolve', 'input': {'uri': {'$ref': 'u2/value'}}, 'error': 'ADDR.CONFLICTING_SELECTORS'}])
    S('STD-ADDR-002', 'branch defaults to main and asof combines with it', ren + [
        {'op': 'create_branch', 'input': {'name': 'feature/x', 'from': 'main'}},
        commit([create(uid(2), 'Customer', 'c-2', {'name': 'Beta'})], branch='feature/x'),
        {'op': 'resolve', 'input': {'uri': 'ubos://acme/Customer/c-2'}, 'error': 'IAM.NOT_FOUND'},
        {'op': 'resolve', 'input': {'uri': 'ubos://acme/Customer/c-2?branch=feature/x&asof=2026-03-15'}}])
    S('STD-ADDR-002', 'a time selector reads the commit current at that time', ren[:1] + [
        {'op': 'set_now', 'input': {'now': '2026-03-15T12:00:00.000000Z'}}] + ren[1:] + [
        {'op': 'resolve', 'input': {'uri': 'ubos://acme/Customer/c-1?time=2026-03-15T11:00:00.000000Z'}}])

    # ---- STD-ADDR-010 did:ubos ----
    pk = crypto.public_key('9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60')
    P('STD-ADDR-010', 'the key form of an Ed25519 key', 'key_did', {'public_key': pk})
    V('STD-ADDR-010', 'the key form parses back to its key', prototype=False, steps=[
        {'op': 'key_did', 'input': {'public_key': pk}, 'label': 'd'},
        {'op': 'parse_did', 'input': {'did': {'$ref': 'd/'}}}])
    P('STD-ADDR-010', 'the registered form names an authority and a UUID', 'parse_did',
      {'did': 'did:ubos:acme.eu:01900000-0000-7000-8000-00000000000a'})
    for desc, did in [('another method', 'did:web:acme.example'), ('a key form without multibase prefix', 'did:ubos:k:6Mk'),
                      ('a registered form without a UUID', 'did:ubos:acme:alice'),
                      ('a key form that is not an Ed25519 key', 'did:ubos:k:z11111111')]:
        P('STD-ADDR-010', 'reject %s' % desc, 'parse_did', {'did': did}, error='MODEL.INVALID_VALUE')
    S('STD-ADDR-010', 'the document of a key-form DID is derived from the key', [
        {'op': 'key_did', 'input': {'public_key': pk}, 'label': 'd'},
        {'op': 'did_document', 'input': {'did': {'$ref': 'd/'}}}])
    S('STD-ADDR-010', 'the node is a registered DID served by the node', [
        {'op': 'node_did', 'input': {}, 'label': 'n'},
        {'op': 'did_document', 'input': {'did': {'$ref': 'n/id'}}, 'same_as': 'n/'}])
    S('STD-ADDR-010', 'key rotation appends verification methods and keeps the history', [
        {'op': 'set_now', 'input': {'now': '2026-04-01T00:00:00.000000Z'}},
        {'op': 'rotate_node_key', 'input': {'seed': '02' * 32}},
        {'op': 'set_now', 'input': {'now': '2026-05-01T00:00:00.000000Z'}},
        {'op': 'rotate_node_key', 'input': {'seed': '03' * 32}}, {'op': 'node_did', 'input': {}}])
    S('STD-ADDR-010', 'an unknown registered DID has no document here', [
        {'op': 'did_document', 'input': {'did': 'did:ubos:acme:01900000-0000-7000-8000-0000000000ff'},
         'error': 'IAM.NOT_FOUND'}])

    # ---- STD-FND-026 custom value types ----
    P('STD-FND-026', 'a value is normalised before it is validated', 'validate_value',
      {'type': 'Isin', 'value': ' us0378331005 ', 'value_types': [ISIN]})
    P('STD-FND-026', 'a value that fails the validation expression is rejected', 'validate_value',
      {'type': 'Isin', 'value': 'US0378331006', 'value_types': [ISIN]}, error='MODEL.CONSTRAINT_VIOLATED')
    P('STD-FND-026', 'the constraints of the base type apply after normalisation', 'validate_value',
      {'type': 'Isin', 'value': 'US03783310050', 'value_types': [ISIN]}, error='MODEL.CONSTRAINT_VIOLATED')
    P('STD-FND-026', 'a value type declares its base and description', 'validate_value',
      {'type': 'Isin', 'value': 'US0378331005', 'value_types': [{'name': 'Isin', 'base': 'text'}]},
      error='MODEL.DECLARATION_INCOMPLETE')
    P('STD-FND-026', 'the base is a scalar type of STD-FND-020', 'validate_value',
      {'type': 'Blobby', 'value': {}, 'value_types': [{'name': 'Blobby', 'base': 'json', 'description': 'x'}]},
      error='MODEL.UNKNOWN_TYPE')
    P('STD-FND-026', 'the normaliser returns the base type', 'validate_value',
      {'type': 'Isin', 'value': 'US0378331005', 'value_types': [dict(ISIN, normalize='len(value)')]},
      error='LOGIC.TYPE_ERROR')
    P('STD-FND-026', 'a decimal type rounds through its normaliser', 'validate_value',
      {'type': 'Rate', 'value': '0.123456', 'value_types': [{'name': 'Rate', 'base': 'decimal', 'description': 'rate',
                                                             'normalize': 'round(value, 4, "half_up")',
                                                             'validate': 'value >= 0 and value <= 1'}]})
    sec = cls('Security', [field('isin', 'Isin', True), field('name', 'text')])
    S('STD-FND-026', 'stored values are normalised before hashing', [
        commit([create(uid(7), 'Security', 'apple', {'isin': 'us0378331005', 'name': 'Apple'})], label='a'),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(7)}}], classes=CLASSES + [sec], value_types=[ISIN])
    S('STD-FND-026', 'objects with an invalid custom value are rejected', [
        commit([create(uid(7), 'Security', 'apple', {'isin': 'US0378331006', 'name': 'Apple'})],
               error='MODEL.CONSTRAINT_VIOLATED')], classes=CLASSES + [sec], value_types=[ISIN])
    V('STD-FND-026', 'a field of an undeclared custom type is rejected', prototype=False,
      setup=[{'op': 'set_now', 'input': {'now': NOW}}],
      steps=[{'op': 'load_genesis', 'input': {'classes': CLASSES + [sec]}, 'error': 'MODEL.UNKNOWN_TYPE'}])
