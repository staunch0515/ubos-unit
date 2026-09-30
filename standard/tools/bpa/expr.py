"""L1 expressions (STD-EXPR-001…005, PH-1 step 1e-1, DEC-043): lexer, parser, static type
checker and evaluator with the ABI 1.0 function library.

Values at run time: str (text), bool, int (integer), Decimal (decimal), Money, date, datetime
(instant, UTC), Time, Duration, Period, list, dict (map or object) and None (null)."""
import calendar
import datetime
import re
import unicodedata
from decimal import Decimal, localcontext
from zoneinfo import ZoneInfo

from . import BpaError
from .values import (MINOR_UNITS, ROUNDING, fmt_decimal, round_decimal, parse_decimal, validate_value, REFERENCE,
                     DATE)

MAX_UNITS = 1000000        # strict profile (STD-CTX-004)
MAX_ITEMS = 65536
REF_COST = 10              # ref() and exists() are charged as read.get (STD-ISA-030)
KEYWORDS = ('and', 'or', 'not', 'in', 'is', 'null', 'true', 'false', 'matches')
ROOTS = ('this', 'ctx', 'params', 'old')
COLL_FN = ('any', 'all', 'count', 'sum', 'min', 'max', 'filter', 'map', 'first', 'sort_by')


def terr(pos, msg):
    raise BpaError('LOGIC.TYPE_ERROR', {'position': pos, 'message': msg})


# ---- run-time value classes ----

class Money:
    __slots__ = ('amount', 'currency')

    def __init__(self, amount, currency):
        self.amount, self.currency = amount, currency

    def __eq__(self, o):
        return isinstance(o, Money) and o.amount == self.amount and o.currency == self.currency

    def __hash__(self):
        return hash((self.amount, self.currency))


class Duration:
    __slots__ = ('us',)

    def __init__(self, us):
        self.us = us

    def __eq__(self, o):
        return isinstance(o, Duration) and o.us == self.us

    def __lt__(self, o):
        return self.us < o.us

    def __hash__(self):
        return hash(self.us)


class Time(str):
    pass


class Period:
    __slots__ = ('lo', 'hi')

    def __init__(self, lo, hi):
        self.lo, self.hi = lo, hi

    def __eq__(self, o):
        return isinstance(o, Period) and (o.lo, o.hi) == (self.lo, self.hi)

    def __hash__(self):
        return hash((self.lo, self.hi))


def fmt_duration(us):
    sign, us = ('-', -us) if us < 0 else ('', us)
    days, rest = divmod(us, 86400 * 10 ** 6)
    h, rest = divmod(rest, 3600 * 10 ** 6)
    m, rest = divmod(rest, 60 * 10 ** 6)
    s, frac = divmod(rest, 10 ** 6)
    out = sign + 'P' + ('%dD' % days if days else '')
    t = ('%dH' % h if h else '') + ('%dM' % m if m else '')
    if s or frac:
        t += ('%d.%06d' % (s, frac)).rstrip('0').rstrip('.') + 'S'
    if t:
        out += 'T' + t
    return out if out not in ('P', '-P') else 'PT0S'


def parse_duration(s):
    m = re.match(r'\A(-)?P(?:([0-9]+)D)?(?:T(?:([0-9]+)H)?(?:([0-9]+)M)?(?:([0-9]+(?:\.[0-9]{1,6})?)S)?)?\Z', s)
    if not m or s in ('P', '-P', 'PT', '-PT'):
        raise BpaError('MODEL.INVALID_VALUE', 'durations with days, hours, minutes and seconds only')
    d, h, mi, sec = (m.group(i) for i in range(2, 6))
    us = ((int(d or 0) * 24 + int(h or 0)) * 60 + int(mi or 0)) * 60 * 10 ** 6 + int(Decimal(sec or 0) * 10 ** 6)
    return Duration(-us if m.group(1) else us)


def to_date(s):
    return datetime.date.fromisoformat(validate_value('date', None, s))


def to_instant(s):
    validate_value('instant', None, s)
    return datetime.datetime.strptime(s, '%Y-%m-%dT%H:%M:%S.%fZ')


def fmt_instant(d):
    return d.strftime('%Y-%m-%dT%H:%M:%S.%fZ')


# ---- types ----
# Types are strings ('text', 'integer', 'decimal', 'money', 'boolean', 'date', 'instant', 'time',
# 'duration', 'period', 'null', 'any'), ('list', T) or ('obj', {member: T}).

NUM = ('integer', 'decimal')
ORDERED = ('integer', 'decimal', 'text', 'date', 'instant', 'time', 'duration', 'money')
SCALAR = {'text': 'text', 'enum': 'text', 'reference': 'text', 'integer': 'integer', 'decimal': 'decimal',
          'money': 'money', 'boolean': 'boolean', 'date': 'date', 'instant': 'instant', 'time': 'time',
          'duration': 'duration', 'period': 'period'}
CTX_TYPES = {'principal': 'text', 'principal_kind': 'text', 'on_behalf_of': ('list', 'text'), 'auth_level': 'text',
             'tenant': 'text', 'vae': 'text', 'branch': 'text', 'asof': 'any', 'time': 'text', 'now': 'instant',
             'locale': 'text', 'time_zone': 'text', 'process_id': 'text', 'trigger': 'any', 'channel': 'text',
             'input': 'any', 'profile': 'text', 'budget_remaining': 'any', 'replay': 'boolean'}


def field_type(f, custom=None):
    """Static type of a field declaration {type, constraints} or of a type name."""
    if isinstance(f, str):
        f = {'type': f}
    t = f.get('type')
    if custom and t in custom:
        t = custom[t]['base']
    if t in SCALAR:
        return SCALAR[t]
    c = f.get('constraints') or {}
    if t == 'list':
        return ('list', field_type(c.get('item_type') or {'type': 'json'}, custom))
    if t == 'composite':
        return ('obj', {m['name']: field_type(m, custom) for m in c.get('members') or []})
    return 'any'


def is_list(t):
    return isinstance(t, tuple) and t[0] == 'list'


def is_obj(t):
    return isinstance(t, tuple) and t[0] == 'obj'


def compat(a, b):
    """The common type of two compatible types, or None."""
    if a == 'any' or b == 'any':
        return 'any'
    if a == 'null':
        return b
    if b == 'null':
        return a
    if a == b:
        return a
    if a in NUM and b in NUM:
        return 'decimal'
    if is_list(a) and is_list(b):
        c = compat(a[1], b[1])
        return ('list', c) if c else None
    if is_obj(a) and is_obj(b):
        return 'any'
    return None


# ---- lexer ----

TOKEN = re.compile(r'''
  (?P<ws>\s+)
| (?P<date>d'[^']*')
| (?P<inst>t'[^']*')
| (?P<money>(?:0|[1-9][0-9]*)(?:\.[0-9]+)?[A-Z]{3}(?![A-Za-z0-9_]))
| (?P<num>(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?![A-Za-z0-9_]))
| (?P<str>"(?:[^"\\]|\\.)*")
| (?P<ident>[A-Za-z_][A-Za-z0-9_]*)
| (?P<op>\?\.|==|!=|<=|>=|->|[-+*/%<>()\[\],.?:])
''', re.X)
ESCAPES = {'"': '"', '\\': '\\', 'n': '\n', 't': '\t', 'r': '\r', '/': '/'}


