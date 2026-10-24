"""Governance sheets (BPA 1.0-rc2, STD-LGS-001…023; PH-1 step 10b, DEC-066): the textual
grammar and its lossless AST, the property registry with the kernel properties of ABI 1.0,
matching, specificity, the cascade and its explanation.

Sheets are objects of the genesis class `GovernanceSheet` and Buk properties objects of
`PropertyDecl`; both are written with `commit`. The adapter operations are `parse_sheet`,
`check_sheet` and `specificity` (pure) and `cascade` and `explain_cascade` (stateful)."""
import re

from . import BpaError
from .canon import content_id
from .classes import effective_class, linearize, depth, _registry
from .values import GRAMMARS, LEVELS, validate_value
from . import expr as _expr

LAYERS = ('platform', 'industry', 'vendor', 'tenant', 'vae', 'unit', 'object')
CTX_KEYS = ('branch_kind', 'channel', 'role', 'principal_kind', 'vae')
ENFORCEMENTS = ('commit', 'action', 'read', 'view', 'retention', 'notify')
COMBINES = ('override', 'max', 'min', 'union', 'all_true', 'any_true')
REASONS = ('won', 'combined', 'same_value', 'locked_elsewhere', 'lower_layer', 'less_specific', 'lower_priority',
           'earlier_position')


def _enum(values):
    return {'type': 'enum', 'constraints': {'allowed_values': list(values)}}


def _map(value):
    return {'type': 'map', 'constraints': {'value_type': value, 'key_pattern': GRAMMARS['member-name'].pattern[2:-2]}}


# STD-LGS-011: the kernel properties of ABI 1.0 (DEC-066 item 8 gives the combine rules).
KERNEL_PROPERTIES = {
    'audit_level': ('commit', 'override', _enum(('basic', 'standard', 'strict'))),
    'require_signature': ('action', 'override', {'type': 'boolean'}),
    'approvals': ('action', 'max', {'type': 'composite', 'constraints': {'members': {
        'count': {'type': 'integer', 'required': True, 'constraints': {'min': 1}},
        'roles': {'type': 'list', 'constraints': {'item_type': {'type': 'text'}}},
        'order': _enum(('any', 'sequence')),
        # since 1.0-rc8 (STD-LGS-041, -042, DEC-091)
        'principals': {'type': 'list', 'constraints': {'item_type': {'type': 'text'}}},
        'incompatible': {'type': 'list', 'constraints': {'item_type': {'type': 'text'}}},
        'expires_in': {'type': 'duration'}}}}),
    'maker_checker': ('action', 'override', {'type': 'boolean'}),
    'read_only': ('commit', 'override', {'type': 'boolean'}),
    'field_required': ('commit', 'union', {'type': 'list', 'constraints': {'item_type': {'type': 'text'}}}),
    'field_default': ('commit', 'override', _map({'type': 'json'})),
    'field_visibility': ('view', 'override', _map(_enum(('visible', 'hidden', 'read_only')))),
    'mask': ('read', 'override', _map(_enum(('hide', 'partial', 'hash')))),
    'retention': ('retention', 'override', {'type': 'composite', 'constraints': {'members': {
        'min': {'type': 'duration'}, 'max': {'type': 'duration'}, 'trigger': {'type': 'text'}}}}),
    'classification': ('commit', 'max', _map(_enum(LEVELS))),
    'notify': ('notify', 'union', {'type': 'list', 'constraints': {'item_type': {'type': 'json'}}}),
    'write_policy': ('commit', 'max', _enum(('direct', 'change_set'))),
    'rate_limit': ('action', 'override', {'type': 'composite', 'constraints': {'members': {
        'per': {'type': 'enum', 'required': True, 'constraints': {'allowed_values': ['second', 'minute', 'hour', 'day']}},
        'count': {'type': 'integer', 'required': True, 'constraints': {'min': 1}}}}}),
    # STD-LGS-051 (1.0-rc8, DEC-093): access rules over authorization relations.
    'access': ('action', 'union', {'type': 'list', 'constraints': {'item_type': {'type': 'composite', 'constraints': {
        'members': {'action': {'type': 'text', 'required': True}, 'allow_if': {'type': 'text', 'required': True}}}}}}),
    # STD-LGS-030, -031 (DEC-067): validations, invariants and decisions declared in sheets.
    'validations': ('commit', 'union', {'type': 'list', 'constraints': {'item_type': {'type': 'composite', 'constraints': {'members': {
        'name': {'type': 'text', 'required': True}, 'rule': {'type': 'text', 'required': True},
        'severity': {'type': 'enum', 'required': True, 'constraints': {'allowed_values': ['error', 'warning', 'info']}},
        'message': {'type': 'text'}}}}}}),
    'invariants': ('commit', 'union', {'type': 'list', 'constraints': {'item_type': {'type': 'composite', 'constraints': {'members': {
        'name': {'type': 'text', 'required': True},
        'scope': {'type': 'list', 'required': True, 'constraints': {'item_type': {'type': 'text'}, 'min_items': 1}},
        'rule': {'type': 'text', 'required': True},
        'severity': {'type': 'enum', 'required': True, 'constraints': {'allowed_values': ['error', 'warning', 'info']}},
        'message': {'type': 'text'}}}}}}),
    'decisions': ('action', 'union', {'type': 'list', 'constraints': {'item_type': {'type': 'composite', 'constraints': {'members': {
        'name': {'type': 'text', 'required': True}, 'when': {'type': 'text'},
        'outcome': {'type': 'enum', 'required': True, 'constraints': {'allowed_values': ['allow', 'deny', 'require']}},
        'steps': {'type': 'list', 'constraints': {'item_type': {'type': 'enum', 'constraints': {'allowed_values': [
            'approval', 'signature', 'reason', 'step_up', 'second_person']}}}},
        'message': {'type': 'text'}}}}}}),
}
# Orders of the `max` properties whose values are not numbers.
ORDERS = {'classification': LEVELS, 'write_policy': ('direct', 'change_set')}


