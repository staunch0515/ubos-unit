"""Decision tables of BPA 1.0-rc8 (PH-2 step 1c, DEC-091): evaluation under the hit policies
(STD-LGS-040, STD-ISA-053) and the authoring check for gaps and overlaps. Mixed into the
reference store; `check_table` is a pure operation."""
import datetime
import itertools
from decimal import Decimal

from . import BpaError
from .canon import canonical
from .values import validate_value, parse_decimal, fmt_decimal

HIT_POLICIES = ('unique', 'first', 'priority', 'collect')
AGGREGATORS = ('sum', 'min', 'max', 'count')
TABLE_CLASS = 'DecisionTable'
MAX_POINTS = 10000
ANY = '-'


def check_decl(t):
    """The members of a decision table (DAT-DecisionTable)."""
    if not isinstance(t, dict):
        raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a decision table is a map')
    for k in ('inputs', 'outputs', 'rows', 'hit_policy'):
        if t.get(k) is None:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'decision table member %s' % k)
    if t['hit_policy'] not in HIT_POLICIES:
        raise BpaError('MODEL.INVALID_VALUE', 'hit_policy is unique, first, priority or collect')
    agg = t.get('aggregator')
    if agg is not None and (agg not in AGGREGATORS or t['hit_policy'] != 'collect'):
        raise BpaError('MODEL.INVALID_VALUE', 'an aggregator (sum, min, max, count) goes with collect')
    for col in ('inputs', 'outputs'):
        if not isinstance(t[col], list) or not t[col] or not all(
                isinstance(f, dict) and isinstance(f.get('name'), str) and isinstance(f.get('type'), str) for f in t[col]):
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', '%s are {name, type} items' % col)
        if len({f['name'] for f in t[col]}) != len(t[col]):
            raise BpaError('MODEL.INVALID_VALUE', '%s have unique names' % col)
    if agg in ('sum', 'min', 'max') and any(o['type'] not in ('integer', 'decimal', 'money') for o in t['outputs']):
        raise BpaError('MODEL.INVALID_VALUE', 'sum, min and max aggregate numeric outputs')
    names = {f['name'] for f in t['inputs']}
    outs = {f['name']: f for f in t['outputs']}
    if not isinstance(t['rows'], list):
        raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'rows')
    for r in t['rows']:
        if not isinstance(r, dict) or not isinstance(r.get('then'), dict):
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a row is {when, then, priority}')
        when = r.get('when') or {}
        if not isinstance(when, dict) or any(k not in names for k in when):
            raise BpaError('MODEL.INVALID_VALUE', 'a row condition names an input')
        if any(k not in outs for k in r['then']) or any(k not in r['then'] for k in outs):
            raise BpaError('MODEL.INVALID_VALUE', 'a row gives every output')
        for k, v in r['then'].items():
            validate_value(outs[k]['type'], outs[k].get('constraints'), v)
        if t['hit_policy'] == 'priority' and not isinstance(r.get('priority'), int):
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'rows of a priority table have a priority')


def _ptypes(t):
    return {f['name']: f for f in t['inputs']}


def _cell(t, cell, inputs, cache):
    """An L1 boolean over `params` (the inputs); `-` matches any value."""
    if cell is None or cell == ANY:
        return True
    from .expr import eval_expr
    key = (cell, canonical(inputs))
    if key not in cache:
        v = eval_expr({'expr': cell, 'params_types': _ptypes(t), 'params': inputs})['value']
        if not isinstance(v, bool):
            raise BpaError('RULE.PARSE_ERROR', 'a condition is an L1 boolean')
        cache[key] = v
    return cache[key]


def matching_rows(t, inputs, cache=None):
    cache = {} if cache is None else cache
    return [n for n, r in enumerate(t['rows'])
            if all(_cell(t, (r.get('when') or {}).get(f['name']), inputs, cache) for f in t['inputs'])]