def lex(src):
    if not isinstance(src, str):
        raise BpaError('LOGIC.TYPE_ERROR', {'position': 0, 'message': 'an expression is text'})
    out, i = [], 0
    while i < len(src):
        m = TOKEN.match(src, i)
        if not m:
            terr(i, 'unexpected character')
        kind = m.lastgroup
        text = m.group()
        if kind == 'str':
            body, s, j = text[1:-1], [], 0
            while j < len(body):
                if body[j] == '\\':
                    e = body[j + 1]
                    if e == 'u' and re.match(r'[0-9a-fA-F]{4}', body[j + 2:j + 6]):
                        s.append(chr(int(body[j + 2:j + 6], 16)))
                        j += 6
                        continue
                    if e not in ESCAPES:
                        terr(i + j + 1, 'unknown escape')
                    s.append(ESCAPES[e])
                    j += 2
                else:
                    s.append(body[j])
                    j += 1
            out.append(('str', unicodedata.normalize('NFC', ''.join(s)), i))
        elif kind == 'ident' and text in KEYWORDS:
            out.append(('kw', text, i))
        elif kind != 'ws':
            out.append((kind, text, i))
        i = m.end()
    out.append(('end', '', len(src)))
    return out


# ---- parser: AST nodes are tuples (kind, pos, ...) ----

class Parser:
    def __init__(self, src):
        self.toks, self.i = lex(src), 0

    def peek(self, k=0):
        return self.toks[min(self.i + k, len(self.toks) - 1)]

    def next(self):
        t = self.toks[self.i]
        self.i += 1
        return t

    def at(self, kind, text=None, k=0):
        t = self.peek(k)
        return t[0] == kind and (text is None or t[1] == text)

    def expect(self, kind, text=None):
        if not self.at(kind, text):
            terr(self.peek()[2], 'expected %s' % (text or kind))
        return self.next()

    def parse(self):
        e = self.expr()
        if not self.at('end'):
            terr(self.peek()[2], 'unexpected token')
        return e

    def expr(self):
        c = self.or_()
        if self.at('op', '?'):
            pos = self.next()[2]
            a = self.expr()
            self.expect('op', ':')
            return ('cond', pos, c, a, self.expr())
        return c

    def or_(self):
        e = self.and_()
        while self.at('kw', 'or'):
            e = ('or', self.next()[2], e, self.and_())
        return e

    def and_(self):
        e = self.not_()
        while self.at('kw', 'and'):
            e = ('and', self.next()[2], e, self.not_())
        return e

    def not_(self):
        if self.at('kw', 'not'):
            return ('not', self.next()[2], self.cmp())
        return self.cmp()

    def cmp(self):
        e = self.add()
        t = self.peek()
        if t[0] == 'op' and t[1] in ('==', '!=', '<', '<=', '>', '>='):
            self.next()
            return ('cmp', t[2], t[1], e, self.add())
        if t[0] == 'kw' and t[1] == 'in':
            self.next()
            if self.at('op', '['):
                return ('in', t[2], e, self.primary())
            p = self.postfix()
            if p[0] not in ('path', 'member'):
                terr(p[1], 'in takes a list literal or a path')
            return ('in', t[2], e, p)
        if t[0] == 'kw' and t[1] == 'is':
            self.next()
            neg = bool(self.at('kw', 'not') and self.next())
            self.expect('kw', 'null')
            return ('isnull', t[2], e, neg)
        if t[0] == 'kw' and t[1] == 'matches':
            self.next()
            s = self.expect('str')
            try:
                re.compile(s[1])
            except re.error:
                terr(s[2], 'invalid pattern')
            return ('matches', t[2], e, s[1])
        return e

    def add(self):
        e = self.mul()
        while self.at('op', '+') or self.at('op', '-'):
            t = self.next()
            e = ('arith', t[2], t[1], e, self.mul())
        return e

    def mul(self):
        e = self.unary()
        while self.at('op', '*') or self.at('op', '/') or self.at('op', '%'):
            t = self.next()
            e = ('arith', t[2], t[1], e, self.unary())
        return e

    def unary(self):
        if self.at('op', '-'):
            return ('neg', self.next()[2], self.postfix())
        return self.postfix()

    def postfix(self):
        e = self.primary()
        while True:
            if self.at('op', '.') or self.at('op', '?.'):
                t = self.next()
                m = self.peek()
                if m[0] not in ('ident', 'kw'):
                    terr(m[2], 'expected a member name')
                self.next()
                e = ('member', t[2], e, m[1], t[1] == '?.')
            elif self.at('op', '['):
                pos = self.next()[2]
                idx = self.expr()
                self.expect('op', ']')
                e = ('index', pos, e, idx)
            else:
                return e

    def primary(self):
        t = self.next()
        k, v, pos = t
        if k == 'num':
            return ('lit', pos, int(v) if '.' not in v else Decimal(v), 'integer' if '.' not in v else 'decimal')
        if k == 'money':
            cur = v[-3:]
            if cur not in MINOR_UNITS:
                terr(pos, 'unknown currency')
            return ('lit', pos, Money(Decimal(v[:-3]), cur), 'money')
        if k == 'str':
            return ('lit', pos, v, 'text')
        if k == 'date':
            try:
                return ('lit', pos, to_date(v[2:-1]), 'date')
            except BpaError:
                terr(pos, 'invalid date literal')
        if k == 'inst':
            try:
                return ('lit', pos, to_instant(v[2:-1]), 'instant')
            except BpaError:
                terr(pos, 'invalid instant literal')
        if k == 'kw' and v in ('true', 'false'):
            return ('lit', pos, v == 'true', 'boolean')
        if k == 'kw' and v == 'null':
            return ('lit', pos, None, 'null')
        if k == 'op' and v == '(':
            e = self.expr()
            self.expect('op', ')')
            return e
        if k == 'op' and v == '[':
            items = []
            if not self.at('op', ']'):
                items.append(self.expr())
                while self.at('op', ','):
                    self.next()
                    items.append(self.expr())
            self.expect('op', ']')
            return ('list', pos, items)
        if k == 'ident':
            if self.at('op', '('):
                self.next()
                if v in COLL_FN and not self.at('op', ')'):
                    first = self.expr()
                    if self.at('op', ',') and self.at('ident', k=1) and self.at('op', '->', k=2):
                        self.next()
                        var = self.next()[1]
                        self.next()
                        body = self.expr()
                        self.expect('op', ')')
                        return ('lambda', pos, v, first, var, body)
                    args = [first]
                else:
                    args = [] if self.at('op', ')') else [self.expr()]
                while self.at('op', ','):
                    self.next()
                    args.append(self.expr())
                self.expect('op', ')')
                return ('call', pos, v, args)
            return ('path', pos, v)
        terr(pos, 'unexpected token')


def parse(src):
    return Parser(src).parse()


# ---- the function library (ABI 1.0) ----

