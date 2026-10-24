"""Search and views of BPA 1.0-rc8 (PH-2 step 1g, DEC-095): grouped aggregates with money
semantics (STD-ISA-081), relationship traversal (STD-ISA-082), full-text search over declared
fields with an index watermark (STD-ISA-083) and view resolution (STD-CLS-023). Mixed into the
reference store; the instructions in `exec.py` call these methods with the process's viewer."""
import math
import re
import unicodedata
from decimal import Decimal

from . import BpaError
from .canon import canonical
from .values import parse_decimal, fmt_decimal, MINOR_UNITS, round_decimal

FUNCTIONS = ('count', 'sum', 'avg', 'min', 'max', 'percentile')
DIRECTIONS = ('out', 'in', 'both')
MAX_DEPTH = 8
DEFAULT_LIMIT = 100
MODES = ('view', 'edit', 'create', 'list', 'card', 'print')
VARIANT_KEYS = ('device', 'role', 'locale')
WORD = re.compile(r'[0-9a-zÀ-ɏ]+')
SNIPPET = 160


def _invalid(what):
    return BpaError('MODEL.INVALID_VALUE', what)


def stem(w):
    """The light English stemmer of STD-ISA-083: plural endings only."""
    if len(w) > 4 and w.endswith('ies'):
        return w[:-3] + 'y'
    if len(w) > 3 and (w.endswith('ses') or w.endswith('xes') or w.endswith('zes') or w.endswith('ches')
                       or w.endswith('shes')):
        return w[:-2]
    if len(w) > 3 and w.endswith('s') and not w.endswith('ss'):
        return w[:-1]
    return w


def tokens(text):
    return [stem(t) for t in WORD.findall(unicodedata.normalize('NFKC', text).lower())]


def _scale(d):
    t = d.as_tuple()
    return max(0, -t.exponent) if isinstance(t.exponent, int) else 0


