"""Schema evolution and upstream changes of BPA 1.0-rc8 (PH-2 step 1d, DEC-092): the computed
classification of class changes (STD-CLS-050), lenses of the nine declarative operations
(STD-CLS-060), their application to reads, writes and merges (STD-CLS-061), the totality check
(STD-CLS-062) and the detection and resolution of upstream changes of overridden objects
(STD-VER-082). Mixed into the reference store; `classify_change`, `apply_lens` and `check_lens`
are also pure operations."""
import copy

from . import BpaError
from .canon import canonical
from .values import validate_value

LENS_CLASS = 'Lens'
# STD-CLS-060: the parameters of each operation
OPS = {'rename': ('from', 'to'), 'split': ('from', 'to', 'forward', 'backward'),
       'join': ('from', 'to', 'forward', 'backward'),
       'convert': ('field', 'from_type', 'to_type', 'forward_expr', 'backward_expr'),
       'default': ('field', 'value'), 'nest': ('fields', 'into'), 'unnest': ('from', 'fields'),
       'map_enum': ('field', 'mapping'), 'drop_with_archive': ('field',)}
ARCHIVE = '_archived'
ORDER = ('additive', 'lens_required', 'breaking')
SAMPLE = 10000
MAX_FAILURES = 10
ANNOTATIONS = ('label', 'description')
# Generated values per field type (STD-CLS-062); values a field's constraints refuse are skipped.
GENERATED = {'text': ['', 'a', 'Zürich 1'], 'integer': [0, 1, -7, 9007199254740991],
             'decimal': ['0', '1.5', '-2.25', '1000000.01'], 'boolean': [True, False],
             'date': ['1970-01-01', '2026-02-28'], 'instant': ['2026-01-01T00:00:00Z'],
             'money': [{'amount': '0.00', 'currency': 'USD'}, {'amount': '12.34', 'currency': 'EUR'},
                       {'amount': '-5', 'currency': 'JPY'}],
             'json': [{}, {'a': [1, 'b']}]}


def _invalid(what):
    return BpaError('MODEL.INVALID_VALUE', what)


def check_lens(lens):
    """DAT-Lens: the operations and their parameters; a lens whose backward mapping is not
    total is declared one-way with a reason."""
    if not isinstance(lens, dict):
        raise _invalid('a lens is a map')
    for k in ('class', 'from', 'to', 'ops'):
        if lens.get(k) is None:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'lens member %s' % k)
    if not isinstance(lens['from'], int) or isinstance(lens['from'], bool) or lens['from'] < 1 or \
            lens['to'] != lens['from'] + 1:
        raise _invalid('a lens maps schema version from to from + 1')
    if lens.get('residual_forward') is not None or lens.get('residual_backward') is not None:
        raise BpaError('LOGIC.NOT_AVAILABLE', 'L2 residual functions of lenses come after BPA 1.0-rc8')
    if not isinstance(lens['ops'], list) or not lens['ops']:
        raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a lens has operations')
    needs_one_way = False
    for op in lens['ops']:
        if not isinstance(op, dict) or op.get('op') not in OPS:
            raise _invalid('a lens operation is one of %s' % ', '.join(OPS))
        for p in OPS[op['op']]:
            if p not in op:
                raise BpaError('MODEL.DECLARATION_INCOMPLETE', '%s needs %s' % (op['op'], p))
        k = op['op']
        if k in ('split', 'join'):
            many = op['to'] if k == 'split' else op['from']
            exprs = op['forward'] if k == 'split' else op['backward']
            if not isinstance(many, list) or not many or not isinstance(exprs, list) or len(exprs) != len(many):
                raise _invalid('%s gives one expression per part' % k)
        if k in ('nest', 'unnest') and (not isinstance(op['fields'], list) or not op['fields']):
            raise _invalid('%s names its fields' % k)
        if k == 'map_enum':
            m = op['mapping']
            if not isinstance(m, dict) or not m:
                raise _invalid('a mapping is a map of values')
            if len(set(canonical(v) for v in m.values())) != len(m):
                needs_one_way = True
    if needs_one_way and not lens.get('one_way'):
        raise _invalid('a mapping that is not injective makes the lens one-way')
    if lens.get('one_way') and not lens.get('one_way_reason'):
        raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a one-way lens states one_way_reason')