def _int(x):
    return isinstance(x, int) and not isinstance(x, bool)


def _num(x):
    return _int(x) or isinstance(x, Decimal)


def _luhn(digits):
    total = 0
    for i, ch in enumerate(reversed(digits)):
        d = int(ch)
        if i % 2 == 1:
            d *= 2
            if d > 9:
                d -= 9
        total += d
    return total % 10 == 0


def _mod97(s):
    if not s or not s.isascii() or not s.isalnum():
        return False
    n = ''.join(str(int(c, 36)) for c in s.upper())
    return int(n) % 97 == 1


def luhn_valid(s):
    return bool(re.fullmatch(r'[0-9]{2,}', s)) and _luhn(s)


def isin_valid(s):
    if not re.fullmatch(r'[A-Z]{2}[A-Z0-9]{9}[0-9]', s):
        return False
    return _luhn(''.join(str(int(c, 36)) for c in s))


def lei_valid(s):
    return bool(re.fullmatch(r'[A-Z0-9]{18}[0-9]{2}', s)) and _mod97(s)


def iban_valid(s):
    if not re.fullmatch(r'[A-Z]{2}[0-9]{2}[A-Z0-9]{11,30}', s):
        return False
    return _mod97(s[4:] + s[:4])


def _add_months(d, n):
    m = d.month - 1 + n
    y, m = d.year + m // 12, m % 12 + 1
    return datetime.date(y, m, min(d.day, calendar.monthrange(y, m)[1]))


def _canon_text(v):
    if isinstance(v, str):
        return v
    return __import__('json').dumps(to_json(v), ensure_ascii=False) if isinstance(v, (list, dict)) else \
        (lambda j: j if isinstance(j, str) else ('%s %s' % (j['amount'], j['currency']) if isinstance(j, dict)
                                                 and 'currency' in j else str(j).lower()))(to_json(v))


# signature checkers: (arg types, pos) -> result type
def _sig(*params, result):
    def check(ts, pos, name):
        if len(ts) != len(params):
            terr(pos, '%s takes %d arguments' % (name, len(params)))
        for t, p in zip(ts, params):
            ok = t in ('any', 'null') or (t in p if isinstance(p, tuple) else t == p)
            if not ok:
                terr(pos, 'argument of %s has type %s' % (name, t if isinstance(t, str) else t[0]))
        return result(ts) if callable(result) else result
    return check


def _var(name_min, ptype, result):
    def check(ts, pos, name):
        if len(ts) < name_min:
            terr(pos, '%s takes at least %d arguments' % (name, name_min))
        for t in ts:
            if t not in ('any', 'null', ptype):
                terr(pos, 'argument of %s has type %s' % (name, t))
        return result
    return check


TEXT, INT, DEC, MON, DATE_T = 'text', 'integer', 'decimal', 'money', 'date'
NUMS = ('integer', 'decimal')
NUMM = ('integer', 'decimal', 'money')
DATEI = ('date', 'instant')


def _numres(ts):
    return 'money' if 'money' in ts else ('integer' if all(t == 'integer' for t in ts if t not in ('any', 'null'))
                                          else 'decimal')


def _minmax(ts, pos, name):
    if len(ts) != 2:
        terr(pos, '%s takes two values, or a list and a lambda' % name)
    c = compat(ts[0], ts[1])
    if c is None or (c not in ORDERED and c not in ('any', 'null')):
        terr(pos, '%s needs two comparable values' % name)
    return c


def _contains(ts, pos, name):
    if len(ts) != 2:
        terr(pos, 'contains takes two arguments')
    if ts[0] == 'period':
        if ts[1] not in DATEI + ('any', 'null'):
            terr(pos, 'contains(period, d) takes a date or an instant')
    elif ts[0] not in ('text', 'any', 'null') or ts[1] not in ('text', 'any', 'null'):
        terr(pos, 'contains takes two texts or a period and a time')
    return 'boolean'


def _list_of(ts, pos, name):
    if len(ts) != 1 or not (is_list(ts[0]) or ts[0] in ('any', 'null')):
        terr(pos, '%s takes a list' % name)
    return ts[0] if is_list(ts[0]) else ('list', 'any')


def _plain_coll(ts, pos, name):
    if len(ts) == 2 and name in ('min', 'max'):
        return _minmax(ts, pos, name)
    if len(ts) != 1 or not (is_list(ts[0]) or ts[0] in ('any', 'null')):
        terr(pos, '%s takes a list, or a list and a lambda' % name)
    item = ts[0][1] if is_list(ts[0]) else 'any'
    if name == 'count':
        return 'integer'
    if name in ('any', 'all'):
        if item not in ('boolean', 'any', 'null'):
            terr(pos, '%s of a list needs boolean items' % name)
        return 'boolean'
    if name == 'sum':
        if item not in NUMM + ('any', 'null', 'duration'):
            terr(pos, 'sum needs numbers or money')
        return item
    if name in ('min', 'max'):
        if item not in ORDERED + ('any', 'null'):
            terr(pos, '%s needs comparable items' % name)
        return item
    if name == 'first':
        return item
    terr(pos, '%s needs a lambda' % name)