class Search:

    def _search_init(self):
        self.index_lag = {}        # STD-ISA-083: branch -> cseq the text index is frozen at

    def _viewer_of(self, principal):
        p = self.principals.get(principal) or {}
        return {'principal': principal, 'clearances': p.get('clearances', ['internal']),
                'ctx': {'principal': principal, 'role': p['roles'][0]} if p.get('roles') else {'principal': principal}}

    # ---- search.aggregate (STD-ISA-081) ----

    def _all_rows(self, q):
        """Object ids of a query, all pages."""
        out, cur = [], None
        while True:
            r = self.query(dict(q, limit=1000, cursor=cur) if cur else dict(q, limit=1000))
            out += r['objects']
            cur = r.get('cursor')
            if not cur:
                return out, r['evaluated_at']

    def aggregate(self, branch, sel, a, viewer):
        q = a.get('query')
        if isinstance(q, str):
            from .lang import parse_query
            q = parse_query(q)
        if not isinstance(q, dict) or not isinstance(q.get('from'), str):
            raise BpaError('QRY.INVALID_QUERY', 'query is a DAT-Query or its text')
        group_by = a.get('group_by') or []
        aggs = a.get('aggregates')
        if not isinstance(group_by, list) or not all(isinstance(g, str) for g in group_by):
            raise _invalid('group_by is a list of fields')
        if not isinstance(aggs, list) or not aggs:
            raise _invalid('aggregates is a non-empty list of {fn, field, as, p}')
        for x in aggs:
            if not isinstance(x, dict) or x.get('fn') not in FUNCTIONS or not isinstance(x.get('as'), str) or \
                    (x['fn'] != 'count' and not isinstance(x.get('field'), str)):
                raise _invalid('an aggregate is {fn, field, as, p}; fn is one of %s' % ', '.join(FUNCTIONS))
            if x['fn'] == 'percentile' and (not isinstance(x.get('p'), int) or not 1 <= x['p'] <= 100):
                raise _invalid('p is an integer from 1 to 100')
        conv = a.get('convert')
        if conv is not None and (not isinstance(conv, dict) or conv.get('currency') not in MINOR_UNITS
                                 or not isinstance(conv.get('rates') or {}, dict)):
            raise _invalid('convert is {currency, rates}')
        ids, ev = self._all_rows(dict(q, branch=branch, **sel, **{'as': viewer}))
        st = self._state(branch, sel)[0]
        fields = self._cls(q['from'])['fields']
        used = set(group_by) | {x['field'] for x in aggs if x.get('field')}
        for f in used:
            if f not in fields:
                raise BpaError('QRY.INVALID_QUERY', 'unknown field %s' % f)
        groups = {}
        for oid in ids:
            hidden = set(self._mask_modes(st, branch, oid, viewer, q.get('asof'))) & used
            if hidden:
                raise BpaError('QRY.INVALID_QUERY', {'masked': sorted(hidden)})
            _, body = self._body_at(st, oid, q.get('asof'))
            key = {g: body.get(g) for g in group_by}
            money = [x for x in aggs if x.get('field') and fields[x['field']]['type'] == 'money'
                     and x['fn'] != 'count']
            cur = sorted({body[x['field']]['currency'] for x in money if body.get(x['field'])})
            if money and conv is None:
                if len(cur) > 1:
                    raise BpaError('MODEL.CURRENCY_MISMATCH', 'aggregated money fields of one row differ in currency')
                key['currency'] = cur[0] if cur else None
            g = groups.setdefault(canonical(key), {'key': key, 'rows': 0, 'values': {x['as']: [] for x in aggs}})
            g['rows'] += 1
            for x in aggs:
                v = body.get(x['field']) if x.get('field') else True
                if v is None:
                    continue
                if isinstance(v, dict) and 'currency' in v and conv is not None:
                    v = self._convert(v, conv)
                g['values'][x['as']].append(v)
        out = []
        for k in sorted(groups):
            g = groups[k]
            vals = {}
            for x in aggs:
                vals[x['as']] = self._agg(x, g['values'][x['as']], fields.get(x.get('field') or '', {}).get('type'))
            out.append({'key': g['key'], 'rows': g['rows'], 'values': vals})
        return {'groups': out, 'evaluated_at': ev}

    def _convert(self, m, conv):
        if m['currency'] == conv['currency']:
            return m
        rate = (conv.get('rates') or {}).get(m['currency'])
        if rate is None:
            raise BpaError('MODEL.CURRENCY_MISMATCH', {'no_rate': m['currency']})
        n = MINOR_UNITS[conv['currency']]
        amount = round_decimal(parse_decimal(m['amount']) * parse_decimal(str(rate)), n, 'half_even')
        return {'amount': fmt_decimal(amount, n), 'currency': conv['currency']}

    def _agg(self, x, vals, typ):
        fn = x['fn']
        if fn == 'count':
            return len(vals)
        if not vals:
            return None
        money = isinstance(vals[0], dict) and 'currency' in vals[0]
        if typ in ('integer', 'decimal', 'money'):
            nums = [parse_decimal(v['amount']) if money else Decimal(v) if isinstance(v, int) else parse_decimal(v)
                    for v in vals]
        else:
            nums = None
        if fn in ('sum', 'avg'):
            if nums is None:
                raise BpaError('QRY.INVALID_QUERY', '%s needs a numeric field' % fn)
            total = sum(nums, Decimal(0))
            if fn == 'sum':
                r = total
                scale = MINOR_UNITS[vals[0]['currency']] if money else max(_scale(n) for n in nums)
            else:
                scale = MINOR_UNITS[vals[0]['currency']] if money else max(_scale(n) for n in nums) + 4
                r = round_decimal(total / len(nums), scale, 'half_even')
            if money:
                return {'amount': fmt_decimal(r, scale), 'currency': vals[0]['currency']}
            if typ == 'integer' and fn == 'sum':
                return int(r)
            return fmt_decimal(r, scale) if fn == 'avg' else fmt_decimal(r)
        order = sorted(range(len(vals)), key=lambda n: nums[n] if nums is not None else canonical(vals[n]))
        if fn == 'min':
            return vals[order[0]]
        if fn == 'max':
            return vals[order[-1]]
        rank = max(1, math.ceil(x['p'] / 100 * len(vals)))
        return vals[order[rank - 1]]

    # ---- search.traverse (STD-ISA-082) ----

    def traverse_graph(self, branch, sel, a, viewer):
        st = self._state(branch, sel)[0]
        start = self._resolve(st, a['start']) if isinstance(a.get('start'), str) and a['start'].startswith('ubos://') \
            else a.get('start')
        if start not in st['objects'] or self._body_at(st, start)[1] is None or \
                not self._access(st, branch, start, viewer['principal'], 'read', viewer['ctx'])[0]:
            raise BpaError('IAM.NOT_FOUND', 'unknown start')
        direction = a.get('direction') or 'out'
        depth = 1 if a.get('depth') is None else a['depth']
        limit = DEFAULT_LIMIT if a.get('limit') is None else a['limit']
        names = a.get('fields')
        classes = a.get('classes')
        if direction not in DIRECTIONS:
            raise _invalid('direction is out, in or both')
        if not isinstance(depth, int) or isinstance(depth, bool) or not 1 <= depth <= MAX_DEPTH:
            raise _invalid('depth is 1 to %d' % MAX_DEPTH)
        if not isinstance(limit, int) or isinstance(limit, bool) or limit < 1:
            raise _invalid('limit is a positive integer')
        edges = self._edges(st)
        seen, frontier, nodes, truncated = {start}, [start], [], False
        for d in range(1, depth + 1):
            nxt = []
            for src in sorted(frontier):
                cand = []
                if direction in ('out', 'both'):
                    cand += [(f, t, 'out') for (s, f, t) in edges if s == src]
                if direction in ('in', 'both'):
                    cand += [(f, s, 'in') for (s, f, t) in edges if t == src]
                for f, t, dirn in sorted(cand):
                    if names is not None and f not in names:
                        continue
                    if t in seen:
                        continue
                    seen.add(t)
                    if not self._access(st, branch, t, viewer['principal'], 'read', viewer['ctx'])[0]:
                        continue
                    nxt.append(t)
                    o = st['objects'][t]
                    if classes is None or o['class'] in classes:
                        if len(nodes) == limit:
                            truncated = True
                            continue
                        nodes.append({'object_id': t, 'uri': 'ubos://%s/%s/%s' % (self.TENANT, o['class'], o['slug']),
                                      'class': o['class'], 'depth': d,
                                      'via': {'from': src, 'field': f, 'direction': dirn}})
            frontier = nxt
        return {'nodes': nodes, 'truncated': truncated, 'evaluated_at': self._state(branch, sel)[2]}

    def _edges(self, st):
        """(source, field, target) of every live reference at the snapshot."""
        out = []
        for oid in sorted(st['objects']):
            o, body = self._body_at(st, oid)
            if body is None:
                continue
            eff = self._cls(o['class'])
            for f, uri in self._refs(eff, body):
                t = self._resolve(st, uri)
                if t and self._body_at(st, t)[1] is not None:
                    out.append((oid, f, t))
        return out

    # ---- search.text (STD-ISA-083) ----

    def set_index_lag(self, i):
        """Freeze the text index of a branch at a commit (`cseq`), or let it follow (null)."""
        branch = i.get('branch', 'main')
        self._branch(branch)
        c = i.get('cseq')
        if c is None:
            self.index_lag.pop(branch, None)
        elif not isinstance(c, int) or isinstance(c, bool) or c < 0:
            raise _invalid('cseq is a commit sequence number')
        else:
            self.index_lag[branch] = c
        return {'branch': branch, 'cseq': c}

    def text_search(self, branch, a, viewer):
        text = a.get('text')
        if not isinstance(text, str) or not tokens(text):
            raise _invalid('text has at least one word')
        limit = 20 if a.get('limit') is None else a['limit']
        if not isinstance(limit, int) or isinstance(limit, bool) or limit < 1:
            raise _invalid('limit is a positive integer')
        classes = a.get('classes')
        head = self.commits[self._branch(branch)['head']]['cseq']
        wm = min(head, self.index_lag.get(branch, head))
        st = self._state(branch, {'time': wm})[0]
        terms = set(tokens(text))
        hits = []
        for oid in sorted(st['objects']):
            o, body = self._body_at(st, oid)
            if body is None or (classes is not None and o['class'] not in classes):
                continue
            idx = [f for f, d in self._cls(o['class'])['fields'].items() if d.get('text_index')]
            if not idx or not self._access(st, branch, oid, viewer['principal'], 'read', viewer['ctx'])[0]:
                continue
            masked = set(self._mask_modes(st, branch, oid, viewer))
            matched, tf, first = set(), 0, None
            for f in idx:
                v = body.get(f)
                if f in masked or not isinstance(v, str):
                    continue
                ts = tokens(v)
                n = sum(1 for t in ts if t in terms)
                if n and first is None:
                    first = (f, v)
                tf += n
                matched |= terms & set(ts)
            if matched:
                hits.append((-len(matched), -tf, oid, o['class'], first, len(matched), tf))
        hits.sort()
        items = [{'object_id': h[2], 'class': h[3], 'score': [h[5], h[6]], 'field': h[4][0],
                  'snippet': self._snippet(h[4][1], terms)} for h in hits[:limit]]
        return {'items': items, 'watermark': {'cseq': wm}, 'lagging': wm < head}

    @staticmethod
    def _snippet(text, terms):
        def mark(m):
            return '<em>%s</em>' % m.group(0) if stem(m.group(0).lower()) in terms else m.group(0)
        word = re.compile(r'[0-9A-Za-zÀ-ɏ]+')
        if len(text) > SNIPPET:
            first = next((m.start() for m in word.finditer(text) if stem(m.group(0).lower()) in terms), 0)
            a = max(0, first - SNIPPET // 2)
            text = ('…' if a else '') + text[a:a + SNIPPET] + ('…' if a + SNIPPET < len(text) else '')
        return word.sub(mark, text)

    # ---- views (STD-CLS-023) ----

    def resolve_view(self, i):
        """The view of a class or object for a mode and a context: the declared view of the
        first class of the linearization that declares one, refined by the best matching
        variant, or the view generated from the effective definition."""
        branch = i.get('branch', 'main')
        st = self._state(branch, i)[0]
        mode = i.get('mode')
        if mode not in MODES:
            raise _invalid('mode is one of %s' % ', '.join(MODES))
        ctx = i.get('ctx') or {}
        if not isinstance(ctx, dict) or any(k not in VARIANT_KEYS for k in ctx):
            raise _invalid('ctx has device, role and locale')
        oid = i.get('object_id')
        if oid is not None:
            o, body = self._body_at(st, oid)
            if body is None:
                raise BpaError('IAM.NOT_FOUND', 'unknown object')
            cls = o['class']
        else:
            cls, body = i.get('class'), None
            self._cls(cls)
        exts = self._extensions(st)
        for c in self._cls(cls)['linearization']:
            declared = list((self.decls.get(c) or {}).get('views') or [])
            for e in sorted([e for e in exts if e.get('target') == c], key=lambda e: (e['order'], e['namespace'])):
                declared += [v for v in e.get('views') or []]
            found = [v for v in declared if isinstance(v, dict) and v.get('mode') == mode]
            if not found:
                continue
            v = found[-1]
            best, n = None, 0
            for k, var in enumerate(v.get('variants') or []):
                keys = [x for x in VARIANT_KEYS if x in var]
                if keys and all(ctx.get(x) == var[x] for x in keys) and len(keys) > n:
                    best, n = k, len(keys)
            uri = v['variants'][best]['view'] if best is not None else v.get('view')
            vid = self._resolve(st, uri) if isinstance(uri, str) else None
            if vid is None or 'ViewAsset' not in self._cls(st['objects'][vid]['class'])['linearization']:
                raise BpaError('MODEL.REFERENCE_INVALID', 'view %s' % uri)
            return {'source': 'declared', 'class': c, 'mode': mode, 'view': uri, 'view_id': vid, 'variant': best}
        return {'source': 'generated', 'class': cls, 'mode': mode,
                'generated': self._generated_view(st, branch, cls, oid, body, mode)}

    def _generated_view(self, st, branch, cls, oid, body, mode):
        eff = self._cls(cls)
        props = {}
        state, rules, lc = None, {}, self._lifecycle(st, cls)
        if oid is not None:
            props, _ = self._effective(st, branch, oid, {}, self._default_valid(cls))
            state = self.versions[st['objects'][oid]['head']].get('state')
            if lc and state is not None:
                rules = (lc[0].get('field_rules') or {}).get(state) or {}
        elif lc:
            state = lc[0]['initial']
        fields = []
        for name, f in eff['fields'].items():
            vis = props.get('field_visibility.%s' % name, 'visible')
            if name in (rules.get('hidden') or []):
                vis = 'hidden'
            if vis == 'hidden':
                continue
            item = {'name': name, 'type': f['type'], 'label': f.get('label', name),
                    'classification': f.get('classification', 'internal')}
            if f.get('required') or name in (rules.get('required') or []):
                item['required'] = True
            if vis == 'read_only' or name in (rules.get('read_only') or []) or mode in ('view', 'card', 'print', 'list'):
                item['read_only'] = True
            fields.append(item)
        ports = sorted(n for n, p in (eff.get('_ports') or {}).items()
                       if not p.get('states') or state in p['states'])
        out = {'fields': fields, 'ports': ports}
        if state is not None:
            out['state'] = state
        return out