def _x(text, types, body):
    from .expr import eval_expr
    return eval_expr({'expr': text, 'params_types': types,
                      'params': {k: v for k, v in body.items() if k in types and v is not None}})['value']


def _forward(op, b, types):
    k = op['op']
    if k == 'rename':
        if op['from'] in b:
            b[op['to']] = b.pop(op['from'])
    elif k == 'split':
        if b.get(op['from']) is not None:
            vals = [_x(e, types, b) for e in op['forward']]
            del b[op['from']]
            b.update(zip(op['to'], vals))
    elif k == 'join':
        if all(b.get(f) is not None for f in op['from']):
            v = _x(op['forward'], types, b)
            for f in op['from']:
                del b[f]
            b[op['to']] = v
    elif k == 'convert':
        if b.get(op['field']) is not None:
            b[op['field']] = _x(op['forward_expr'], {op['field']: {'type': op['from_type']}}, b)
    elif k == 'default':
        if b.get(op['field']) is None:
            b[op['field']] = op['value']
    elif k == 'nest':
        inner = {f: b.pop(f) for f in op['fields'] if f in b}
        if inner:
            b[op['into']] = inner
    elif k == 'unnest':
        inner = b.pop(op['from'], None)
        if isinstance(inner, dict):
            b.update({f: inner[f] for f in op['fields'] if f in inner})
    elif k == 'map_enum':
        v = b.get(op['field'])
        if v is not None and v in op['mapping']:
            b[op['field']] = op['mapping'][v]
    elif k == 'drop_with_archive':
        if op['field'] in b:
            b.setdefault(ARCHIVE, {})[op['field']] = b.pop(op['field'])
    return b


def _backward(op, b, types):
    k = op['op']
    if k == 'rename':
        if op['to'] in b:
            b[op['from']] = b.pop(op['to'])
    elif k == 'split':
        if all(b.get(f) is not None for f in op['to']):
            v = _x(op['backward'], types, b)
            for f in op['to']:
                del b[f]
            b[op['from']] = v
    elif k == 'join':
        if b.get(op['to']) is not None:
            vals = [_x(e, types, b) for e in op['backward']]
            del b[op['to']]
            b.update(zip(op['from'], vals))
    elif k == 'convert':
        if b.get(op['field']) is not None:
            b[op['field']] = _x(op['backward_expr'], {op['field']: {'type': op['to_type']}}, b)
    elif k == 'default':
        if op['field'] in b and canonical(b[op['field']]) == canonical(op['value']):
            del b[op['field']]
    elif k == 'nest':
        inner = b.pop(op['into'], None)
        if isinstance(inner, dict):
            b.update({f: inner[f] for f in op['fields'] if f in inner})
    elif k == 'unnest':
        inner = {f: b.pop(f) for f in op['fields'] if f in b}
        if inner:
            b[op['from']] = inner
    elif k == 'map_enum':
        inv = {canonical(v): x for x, v in op['mapping'].items()}
        v = b.get(op['field'])
        if v is not None and canonical(v) in inv:
            b[op['field']] = inv[canonical(v)]
    elif k == 'drop_with_archive':
        arch = b.get(ARCHIVE)
        if isinstance(arch, dict) and op['field'] in arch:
            b[op['field']] = arch.pop(op['field'])
            if not arch:
                del b[ARCHIVE]
    return b


def lens_forward(lens, body, old_fields, new_fields):
    types = dict(new_fields)
    types.update(old_fields)
    b = copy.deepcopy(body)
    for op in lens['ops']:
        b = _forward(op, b, types)
    return b