def registry(properties=()):
    """The property registry: the kernel properties plus the given `DAT-PropertyDecl` items
    (STD-LGS-010); Buk properties are namespaced and cannot shadow kernel properties."""
    out = {}
    for name, (enf, comb, schema) in KERNEL_PROPERTIES.items():
        out[name] = {'name': name, 'schema': schema, 'enforcement': enf, 'combine': comb, 'owner': 'kernel'}
    for d in properties:
        name = d.get('name')
        if not isinstance(name, str) or '.' not in name or not GRAMMARS['namespace'].match(name):
            raise BpaError('MODEL.INVALID_VALUE', 'a Buk property is namespaced')
        if name in out:
            raise BpaError('MODEL.INVALID_VALUE', 'duplicate property %s' % name)
        schema = d.get('schema')
        if not isinstance(schema, dict) or 'type' not in schema:
            raise BpaError('MODEL.INVALID_VALUE', 'a property schema is a field declaration')
        if d.get('enforcement') not in ENFORCEMENTS or d.get('combine') not in COMBINES:
            raise BpaError('MODEL.INVALID_VALUE', 'enforcement or combine rule')
        out[name] = {'name': name, 'schema': schema, 'enforcement': d['enforcement'], 'combine': d['combine'],
                     'owner': d.get('owner', name.rsplit('.', 1)[0])}
    return out


# ---- grammar (STD-LGS-001) ----

class _Scanner:
    def __init__(self, src):
        if not isinstance(src, str):
            raise BpaError('RULE.PARSE_ERROR', {'position': 0, 'message': 'a sheet is text'})
        self.s, self.i = src, 0

    def err(self, msg):
        raise BpaError('RULE.PARSE_ERROR', {'position': self.i, 'message': msg})

    def ws(self):
        s = self.s
        while self.i < len(s):
            if s[self.i] in ' \t\r\n':
                self.i += 1
            elif s.startswith('//', self.i):
                j = s.find('\n', self.i)
                self.i = len(s) if j < 0 else j
            else:
                break

    def peek(self, text):
        self.ws()
        return self.s.startswith(text, self.i)

    def eat(self, text):
        if not self.peek(text):
            self.err('expected %r' % text)
        self.i += len(text)

    def word(self, pattern, what):
        self.ws()
        m = re.compile(pattern).match(self.s, self.i)
        if not m:
            self.err('expected %s' % what)
        self.i = m.end()
        return m.group()

    def bracketed(self, open_, close):
        """The text between matching brackets, string literals respected."""
        self.eat(open_)
        start, level = self.i, 1
        s = self.s
        while self.i < len(s):
            c = s[self.i]
            if c == '"':
                self.i += 1
                while self.i < len(s) and s[self.i] != '"':
                    self.i += 2 if s[self.i] == '\\' else 1
                if self.i >= len(s):
                    self.err('unterminated string')
            elif c == open_:
                level += 1
            elif c == close:
                level -= 1
                if level == 0:
                    text = s[start:self.i].strip()
                    self.i += 1
                    return text
            self.i += 1
        self.err('expected %r' % close)

    def at_end(self):
        self.ws()
        return self.i >= len(self.s)


def _expr_text(text, where):
    """An L1 expression kept as text; it must parse (STD-EXPR-001)."""
    if not text:
        raise BpaError('RULE.PARSE_ERROR', {'position': where, 'message': 'empty expression'})
    try:
        _expr.parse(text)
    except BpaError as e:
        raise BpaError('RULE.PARSE_ERROR', {'position': where, 'message': 'expression: %s' % (
            e.detail['message'] if isinstance(e.detail, dict) else e.detail)})
    return ' '.join(text.split())