def check_inputs(t, inputs):
    if not isinstance(inputs, dict):
        raise BpaError('MODEL.INVALID_VALUE', 'inputs are a map')
    out = {}
    for f in t['inputs']:
        if inputs.get(f['name']) is None:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'input %s' % f['name'])
        out[f['name']] = validate_value(f['type'], f.get('constraints'), inputs[f['name']])
    if any(k not in out for k in inputs):
        raise BpaError('MODEL.INVALID_VALUE', 'unknown input')
    return out


def _num(v):
    if isinstance(v, dict):
        return v['currency'], parse_decimal(v['amount'])
    return '', parse_decimal(str(v))


def evaluate(t, inputs):
    """STD-LGS-040: `{outputs, rows}` of a table under its hit policy."""
    check_decl(t)
    inputs = check_inputs(t, inputs)
    rows = matching_rows(t, inputs)
    pol = t['hit_policy']
    if pol in ('unique', 'first', 'priority') and not rows:
        raise BpaError('RULE.NO_MATCH', {'inputs': inputs})
    if pol == 'unique':
        if len(rows) > 1:
            raise BpaError('RULE.MULTIPLE_MATCH', {'rows': rows})
        return {'outputs': dict(t['rows'][rows[0]]['then']), 'rows': rows}
    if pol == 'first':
        return {'outputs': dict(t['rows'][rows[0]]['then']), 'rows': rows[:1]}
    if pol == 'priority':
        best = max(rows, key=lambda n: (t['rows'][n]['priority'], -n))
        return {'outputs': dict(t['rows'][best]['then']), 'rows': [best]}
    agg = t.get('aggregator')
    if agg is None:
        return {'outputs': [dict(t['rows'][n]['then']) for n in rows], 'rows': rows}
    if agg == 'count':
        return {'outputs': {'count': len(rows)}, 'rows': rows}
    out = {}
    for f in t['outputs']:
        vals = [t['rows'][n]['then'][f['name']] for n in rows]
        if not vals:
            out[f['name']] = None
            continue
        cur = {_num(v)[0] for v in vals}
        if len(cur) > 1:
            raise BpaError('MODEL.CURRENCY_MISMATCH')
        nums = [_num(v)[1] for v in vals]
        r = sum(nums, Decimal(0)) if agg == 'sum' else min(nums) if agg == 'min' else max(nums)
        if f['type'] == 'money':
            from .values import MINOR_UNITS
            c = cur.pop()
            out[f['name']] = {'amount': fmt_decimal(r, MINOR_UNITS[c]), 'currency': c}
        elif f['type'] == 'integer':
            out[f['name']] = int(r)
        else:
            out[f['name']] = fmt_decimal(r)
    return {'outputs': out, 'rows': rows}


# ---- the authoring check (FR-RULE-042) ----

def _literals(node, name, out):
    """Literals compared with params.<name> in a condition AST."""
    if isinstance(node, tuple):
        if node[0] == 'cmp':
            sides = (node[3], node[4])
            for a, b in (sides, sides[::-1]):
                if a[0] == 'member' and a[2] == ('path', a[2][1], 'params') and a[3] == name:
                    v = _lit(b)
                    if v is not None:
                        out.append(v)
        elif node[0] == 'in' and node[2][0] == 'member' and node[2][3] == name and node[3][0] == 'list':
            for x in node[3][2]:
                v = _lit(x)
                if v is not None:
                    out.append(v)
        for x in node[2:]:
            if isinstance(x, tuple):
                _literals(x, name, out)
            elif isinstance(x, list):
                for y in x:
                    _literals(y, name, out)


def _lit(node):
    if node[0] == 'lit':
        return node[2]
    if node[0] == 'call' and node[2] == 'date' and len(node[3]) == 1 and node[3][0][0] == 'lit':
        return node[3][0][2]
    if node[0] == 'neg' and node[2][0] == 'lit' and not isinstance(node[2][2], str):
        return -node[2][2]
    return None