def lens_backward(lens, body, old_fields, new_fields):
    if lens.get('one_way'):
        raise BpaError('MODEL.LENS_REQUIRED', {'one_way': lens.get('one_way_reason')})
    types = dict(old_fields)
    types.update(new_fields)
    b = copy.deepcopy(body)
    for op in reversed(lens['ops']):
        b = _backward(op, b, types)
    return b


def _fields(decl):
    return {f['name']: f for f in decl.get('fields') or []}


def _satisfies(f, v):
    if v is None:
        return not f.get('required')
    try:
        validate_value(f['type'], f.get('constraints'), v)
    except BpaError:
        return False
    return True


def body_problems(fields, body):
    """Why a body does not conform to the fields of a schema version (empty when it does)."""
    out = []
    for k, v in sorted(body.items()):
        if k == ARCHIVE:
            continue
        if k not in fields:
            out.append({'field': k, 'problem': 'undeclared'})
        elif not _satisfies(fields[k], v):
            out.append({'field': k, 'problem': 'invalid'})
    for k, f in sorted(fields.items()):
        if f.get('required') and body.get(k) is None:
            out.append({'field': k, 'problem': 'missing'})
    return out


def _generated(f):
    if f['type'] == 'enum':
        vals = list((f.get('constraints') or {}).get('allowed_values') or [])
    else:
        vals = GENERATED.get(f['type'], [])
    out = []
    for v in vals:
        try:
            out.append(validate_value(f['type'], f.get('constraints'), v))
        except BpaError:
            pass
    return out


def _affected(lens, old_fields):
    names = []
    for op in lens['ops']:
        for p in ('from', 'field', 'fields'):
            v = op.get(p)
            for n in (v if isinstance(v, list) else [v]):
                if isinstance(n, str) and n in old_fields and n not in names:
                    names.append(n)
    return names


def generated_bodies(lens, old_fields):
    """Bodies of the source schema version that vary each field a lens touches over the
    generated values of its type, the other fields set to their first generated value."""
    base = {}
    for n, f in sorted(old_fields.items()):
        vals = _generated(f)
        if vals:
            base[n] = vals[0]
        elif f.get('required'):
            return [], [n]
    out, seen, ungenerated = [], set(), []
    for n in _affected(lens, old_fields):
        vals = _generated(old_fields[n])
        if not vals:
            ungenerated.append(n)
        for v in vals + ([] if old_fields[n].get('required') else [None]):
            b = dict(base)
            if v is None:
                b.pop(n, None)
            else:
                b[n] = v
            c = canonical(b)
            if c not in seen:
                seen.add(c)
                out.append(b)
    if canonical(base) not in seen:
        out.insert(0, base)
    return out, ungenerated


def totality(lens, old_fields, new_fields, stored=()):
    """STD-CLS-062: round trips backward(forward(x)) = x and conformance of forward(x) for
    generated values and the stored current versions; failures carry the sample values."""
    gen, ungenerated = generated_bodies(lens, old_fields)
    failures = []
    for source, bodies in (('generated', gen), ('stored', list(stored)[:SAMPLE])):
        for x in bodies:
            fail = {'source': source, 'value': x}
            try:
                fx = lens_forward(lens, x, old_fields, new_fields)
                probs = body_problems(new_fields, fx)
                if probs:
                    fail.update(reason='forward_invalid', forward=fx, problems=probs)
                elif not lens.get('one_way'):
                    bx = lens_backward(lens, fx, old_fields, new_fields)
                    if canonical(bx) != canonical(x):
                        fail.update(reason='round_trip', forward=fx, backward=bx)
            except BpaError as e:
                fail.update(reason='error', code=e.code)
            if 'reason' in fail and len(failures) < MAX_FAILURES:
                failures.append(fail)
    out = {'total': not failures, 'generated': len(gen), 'stored': min(len(list(stored)), SAMPLE),
           'failures': failures}
    if ungenerated:
        out['ungenerated'] = ungenerated
    return out