def _const(sc):
    """A constant value: an L1 literal, a list literal or a map literal, as canonical JSON."""
    sc.ws()
    s = sc.s
    if sc.peek('{'):
        sc.eat('{')
        out = {}
        if sc.peek('}'):
            sc.eat('}')
            return out
        while True:
            k = sc.word(r'[a-z][a-z0-9_]{0,63}', 'member name')
            if k in out:
                sc.err('duplicate member %s' % k)
            sc.eat(':')
            out[k] = _const(sc)
            if sc.peek(','):
                sc.eat(',')
                continue
            sc.eat('}')
            return out
    if sc.peek('['):
        sc.eat('[')
        out = []
        if sc.peek(']'):
            sc.eat(']')
            return out
        while True:
            out.append(_const(sc))
            if sc.peek(','):
                sc.eat(',')
                continue
            sc.eat(']')
            return out
    if sc.peek('"'):
        start = sc.i
        sc.i += 1
        while sc.i < len(s) and s[sc.i] != '"':
            sc.i += 2 if s[sc.i] == '\\' else 1
        if sc.i >= len(s):
            sc.err('unterminated string')
        sc.i += 1
        node = _expr.parse(s[start:sc.i])
        return node[2]
    m = re.compile(r'\d{4}-\d{2}-\d{2}|-?[0-9]+(\.[0-9]+)?([A-Z]{3})?|true|false|null').match(s, sc.i)
    if not m:
        sc.err('expected a value')
    sc.i = m.end()
    text = m.group()
    if text == 'true':
        return True
    if text == 'false':
        return False
    if text == 'null':
        return None
    if re.fullmatch(r'\d{4}-\d{2}-\d{2}', text):
        return text
    m2 = re.fullmatch(r'(-?[0-9]+(?:\.[0-9]+)?)([A-Z]{3})?', text)
    num, cur = m2.group(1), m2.group(2)
    if cur:
        return {'amount': num, 'currency': cur}
    if '.' in num:
        return num
    return int(num)


def _selector(sc):
    sc.ws()
    if sc.peek('*'):
        sc.eat('*')
        sel = {'type': '*'}
    else:
        sel = {'type': sc.word(r'[A-Z][A-Za-z0-9]{0,63}', 'class name or *')}
    quals = []
    while True:
        if sc.peek('['):
            where = sc.i
            quals.append({'kind': 'attr', 'expr': _expr_text(sc.bracketed('[', ']'), where)})
        elif sc.peek(':tag('):
            sc.eat(':tag(')
            quals.append({'kind': 'tag', 'tag': sc.word(r'[a-z0-9]+(-[a-z0-9]+)*', 'slug')})
            sc.eat(')')
        elif sc.peek(':ctx('):
            sc.eat(':ctx(')
            key = sc.word(r'[a-z_]+', 'context key')
            if key not in CTX_KEYS:
                sc.err('unknown context key %s' % key)
            op = sc.word(r'!=|=|in', 'comparison')
            if op == 'in':
                value = _const(sc)
                if not isinstance(value, list) or not all(isinstance(v, str) for v in value):
                    sc.err('in takes a list of texts')
            else:
                value = _const(sc)
                if not isinstance(value, str):
                    sc.err('a context condition compares a text')
            sc.eat(')')
            quals.append({'kind': 'ctx', 'key': key, 'op': op, 'value': value})
        elif sc.peek(':rel('):
            sc.eat(':rel(')
            member = sc.word(r'[a-z][a-z0-9_]{0,63}', 'member name')
            sc.eat(')')
            where = sc.i
            quals.append({'kind': 'rel', 'member': member, 'expr': _expr_text(sc.bracketed('[', ']'), where)})
        elif sc.peek(':'):
            sc.eat(':')
            quals.append({'kind': 'state', 'state': sc.word(r'[a-z][a-z0-9_]{0,63}', 'state name')})
        else:
            break
    if quals:
        sel['qualifiers'] = quals
    return sel


def parse(source):
    """STD-LGS-001: the AST of a sheet; `RULE.PARSE_ERROR` with the position otherwise."""
    sc = _Scanner(source)
    blocks = []
    while not sc.at_end():
        selectors = [_selector(sc)]
        while sc.peek(','):
            sc.eat(',')
            selectors.append(_selector(sc))
        sc.eat('{')
        decls = []
        while not sc.peek('}'):
            prop = sc.word(r'[a-z][a-z0-9_]{0,63}(\.[a-z][a-z0-9_]{0,63})*', 'property')
            sc.eat(':')
            value = _const(sc)
            d = {'property': prop, 'value': value}
            if sc.peek('!locked'):
                sc.eat('!locked')
                d['locked'] = True
            sc.eat(';')
            decls.append(d)
        sc.eat('}')
        if not decls:
            sc.err('a block declares at least one property')
        blocks.append({'selectors': selectors, 'declarations': decls})
    return {'blocks': blocks}


