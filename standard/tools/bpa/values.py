"""Value types, decimals, money, names, time and classification (STD-FND-015…027)."""
import datetime
import re
from decimal import Decimal, localcontext, ROUND_HALF_EVEN, ROUND_HALF_UP, ROUND_DOWN, ROUND_UP, \
    ROUND_CEILING, ROUND_FLOOR, InvalidOperation
from zoneinfo import ZoneInfo

from . import BpaError
from .canon import MAX_SAFE, canonical, nfc

# ---- names (STD-FND-016, STD-VER-013, STD-FND-015) ----

SLUG = r'[a-z0-9][a-z0-9_-]{0,62}'
GRAMMARS = {
    'slug': re.compile(r'\A' + SLUG + r'\Z'),
    'class-name': re.compile(r'\A[A-Z][A-Za-z0-9]{0,63}\Z'),
    'member-name': re.compile(r'\A[a-z][a-z0-9_]{0,63}\Z'),
    'namespace': re.compile(r'\A' + SLUG + r'(\.' + SLUG + r')*\Z'),
    'uuid': re.compile(r'\A[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\Z'),
    'uuid-v7': re.compile(r'\A[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\Z'),
}
BRANCH_KINDS = ('release', 'draft', 'feature', 'agent', 'overlay', 'loader', 'evolution', 'correction',
                'sandbox', 'device')


def check_name(grammar, text):
    if grammar == 'branch-name':
        parts = text.split('/')
        ok = text == 'main' or (len(parts) >= 2 and parts[0] in BRANCH_KINDS
                                and all(GRAMMARS['slug'].match(p) for p in parts))
        if not ok:
            raise BpaError('VER.INVALID_BRANCH_NAME')
        return True
    if grammar not in GRAMMARS:
        raise BpaError('MODEL.INVALID_VALUE', 'unknown grammar')
    if not isinstance(text, str) or not GRAMMARS[grammar].match(text):
        raise BpaError('MODEL.INVALID_NAME')
    return True

# ---- decimals (STD-FND-022) ----

DECIMAL = re.compile(r'\A-?(0|[1-9][0-9]*)(\.[0-9]+)?\Z')
ROUNDING = {'half_even': ROUND_HALF_EVEN, 'half_up': ROUND_HALF_UP, 'down': ROUND_DOWN, 'up': ROUND_UP,
            'ceiling': ROUND_CEILING, 'floor': ROUND_FLOOR}


def parse_decimal(text):
    if not isinstance(text, str) or not DECIMAL.match(text):
        raise BpaError('MODEL.INVALID_VALUE', 'not a decimal string')
    return Decimal(text)


def fmt_decimal(d, scale=None):
    """Canonical text: exactly `scale` fraction digits when a scale is declared, otherwise
    no trailing fraction zeros; never a negative zero."""
    if scale is None:
        d = d.normalize() if d != 0 else Decimal(0)
        s = format(d, 'f')
    else:
        s = format(d.quantize(Decimal(1).scaleb(-scale)), 'f')
    if s.startswith('-') and Decimal(s) == 0:
        s = s[1:]
    return s


def round_decimal(d, scale, rounding='half_even'):
    if rounding not in ROUNDING:
        raise BpaError('MODEL.INVALID_VALUE', 'unknown rounding mode')
    with localcontext() as ctx:
        ctx.prec = 200
        return d.quantize(Decimal(1).scaleb(-scale), rounding=ROUNDING[rounding])


def decimal_op(op, a, b=None, scale=None, rounding=None):
    with localcontext() as ctx:
        ctx.prec = 200
        x = parse_decimal(a)
        if op == 'canonical':
            if scale is not None and -x.as_tuple().exponent > scale:
                raise BpaError('MODEL.INVALID_VALUE', 'more fraction digits than the scale')
            return fmt_decimal(x, scale)
        if op == 'round':
            return fmt_decimal(round_decimal(x, scale, rounding or 'half_even'), scale)
        y = parse_decimal(b)
        if op == 'div':
            if scale is None or rounding is None:
                raise BpaError('LOGIC.TYPE_ERROR', 'division needs a scale and a rounding mode')
            if y == 0:
                raise BpaError('LOGIC.DIVISION_BY_ZERO')
            return fmt_decimal(round_decimal(x / y, scale, rounding), scale)
        r = {'add': lambda: x + y, 'sub': lambda: x - y, 'mul': lambda: x * y}.get(op)
        if r is None:
            raise BpaError('MODEL.INVALID_VALUE', 'unknown decimal operation')
        v = r()
        if scale is not None:
            return fmt_decimal(round_decimal(v, scale, rounding or 'half_even'), scale)
        return fmt_decimal(v)