FUNCTIONS = {
    'len': _sig(('text',), result=INT), 'lower': _sig(TEXT, result=TEXT), 'upper': _sig(TEXT, result=TEXT),
    'trim': _sig(TEXT, result=TEXT),
    'substr': lambda ts, pos, n: (_sig(TEXT, INT, result=TEXT) if len(ts) == 2 else
                                 _sig(TEXT, INT, INT, result=TEXT))(ts, pos, n),
    'concat': _var(1, 'text', TEXT), 'contains': _contains, 'starts_with': _sig(TEXT, TEXT, result='boolean'),
    'ends_with': _sig(TEXT, TEXT, result='boolean'), 'replace': _sig(TEXT, TEXT, TEXT, result=TEXT),
    'pad_left': _sig(TEXT, INT, TEXT, result=TEXT),
    'format': lambda ts, pos, n: (terr(pos, 'format takes a template') if not ts or ts[0] not in (TEXT, 'any')
                                  else TEXT),
    'abs': _sig(NUMM, result=lambda ts: ts[0]),
    'round': lambda ts, pos, n: (_sig(NUMM, INT, result=lambda t: 'money' if t[0] == 'money' else DEC) if len(ts) == 2
                                 else _sig(NUMM, INT, TEXT, result=lambda t: 'money' if t[0] == 'money' else DEC))(
        ts, pos, n),
    'floor': _sig(NUMS, result=INT), 'ceil': _sig(NUMS, result=INT),
    'div': lambda ts, pos, n: (_sig(NUMS, NUMS, INT, result=DEC) if len(ts) == 3 else
                               _sig(NUMS, NUMS, INT, TEXT, result=DEC))(ts, pos, n),
    'clamp': lambda ts, pos, n: (terr(pos, 'clamp takes three values') if len(ts) != 3 else
                                 _minmax([compat(ts[0], ts[1]) or 'text', ts[2]] if compat(ts[0], ts[1]) else
                                         [ts[0], 'boolean'], pos, n)),
    'amount': _sig(MON, result=DEC), 'currency': _sig(MON, result=TEXT), 'money': _sig(NUMS, TEXT, result=MON),
    'convert': _sig(MON, TEXT, TEXT, result=MON),
    'allocate': _sig(MON, ('list',) and ('any',), result=('list', 'money')),
    'today': _sig(result=DATE_T), 'now': _sig(result='instant'), 'date': _sig(INT, INT, INT, result=DATE_T),
    'year': _sig(DATE_T, result=INT), 'month': _sig(DATE_T, result=INT), 'day': _sig(DATE_T, result=INT),
    'weekday': _sig(DATE_T, result=INT), 'add_days': _sig(DATE_T, INT, result=DATE_T),
    'add_months': _sig(DATE_T, INT, result=DATE_T), 'days_between': _sig(DATE_T, DATE_T, result=INT),
    'start_of_month': _sig(DATE_T, result=DATE_T), 'end_of_month': _sig(DATE_T, result=DATE_T),
    'to_zone': _sig('instant', TEXT, result=TEXT),
    'is_business_day': _sig(DATE_T, TEXT, result='boolean'),
    'add_business_days': _sig(DATE_T, INT, TEXT, result=DATE_T),
    'next_business_day': _sig(DATE_T, TEXT, result=DATE_T), 'previous_business_day': _sig(DATE_T, TEXT, result=DATE_T),
    'cutoff': _sig(DATE_T, TEXT, TEXT, TEXT, result='instant'),
    'overlaps': _sig('period', 'period', result='boolean'), 'period': _sig(DATEI, DATEI, result='period'),
    'distinct': _list_of,
    'ref': _sig(TEXT, result='any'), 'exists': _sig(TEXT, result='boolean'),
    'luhn_valid': _sig(TEXT, result='boolean'), 'iso7064_mod97_10': _sig(TEXT, result='boolean'),
    'isin_valid': _sig(TEXT, result='boolean'), 'lei_valid': _sig(TEXT, result='boolean'),
    'iban_valid': _sig(TEXT, result='boolean'),
}
for _n in ('any', 'all', 'count', 'sum', 'min', 'max', 'first'):
    FUNCTIONS.setdefault(_n, _plain_coll)
FUNCTIONS['min'] = FUNCTIONS['max'] = _plain_coll


def _allocate_check(ts, pos, name):
    if len(ts) != 2 or ts[0] not in ('money', 'any', 'null') or not (
            ts[1] in ('any', 'null') or (is_list(ts[1]) and ts[1][1] in NUMS + ('any', 'null'))):
        terr(pos, 'allocate takes money and a list of weights')
    return ('list', 'money')


FUNCTIONS['allocate'] = _allocate_check
TIME_T = 'time'


# ---- type checker ----

class Env:
    def __init__(self, this=None, params=None, old=None, target=None):
        self.roots = {'this': this, 'params': params, 'old': old, 'ctx': ('obj', CTX_TYPES)}
        self.vars = {}
        self.target = target          # (scale, rounding) for '/' or None


def check(node, env):
    k, pos = node[0], node[1]
    if k == 'lit':
        return node[3]
    if k == 'list':
        t = 'null'
        for it in node[2]:
            t = compat(t, check(it, env)) or 'any'      # items of different types give list<any>
        return ('list', t)
    if k == 'path':
        name = node[2]
        if name in env.vars:
            return env.vars[name]
        if name in ROOTS:
            t = env.roots.get(name)
            if t is None:
                terr(pos, '%s is not available here' % name)
            return t
        terr(pos, 'unknown name %s' % name)
    if k == 'member':
        t = check(node[2], env)
        if t in ('any', 'null'):
            return 'any'
        if is_obj(t):
            if node[3] not in t[1]:
                terr(pos, 'unknown member %s' % node[3])
            return t[1][node[3]]
        if t == 'money' and node[3] in ('amount', 'currency'):
            return 'decimal' if node[3] == 'amount' else 'text'
        if t == 'period' and node[3] in ('from', 'to'):
            return 'any'
        terr(pos, 'no member %s on %s' % (node[3], t if isinstance(t, str) else t[0]))
    if k == 'index':
        t, i = check(node[2], env), check(node[3], env)
        if is_list(t):
            if i not in ('integer', 'any', 'null'):
                terr(node[3][1], 'a list index is an integer')
            return t[1]
        if t in ('any', 'null') or is_obj(t):
            if i not in ('text', 'integer', 'any', 'null'):
                terr(node[3][1], 'an index is text or an integer')
            return 'any' if not is_obj(t) else (t[1].get(node[3][2], 'any') if node[3][0] == 'lit' else 'any')
        terr(pos, 'cannot index %s' % (t if isinstance(t, str) else t[0]))
    if k == 'neg':
        t = check(node[2], env)
        if t not in ('integer', 'decimal', 'money', 'duration', 'any', 'null'):
            terr(pos, 'cannot negate %s' % t)
        return t
    if k == 'arith':
        op, a, b = node[2], check(node[3], env), check(node[4], env)
        return arith_type(op, a, b, pos, env)
    if k == 'cmp':
        a, b = check(node[3], env), check(node[4], env)
        c = compat(a, b)
        if c is None:
            terr(pos, 'cannot compare %s with %s' % (a if isinstance(a, str) else a[0], b if isinstance(b, str) else b[0]))
        if node[2] not in ('==', '!=') and c not in ORDERED + ('any', 'null'):
            terr(pos, '%s values are not ordered' % (c if isinstance(c, str) else c[0]))
        return 'boolean'
    if k == 'in':
        a, lt = check(node[2], env), check(node[3], env)
        if lt not in ('any', 'null') and not is_list(lt):
            terr(node[3][1], 'in needs a list')
        if is_list(lt) and compat(a, lt[1]) is None:
            terr(pos, 'value and list items are not comparable')
        return 'boolean'
    if k == 'isnull':
        check(node[2], env)
        return 'boolean'
    if k == 'matches':
        if check(node[2], env) not in ('text', 'any', 'null'):
            terr(pos, 'matches needs text')
        return 'boolean'
    if k in ('and', 'or'):
        for x in node[2:4]:
            if check(x, env) not in ('boolean', 'any', 'null'):
                terr(x[1], '%s needs boolean operands' % k)
        return 'boolean'
    if k == 'not':
        if check(node[2], env) not in ('boolean', 'any', 'null'):
            terr(pos, 'not needs a boolean operand')
        return 'boolean'
    if k == 'cond':
        if check(node[2], env) not in ('boolean', 'any', 'null'):
            terr(node[2][1], 'a condition is boolean')
        c = compat(check(node[3], env), check(node[4], env))
        if c is None:
            terr(pos, 'branches of different types')
        return c
    if k == 'call':
        name, args = node[2], node[3]
        if name not in FUNCTIONS:
            terr(pos, 'unknown function %s' % name)
        ts = [check(a, env) for a in args]
        return FUNCTIONS[name](ts, pos, name)
    if k == 'lambda':
        name, lst, var, body = node[2:6]
        lt = check(lst, env)
        if lt not in ('any', 'null') and not is_list(lt):
            terr(lst[1], '%s needs a list' % name)
        if var in ROOTS or var in env.vars:
            terr(pos, 'the lambda variable %s hides another name' % var)
        env.vars[var] = lt[1] if is_list(lt) else 'any'
        try:
            bt = check(body, env)
        finally:
            del env.vars[var]
        if name in ('any', 'all', 'count', 'filter', 'first'):
            if bt not in ('boolean', 'any', 'null'):
                terr(body[1], '%s needs a boolean lambda' % name)
            return {'any': 'boolean', 'all': 'boolean', 'count': 'integer'}.get(name, lt if name == 'filter' else (
                lt[1] if is_list(lt) else 'any'))
        if name == 'sum':
            if bt not in NUMM + ('any', 'null', 'duration'):
                terr(body[1], 'sum needs numbers or money')
            return bt
        if name in ('min', 'max', 'sort_by'):
            if bt not in ORDERED + ('any', 'null'):
                terr(body[1], '%s needs comparable keys' % name)
            return bt if name != 'sort_by' else (lt if is_list(lt) else ('list', 'any'))
        return ('list', bt)                                      # map
    terr(pos, 'unknown expression')