def classify(old, new, old_fields, new_fields, stored=(), lens=None):
    """STD-CLS-050: the computed classification of a class change. With a lens, constraints
    are checked on the stored versions mapped forward."""
    changes = []
    stored = list(stored)
    mapped = stored
    if lens is not None:
        check_lens(lens)
        mapped = []
        for x in stored:
            try:
                mapped.append(lens_forward(lens, x, old_fields, new_fields))
            except BpaError:
                mapped.append(x)

    def add(change, cls, **extra):
        changes.append(dict({'change': change, 'classification': cls}, **extra))

    for n in sorted(set(old_fields) | set(new_fields)):
        a, b = old_fields.get(n), new_fields.get(n)
        if a is None:
            req = b.get('required') and b.get('default') is None
            add('field_added', 'lens_required' if req else 'additive', field=n)
        elif b is None:
            add('field_removed', 'lens_required', field=n)
        elif a['type'] != b['type']:
            add('type_changed', 'lens_required', field=n)
        elif (a.get('constraints') or {}) != (b.get('constraints') or {}) or \
                bool(a.get('required')) != bool(b.get('required')):
            bad = sum(1 for x in mapped if not _satisfies(b, x.get(n)))
            raw = sum(1 for x in stored if not _satisfies(b, x.get(n)))
            if bad:
                add('constraint_changed', 'breaking', field=n, violations=bad)
            elif raw:
                add('constraint_changed', 'lens_required', field=n, mapped=raw)
            else:
                add('constraint_changed', 'additive', field=n)
        elif {k: v for k, v in a.items() if k not in ANNOTATIONS + ('default',)} != \
                {k: v for k, v in b.items() if k not in ANNOTATIONS + ('default',)}:
            add('field_changed', 'breaking', field=n)
        elif a != b:
            add('annotation', 'additive', field=n)
    po = {p['name']: p for p in old.get('ports') or []}
    pn = {p['name']: p for p in new.get('ports') or []}
    for n in sorted(set(po) | set(pn)):
        if n not in po:
            add('port_added', 'additive', port=n)
        elif n not in pn:
            add('port_removed', 'breaking', port=n)
        elif po[n] != pn[n]:
            add('port_changed', 'breaking', port=n)
    for m in ('kind', 'parent', 'namespace', 'abstract'):
        if old.get(m) != new.get(m):
            add('%s_changed' % m, 'breaking')
    rest = sorted(k for k in set(old) | set(new) if k not in ('fields', 'ports', 'kind', 'parent', 'namespace',
                                                                'abstract', 'schema_version', 'name')
                  and old.get(k) != new.get(k))
    for k in rest:
        add('annotation' if k in ANNOTATIONS else 'member_changed', 'additive' if k in ANNOTATIONS else 'breaking',
            member=k)
    level = max([ORDER.index(c['classification']) for c in changes] or [0])
    out = {'classification': ORDER[level], 'changes': changes}
    if lens is not None and level < 2:
        check_lens(lens)
        t = totality(lens, old_fields, new_fields, stored)
        out['totality'] = t
        if not t['total']:
            out['classification'] = 'breaking'
    return out


def classify_change(i):
    """The pure form of STD-CLS-050: `from` and `to` are class declarations with their own
    fields; `stored` are bodies of current versions."""
    old, new = i.get('from'), i.get('to')
    if not isinstance(old, dict) or not isinstance(new, dict):
        raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'from and to are class declarations')
    return classify(old, new, _fields(old), _fields(new), i.get('stored') or [], i.get('lens'))


def apply_lens(i):
    """The pure form of STD-CLS-061 for one lens: `direction` forward or backward."""
    lens = i.get('lens')
    check_lens(lens)
    old, new = _fields(i.get('from') or {}), _fields(i.get('to') or {})
    if i.get('direction', 'forward') == 'forward':
        return {'body': lens_forward(lens, i['body'], old, new)}
    if i['direction'] != 'backward':
        raise _invalid('direction is forward or backward')
    return {'body': lens_backward(lens, i['body'], old, new)}


def lens_check(i):
    """The pure form of STD-CLS-062."""
    lens = i.get('lens')
    check_lens(lens)
    return totality(lens, _fields(i.get('from') or {}), _fields(i.get('to') or {}), i.get('stored') or [])


