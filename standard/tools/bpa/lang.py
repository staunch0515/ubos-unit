"""Stateful operations of PH-1 step 1e-1 (DEC-043): URI resolution (STD-ADDR-001, -002),
criteria queries over a snapshot (STD-EXPR-011), DID documents (STD-ADDR-010) and custom
value types (STD-FND-026). Mixed into the reference store."""
import base64
import json

from . import BpaError, addr, expr
from .canon import canonical, content_id
from .values import GRAMMARS, validate_value, TYPES

MAX_LIMIT = 1000
DEFAULT_LIMIT = 100


def parse_query(text):
    """STD-EXPR-010 (1.0-rc2, DEC-067): the `DAT-Query` of the textual form
    `from <Class> [only] where <expr> order by <path> [asc|desc], ... limit n`."""
    import re
    if not isinstance(text, str):
        raise BpaError('QRY.INVALID_QUERY', 'a query text is text')
    s = text.strip()
    m = re.match(r'from\s+([A-Z][A-Za-z0-9]*)(\s+only)?(?=\s|$)', s)
    if not m:
        raise BpaError('QRY.INVALID_QUERY', 'a query starts with from <Class>')
    out = {'from': m.group(1)}
    if m.group(2):
        out['only'] = True
    rest = s[m.end():].strip()
    where, order, limit = None, None, None
    mo = re.search(r'(?:^|\s)order\s+by\s', rest)
    ml = re.search(r'(?:^|\s)limit\s+(\d+)\s*$', rest)
    if ml:
        limit = int(ml.group(1))
        rest = rest[:ml.start()].strip()
    if mo and (not ml or mo.start() < ml.start()):
        order = rest[mo.end():].strip()
        rest = rest[:mo.start()].strip()
    if rest:
        if not rest.startswith('where '):
            raise BpaError('QRY.INVALID_QUERY', 'unexpected text %r' % rest[:20])
        where = rest[len('where '):].strip()
        if not where:
            raise BpaError('QRY.INVALID_QUERY', 'where needs an expression')
    if where is not None:
        out['where'] = where
    if order is not None:
        items = []
        for part in order.split(','):
            toks = part.split()
            if not toks or len(toks) > 2 or (len(toks) == 2 and toks[1] not in ('asc', 'desc')):
                raise BpaError('QRY.INVALID_QUERY', 'order by takes <path> [asc|desc]')
            items.append({'path': toks[0], 'dir': toks[1] if len(toks) == 2 else 'asc'})
        out['order_by'] = items
    if limit is not None:
        out['limit'] = limit
    return out


def check_value_type(d, known=()):
    """DAT-ValueTypeDecl (STD-FND-026)."""
    for k in ('name', 'base', 'description'):
        if not isinstance(d, dict) or k not in d:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a value type declares %s' % k)
    if not GRAMMARS['class-name'].match(d['name']) or d['name'] in known:
        raise BpaError('MODEL.INVALID_NAME', 'a value type has a PascalCase name of its own')
    if d['base'] not in TYPES or d['base'] in ('composite', 'list', 'map', 'document', 'blob', 'json'):
        raise BpaError('MODEL.UNKNOWN_TYPE', 'the base of a value type is a scalar type of STD-FND-020')
    base_t = expr.field_type({'type': d['base']})
    for member, want in (('normalize', base_t), ('validate', 'boolean')):
        if d.get(member) is None:
            continue
        env = expr.Env()
        env.vars['value'] = base_t
        t = expr.check(expr.parse(d[member]), env)
        if expr.compat(t, want) != want and t not in ('any', 'null'):
            raise BpaError('LOGIC.TYPE_ERROR', {'position': 0, 'message': '%s returns %s' % (member, expr.type_name(t))})


def custom_value(d, value, constraints=None):
    """Normalise, then validate, a value of a custom type; returns the canonical value."""
    c = dict(d.get('constraints') or {})
    c.update(constraints or {})
    base = d['base']
    v = validate_value(base, None, value)
    if d.get('normalize'):
        v = _run(d, 'normalize', v)
        if v is None:
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'the normaliser gave null')
    v = validate_value(base, c or None, v)
    if d.get('validate') and _run(d, 'validate', v) is False:
        raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'validation of %s failed' % d['name'])
    return v


def _run(d, member, v):
    t = expr.field_type({'type': d['base']})
    env = expr.Env()
    env.vars['value'] = t
    ast = expr.parse(d[member])
    expr.check(ast, env)
    ev = expr.Evaluator(env)
    ev.vars['value'] = expr.from_json(t, v)
    return expr.to_json(ev.ev(ast))