# ---- money (STD-FND-023) ----

MINOR_UNITS = {'USD': 2, 'EUR': 2, 'GBP': 2, 'CHF': 2, 'CAD': 2, 'CNY': 2, 'JPY': 0, 'KRW': 0,
               'BHD': 3, 'KWD': 3}


def money_value(v, constraints=None):
    constraints = constraints or {}
    if not isinstance(v, dict) or set(v) != {'amount', 'currency'}:
        raise BpaError('MODEL.INVALID_VALUE', 'money is {amount, currency}')
    cur = v['currency']
    if cur not in MINOR_UNITS:
        raise BpaError('MODEL.INVALID_VALUE', 'unknown ISO 4217 currency')
    if 'currencies' in constraints and cur not in constraints['currencies']:
        raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'currency not allowed')
    scale = constraints.get('scale', MINOR_UNITS[cur])
    amount = round_decimal(parse_decimal(v['amount']), scale, constraints.get('rounding', 'half_even'))
    _bounds(amount, constraints, parse_decimal)
    return {'amount': fmt_decimal(amount, scale), 'currency': cur}


def money_op(op, a, b):
    a, b = money_value(a), money_value(b)
    if a['currency'] != b['currency']:
        raise BpaError('MODEL.CURRENCY_MISMATCH')
    x, y = Decimal(a['amount']), Decimal(b['amount'])
    r = {'add': x + y, 'sub': x - y}.get(op)
    if r is None:
        raise BpaError('MODEL.INVALID_VALUE', 'unknown money operation')
    return {'amount': fmt_decimal(r, MINOR_UNITS[a['currency']]), 'currency': a['currency']}

# ---- time (STD-FND-024) ----

DATE = re.compile(r'\A[0-9]{4}-[0-9]{2}-[0-9]{2}\Z')
TIME = re.compile(r'\A([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9](\.[0-9]{6})?\Z')
INSTANT = re.compile(r'\A([0-9]{4}-[0-9]{2}-[0-9]{2})T([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9]\.[0-9]{6}Z\Z')
DURATION = re.compile(r'\AP(?!\Z)([0-9]+Y)?([0-9]+M)?([0-9]+W)?([0-9]+D)?(T(?=[0-9])([0-9]+H)?([0-9]+M)?([0-9]+(\.[0-9]{1,6})?S)?)?\Z')


def parse_date(s):
    if not isinstance(s, str) or not DATE.match(s):
        raise BpaError('MODEL.INVALID_VALUE', 'not a date')
    try:
        return datetime.date.fromisoformat(s)
    except ValueError:
        raise BpaError('MODEL.INVALID_VALUE', 'not a calendar date')


def parse_instant(s):
    if not isinstance(s, str) or not INSTANT.match(s):
        raise BpaError('MODEL.INVALID_VALUE', 'not a canonical instant')
    parse_date(s[:10])
    return datetime.datetime.strptime(s, '%Y-%m-%dT%H:%M:%S.%fZ').replace(tzinfo=datetime.timezone.utc)