def _lit(v):
    if v is True:
        return 'true'
    if v is False:
        return 'false'
    if v is None:
        return 'null'
    if isinstance(v, int):
        return str(v)
    if isinstance(v, str):
        if re.fullmatch(r'-?[0-9]+\.[0-9]+', v) or re.fullmatch(r'\d{4}-\d{2}-\d{2}', v):
            return v
        return '"' + v.replace('\\', '\\\\').replace('"', '\\"').replace('\n', '\\n').replace('\t', '\\t') + '"'
    if isinstance(v, list):
        return '[' + ', '.join(_lit(x) for x in v) + ']'
    if isinstance(v, dict):
        if set(v) == {'amount', 'currency'} and isinstance(v['amount'], str):
            return '%s%s' % (v['amount'], v['currency'])
        return '{' + ', '.join('%s: %s' % (k, _lit(x)) for k, x in v.items()) + '}'
    raise BpaError('MODEL.INVALID_VALUE', 'not a sheet value')


def _sel_text(sel):
    out = sel['type']
    for q in sel.get('qualifiers', ()):
        k = q['kind']
        if k == 'attr':
            out += '[%s]' % q['expr']
        elif k == 'state':
            out += ':' + q['state']
        elif k == 'tag':
            out += ':tag(%s)' % q['tag']
        elif k == 'ctx':
            out += ':ctx(%s %s %s)' % (q['key'], q['op'], _lit(q['value']))
        elif k == 'rel':
            out += ':rel(%s)[%s]' % (q['member'], q['expr'])
    return out


def print_sheet(ast):
    """The canonical text of an AST (STD-LGS-002: `source` holds this form)."""
    _check_ast(ast)
    blocks = []
    for b in ast['blocks']:
        lines = [', '.join(_sel_text(s) for s in b['selectors']) + ' {']
        for d in b['declarations']:
            lines.append('  %s: %s%s;' % (d['property'], _lit(d['value']), ' !locked' if d.get('locked') else ''))
        lines.append('}')
        blocks.append('\n'.join(lines))
    return '\n\n'.join(blocks) + '\n'


def _check_ast(ast):
    """The AST is well formed (its shape); re-parsing the printed text checks the details."""
    if not isinstance(ast, dict) or not isinstance(ast.get('blocks'), list):
        raise BpaError('MODEL.INVALID_VALUE', 'a sheet AST is {blocks}')
    for b in ast['blocks']:
        if not isinstance(b, dict) or not isinstance(b.get('selectors'), list) or not b['selectors'] \
                or not isinstance(b.get('declarations'), list) or not b['declarations']:
            raise BpaError('MODEL.INVALID_VALUE', 'a block has selectors and declarations')


def parse_sheet(source):
    ast = parse(source)
    text = print_sheet(ast)
    if parse(text) != ast:
        raise BpaError('RULE.PARSE_ERROR', {'position': 0, 'message': 'the sheet does not round-trip'})
    return {'ast': ast, 'source': text}


# ---- checking (STD-LGS-001, -010, -020) ----

def _class_of(reg, classes, name):
    if name == '*':
        return 'Object'
    if name not in reg:
        raise BpaError('MODEL.REFERENCE_INVALID', 'unknown class %s' % name)
    return name


def _check_expr(classes, cls, text, where):
    try:
        ast = _expr.parse(text)
        env = _expr.Env(this=_expr._class_type(classes, cls))
        t = _expr.check(ast, env)
    except BpaError as e:
        if e.code == 'LOGIC.TYPE_ERROR':
            raise BpaError('LOGIC.TYPE_ERROR', {'selector': where, 'message': e.detail.get('message')
                           if isinstance(e.detail, dict) else e.detail})
        raise
    if t not in ('boolean', 'any'):
        raise BpaError('LOGIC.TYPE_ERROR', {'selector': where, 'message': 'a qualifier is boolean'})


def check_sheet(ast, classes, properties=(), reg=None):
    """STD-LGS-001, -010: selectors are checked against the classes and declarations against
    the property registry; returns the registry used."""
    _check_ast(ast)
    reg = reg or registry(properties)
    creg = _registry(classes)
    for b in ast['blocks']:
        for sel in b['selectors']:
            cls = _class_of(creg, classes, sel['type'])
            eff = effective_class(classes, cls) if cls != 'Object' else {'kind': 'definition', 'fields': {}}
            where = _sel_text(sel)
            for q in sel.get('qualifiers', ()):
                if q['kind'] in ('attr', 'rel') and eff.get('kind') == 'raw_document':
                    raise BpaError('RULE.UNTYPED_ACCESS', 'selector on RawDocument content')
                if q['kind'] == 'attr':
                    _check_expr(classes, cls, q['expr'], where)
                elif q['kind'] == 'rel':
                    f = eff['fields'].get(q['member']) if cls != 'Object' else None
                    if f is None:
                        raise BpaError('MODEL.REFERENCE_INVALID', 'unknown member %s' % q['member'])
                    item = f.get('constraints', {}).get('item_type', f) if f['type'] == 'list' else f
                    if item.get('type') != 'reference':
                        raise BpaError('MODEL.INVALID_VALUE', ':rel needs a reference member')
                    target = (item.get('constraints') or {}).get('target_class', 'Object')
                    if target not in creg:
                        raise BpaError('MODEL.REFERENCE_INVALID', 'unknown class %s' % target)
                    _check_expr(classes, target, q['expr'], where)
        for d in b['declarations']:
            _check_declaration(reg, d)
    return reg