def arith_type(op, a, b, pos, env):
    if 'any' in (a, b):
        return 'any'
    if a == 'null' or b == 'null':
        return b if a == 'null' else a
    if op in ('+', '-'):
        if a in NUM and b in NUM:
            return 'integer' if a == b == 'integer' else 'decimal'
        if a == b == 'money':
            return 'money'
        if op == '-' and a == b == 'date':
            return 'integer'
        if op == '-' and a == b == 'instant':
            return 'duration'
        if a == b == 'duration':
            return 'duration'
    elif op == '*':
        if a in NUM and b in NUM:
            return 'integer' if a == b == 'integer' else 'decimal'
        if (a == 'money' and b in NUM) or (b == 'money' and a in NUM):
            return 'money'
    elif op == '/':
        if a in NUM and b in NUM:
            if env.target is None:
                terr(pos, 'division needs the scale of a target field; use div(a, b, scale, rounding)')
            return 'decimal'
    elif op == '%':
        if a == b == 'integer':
            return 'integer'
    terr(pos, 'operator %s does not apply to %s and %s' % (op, a if isinstance(a, str) else a[0],
                                                             b if isinstance(b, str) else b[0]))


# ---- evaluation ----

class Limit(Exception):
    pass


class Evaluator:
    def __init__(self, env, ctx=None, objects=None, calendars=None, rates=None, max_units=MAX_UNITS,
                 max_items=MAX_ITEMS, business_tz='UTC', this=None, params=None, old=None, typed_ref=None):
        self.env, self.ctx = env, ctx or {}
        self.objects, self.calendars, self.rates = objects or {}, calendars or {}, rates or {}
        self.max_units, self.max_items, self.tz = max_units, max_items, business_tz
        self.units = 0
        self.roots = {'this': this, 'params': params, 'old': old, 'ctx': self.ctx}
        self.vars = {}
        self.typed_ref = typed_ref

    def charge(self, n=1):
        self.units += n
        if self.units > self.max_units:
            raise BpaError('LOGIC.LIMIT_EXCEEDED', 'operation budget exhausted')

    def items(self, v):
        if len(v) > self.max_items:
            raise BpaError('LOGIC.LIMIT_EXCEEDED', 'too many items')
        return v

    def rt(self, pos, msg):
        raise BpaError('LOGIC.TYPE_ERROR', {'position': pos, 'message': msg})

    def ev(self, node):
        self.charge()
        k, pos = node[0], node[1]
        if k == 'lit':
            return node[2]
        if k == 'list':
            return self.items([self.ev(x) for x in node[2]])
        if k == 'path':
            name = node[2]
            return self.vars[name] if name in self.vars else self.roots.get(name)
        if k == 'member':
            r = self.ev(node[2])
            if r is None:
                if node[4]:
                    return None
                self.rt(pos, 'member %s of null; use ?.' % node[3])
            if isinstance(r, dict):
                return r.get(node[3])
            if isinstance(r, Money):
                return r.amount if node[3] == 'amount' else r.currency
            if isinstance(r, Period):
                return r.lo if node[3] == 'from' else r.hi
            self.rt(pos, 'no member %s' % node[3])
        if k == 'index':
            r, i = self.ev(node[2]), self.ev(node[3])
            if r is None or i is None:
                return None
            if isinstance(r, list) and _int(i):
                return r[i] if 0 <= i < len(r) else None
            if isinstance(r, dict) and isinstance(i, str):
                return r.get(i)
            self.rt(pos, 'invalid index')
        if k == 'neg':
            v = self.ev(node[2])
            if v is None:
                return None
            if isinstance(v, Money):
                return Money(-v.amount, v.currency)
            if isinstance(v, Duration):
                return Duration(-v.us)
            if _num(v):
                return -v
            self.rt(pos, 'cannot negate')
        if k == 'arith':
            return self.arith(node[2], self.ev(node[3]), self.ev(node[4]), pos)
        if k == 'cmp':
            return self.compare(node[2], self.ev(node[3]), self.ev(node[4]), pos)
        if k == 'in':
            a, lst = self.ev(node[2]), self.ev(node[3])
            if a is None or lst is None:
                return None
            if not isinstance(lst, list):
                self.rt(pos, 'in needs a list')
            self.charge(len(lst))
            res = [self.compare('==', a, x, pos) for x in lst]
            return True if True in res else (None if None in res else False)
        if k == 'isnull':
            v = self.ev(node[2]) is None
            return not v if node[3] else v
        if k == 'matches':
            v = self.ev(node[2])
            if v is None:
                return None
            if not isinstance(v, str):
                self.rt(pos, 'matches needs text')
            return re.fullmatch(node[3], v) is not None
        if k == 'and':
            a = self.bool(self.ev(node[2]), pos)
            if a is False:
                return False
            b = self.bool(self.ev(node[3]), pos)
            return False if b is False else (None if None in (a, b) else True)
        if k == 'or':
            a = self.bool(self.ev(node[2]), pos)
            if a is True:
                return True
            b = self.bool(self.ev(node[3]), pos)
            return True if b is True else (None if None in (a, b) else False)
        if k == 'not':
            v = self.bool(self.ev(node[2]), pos)
            return None if v is None else not v
        if k == 'cond':
            c = self.bool(self.ev(node[2]), pos)
            return self.ev(node[3]) if c is True else self.ev(node[4])
        if k == 'call':
            return self.call(node[2], [self.ev(a) for a in node[3]], pos)
        if k == 'lambda':
            return self.lam(node, pos)
        self.rt(pos, 'unknown expression')

    def bool(self, v, pos):
        if v is not None and not isinstance(v, bool):
            self.rt(pos, 'a boolean is needed')
        return v

    def arith(self, op, a, b, pos):
        if a is None or b is None:
            return None
        with localcontext() as c:
            c.prec = 200
            if _num(a) and _num(b):
                if op == '+':
                    return a + b
                if op == '-':
                    return a - b
                if op == '*':
                    return a * b
                if op == '/':
                    if b == 0:
                        raise BpaError('LOGIC.DIVISION_BY_ZERO')
                    scale, rounding = self.env.target or (None, None)
                    if scale is None:
                        self.rt(pos, 'division needs a scale')
                    return round_decimal(Decimal(a) / Decimal(b), scale, rounding)
                if op == '%' and _int(a) and _int(b):
                    if b == 0:
                        raise BpaError('LOGIC.DIVISION_BY_ZERO')
                    r = abs(a) % abs(b)
                    return -r if a < 0 else r
            if isinstance(a, Money) and isinstance(b, Money) and op in ('+', '-'):
                if a.currency != b.currency:
                    raise BpaError('MODEL.CURRENCY_MISMATCH')
                return Money(a.amount + b.amount if op == '+' else a.amount - b.amount, a.currency)
            if op == '*' and isinstance(a, Money) and _num(b):
                return Money(a.amount * b, a.currency)
            if op == '*' and isinstance(b, Money) and _num(a):
                return Money(b.amount * a, b.currency)
            if op == '-' and type(a) is datetime.date and type(b) is datetime.date:
                return (a - b).days
            if op == '-' and isinstance(a, datetime.datetime) and isinstance(b, datetime.datetime):
                d = a - b
                return Duration((d.days * 86400 + d.seconds) * 10 ** 6 + d.microseconds)
            if isinstance(a, Duration) and isinstance(b, Duration) and op in ('+', '-'):
                return Duration(a.us + b.us if op == '+' else a.us - b.us)
        self.rt(pos, 'operator %s does not apply to these values' % op)

    def order_key(self, v, pos):
        if _num(v):
            return (0, Decimal(v))
        if isinstance(v, Money):
            return (1, v.currency, v.amount)
        for i, t in enumerate((str, datetime.datetime, datetime.date, Duration)):
            if isinstance(v, t):
                return (2 + i, v.us if t is Duration else v)
        self.rt(pos, 'values are not ordered')

    def compare(self, op, a, b, pos):
        if a is None or b is None:
            return None
        if isinstance(a, Money) and isinstance(b, Money) and a.currency != b.currency and op not in ('==', '!='):
            raise BpaError('MODEL.CURRENCY_MISMATCH')
        if op in ('==', '!='):
            if _num(a) and _num(b):
                eq = Decimal(a) == Decimal(b)
            elif isinstance(a, bool) != isinstance(b, bool) or (type(a) is datetime.date) != (type(b) is datetime.date):
                eq = False
            else:
                eq = a == b
            return eq if op == '==' else not eq
        ka, kb = self.order_key(a, pos), self.order_key(b, pos)
        if ka[0] != kb[0]:
            self.rt(pos, 'values of different types')
        return {'<': ka < kb, '<=': ka <= kb, '>': ka > kb, '>=': ka >= kb}[op]

    # -- collections --

    def lam(self, node, pos):
        name, lst, var, body = node[2:6]
        items = self.ev(lst)
        if items is None:
            return None
        if not isinstance(items, list):
            self.rt(pos, '%s needs a list' % name)
        self.items(items)
        out = []
        for x in items:
            self.charge()
            self.vars[var] = x
            try:
                out.append((x, self.ev(body)))
            finally:
                del self.vars[var]
            if name == 'first' and out[-1][1] is True:
                return x
            if name == 'any' and out[-1][1] is True:
                return True
            if name == 'all' and out[-1][1] is False:
                return False
        vals = [v for _, v in out]
        if name == 'any':
            return None if None in vals else False
        if name == 'all':
            return None if None in vals else True
        if name == 'count':
            return sum(1 for v in vals if v is True)
        if name == 'filter':
            return [x for x, v in out if v is True]
        if name == 'map':
            return self.items(vals)
        if name == 'first':
            return None
        if name == 'sum':
            return self.total([v for v in vals if v is not None], pos)
        if name in ('min', 'max'):
            pairs = [(x, v) for x, v in out if v is not None]
            if not pairs:
                return None
            keyf = lambda p: self.order_key(p[1], pos)
            return (min if name == 'min' else max)(pairs, key=keyf)[1]
        if name == 'sort_by':
            nulls = [x for x, v in out if v is None]
            rest = sorted(((x, v) for x, v in out if v is not None), key=lambda p: self.order_key(p[1], pos))
            return nulls + [x for x, _ in rest]
        self.rt(pos, 'unknown collection function')

    def total(self, vals, pos):
        if not vals:
            return None
        acc = vals[0]
        for v in vals[1:]:
            acc = self.arith('+', acc, v, pos)
        return acc

    # -- functions --

    def call(self, name, a, pos):
        n = len(a)
        if name in ('any', 'all', 'count', 'sum', 'min', 'max', 'first', 'distinct') and n == 1:
            lst = a[0]
            if lst is None:
                return None
            if not isinstance(lst, list):
                self.rt(pos, '%s needs a list' % name)
            self.items(lst)
            self.charge(len(lst))
            if name == 'count':
                return len(lst)
            if name == 'first':
                return lst[0] if lst else None
            if name == 'distinct':
                out = []
                for x in lst:
                    if not any(self.compare('==', x, y, pos) is True or (x is None and y is None) for y in out):
                        out.append(x)
                return out
            if name in ('any', 'all'):
                vals = [self.bool(x, pos) for x in lst]
                if name == 'any':
                    return True if True in vals else (None if None in vals else False)
                return False if False in vals else (None if None in vals else True)
            vals = [x for x in lst if x is not None]
            if name == 'sum':
                return self.total(vals, pos)
            if not vals:
                return None
            return (min if name == 'min' else max)(vals, key=lambda v: self.order_key(v, pos))
        nullable = name not in ('today', 'now', 'exists', 'concat', 'format', 'period')
        if nullable and any(x is None for x in a):
            return None
        if name in ('min', 'max'):
            ka, kb = self.order_key(a[0], pos), self.order_key(a[1], pos)
            if ka[0] != kb[0]:
                self.rt(pos, 'values of different types')
            if isinstance(a[0], Money) and a[0].currency != a[1].currency:
                raise BpaError('MODEL.CURRENCY_MISMATCH')
            return a[0] if (ka <= kb) == (name == 'min') else a[1]
        f = getattr(self, 'f_' + name, None)
        if f is None:
            self.rt(pos, 'unknown function %s' % name)
        return f(pos, *a)

    # text
    def f_len(self, pos, s):
        return len(s)

    def f_lower(self, pos, s):
        return unicodedata.normalize('NFC', s.lower())

    def f_upper(self, pos, s):
        return unicodedata.normalize('NFC', s.upper())

    def f_trim(self, pos, s):
        return s.strip()

    def f_substr(self, pos, s, start, length=None):
        start = max(0, start)
        return s[start:] if length is None else s[start:start + max(0, length)]

    def f_concat(self, pos, *xs):
        return None if any(x is None for x in xs) else ''.join(xs)

    def f_contains(self, pos, a, b):
        if isinstance(a, Period):
            return self.in_period(a, b, pos)
        return b in a

    def f_starts_with(self, pos, s, p):
        return s.startswith(p)

    def f_ends_with(self, pos, s, p):
        return s.endswith(p)

    def f_replace(self, pos, s, old, new):
        if old == '':
            self.rt(pos, 'replace needs a non-empty text to find')
        return s.replace(old, new)

    def f_pad_left(self, pos, s, width, fill):
        if len(fill) != 1:
            raise BpaError('MODEL.INVALID_VALUE', 'the fill is one character')
        return s if len(s) >= width else fill * (width - len(s)) + s

    def f_format(self, pos, template, *args):
        if template is None:
            return None

        def sub(m):
            i = int(m.group(1))
            if i >= len(args):
                raise BpaError('MODEL.INVALID_VALUE', 'format placeholder {%d} has no argument' % i)
            return 'null' if args[i] is None else _canon_text(args[i])
        return re.sub(r'\{([0-9]+)\}', sub, template)

    # numbers
    def f_abs(self, pos, x):
        return Money(abs(x.amount), x.currency) if isinstance(x, Money) else abs(x)

    def f_round(self, pos, x, scale, mode='half_even'):
        if isinstance(x, Money):
            return Money(round_decimal(x.amount, scale, mode), x.currency)
        return round_decimal(Decimal(x), scale, mode)

    def f_floor(self, pos, x):
        return int(Decimal(x).to_integral_value(rounding=ROUNDING['floor']))

    def f_ceil(self, pos, x):
        return int(Decimal(x).to_integral_value(rounding=ROUNDING['ceiling']))

    def f_div(self, pos, a, b, scale, rounding='half_even'):
        if b == 0:
            raise BpaError('LOGIC.DIVISION_BY_ZERO')
        with localcontext() as c:
            c.prec = 200
            return round_decimal(Decimal(a) / Decimal(b), scale, rounding)

    def f_clamp(self, pos, x, lo, hi):
        if self.compare('>', lo, hi, pos):
            raise BpaError('MODEL.INVALID_VALUE', 'clamp bounds are reversed')
        return lo if self.compare('<', x, lo, pos) else (hi if self.compare('>', x, hi, pos) else x)

    # money
    def f_amount(self, pos, m):
        return m.amount

    def f_currency(self, pos, m):
        return m.currency

    def f_money(self, pos, amount, cur):
        if cur not in MINOR_UNITS:
            raise BpaError('MODEL.INVALID_VALUE', 'unknown ISO 4217 currency')
        return Money(Decimal(amount), cur)

    def f_convert(self, pos, m, cur, rate_ref):
        r = self.rates.get(rate_ref)
        if r is None:
            raise BpaError('MODEL.REFERENCE_INVALID', 'unknown rate %s' % rate_ref)
        if r.get('from') != m.currency or r.get('to') != cur:
            raise BpaError('MODEL.CURRENCY_MISMATCH', 'the rate converts %s to %s' % (r.get('from'), r.get('to')))
        with localcontext() as c:
            c.prec = 200
            return Money(m.amount * parse_decimal(r['rate']), cur)

    def f_allocate(self, pos, m, weights):
        if not weights or any(not _num(w) or w < 0 for w in weights) or sum(Decimal(w) for w in weights) == 0:
            raise BpaError('MODEL.INVALID_VALUE', 'weights are non-negative numbers with a positive sum')
        scale = MINOR_UNITS[m.currency]
        unit = Decimal(1).scaleb(-scale)
        total = round_decimal(m.amount, scale)
        units = int(total / unit)
        wsum = sum(Decimal(w) for w in weights)
        with localcontext() as c:
            c.prec = 200
            exact = [Decimal(units) * Decimal(w) / wsum for w in weights]
        base = [int(e.to_integral_value(rounding=ROUNDING['floor'])) for e in exact]
        rest = units - sum(base)
        order = sorted(range(len(weights)), key=lambda i: (-(exact[i] - base[i]), i))
        for i in order[:rest]:
            base[i] += 1
        return [Money(Decimal(b) * unit, m.currency) for b in base]

    # dates
    def today_date(self):
        now = self.ctx.get('now')
        if now is None:
            raise BpaError('LOGIC.TYPE_ERROR', 'the context has no now')
        return now.replace(tzinfo=datetime.timezone.utc).astimezone(ZoneInfo(self.tz)).date()

    def f_today(self, pos):
        return self.today_date()

    def f_now(self, pos):
        return self.ctx.get('now')

    def f_date(self, pos, y, m, d):
        try:
            return datetime.date(y, m, d)
        except ValueError:
            raise BpaError('MODEL.INVALID_VALUE', 'not a calendar date')

    def f_year(self, pos, d):
        return d.year

    def f_month(self, pos, d):
        return d.month

    def f_day(self, pos, d):
        return d.day

    def f_weekday(self, pos, d):
        return d.isoweekday()

    def f_add_days(self, pos, d, n):
        return d + datetime.timedelta(days=n)

    def f_add_months(self, pos, d, n):
        return _add_months(d, n)

    def f_days_between(self, pos, a, b):
        return (b - a).days

    def f_start_of_month(self, pos, d):
        return d.replace(day=1)

    def f_end_of_month(self, pos, d):
        return d.replace(day=calendar.monthrange(d.year, d.month)[1])

    def f_to_zone(self, pos, i, tz):
        try:
            z = ZoneInfo(tz)
        except Exception:
            raise BpaError('MODEL.INVALID_VALUE', 'unknown time zone')
        local = i.replace(tzinfo=datetime.timezone.utc).astimezone(z)
        off = local.utcoffset()
        mins = int(off.total_seconds()) // 60
        sign = '+' if mins >= 0 else '-'
        return local.strftime('%Y-%m-%dT%H:%M:%S.%f') + '%s%02d:%02d' % (sign, abs(mins) // 60, abs(mins) % 60)

    # calendars
    def cal(self, name):
        c = self.calendars.get(name)
        if c is None:
            raise BpaError('MODEL.REFERENCE_INVALID', 'unknown calendar %s' % name)
        return set(c.get('weekend', [6, 7])), set(c.get('holidays', []))

    def is_bd(self, d, name):
        weekend, holidays = self.cal(name)
        return d.isoweekday() not in weekend and d.isoformat() not in holidays

    def step_bd(self, d, name, direction):
        for _ in range(3660):
            d += datetime.timedelta(days=direction)
            if self.is_bd(d, name):
                return d
        raise BpaError('MODEL.INVALID_VALUE', 'the calendar has no business day within ten years')

    def f_is_business_day(self, pos, d, name):
        return self.is_bd(d, name)

    def f_add_business_days(self, pos, d, n, name):
        self.charge(abs(n))
        for _ in range(abs(n)):
            d = self.step_bd(d, name, 1 if n > 0 else -1)
        return d

    def f_next_business_day(self, pos, d, name):
        return self.step_bd(d, name, 1)

    def f_previous_business_day(self, pos, d, name):
        return self.step_bd(d, name, -1)

    def f_cutoff(self, pos, d, t, tz, name):
        if not self.is_bd(d, name):
            d = self.step_bd(d, name, 1)
        from .values import local_to_instant
        return to_instant(local_to_instant(d.isoformat(), validate_value('time', None, t), tz, 'earlier'))

    # periods
    def in_period(self, p, d, pos):
        if type(d) is not type(p.lo):
            self.rt(pos, 'the period and the value have different granularity')
        return p.lo <= d and (p.hi is None or d < p.hi)

    def f_overlaps(self, pos, p, q):
        return (q.hi is None or p.lo < q.hi) and (p.hi is None or q.lo < p.hi)

    def f_period(self, pos, lo, hi):
        if lo is None:
            return None
        if hi is not None and (type(lo) is not type(hi) or not lo < hi):
            raise BpaError('MODEL.INVALID_VALUE', 'a period runs from an earlier to a later time of one granularity')
        return Period(lo, hi)

    # references
    def lookup(self, uri, pos):
        self.charge(REF_COST)
        if not isinstance(uri, str) or not REFERENCE.match(uri):
            raise BpaError('MODEL.INVALID_VALUE', 'a reference is a canonical URI')
        return self.typed_ref(uri) if self.typed_ref else self.objects.get(uri)

    def f_ref(self, pos, uri):
        v = self.lookup(uri, pos)
        if v is None:
            raise BpaError('IAM.NOT_FOUND', 'no object at %s' % uri)
        return v

    def f_exists(self, pos, uri):
        if uri is None:
            return None
        return self.lookup(uri, pos) is not None

    # identifiers
    def f_luhn_valid(self, pos, s):
        return luhn_valid(s)

    def f_iso7064_mod97_10(self, pos, s):
        return _mod97(s)

    def f_isin_valid(self, pos, s):
        return isin_valid(s)

    def f_lei_valid(self, pos, s):
        return lei_valid(s)

    def f_iban_valid(self, pos, s):
        return iban_valid(s)


# ---- conversion between JSON and run-time values ----

def from_json(t, v, classes_fields=None):
    """Run-time value of canonical JSON v of static type t."""
    if v is None:
        return None
    if t == 'integer':
        return int(v)
    if t == 'decimal':
        return parse_decimal(v)
    if t == 'money':
        return Money(parse_decimal(v['amount']), v['currency'])
    if t == 'date':
        return to_date(v)
    if t == 'instant':
        return to_instant(v)
    if t == 'time':
        return Time(v)
    if t == 'duration':
        return parse_duration(v)
    if t == 'period':
        g = to_date if DATE.match(v['from']) else to_instant
        return Period(g(v['from']), None if v.get('to') is None else g(v['to']))
    if is_list(t):
        return [from_json(t[1], x) for x in v]
    if is_obj(t):
        return {k: from_json(t[1].get(k, 'any'), x) for k, x in v.items()}
    return v


def to_json(v):
    if isinstance(v, bool) or v is None or isinstance(v, str):
        return v
    if _int(v):
        return v if abs(v) <= 2 ** 53 - 1 else str(v)
    if isinstance(v, Decimal):
        return fmt_decimal(v)
    if isinstance(v, Money):
        return {'amount': fmt_decimal(round_decimal(v.amount, MINOR_UNITS[v.currency]), MINOR_UNITS[v.currency]),
                'currency': v.currency}
    if isinstance(v, datetime.datetime):
        return fmt_instant(v)
    if isinstance(v, datetime.date):
        return v.isoformat()
    if isinstance(v, Duration):
        return fmt_duration(v.us)
    if isinstance(v, Period):
        return {'from': to_json(v.lo), 'to': to_json(v.hi)}
    if isinstance(v, list):
        return [to_json(x) for x in v]
    if isinstance(v, dict):
        return {k: to_json(x) for k, x in v.items()}
    raise BpaError('LOGIC.TYPE_ERROR', 'value without a JSON form')


def type_name(t):
    if isinstance(t, str):
        return t
    if is_list(t):
        return 'list<%s>' % type_name(t[1])
    return 'object'


# ---- entry points (adapter operations) ----

def _class_type(classes, name, custom=None):
    from .classes import effective_class
    eff = effective_class(classes, name)
    return ('obj', {k: field_type(f, custom) for k, f in eff['fields'].items()})


def _params_type(params, custom=None):
    return ('obj', {k: field_type(v, custom) for k, v in (params or {}).items()})


def _target(target):
    if target is None:
        return None, None
    t = target.get('type')
    c = target.get('constraints') or {}
    if t == 'decimal':
        if 'scale' not in c:
            return None, None
        return (c['scale'], c.get('rounding', 'half_even')), t
    if t == 'money':
        return (c.get('scale', 2), c.get('rounding', 'half_even')), t
    return None, t


def prepare(i, custom=None):
    """(ast, env, static type) of an expression input."""
    classes = i.get('classes') or []
    this_t = _class_type(classes, i['class'], custom) if i.get('class') else None
    env = Env(this=this_t, params=_params_type(i.get('params_types'), custom) if i.get('params_types') is not None
              else None, old=this_t if i.get('old') is not None or i.get('with_old') else None,
              target=_target(i.get('target'))[0])
    ast = parse(i['expr'])
    t = check(ast, env)
    return ast, env, t


def check_expr(i):
    """STD-EXPR-001, -002: the static type of an expression."""
    _, _, t = prepare(i)
    return {'type': type_name(t)}


def eval_expr(i, typed_ref=None, custom=None):
    """STD-EXPR-001…005: evaluate an expression; the result is canonical JSON, assigned to
    `target` when given (rounded to its scale)."""
    ast, env, t = prepare(i, custom)
    this_t = env.roots['this']
    ctx = dict(i.get('ctx') or {})
    if 'now' in ctx and ctx['now'] is not None:
        ctx['now'] = to_instant(ctx['now'])
    ev = Evaluator(env, ctx=ctx, objects=i.get('objects'), calendars=i.get('calendars'), rates=i.get('rates'),
                   max_units=i.get('max_units', MAX_UNITS), max_items=i.get('max_items', MAX_ITEMS),
                   business_tz=i.get('business_tz', 'UTC'),
                   this=from_json(this_t, i.get('this')) if this_t else None,
                   params=from_json(env.roots['params'], i.get('params')) if env.roots['params'] else None,
                   old=from_json(this_t, i.get('old')) if env.roots['old'] and this_t else None,
                   typed_ref=typed_ref)
    if i.get('objects') and not typed_ref:
        classes = i.get('classes') or []
        ev.typed_ref = lambda uri: (None if uri not in i['objects'] else
                                    from_json(_class_type(classes, uri.split('/')[3], custom)
                                              if any(d.get('name') == uri.split('/')[3] for d in classes) else 'any',
                                              i['objects'][uri]))
    v = to_json(ev.ev(ast))
    target = i.get('target')
    if target is not None and v is not None:
        c = dict(target.get('constraints') or {})
        if target.get('type') == 'decimal' and 'scale' in c and isinstance(v, str):
            v = fmt_decimal(round_decimal(parse_decimal(v), c['scale'], c.get('rounding', 'half_even')), c['scale'])
        v = validate_value(target['type'], c or None, v)
    return {'value': v, 'type': type_name(t)}