def fmt_instant(dt):
    return dt.astimezone(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%S.%fZ')


def local_to_instant(date, time, tz, disambiguation=None):
    """Local date and time in a named zone to an instant. A non-existent local time resolves to
    the first instant after the gap; a repeated one to the earlier instant unless
    disambiguation is `later`."""
    d = parse_date(date)
    if not TIME.match(time):
        raise BpaError('MODEL.INVALID_VALUE', 'not a time')
    t = datetime.time.fromisoformat(time)
    try:
        zone = ZoneInfo(tz)
    except Exception:
        raise BpaError('MODEL.INVALID_VALUE', 'unknown time zone')
    naive = datetime.datetime.combine(d, t)
    first = naive.replace(tzinfo=zone, fold=0)
    second = naive.replace(tzinfo=zone, fold=1)
    u0, u1 = first.astimezone(datetime.timezone.utc), second.astimezone(datetime.timezone.utc)
    exists0 = u0.astimezone(zone).replace(tzinfo=None) == naive
    exists1 = u1.astimezone(zone).replace(tzinfo=None) == naive
    if exists0 and exists1 and u0 != u1:           # repeated local time
        return fmt_instant(u1 if disambiguation == 'later' else u0)
    if exists0 or exists1:
        return fmt_instant(u0 if exists0 else u1)
    # gap: the first instant after it is the transition instant
    lo, hi = min(u0, u1), max(u0, u1)
    while hi - lo > datetime.timedelta(microseconds=1):
        mid = lo + (hi - lo) / 2
        if mid.astimezone(zone).replace(tzinfo=None) >= naive:
            hi = mid
        else:
            lo = mid
    return fmt_instant(hi.replace(microsecond=0) if hi.microsecond == 0 else hi)

# ---- classification (STD-FND-027) ----

LEVELS = ('public', 'internal', 'confidential', 'personal', 'sensitive_personal')
EXPLICIT = ('personal', 'sensitive_personal')


def classification_covers(clearances, level):
    for c in list(clearances) + [level]:
        if c not in LEVELS:
            raise BpaError('MODEL.INVALID_VALUE', 'unknown classification')
    if level in clearances:
        return True
    if level in EXPLICIT:
        return False
    return any(LEVELS.index(c) > LEVELS.index(level) for c in clearances)

# ---- value types (STD-FND-020, -021, -025) ----

BLOCKS = ('paragraph', 'heading', 'list', 'list_item', 'table', 'row', 'cell', 'quote', 'code', 'image',
          'section', 'clause_ref', 'binding', 'logic_block', 'page_break')
MARKS = ('bold', 'italic', 'underline', 'strike', 'link', 'comment_ref')
TYPES = ('text', 'integer', 'decimal', 'money', 'quantity', 'boolean', 'date', 'time', 'instant', 'period',
         'duration', 'enum', 'reference', 'composite', 'list', 'map', 'document', 'blob', 'json')
CONTENT_ID = re.compile(r'\Asha256:[0-9a-f]{64}\Z')


def _bounds(v, c, conv):
    if 'min' in c and v < conv(c['min']):
        raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'below min')
    if 'max' in c and v > conv(c['max']):
        raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'above max')


def _integer(v):
    if isinstance(v, bool):
        raise BpaError('MODEL.INVALID_VALUE', 'boolean is not an integer')
    if isinstance(v, int):
        if abs(v) > MAX_SAFE:
            raise BpaError('MODEL.INVALID_VALUE', 'integer beyond 2^53-1 must be a string')
        return v
    if isinstance(v, str) and re.match(r'\A-?[1-9][0-9]*\Z', v) and abs(int(v)) > MAX_SAFE:
        return int(v)
    raise BpaError('MODEL.INVALID_VALUE', 'not an integer')


def _document(node):
    if not isinstance(node, dict) or 'kind' not in node:
        raise BpaError('MODEL.INVALID_VALUE', 'document node needs a kind')
    if set(node) - {'kind', 'attrs', 'text', 'marks', 'children'}:
        raise BpaError('MODEL.INVALID_VALUE', 'unknown document node member')
    if node['kind'] not in BLOCKS + ('text',):
        raise BpaError('MODEL.INVALID_VALUE', 'unknown document node kind')
    for m in node.get('marks', []):
        if m not in MARKS:
            raise BpaError('MODEL.INVALID_VALUE', 'unknown inline mark')
    if node['kind'] == 'text' and 'children' in node:
        raise BpaError('MODEL.INVALID_VALUE', 'text leaves have no children')
    out = dict(node)
    if 'text' in out:
        out['text'] = nfc(out['text'])
    if 'children' in node:
        out['children'] = [_document(ch) for ch in node['children']]
    return out