def split_path(reg, path):
    """`(property name, map key or None)` of a property path: the registered name is the path
    itself or its longest registered prefix followed by one key (Buk names carry dots)."""
    if path in reg:
        return path, None
    root, _, key = path.rpartition('.')
    if root in reg and key:
        return root, key
    raise BpaError('RULE.UNKNOWN_PROPERTY', path)


def _check_declaration(reg, d):
    root, rest = split_path(reg, d['property'])
    p = reg[root]
    schema = p['schema']
    if rest:
        if schema['type'] != 'map':
            raise BpaError('MODEL.INVALID_VALUE', 'a dotted path names one key of a map property')
        vt = schema.get('constraints', {}).get('value_type') or {'type': 'json'}
        return validate_value(vt['type'], vt.get('constraints'), d['value'])
    value = d['value']
    if root == 'approvals' and isinstance(value, int) and not isinstance(value, bool):
        value = {'count': value}
    out = validate_value(schema['type'], schema.get('constraints'), value)
    if root == 'access':
        from .protection import check_rules
        check_rules(out)
    return out


def _normalise(reg, d):
    """The (path, value) pairs a declaration contributes: a whole-map declaration contributes
    one pair per key (DEC-066 item 3)."""
    root, rest = split_path(reg, d['property'])
    value = _check_declaration(reg, d)
    if not rest and reg[root]['schema']['type'] == 'map':
        return [('%s.%s' % (root, k), v) for k, v in value.items()]
    return [(d['property'], value)]


# ---- specificity (STD-LGS-021) ----

def specificity(classes, sel):
    """`(d, a, s, x)` of one selector (an AST selector or its text)."""
    if isinstance(sel, str):
        sc = _Scanner(sel)
        sel = _selector(sc)
        if not sc.at_end():
            sc.err('trailing text')
    creg = _registry(classes)
    d = 0 if sel['type'] == '*' else depth(creg, _class_of(creg, classes, sel['type']))
    a = s = x = 0
    for q in sel.get('qualifiers', ()):
        if q['kind'] in ('attr', 'rel'):
            a += 1
        elif q['kind'] in ('state', 'tag'):
            s += 1
        else:
            x += 1
    return [d, a, s, x]


# ---- static overlap analysis (STD-LGS-022, DEC-066 item 14) ----

def may_overlap(classes, a, b):
    creg = _registry(classes)
    ta, tb = a['type'], b['type']
    if ta != '*' and tb != '*' and ta != tb:
        da, db = creg.get(ta, {}), creg.get(tb, {})
        if not da.get('trait') and not db.get('trait'):
            la, lb = linearize(classes, ta, creg), linearize(classes, tb, creg)
            if ta not in lb and tb not in la:
                if not any(ta in linearize(classes, n, creg) and tb in linearize(classes, n, creg)
                           for n, d in creg.items() if not d.get('trait') and not d.get('abstract')):
                    return False
    qa, qb = a.get('qualifiers', ()), b.get('qualifiers', ())
    sa = {q['state'] for q in qa if q['kind'] == 'state'}
    sb = {q['state'] for q in qb if q['kind'] == 'state'}
    if sa and sb and sa != sb:
        return False
    for ka in qa:
        if ka['kind'] != 'ctx' or ka['op'] == '!=':
            continue
        for kb in qb:
            if kb['kind'] != 'ctx' or kb['op'] == '!=' or kb['key'] != ka['key']:
                continue
            va = {ka['value']} if ka['op'] == '=' else set(ka['value'])
            vb = {kb['value']} if kb['op'] == '=' else set(kb['value'])
            if not (va & vb):
                return False
    return True


def _rank(layer, locked, spec, priority):
    """Rank key: locked declarations first, among them the lowest layer wins; among unlocked
    the highest layer wins; then specificity and priority (STD-LGS-022)."""
    li = LAYERS.index(layer)
    return (1, -li, tuple(spec), priority) if locked else (0, li, tuple(spec), priority)


def _candidates_of(reg, classes, sheet, sheet_id, sheet_version, body):
    """Every (path, candidate) a sheet declares, with the selector that carries it."""
    ast = body['ast']
    out = []
    for bi, b in enumerate(ast['blocks']):
        for di, d in enumerate(b['declarations']):
            for path, value in _normalise(reg, d):
                for sel in b['selectors']:
                    out.append((path, {'sheet': sheet_id, 'sheet_version': sheet_version, 'block': bi,
                                       'selector': _sel_text(sel), 'sel': sel, 'layer': body['layer'],
                                       'specificity': specificity(classes, sel), 'priority': body.get('priority', 0),
                                       'locked': bool(d.get('locked')), 'value': value, 'position': (bi, di)}))
    return out