def _points(t, f):
    """Representative values of an input: its domain when enumerable, the boundaries of its
    conditions and the values between and around them when range-based; None otherwise."""
    from .expr import parse
    typ = f['type']
    if typ == 'boolean':
        return [True, False]
    if typ == 'enum':
        return list((f.get('constraints') or {}).get('allowed_values') or [])
    if typ not in ('integer', 'decimal', 'date'):
        return None
    lits = []
    for r in t['rows']:
        cell = (r.get('when') or {}).get(f['name'])
        if cell and cell != ANY:
            _literals(parse(cell), f['name'], lits)
    if typ == 'date':
        ds = sorted({datetime.date.fromisoformat(str(x)) for x in lits})
        if not ds:
            return ['2000-01-01']
        pts = {ds[0] - datetime.timedelta(days=1)}
        for d in ds:
            pts |= {d - datetime.timedelta(days=1), d, d + datetime.timedelta(days=1)}
        return [d.isoformat() for d in sorted(pts)]
    cs = sorted({Decimal(str(x)) for x in lits})
    if not cs:
        return [0 if typ == 'integer' else '0']
    step = Decimal(1) if typ == 'integer' else Decimal('0.01')
    pts = set()
    for c in cs:
        pts |= {c - step, c, c + step}
    for a, b in zip(cs, cs[1:]):
        if typ == 'integer':
            pts.add((a + b) // 2)
        else:
            pts.add((a + b) / 2)
    pts = sorted(pts)
    if typ == 'integer':
        return [int(p) for p in pts]
    return [fmt_decimal(p) for p in pts]


def check_table(t):
    """FR-RULE-042: the gaps (input combinations no row matches) and the overlaps the hit
    policy forbids, over the representative values of the enumerable and range-based inputs;
    other inputs are listed as unchecked."""
    check_decl(t)
    axes, unchecked = [], []
    for f in t['inputs']:
        pts = _points(t, f)
        if pts is None or not pts:
            unchecked.append(f['name'])
        else:
            axes.append((f['name'], pts))
    if unchecked:
        return {'gaps': [], 'overlaps': [], 'unchecked': unchecked, 'points': 0}
    total = 1
    for _, pts in axes:
        total *= len(pts)
    if total > MAX_POINTS:
        return {'gaps': [], 'overlaps': [], 'unchecked': [n for n, _ in axes], 'points': 0}
    gaps, overlaps, cache = [], [], {}
    for combo in itertools.product(*[pts for _, pts in axes]):
        point = {n: v for (n, _), v in zip(axes, combo)}
        rows = matching_rows(t, point, cache)
        if not rows and t['hit_policy'] != 'collect':
            gaps.append(point)
        if len(rows) > 1:
            if t['hit_policy'] == 'unique':
                overlaps.append({'point': point, 'rows': rows})
            elif t['hit_policy'] == 'priority':
                top = max(t['rows'][n]['priority'] for n in rows)
                tied = [n for n in rows if t['rows'][n]['priority'] == top]
                if len(tied) > 1:
                    overlaps.append({'point': point, 'rows': tied})
    return {'gaps': gaps, 'overlaps': overlaps, 'unchecked': [], 'points': total}


class Tables:

    def _table(self, st, ref):
        """(object id, version hash, body) of a decision table by object id or URI."""
        oid = ref
        if isinstance(ref, str) and ref.startswith('ubos://'):
            parts = ref.split('/')
            oid = next((k for k, o in sorted(st['objects'].items())
                        if o['class'] == parts[3] and o['slug'] == parts[4]), None)
        o = st['objects'].get(oid)
        if o is None or o['class'] != TABLE_CLASS:
            raise BpaError('IAM.NOT_FOUND', 'unknown decision table')
        return oid, o

    def decide_table(self, i):
        """STD-LGS-040: evaluate a decision table at `asof` (default today)."""
        from .store import tl_at
        st = self._state(i.get('branch', 'main'), i)[0]
        oid, o = self._table(st, i.get('table'))
        seg = tl_at(o['timeline'], i.get('asof') or self._today())
        if seg is None:
            raise BpaError('QRY.NOT_FOUND_AT_TIME', 'no version of the table at asof')
        return evaluate(self.versions[seg[2]]['body'], i.get('inputs'))