def validate_value(type_, constraints, value):
    """Return the canonical value, or fail with MODEL.INVALID_VALUE (representation),
    MODEL.CONSTRAINT_VIOLATED (constraint) or MODEL.UNKNOWN_TYPE."""
    c = constraints or {}
    if type_ not in TYPES:
        raise BpaError('MODEL.UNKNOWN_TYPE')
    if value is None:
        return None                                   # explicitly cleared (STD-FND-021)
    if type_ == 'text':
        if not isinstance(value, str):
            raise BpaError('MODEL.INVALID_VALUE', 'not text')
        v = nfc(value)
        n = len(v)
        if n < c.get('min_length', 0) or n > c.get('max_length', n):
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'length')
        if 'pattern' in c and not re.fullmatch(c['pattern'], v):
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'pattern')
        return v
    if type_ == 'integer':
        v = _integer(value)
        _bounds(v, c, int)
        return value
    if type_ == 'decimal':
        d = parse_decimal(value)
        if 'scale' in c:
            d = round_decimal(d, c['scale'], c.get('rounding', 'half_even'))
        if 'precision' in c and len(d.as_tuple().digits) > c['precision']:
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'precision')
        _bounds(d, c, parse_decimal)
        return fmt_decimal(d, c.get('scale'))
    if type_ == 'money':
        return money_value(value, c)
    if type_ == 'quantity':
        if not isinstance(value, dict) or set(value) != {'value', 'unit'} or not isinstance(value['unit'], str):
            raise BpaError('MODEL.INVALID_VALUE', 'quantity is {value, unit}')
        d = parse_decimal(value['value'])
        if 'units' in c and value['unit'] not in c['units']:
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'unit not allowed')
        _bounds(d, c, parse_decimal)
        return {'value': fmt_decimal(d), 'unit': value['unit']}
    if type_ == 'boolean':
        if not isinstance(value, bool):
            raise BpaError('MODEL.INVALID_VALUE', 'not a boolean')
        return value
    if type_ == 'date':
        d = parse_date(value)
        _bounds(d, c, parse_date)
        return value
    if type_ == 'time':
        if not isinstance(value, str) or not TIME.match(value):
            raise BpaError('MODEL.INVALID_VALUE', 'not a time')
        return value
    if type_ == 'instant':
        i = parse_instant(value)
        _bounds(i, c, parse_instant)
        return value
    if type_ == 'period':
        if not isinstance(value, dict) or set(value) != {'from', 'to'}:
            raise BpaError('MODEL.INVALID_VALUE', 'period is {from, to}')
        gran = c.get('granularity', 'date')
        conv = parse_date if gran == 'date' else parse_instant
        f = conv(value['from'])
        if value['to'] is not None and conv(value['to']) <= f:
            raise BpaError('MODEL.INVALID_VALUE', 'empty or inverted period')
        return value
    if type_ == 'duration':
        if not isinstance(value, str) or not DURATION.match(value):
            raise BpaError('MODEL.INVALID_VALUE', 'not an ISO 8601 duration')
        return value
    if type_ == 'enum':
        if not isinstance(value, str):
            raise BpaError('MODEL.INVALID_VALUE', 'not an enum value')
        if value not in c.get('allowed_values', []):
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'not an allowed value')
        return value
    if type_ == 'reference':
        raise BpaError('MODEL.UNKNOWN_TYPE', 'reference values are not in release 0.1 vectors')
    if type_ == 'composite':
        members = c.get('members', {})
        if not isinstance(value, dict):
            raise BpaError('MODEL.INVALID_VALUE', 'not a composite')
        if set(value) - set(members):
            raise BpaError('MODEL.INVALID_VALUE', 'undeclared member')
        out = {}
        for name, m in members.items():
            if name in value:
                out[name] = validate_value(m['type'], m.get('constraints'), value[name])
            elif m.get('required'):
                raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'required member missing')
        return out
    if type_ == 'list':
        if not isinstance(value, list):
            raise BpaError('MODEL.INVALID_VALUE', 'not a list')
        if len(value) < c.get('min_items', 0) or len(value) > c.get('max_items', len(value)):
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'item count')
        item = c.get('item_type')
        out = [validate_value(item['type'], item.get('constraints'), v) for v in value] if item else list(value)
        if 'key' in c:
            keys = [canonical(v.get(c['key'])) if isinstance(v, dict) else None for v in out]
            if None in keys or len(set(keys)) != len(keys):
                raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'keyed list needs unique keys')
        return out
    if type_ == 'map':
        if not isinstance(value, dict):
            raise BpaError('MODEL.INVALID_VALUE', 'not a map')
        vt = c.get('value_type')
        out = {}
        for k, v in value.items():
            if 'key_pattern' in c and not re.fullmatch(c['key_pattern'], k):
                raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'key pattern')
            out[k] = validate_value(vt['type'], vt.get('constraints'), v) if vt else v
        return out
    if type_ == 'document':
        return _document(value)
    if type_ == 'blob':
        if not isinstance(value, dict) or set(value) != {'content_id', 'size', 'media_type', 'name'}:
            raise BpaError('MODEL.INVALID_VALUE', 'blob is {content_id, size, media_type, name}')
        if not isinstance(value['content_id'], str) or not CONTENT_ID.match(value['content_id']):
            raise BpaError('MODEL.INVALID_VALUE', 'not a content identifier')
        if isinstance(value['size'], bool) or not isinstance(value['size'], int) or value['size'] < 0:
            raise BpaError('MODEL.INVALID_VALUE', 'size')
        if 'media_types' in c and value['media_type'] not in c['media_types']:
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'media type')
        if 'max_size' in c and value['size'] > c['max_size']:
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'size')
        return value
    if type_ == 'json':
        text = canonical(value)
        if 'max_size' in c and len(text.encode('utf-8')) > c['max_size']:
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'size')
        return value
    raise BpaError('MODEL.UNKNOWN_TYPE')