def check_set(reg, classes, sheets, written):
    """Static checks of a set of live sheets when `written` (sheet ids) are committed:
    ambiguity and locked overrides between overlapping selectors (STD-LGS-022)."""
    cands = []
    for sid, (vh, body) in sheets.items():
        cands += _candidates_of(reg, classes, None, sid, vh, body)
    by_path = {}
    for path, c in cands:
        by_path.setdefault(path, []).append(c)
    for path, cs in sorted(by_path.items()):
        combine = reg[split_path(reg, path)[0]]['combine']
        for i, a in enumerate(cs):
            for b in cs[i + 1:]:
                if a['sheet'] == b['sheet'] or (a['sheet'] not in written and b['sheet'] not in written):
                    continue
                if not may_overlap(classes, a['sel'], b['sel']):
                    continue
                for lo, hi in ((a, b), (b, a)):
                    if lo['locked'] and LAYERS.index(hi['layer']) > LAYERS.index(lo['layer']):
                        raise BpaError('RULE.LOCKED_OVERRIDE', {'property': path, 'locked_by': lo['sheet'],
                                                                'sheet': hi['sheet']})
                ra = _rank(a['layer'], a['locked'], a['specificity'], a['priority'])
                rb = _rank(b['layer'], b['locked'], b['specificity'], b['priority'])
                if ra == rb and a['value'] != b['value'] and combine == 'override':
                    raise BpaError('RULE.AMBIGUOUS_CASCADE', {'property': path,
                                                               'sheets': sorted([a['sheet'], b['sheet']])})


# ---- matching and cascade (STD-LGS-020, -022, -023) ----

def _combine(rule, root, values):
    if rule == 'max' or rule == 'min':
        order = ORDERS.get(root)
        key = (lambda v: order.index(v)) if order else (lambda v: v['count'] if isinstance(v, dict) else v)
        return (max if rule == 'max' else min)(values, key=key)
    if rule == 'union':
        out = []
        for v in values:
            for x in v:
                if x not in out:
                    out.append(x)
        return out
    if rule == 'all_true':
        return all(values)
    if rule == 'any_true':
        return any(values)
    raise BpaError('MODEL.INVALID_VALUE', 'combine rule')