class Lenses:

    def _lens_init(self):
        self.schema_steps = {}     # (class, schema version) -> {classification, lens, from_fields, to_fields}
        self._overrides = {}       # (branch, object id) -> parent version at the override (STD-VER-082)

    # ---- class changes (STD-CLS-050, -062) ----

    def _schema(self, cls):
        return self._decl(cls).get('schema_version', 1)

    def _stored(self, st, cls):
        """The current versions of a class at its current schema version, sampled in object
        order (STD-CLS-062)."""
        out = []
        for oid, o in sorted(st['objects'].items()):
            if o['class'] != cls:
                continue
            v = self.versions[o['head']]
            if v['tombstone']:
                continue
            out.append(self._current_body(v))
            if len(out) == SAMPLE:
                break
        return out

    def change_class(self, i):
        """STD-CLS-050: change a class declaration by one schema version. The change is
        classified by the implementation; a lens-required change needs a total lens, a
        breaking change is refused."""
        from .classes import effective_class
        new = i.get('class')
        if not isinstance(new, dict) or new.get('name') not in self.decls:
            raise BpaError('MODEL.REFERENCE_INVALID', 'a change names a declared class')
        name = new['name']
        old = self.decls[name]
        s = old.get('schema_version', 1)
        if new.get('schema_version') != s + 1:
            raise _invalid('a class change increments schema_version by one')
        branch = i.get('branch', 'main')
        st = self._state(branch)[0]
        lens = None
        if i.get('lens') is not None:
            lid, o = self._ref_oid(st, i['lens'], LENS_CLASS)
            lens = self.versions[o['head']]['body']
            if lens['class'] not in (name, 'ubos://%s/Class/%s' % (self.TENANT, name)) or lens['from'] != s:
                raise _invalid('the lens maps this class from its current schema version')
        old_fields = self._cls(name)['fields']
        decls = dict(self.decls)
        decls[name] = new
        new_fields = effective_class(list(decls.values()), name)['fields']
        stored = self._stored(st, name)
        res = classify(old, new, old_fields, new_fields, stored, lens)
        out = {'class': name, 'from': s, 'to': s + 1, 'classification': res['classification'],
               'changes': res['changes']}
        if 'totality' in res:
            out['totality'] = res['totality']
        if res['classification'] == 'breaking':
            raise BpaError('MODEL.MIGRATION_REQUIRED', out)
        if res['classification'] == 'lens_required' and lens is None:
            raise BpaError('MODEL.LENS_REQUIRED', {'changes': [c for c in res['changes']
                                                               if c['classification'] == 'lens_required']})
        self.load_genesis([new])
        self.schema_steps[(name, s + 1)] = {'classification': res['classification'], 'lens': copy.deepcopy(lens),
                                            'from_fields': copy.deepcopy(old_fields),
                                            'to_fields': copy.deepcopy(dict(self._cls(name)['fields']))}
        return out

    # ---- lens application (STD-CLS-061) ----

    def _lens_body(self, cls, body, s, t):
        """A body stored at schema version s in schema version t: the lenses s→s+1→…→t
        forward, or backward for t < s; a step without a lens was additive."""
        cur = self._schema(cls)
        if not isinstance(t, int) or isinstance(t, bool) or t < 1 or t > cur:
            raise _invalid('schema_version is a schema version of the class')
        b, v = body, s
        while v < t:
            step = self.schema_steps.get((cls, v + 1))
            if step and step['lens']:
                b = lens_forward(step['lens'], b, step['from_fields'], step['to_fields'])
            v += 1
        while v > t:
            step = self.schema_steps.get((cls, v))
            if step:
                if step['lens']:
                    b = lens_backward(step['lens'], b, step['from_fields'], step['to_fields'])
                else:
                    b = {k: x for k, x in b.items() if k in step['from_fields'] or k == ARCHIVE}
            v -= 1
        return b

    def _current_body(self, v):
        s = v['envelope']['schema_version']
        cur = self._schema(v['class'])
        return v['body'] if s == cur else self._lens_body(v['class'], v['body'], s, cur)

    def _lens_read(self, out, v, sel):
        """STD-CLS-061: a read in the schema version asked for (default: the current one);
        the result is not stored."""
        s = v['envelope']['schema_version']
        t = sel.get('schema_version')
        if t is None:
            t = self._schema(v['class'])
        if s != t:
            out['body'] = self._lens_body(v['class'], v['body'], s, t)
        if s != t or sel.get('schema_version') is not None:
            out['schema_version'] = t
        return out

    def _lens_change(self, st, ch):
        """STD-CLS-061: a change written in an older schema version is lensed forward before
        it is stored; an update's patch applies to the base seen in that version."""
        s = ch['schema_version']
        if ch.get('op') == 'create':
            cls = ch.get('class')
            body = self._lens_body(cls, ch.get('body') or {}, s, self._schema(cls))
        else:
            o = st['objects'].get(ch.get('object_id'))
            if o is None:
                raise BpaError('IAM.NOT_FOUND', 'unknown object')
            cls, cur = o['class'], self._schema(o['class'])
            seen = self._lens_body(cls, self._current_body(self.versions[o['head']]), cur, s)
            seen.update(ch.get('body') or {})
            body = self._lens_body(cls, seen, s, cur)
        out = {k: v for k, v in ch.items() if k != 'schema_version'}
        out['body'] = body
        return out

    # ---- upstream changes (STD-VER-082) ----

    def _note_overrides(self, branch, own, inh):
        for oid in own:
            if oid in inh:
                self._overrides.setdefault((branch, oid), inh[oid][0])

    def _upstream(self, branch):
        pv = self._parent_view(branch, self._branch(branch)['head'], None, {})
        out = {}
        for (b, oid), before in sorted(self._overrides.items()):
            if b != branch:
                continue
            o = pv[0]['objects'].get(oid) if pv else None
            after = o['head'] if o else None
            if after != before:
                out[oid] = {'object_id': oid, 'state': 'upstream_changed', 'before': before, 'after': after}
        return out

    def upstream_changes(self, i):
        """STD-VER-082: the overrides of a branch whose parent-line version changed since the
        override or its last resolution."""
        branch = i.get('branch', 'main')
        self._branch(branch)
        return {'changes': list(self._upstream(branch).values())}

    def resolve_upstream(self, i):
        """STD-VER-082: resolve an upstream change by a three-way merge (base = the parent
        version at the override) or by acknowledgement; both clear the mark."""
        branch = i.get('branch', 'main')
        oid, mode = i.get('object_id'), i.get('mode')
        mark = self._upstream(branch).get(oid)
        if mark is None:
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'the object has no upstream change')
        if mode == 'acknowledge':
            self._overrides[(branch, oid)] = mark['after']
            return {'object_id': oid, 'mode': mode, 'before': mark['before'], 'after': mark['after']}
        if mode != 'merge':
            raise _invalid('mode is merge or acknowledge')
        if mark['after'] is None or self.versions[mark['after']]['tombstone']:
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'the parent line ended the object; acknowledge it')
        st = self._state(branch)[0]
        head = st['objects'][oid]['head']
        self._res, self._used = self._resolutions(i.get('resolutions')), {}
        try:
            merged, conflicts = self._merge_body(oid, mark['before'], mark['after'], head, False)
        finally:
            self._res, self._used = {}, {}
        if conflicts:
            raise BpaError('VER.MERGE_CONFLICT', {'conflicts': conflicts})
        body = dict(merged)
        out = {'object_id': oid, 'mode': mode, 'before': mark['before'], 'after': mark['after'], 'body': body}
        if canonical(body) != canonical(self._current_body(self.versions[head])):
            res = self.commit(branch, [{'op': 'update', 'object_id': oid, 'base': head, 'body': body}], i.get('process'))
            out['cseq'] = res['cseq']
        self._overrides[(branch, oid)] = mark['after']
        return out