class Lang:

    # ---- custom value types (STD-FND-026) ----

    def _load_value_types(self, decls):
        from . import classes as cl
        known = set()
        for d in decls or []:
            check_value_type(d, known)
            known.add(d['name'])
        self.value_types = {d['name']: d for d in decls or []}
        cl.CUSTOM_TYPES = dict(self.value_types)

    # ---- URI resolution (STD-ADDR-001, -002) ----

    def _slugs(self, oid):
        return {v['slug'] for v in self.versions.values() if v['object_id'] == oid}

    def resolve(self, uri):
        # STD-ADDR-003 (1.0-rc8): short forms and VFS paths are normalised first
        if isinstance(uri, str) and not uri.startswith('ubos://'):
            q = dict(x.partition('=')[::2] for x in uri.partition('?')[2].split('&') if x)
            uri = self._canonical_ref(self._state(q.get('branch', 'main'))[0], uri)
        p = addr.parse_uri(uri)
        if p['tenant'] != self.TENANT or 'vae' in p:
            raise BpaError('IAM.NOT_FOUND', 'unknown authority')
        sel = p['selectors']
        if 'key' in sel:
            raise BpaError('QRY.INVALID_QUERY', 'key selectors are not supported in BPA 1.0-rc1')
        branch = sel.get('branch')
        i = {}
        if 'commit' in sel:
            owners = [c['branch'] for c in self.commits.values() if c['commit_id'] == sel['commit']]
            if not owners:
                raise BpaError('IAM.NOT_FOUND', 'unknown commit')
            if branch and branch != owners[0]:
                raise BpaError('ADDR.CONFLICTING_SELECTORS', 'the commit belongs to branch %s' % owners[0])
            branch, i['commit'] = owners[0], sel['commit']
        if 'tag' in sel:
            t = self.tags.get(sel['tag'])
            if t is None or t['deleted']:
                raise BpaError('IAM.NOT_FOUND', 'unknown tag')
            if branch and branch != t['branch']:
                raise BpaError('ADDR.CONFLICTING_SELECTORS', 'the tag belongs to branch %s' % t['branch'])
            branch, i['tag'] = t['branch'], sel['tag']
        if 'time' in sel:
            i['tx_time'] = sel['time']
        branch = branch or 'main'
        st, prov, ev, cid = self._state(branch, i)
        cands = sorted(oid for oid, o in st['objects'].items() if o['class'] == p['class'] and o['slug'] == p['slug'])
        redirected = False
        if not cands:
            cands = sorted(oid for oid, o in st['objects'].items()
                           if o['class'] == p['class'] and p['slug'] in self._slugs(oid))
            redirected = bool(cands)
        if not cands:
            raise BpaError('IAM.NOT_FOUND', 'no object at %s' % p['canonical'])
        oid = cands[0]
        o = st['objects'][oid]
        if 'version' in p:
            heads = []
            for c in reversed(list(self._first_parents(cid))):
                h = self.commits[c]['state']['objects'].get(oid)
                if h and (not heads or heads[-1] != h['head']):
                    heads.append(h['head'])
            if p['version'] > len(heads):
                raise BpaError('IAM.NOT_FOUND', 'the object has %d versions' % len(heads))
            vh = heads[p['version'] - 1]
            v = self.versions[vh]
            out = {'version_hash': vh, 'body': v['body'], 'slug': v['slug'], 'valid': v['valid']}
        else:
            a = sel.get('asof') or self._default_valid(o['class'])
            from .store import tl_at
            seg = tl_at(o['timeline'], a)
            if seg is None:
                raise BpaError('QRY.NOT_FOUND_AT_TIME')
            out = self._segment(seg)
        out.update(object_id=oid, branch=branch, redirected=redirected, evaluated_at=ev,
                   canonical='ubos://%s/%s/%s' % (self.TENANT, o['class'], o['slug']))
        return out

    # ---- DID documents (STD-ADDR-010) ----

    def did_document(self, did):
        p = addr.parse_did(did)
        if p['form'] == 'key':
            return addr.key_document(did)
        doc = self.node_did()
        if did != doc['id']:
            raise BpaError('IAM.NOT_FOUND', 'no DID document for %s' % did)
        return doc

    # ---- criteria queries (STD-EXPR-011) ----

    def _typed_reader(self, st, asof):
        from .store import tl_at

        def read(uri):
            oid = self._resolve(st, uri)
            if oid is None:
                return None
            o = st['objects'][oid]
            seg = tl_at(o['timeline'], asof or self._default_valid(o['class']))
            if seg is None:
                return None
            return expr.from_json(expr._class_type(list(self.decls.values()), o['class'], self.value_types),
                                  self.versions[seg[2]]['body'])
        return read

    def query(self, i):
        if i.get('text') is not None:
            i = dict(parse_query(i['text']), **{k: v for k, v in i.items() if k != 'text'})
        spec = {k: i.get(k) for k in ('from', 'only', 'where', 'order_by', 'limit', 'asof')}
        qhash = content_id('ubos.chunk.v1', spec)
        offset = 0
        if i.get('cursor'):
            try:
                cur = json.loads(base64.urlsafe_b64decode(i['cursor'] + '=' * (-len(i['cursor']) % 4)))
            except ValueError:
                raise BpaError('QRY.INVALID_QUERY', 'unknown cursor')
            if not isinstance(cur, dict) or cur.get('q') != qhash or cur.get('branch') != i.get('branch', 'main'):
                raise BpaError('QRY.INVALID_QUERY', 'the cursor belongs to another query')
            sel, offset = {'commit': cur['commit']}, cur['offset']
        else:
            sel = {k: i[k] for k in ('time', 'commit', 'tag', 'tx_time') if i.get(k) is not None}
        branch = i.get('branch', 'main')
        st, _, ev, cid = self._state(branch, sel)
        cls = spec['from']
        if not isinstance(cls, str) or (cls not in self.classes):
            raise BpaError('QRY.INVALID_QUERY', 'unknown class %s' % cls)
        limit = spec['limit'] if spec['limit'] is not None else DEFAULT_LIMIT
        if not isinstance(limit, int) or isinstance(limit, bool) or not 1 <= limit <= MAX_LIMIT:
            raise BpaError('QRY.INVALID_QUERY', 'limit is 1 to %d' % MAX_LIMIT)
        classes = list(self.decls.values())
        this_t = expr._class_type(classes, cls, self.value_types)
        where = None
        if spec['where'] is not None:
            where = expr.parse(spec['where'])
            env = expr.Env(this=this_t)
            if expr.check(where, env) not in ('boolean', 'any', 'null'):
                raise BpaError('LOGIC.TYPE_ERROR', {'position': 0, 'message': 'where is a boolean expression'})
        orders = []
        for o in spec['order_by'] or []:
            if not isinstance(o, dict) or o.get('dir', 'asc') not in ('asc', 'desc'):
                raise BpaError('QRY.INVALID_QUERY', 'order_by items are {path, dir}')
            ast = expr.parse(o['path'])
            if ast[0] not in ('member', 'path'):
                raise BpaError('QRY.INVALID_QUERY', 'order_by names a path')
            expr.check(ast, expr.Env(this=this_t))
            orders.append((ast, o.get('dir', 'asc')))
        asof = spec['asof']
        reader = self._typed_reader(st, asof)
        # STD-LGS-051, -052 (1.0-rc8): a query made as a viewer filters rows inside the query
        # and refuses fields masked for the viewer
        viewer = self._viewer(i.get('as'))
        names = self._this_members([where] + [a for a, _ in orders], set()) if viewer else set()
        from .store import tl_at
        rows = []
        for oid in sorted(st['objects']):
            o = st['objects'][oid]
            lin = self._cls(o['class'])['linearization']
            if (o['class'] != cls) if spec['only'] else (cls not in lin):
                continue
            seg = tl_at(o['timeline'], asof or self._default_valid(o['class']))
            if seg is None or self.versions[seg[2]]['tombstone']:
                continue
            if viewer is not None and not self._view_filter(st, branch, [oid], viewer, names, asof):
                continue
            this = expr.from_json(this_t, {k: v for k, v in self.versions[seg[2]]['body'].items()
                                           if k in this_t[1]})
            env = expr.Env(this=this_t)
            if where is not None:
                e = expr.Evaluator(env, this=this, typed_ref=reader)
                if e.ev(where) is not True:
                    continue
            keys = []
            for ast, d in orders:
                e = expr.Evaluator(env, this=this, typed_ref=reader)
                keys.append((e.ev(ast), d))
            rows.append((oid, keys))

        rows = self._order(rows, orders)
        page = rows[offset:offset + limit]
        out = {'objects': [oid for oid, _ in page], 'evaluated_at': ev}
        if offset + limit < len(rows):
            c = {'branch': branch, 'commit': self.commits[cid]['commit_id'], 'offset': offset + limit, 'q': qhash}
            out['cursor'] = base64.urlsafe_b64encode(canonical(c).encode()).rstrip(b'=').decode()
        return out

    @staticmethod
    def _order(rows, orders):
        """Sort by the order_by keys (nulls first, then by value) and the object id."""
        ev = expr.Evaluator(expr.Env())

        def key(v):
            return (0,) if v is None else (1,) + ev.order_key(v, 0)
        rows = sorted(rows, key=lambda r: r[0])
        for n in reversed(range(len(orders))):
            rows = sorted(rows, key=lambda r: key(r[1][n][0]), reverse=orders[n][1] == 'desc')
        return rows