class Sheets:
    """Store mixin: sheet objects, the registry at a coordinate, matching and the cascade."""

    # -- objects of the genesis classes at commit (STD-LGS-002, -010) --

    def _sheet_body(self, body):
        """`ast` is derived from `source`; a given `ast` must agree (DEC-066 item 5)."""
        if not isinstance(body, dict):
            return body
        body = dict(body)
        if 'source' in body and isinstance(body['source'], str):
            p = parse_sheet(body['source'])
            if body.get('ast') is not None and body['ast'] != p['ast']:
                raise BpaError('MODEL.INVALID_VALUE', 'ast does not match source')
            body['ast'], body['source'] = p['ast'], p['source']
        return body

    def _sheet_objects(self, st, asof=None):
        """Live sheets of a state as {object_id: (version_hash, body)}, at `asof` when given."""
        from .store import tl_at
        out = {}
        for oid in sorted(st['objects']):
            o = st['objects'][oid]
            if o['class'] != 'GovernanceSheet':
                continue
            if asof is None:
                vh = o['head']
            else:
                seg = tl_at(o['timeline'], asof)
                if seg is None:
                    continue
                vh = seg[2]
            v = self.versions[vh]
            if not v['tombstone']:
                out[oid] = (vh, v['body'])
        return out

    def _property_decls(self, st, asof=None):
        from .store import tl_at
        out = []
        for oid in sorted(st['objects']):
            o = st['objects'][oid]
            if o['class'] != 'PropertyDecl':
                continue
            seg = tl_at(o['timeline'], asof) if asof is not None else None
            vh = o['head'] if asof is None else (seg[2] if seg else None)
            if vh and not self.versions[vh]['tombstone']:
                out.append(self.versions[vh]['body'])
        return out

    def _check_sheets(self, st, written=None):
        """Checks of the sheets and property declarations written by a commit, or brought into
        the target by a merge or a revert (`written` given, 1.0-rc7)."""
        if written is None:
            written = {oid for oid, _ in self._written
                       if st['objects'][oid]['class'] in ('GovernanceSheet', 'PropertyDecl')}
        if not written:
            return
        reg = registry(self._property_decls(st))
        classes = list(self.decls.values())
        sheets = self._sheet_objects(st)
        for sid in sorted(sheets):
            if sid in written:
                check_sheet(sheets[sid][1]['ast'], classes, reg=reg)
        for sid, (vh, body) in sheets.items():
            if sid not in written:
                check_sheet(body['ast'], classes, reg=reg)
        check_set(reg, classes, sheets, written)

    def _check_merged_sheets(self, T, st):
        """STD-LGS-022 on the sheets and property declarations a merge or a revert changes
        relative to the target state `T` (STD-VER-055, 1.0-rc7)."""
        changed = {oid for oid, o in st['objects'].items()
                   if o['class'] in ('GovernanceSheet', 'PropertyDecl')
                   and (T['objects'].get(oid) or {}).get('head') != o['head']}
        live = {oid for oid in changed if not self.versions[st['objects'][oid]['head']]['tombstone']}
        self._check_sheets(st, live)

    # -- matching (STD-LGS-020) --

    def _sheet_applies(self, body, ctx, branch, cls_lin):
        scope = body.get('scope') or {}
        if scope.get('vaes') and ctx.get('vae') not in scope['vaes']:
            return False
        if scope.get('branch_kinds') and ctx.get('branch_kind') not in scope['branch_kinds']:
            return False
        if scope.get('classes') and not (set(scope['classes']) & set(cls_lin)):
            return False
        return True

    def _eval_qualifier(self, st, asof, classes, cls, body, text):
        try:
            r = _expr.eval_expr({'expr': text, 'classes': classes, 'class': cls, 'this': body},
                                typed_ref=lambda uri: None)
        except BpaError as e:
            if e.code in ('LOGIC.TYPE_ERROR', 'LOGIC.NULL_ACCESS'):
                return False
            raise
        return r['value'] is True

    def _matches(self, st, asof, classes, sel, oid, o, body, ctx, state=None):
        lin = self._cls(o['class'])['linearization']
        if sel['type'] != '*' and sel['type'] not in lin:
            return False
        for q in sel.get('qualifiers', ()):
            k = q['kind']
            if k == 'attr':
                if not self._eval_qualifier(st, asof, classes, o['class'], body, q['expr']):
                    return False
            elif k == 'state':
                # the lifecycle state (STD-CLS-004) or a field named state
                if (state if state is not None else body.get('state')) != q['state']:
                    return False
            elif k == 'tag':
                tags = body.get('tags')
                if not isinstance(tags, list) or q['tag'] not in tags:
                    return False
            elif k == 'ctx':
                v = ctx.get(q['key'])
                ok = (v == q['value']) if q['op'] == '=' else (v != q['value']) if q['op'] == '!=' else v in q['value']
                if not ok:
                    return False
            elif k == 'rel':
                refs = body.get(q['member'])
                refs = refs if isinstance(refs, list) else [refs] if refs else []
                hit = False
                for uri in refs:
                    tid = self._resolve(st, uri) if isinstance(uri, str) else None
                    if tid is None or not self._live(st, tid):
                        continue
                    t = st['objects'][tid]
                    from .store import tl_at
                    seg = tl_at(t['timeline'], asof)
                    if seg is None:
                        continue
                    if self._eval_qualifier(st, asof, classes, t['class'], self.versions[seg[2]]['body'], q['expr']):
                        hit = True
                        break
                if not hit:
                    return False
        return True

    # -- cascade (STD-LGS-022, -023) --

    def _cascade(self, branch, object_id, ctx, sel):
        from .store import tl_at
        sel = sel or {}
        st, _, ev, _ = self._state(branch, sel)
        o = st['objects'].get(object_id)
        if o is None or not self._live(st, object_id):
            raise BpaError('IAM.NOT_FOUND', 'unknown object')
        asof = sel.get('asof') or self._default_valid(o['class'])
        seg = tl_at(o['timeline'], asof)
        if seg is None:
            raise BpaError('QRY.NOT_FOUND_AT_TIME')
        body = self.versions[seg[2]]['body']
        out, bindings = self._cascade_in(st, branch, object_id, o, body, ctx, asof)
        return out, ev, o, seg[2], st, bindings

    def _cascade_in(self, st, branch, object_id, o, body, ctx, asof, state=None):
        """The cascade over a state: `{path: (value, candidates)}` and the sheet versions bound."""
        from .store import branch_kind, tl_at
        if state is None and o.get('head'):
            seg = tl_at(o['timeline'], asof)
            state = self.versions[seg[2] if seg else o['head']].get('state')
        ctx = dict(ctx or {})
        ctx.setdefault('branch_kind', branch_kind(branch))
        classes = list(self.decls.values())
        reg = registry(self._property_decls(st, asof))
        lin = self._cls(o['class'])['linearization']
        cands, bindings = {}, []
        for sid, (vh, sbody) in self._sheet_objects(st, asof).items():
            if not self._sheet_applies(sbody, ctx, branch, lin):
                continue
            bindings.append(vh)
            for bi, b in enumerate(sbody['ast']['blocks']):
                hits = [s for s in b['selectors'] if self._matches(st, asof, classes, s, object_id, o, body, ctx, state)]
                if not hits:
                    continue
                best = max(hits, key=lambda s: specificity(classes, s))
                for di, d in enumerate(b['declarations']):
                    for path, value in _normalise(reg, d):
                        cands.setdefault(path, []).append({
                            'sheet': self._uri(st['objects'][sid]), 'sheet_version': vh, 'block': bi,
                            'selector': _sel_text(best), 'layer': sbody['layer'],
                            'specificity': specificity(classes, best), 'priority': sbody.get('priority', 0),
                            'locked': bool(d.get('locked')), 'value': value, 'position': (sid, bi, di)})
        out = {}
        for path in sorted(cands):
            cs = cands[path]
            root = split_path(reg, path)[0]
            rule = reg[root]['combine']
            key = lambda c: _rank(c['layer'], c['locked'], c['specificity'], c['priority'])  # noqa: E731
            top_rank = max(key(c) for c in cs)
            top = [c for c in cs if key(c) == top_rank]
            sheets = {c['sheet'] for c in top}
            if rule != 'override':
                # A combinable property combines every matching candidate: the rules are
                # commutative, so the rank only orders the explanation (DEC-066 item 8).
                value = _combine(rule, root, [c['value'] for c in sorted(cs, key=lambda c: c['position'])])
                for c in cs:
                    c['won'], c['reason'] = True, 'combined'
            else:
                if len(sheets) > 1 and len({_json(c['value']) for c in top}) > 1:
                    raise BpaError('RULE.AMBIGUOUS_CASCADE', {'property': path, 'sheets': sorted(sheets)})
                winner = max(top, key=lambda c: c['position'])
                value = winner['value']
                for c in cs:
                    if c is winner:
                        c['won'], c['reason'] = True, 'won'
                    elif c in top:
                        c['won'], c['reason'] = False, 'same_value' if c['sheet'] != winner['sheet'] else 'earlier_position'
                    else:
                        c['won'], c['reason'] = False, self._reason(c, winner, key)
            out[path] = (value, cs)
        return out, bindings

    def _effective(self, st, branch, object_id, ctx, asof, body=None, cls=None):
        """Effective property paths of an object of the state, or of a provisional object
        (`body` and `cls` given, for defaults before a create): `({path: value}, bindings)`."""
        from .store import tl_at
        o = st['objects'].get(object_id)
        if o is None:
            eff = self._cls(cls)
            o = {'class': cls, 'kind': eff['kind'], 'slug': '', 'head': '', 'timeline': []}
            st = {'objects': dict(st['objects'], **{object_id: o}), 'ledger': st['ledger']}
        state = None
        if body is None:
            seg = tl_at(o['timeline'], asof)
            body = self.versions[seg[2]]['body'] if seg else self.versions[o['head']]['body']
        elif not o.get('head'):
            lc = self._lifecycle(st, o['class'])
            state = lc[0]['initial'] if lc else None
        out, bindings = self._cascade_in(st, branch, object_id, o, body, ctx, asof, state)
        return {p: v for p, (v, _) in out.items()}, bindings

    @staticmethod
    def _reason(c, w, key):
        if w['locked'] and not c['locked']:
            return 'locked_elsewhere'
        if c['layer'] != w['layer']:
            return 'lower_layer'
        if list(c['specificity']) != list(w['specificity']):
            return 'less_specific'
        if c['priority'] != w['priority']:
            return 'lower_priority'
        return 'earlier_position'

    def cascade(self, branch, object_id, ctx=None, sel=None):
        """The effective declarations of an object in a context (STD-LGS-020, -022)."""
        out, ev, _, _, _, bindings = self._cascade(branch, object_id, ctx, sel)
        return {'properties': {p: v for p, (v, _) in out.items()}, 'bindings': bindings, 'evaluated_at': ev}

    def explain_cascade(self, branch, object_id, prop, ctx=None, sel=None):
        """`DAT-CascadeExplanation` of one property (STD-LGS-023)."""
        out, ev, o, vh, st, _ = self._cascade(branch, object_id, ctx, sel)
        split_path(registry(self._property_decls(st)), prop)
        value, cs = out.get(prop, (None, []))
        key = lambda c: _rank(c['layer'], c['locked'], c['specificity'], c['priority'])  # noqa: E731
        cands = sorted(cs, key=lambda c: (key(c), c['position']), reverse=True)
        items = [{k: c[k] for k in ('sheet', 'sheet_version', 'block', 'selector', 'layer', 'specificity',
                                    'priority', 'locked', 'value', 'won', 'reason')} for c in cands]
        return {'object': self._uri(o), 'version_hash': vh, 'property': prop, 'effective': value,
                'candidates': items, 'evaluated_at': ev}


def _json(v):
    from .canon import canonical
    return canonical(v)


def sheet_ops():
    """Pure operations of this module (STD-CONF-005, since 1.0-rc2)."""
    return {
        'parse_sheet': lambda i: parse_sheet(i['source']),
        'check_sheet': lambda i: (check_sheet(parse(i['source']) if 'source' in i else i['ast'], i['classes'],
                                              i.get('properties', ())) and True) or True,
        'specificity': lambda i: {'specificity': specificity(i['classes'], i['selector'])},
    }


def sheet_version_id(body):
    return content_id('ubos.chunk.v1', body)
