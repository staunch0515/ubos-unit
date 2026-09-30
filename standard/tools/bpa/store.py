"""In-memory reference store for the stateful adapter operations (STD-CONF-005).

Covers objects of Definition, RawDocument and System kind with bitemporal timelines
(STD-TIME-011), ledgers (STD-KIND-020…025), branches and commits (STD-VER-001…013),
base and read-set validation (STD-TXN-020…024), three-way merge with ledger append and
virtual bases (STD-VER-040…056), diff (STD-VER-030) and the state tree (STD-PROOF-001).

Timelines are lists of segments [from, to, version] with to = None for an open end; valid
times are canonical dates or instants, so string order is time order."""
import copy
import itertools

from . import BpaError
from .canon import canonical, content_id
import hmac
import hashlib

from .classes import GENESIS, effective_class, is_reference_list, public
from .tree import tree_root
from .values import GRAMMARS, check_name, validate_value, parse_decimal, fmt_decimal, MINOR_UNITS, REFERENCE

DEFAULT_NOW = '2026-01-01T00:00:00.000000Z'
TENANT = 'acme'                  # authority of object URIs in adapter inputs (STD-CONF-005)
MAX_BODY = 16 * 1024 * 1024      # STD-OBJ-009: bodies up to 16 MiB


def _lt(a, b):
    """a < b where None is +infinity."""
    if a is None:
        return False
    return b is None or a < b


def _min(a, b):
    return a if not _lt(b, a) else b

# ---- timeline algebra (STD-TIME-011, DEC-032) ----


def tl_remove(tl, a, b):
    out = []
    for f, t, v in tl:
        if _lt(f, a):
            out.append([f, _min(t, a), v])
        if _lt(b, t) and b is not None:
            out.append([b if _lt(f, b) else f, t, v])
    return [s for s in out if _lt(s[0], s[1])]


def tl_apply(tl, kind, v, a, b=None):
    if kind == 'create':
        if tl:
            raise BpaError('TXN.ALREADY_EXISTS')
        return [[a, None, v]]
    if not tl:
        raise BpaError('QRY.NOT_FOUND_AT_TIME', 'no timeline to change')
    if kind == 'supersede_from':
        starts = [f for f, _, _ in tl if _lt(a, f)]
        e = min(starts) if starts else None
        out = tl_remove(tl, a, e) + [[a, e, v]]
    elif kind == 'set_period':
        if not _lt(a, b):
            raise BpaError('MODEL.INVALID_VALUE', 'empty period')
        for f, t, _ in tl:
            if _lt(f, b) and _lt(a, t) and not (not _lt(f, a) and not _lt(b, t)):
                raise BpaError('TIME.OVERLAPPING_VALIDITY')
        out = tl_remove(tl, a, b) + [[a, b, v]]
    elif kind == 'replace_future':
        out = tl_remove(tl, a, None) + [[a, None, v]]
    elif kind == 'end':
        out = tl_remove(tl, a, None)
    else:
        raise BpaError('MODEL.INVALID_VALUE', 'unknown timeline operation')
    return sorted(out, key=lambda s: s[0])


def tl_range(tl, kind, a, b=None):
    """Valid range changed by an operation, as [from, to)."""
    if kind == 'supersede_from':
        starts = [f for f, _, _ in tl if _lt(a, f)]
        return [a, min(starts) if starts else None]
    if kind == 'set_period':
        return [a, b]
    return [a, None]


def tl_at(tl, asof):
    for f, t, v in tl:
        if not _lt(asof, f) and _lt(asof, t):
            return [f, t, v]
    return None


class Store:
    def __init__(self):
        self.reset()

    # ---- adapter lifecycle (STD-CONF-002) ----

    def reset(self):
        self.now = DEFAULT_NOW
        self.max_body = MAX_BODY
        self._exts = None
        self._eff_cache = {}
        self.decls = {}
        self.classes = {}
        self.versions = {}                     # version hash -> {object_id, class, body, valid, tombstone}
        self.commits = {0: {'parents': [], 'state': {'objects': {}, 'ledger': {}}, 'cseq': 0}}
        self.branches = {'main': {'head': 0}}
        self.ids = itertools.count(1)
        return True

    def set_now(self, now):
        self.now = now
        return True

    def set_limits(self, max_body_bytes=None):
        if max_body_bytes is not None:
            if not isinstance(max_body_bytes, int) or isinstance(max_body_bytes, bool) or max_body_bytes < 1:
                raise BpaError('MODEL.INVALID_VALUE', 'max_body_bytes is a positive integer')
            self.max_body = max_body_bytes
        return True

    def load_genesis(self, classes):
        decls = dict(self.decls)
        for d in classes:
            decls[d['name']] = d
        eff = {}
        for n in decls:
            eff[n] = effective_class(list(decls.values()), n)
        for n, e in eff.items():
            old = self.classes.get(n)
            if old and old['kind'] != e['kind'] and self._instances(n):
                raise BpaError('MODEL.KIND_IMMUTABLE')
        self.decls, self.classes = decls, eff
        return True

    def _instances(self, name):
        subclasses = {n for n, e in self.classes.items() if name in e['linearization']}
        for c in self.commits.values():
            st = c['state']
            if any(o['class'] in subclasses for o in st['objects'].values()):
                return True
            if any(k[0] in subclasses for k in st['ledger']):
                return True
        return False

    # ---- helpers ----

    def _branch(self, name):
        if name not in self.branches:
            raise BpaError('IAM.NOT_FOUND', 'unknown branch')
        return self.branches[name]

    def _commit_at(self, branch, time=None):
        cid = self._branch(branch)['head']
        if time is None:
            return cid
        while self.commits[cid]['cseq'] > time:
            cid = self.commits[cid]['parents'][0]
        if self.commits[cid]['cseq'] != time:
            raise BpaError('IAM.NOT_FOUND', 'no commit with this cseq on the branch')
        return cid

    def _decl(self, name):
        return self.decls.get(name) or GENESIS[name]

    def _cls(self, name):
        exts = getattr(self, '_exts', None)
        if exts or name == 'ClassExtension':
            if name != 'ClassExtension' and name not in self.classes:
                raise BpaError('MODEL.REFERENCE_INVALID', 'unknown class %s' % name)
            key = (name, id(exts))
            if key not in self._eff_cache:
                self._eff_cache[key] = effective_class(list(self.decls.values()), name, exts or ())
            return self._eff_cache[key]
        if name not in self.classes:
            raise BpaError('MODEL.REFERENCE_INVALID', 'unknown class %s' % name)
        return self.classes[name]

    def _today(self):
        return self.now[:10]

    def _default_valid(self, cls):
        return self._today() if self._decl(cls).get('valid_time', 'date') == 'date' else self.now

    def _check_body(self, cls, body, partial=False):
        eff = self._cls(cls)
        if not isinstance(body, dict):
            raise BpaError('MODEL.INVALID_VALUE', 'body is an object')
        out = {}
        for k, v in body.items():
            if k not in eff['fields']:
                if eff['kind'] == 'raw_document':
                    out[k] = v
                    canonical(v)
                    continue
                raise BpaError('MODEL.INVALID_VALUE', 'undeclared field %s' % k)
            f = eff['fields'][k]
            out[k] = validate_value(f['type'], f.get('constraints'), v)
        if not partial:
            for name, f in eff['fields'].items():
                if f.get('required') and out.get(name) is None:
                    raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'required field %s' % name)
            if len(canonical(out).encode('utf-8')) > self.max_body:
                raise BpaError('MODEL.OBJECT_TOO_LARGE', 'body larger than the configured limit')
        return out

    def _version(self, oid, cls, slug, body, valid, predecessor=None, tombstone=False):
        env = {'object_id': oid, 'class': cls, 'class_version': content_id('ubos.chunk.v1', self._decl(cls)),
               'schema_version': self._decl(cls).get('schema_version', 1), 'kind': self._cls(cls)['kind'],
               'slug': slug, 'valid': valid, 'body_ref': content_id('ubos.chunk.v1', body)}
        if predecessor:
            env['predecessor'] = predecessor
        if tombstone:
            env['tombstone'] = True
        vh = content_id('ubos.version.v1', env)
        self.versions[vh] = {'object_id': oid, 'class': cls, 'slug': slug, 'body': body, 'valid': valid,
                             'tombstone': tombstone}
        return vh

    # ---- branches (STD-VER-010, -013) ----

    def create_branch(self, name, source='main', time=None):
        check_name('branch-name', name)
        if name in self.branches:
            raise BpaError('TXN.ALREADY_EXISTS', 'branch exists')
        cid = self._commit_at(source, time)
        self.branches[name] = {'head': cid}
        return {'head': self.commits[cid]['cseq']}

    # ---- commit (STD-TXN-001…024, STD-KIND-020…025) ----

    def commit(self, branch, changes, process=None, tx_time=None):
        if tx_time is not None or any('tx_time' in ch for ch in changes):
            raise BpaError('TIME.RESERVED_FIELD')
        br = self._branch(branch)
        head = self.commits[br['head']]
        st = copy.deepcopy(head['state'])
        cseq = head['cseq'] + 1
        result = {'cseq': cseq, 'versions': {}, 'entries': {}, 'retroactive': []}
        if process and process.get('serializable'):
            for oid, vh in process.get('reads', {}).items():
                if st['objects'].get(oid, {}).get('head') != vh:
                    raise BpaError('TXN.READ_SET_CHANGED')
        self._exts = self._extensions(st)
        self._eff_cache = {}
        self._written, self._ended = [], []
        try:
            return self._commit(br, head, st, cseq, result, changes, process)
        finally:
            self._exts = None
            self._eff_cache = {}

    def _commit(self, br, head, st, cseq, result, changes, process):
        appended = []
        stale = []
        for ch in changes:
            op = ch.get('op')
            if op in ('update', 'end') and ch.get('base') is not None:
                o = st['objects'].get(ch.get('object_id'))
                if o is not None and o['head'] != ch['base']:
                    stale.append(ch['object_id'])
        if stale:
            raise BpaError('TXN.HEAD_MOVED', {'objects': sorted(stale)})
        for ch in changes:
            op = ch.get('op')
            if op == 'create':
                self._create(st, ch, result)
            elif op in ('update', 'end'):
                self._update(st, ch, result, op)
            elif op in ('append', 'reverse', 'adjust'):
                appended.append(self._append(st, ch, result, op, cseq))
            else:
                raise BpaError('MODEL.INVALID_VALUE', 'unknown change')
        self._check_double_entry(appended)
        self._relationships(st, result)
        self._integrity(st, appended)
        self._check_extensions(st)
        cid = next(self.ids)
        self.commits[cid] = {'parents': [br['head']], 'state': st, 'cseq': cseq}
        br['head'] = cid
        return result

    def _entry_owner(self, st, entry_id):
        for k, q in st['ledger'].items():
            for e in q:
                if e['entry_id'] == entry_id:
                    return k, e
        return None, None

    def _create(self, st, ch, result):
        oid, cls = ch['object_id'], ch['class']
        eff = self._cls(cls)
        if not GRAMMARS['uuid'].match(oid):
            raise BpaError('MODEL.INVALID_VALUE', 'object_id is a UUID')
        if self.decls.get(cls, {}).get('abstract'):
            raise BpaError('MODEL.ABSTRACT_INSTANTIATION')
        if eff['kind'] == 'system' or eff['kind'] == 'ledger':
            raise BpaError('LOGIC.FORBIDDEN_TARGET')
        if oid in st['objects'] or self._entry_owner(st, oid)[1]:
            raise BpaError('TXN.ALREADY_EXISTS')
        check_name('slug', ch['slug'])
        body = self._check_body(cls, ch.get('body', {}))
        valid = ch.get('valid') or {}
        a = valid.get('from') or self._default_valid(cls)
        vh = self._version(oid, cls, ch['slug'], body, {'from': a, 'to': None})
        st['objects'][oid] = {'class': cls, 'kind': eff['kind'], 'slug': ch['slug'], 'head': vh,
                              'timeline': tl_apply([], 'create', vh, a)}
        result['versions'][oid] = vh
        self._written.append((oid, vh))

    def _update(self, st, ch, result, op):
        oid = ch.get('object_id')
        o = st['objects'].get(oid)
        if o is None:
            if self._entry_owner(st, oid)[1]:
                raise BpaError('LEDG.IMMUTABLE_ENTRY')
            raise BpaError('IAM.NOT_FOUND', 'unknown object')
        if o['kind'] == 'system':
            raise BpaError('LOGIC.FORBIDDEN_TARGET')
        if ch.get('base') is None:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'updates name their base (STD-TXN-020)')
        base = self.versions[o['head']]
        valid = ch.get('valid') or {}
        if op == 'end':
            d = valid.get('from') or self._default_valid(o['class'])
            vh = self._version(oid, o['class'], o['slug'], base['body'], {'from': d, 'to': None},
                               predecessor=o['head'], tombstone=True)
            o['timeline'] = tl_apply(o['timeline'], 'end', vh, d)
            self._ended.append((oid, d))
            if d < self._default_valid(o['class']):
                result['retroactive'].append({'object_id': oid, 'from': d, 'to': None})
        else:
            body = dict(base['body'])
            body.update(self._check_body(o['class'], ch.get('body', {}), partial=True))
            self._check_body(o['class'], body)
            slug = ch.get('slug', o['slug'])
            check_name('slug', slug)
            kind = valid.get('kind', 'supersede_from')
            a = valid.get('from') or self._default_valid(o['class'])
            b = valid.get('to')
            to = b if kind == 'set_period' else (None if kind == 'replace_future' else tl_range(o['timeline'], kind, a)[1])
            vh = self._version(oid, o['class'], slug, body, {'from': a, 'to': to}, predecessor=o['head'])
            rng = tl_range(o['timeline'], kind, a, b)
            o['timeline'] = tl_apply(o['timeline'], kind, vh, a, b)
            o['slug'] = slug
            self._written.append((oid, vh))
            if rng[0] < self._default_valid(o['class']):
                result['retroactive'].append({'object_id': oid, 'from': rng[0], 'to': rng[1]})
        o['head'] = vh
        result['versions'][oid] = vh

    def _amounts(self, cls):
        return self.decls[cls].get('ledger', {}).get('amounts', [])

    def _append(self, st, ch, result, op, cseq):
        if op == 'append':
            cls = ch['class']
            eff = self._cls(cls)
            if eff['kind'] != 'ledger':
                raise BpaError('LOGIC.FORBIDDEN_TARGET')
            spec = self.decls[cls]['ledger']
            lk = ch['ledger_key']
            if sorted(lk) != sorted(spec['key']):
                raise BpaError('MODEL.INVALID_VALUE', 'ledger key fields')
            for k in spec['key']:
                f = eff['fields'][k]
                lk[k] = validate_value(f['type'], f.get('constraints'), lk[k])
            body = self._check_body(cls, ch.get('body', {}))
            entry = {'entry_id': ch['entry_id'], 'class': cls, 'ledger_key': lk,
                     'valid_at': ch.get('valid_at') or self._default_valid(cls), 'body': body}
            if ch.get('posting_group'):
                entry['posting_group'] = ch['posting_group']
        else:
            key, target = self._entry_owner(st, ch['entry_id'])
            if target is None:
                raise BpaError('IAM.NOT_FOUND', 'unknown entry')
            cls = target['class']
            q = st['ledger'][key]
            if op == 'reverse':
                if target.get('correction_type') == 'reversal':
                    raise BpaError('LEDG.REVERSAL_NOT_REVERSIBLE')
                if any(e.get('correction_type') == 'reversal' and target['entry_id'] in e.get('corrects', [])
                       for e in q):
                    raise BpaError('LEDG.ALREADY_REVERSED')
                body = dict(target['body'])
                for a in self._amounts(cls):
                    body[a] = self._negate(body[a])
            else:
                body = self._check_body(cls, ch.get('body', {}))
            entry = {'entry_id': ch['new_entry_id'], 'class': cls, 'ledger_key': target['ledger_key'],
                     'valid_at': ch.get('valid_at') or target['valid_at'], 'body': body,
                     'corrects': [target['entry_id']],
                     'correction_type': 'reversal' if op == 'reverse' else 'adjustment'}
            if target.get('posting_group') and op == 'reverse':
                entry['posting_group'] = target['posting_group'] + '/reversal'
        if not GRAMMARS['uuid'].match(entry['entry_id']):
            raise BpaError('MODEL.INVALID_VALUE', 'entry_id is a UUID')
        if self._entry_owner(st, entry['entry_id'])[1] or entry['entry_id'] in st['objects']:
            raise BpaError('TXN.ALREADY_EXISTS')
        self._check_period(st, cls)
        key = (cls, canonical(entry['ledger_key']))
        q = st['ledger'].setdefault(key, [])
        entry['seq'] = len(q) + 1
        entry['cseq'] = cseq
        entry['entry_hash'] = self.entry_hash(entry)
        q.append(entry)
        result['entries'][entry['entry_id']] = {'seq': entry['seq']}
        return entry

    @staticmethod
    def _negate(v):
        if isinstance(v, dict):
            return {'amount': fmt_decimal(-parse_decimal(v['amount']), MINOR_UNITS[v['currency']]),
                    'currency': v['currency']}
        return fmt_decimal(-parse_decimal(v))

    def entry_hash(self, e):
        rec = {k: e[k] for k in ('entry_id', 'class', 'ledger_key', 'valid_at', 'corrects', 'correction_type',
                                 'posting_group') if k in e}
        rec['class_version'] = content_id('ubos.chunk.v1', self.decls[e['class']])
        rec['body_ref'] = content_id('ubos.chunk.v1', e['body'])
        return content_id('ubos.entry.v1', rec)

    def _check_period(self, st, cls):
        scope = self.decls[cls].get('ledger', {}).get('period_scope')
        if not scope:
            return
        day = self._today()
        for o in st['objects'].values():
            if o['class'] != scope:
                continue
            seg = tl_at(o['timeline'], day)
            body = self.versions[o['head']]['body'] if seg is None else self.versions[seg[2]]['body']
            if body.get('state') == 'closed' and body['start'] <= day < body['end']:
                raise BpaError('LEDG.PERIOD_CLOSED')

    def _check_double_entry(self, appended):
        groups = {}
        for e in appended:
            if not self.decls[e['class']].get('ledger', {}).get('double_entry'):
                continue
            g = groups.setdefault((e['class'], e.get('posting_group', e['entry_id'])), {})
            for a in self._amounts(e['class']):
                v = e['body'][a]
                cur = v['currency'] if isinstance(v, dict) else ''
                amt = parse_decimal(v['amount'] if isinstance(v, dict) else v)
                g[cur] = g.get(cur, 0) + amt
        for totals in groups.values():
            if any(t != 0 for t in totals.values()):
                raise BpaError('LEDG.UNBALANCED')

    # ---- extensions (STD-CLS-030) ----

    def _extensions(self, st):
        out = []
        for oid in sorted(st['objects']):
            o = st['objects'][oid]
            if o['class'] == 'ClassExtension' and not self.versions[o['head']]['tombstone']:
                out.append(self.versions[o['head']]['body'])
        return tuple(out)

    def _check_extensions(self, st):
        """Every extension written in this commit must apply to its target."""
        exts = self._extensions(st)
        for oid, vh in self._written:
            if st['objects'][oid]['class'] != 'ClassExtension':
                continue
            target = self.versions[vh]['body']['target']
            if target not in self.classes:
                raise BpaError('MODEL.REFERENCE_INVALID', 'unknown target class %s' % target)
            for name, e in self.classes.items():
                if target in e['linearization']:
                    effective_class(list(self.decls.values()), name, exts)

    # ---- references and relationships (STD-CLS-040, -041) ----

    def _uri(self, o):
        return 'ubos://%s/%s/%s' % (TENANT, o['class'], o['slug'])

    def _resolve(self, st, uri):
        """Object ID of the object a canonical URI names on this state, or None."""
        if not REFERENCE.match(uri):
            return None
        authority, cls, slug = uri[len('ubos://'):].split('/')
        if authority != TENANT:
            return None
        for oid, o in st['objects'].items():
            if o['class'] == cls and o['slug'] == slug:
                return oid
        return None

    def _live(self, st, oid):
        o = st['objects'].get(oid)
        return o is not None and not self.versions[o['head']]['tombstone']

    def _refs(self, eff, body):
        """[(field, uri)] of the reference values of a body."""
        out = []
        for name, f in eff['fields'].items():
            v = body.get(name)
            if v is None:
                continue
            if f['type'] == 'reference':
                out.append((name, v))
            elif is_reference_list(f):
                out += [(name, x) for x in v]
        return out

    def _covered(self, tl, f, t):
        """True when the segments of a timeline cover [f, t) without a gap."""
        at = f
        for sf, stt, _ in sorted(tl, key=lambda x: x[0]):
            if _lt(at, sf):
                break
            if _lt(at, stt) or stt is None:
                at = stt
                if at is None or (t is not None and not _lt(at, t)):
                    return True
        return at is None or (t is not None and not _lt(at, t))

    def _target_ok(self, st, uri, field, eff, period=None, instant=None):
        oid = self._resolve(st, uri)
        if oid is None:
            raise BpaError('MODEL.REFERENCE_INVALID', 'reference %s does not resolve' % uri)
        o = st['objects'][oid]
        tc = (eff['fields'][field].get('constraints') or {}).get('target_class')
        item = (eff['fields'][field].get('constraints') or {}).get('item_type') or {}
        tc = tc or (item.get('constraints') or {}).get('target_class')
        if tc and tc not in self._cls(o['class'])['linearization']:
            raise BpaError('MODEL.REFERENCE_INVALID', 'target is not a %s' % tc)
        if period is not None and not self._covered(o['timeline'], period[0], period[1]):
            raise BpaError('MODEL.REFERENCE_INVALID', 'target not valid for the whole period')
        if instant is not None and tl_at(o['timeline'], instant) is None:
            raise BpaError('MODEL.REFERENCE_INVALID', 'target not valid at the entry time')
        return oid

    def _integrity(self, st, appended):
        for oid, vh in self._written:
            v = self.versions[vh]
            if v['tombstone'] or not self._live(st, oid):
                continue
            eff = self._cls(v['class'])
            for field, uri in self._refs(eff, v['body']):
                self._target_ok(st, uri, field, eff, period=(v['valid']['from'], v['valid']['to']))
            self._links(st, oid, eff, v['body'])
        for e in appended:
            eff = self._cls(e['class'])
            for field, uri in self._refs(eff, e['body']):
                self._target_ok(st, uri, field, eff, instant=e['valid_at'])

    def _links(self, st, oid, eff, body):
        """Link objects of relationships with an attributes class (STD-CLS-040)."""
        me = self._uri(st['objects'][oid])
        for name, f in eff['fields'].items():
            rel = f.get('relationship') or {}
            if not rel.get('attributes_class') or body.get(name) is None:
                continue
            for uri in (body[name] if isinstance(body[name], list) else [body[name]]):
                lid = self._resolve(st, uri)
                link = st['objects'].get(lid) if lid else None
                if link is None or rel['attributes_class'] not in self._cls(link['class'])['linearization']:
                    raise BpaError('MODEL.REFERENCE_INVALID', 'link is not a %s' % rel['attributes_class'])
                if self.versions[link['head']]['body'].get('source') != me:
                    raise BpaError('MODEL.REFERENCE_INVALID', 'link does not reference its source')

    def _relations(self, st):
        """[(source oid, field, relationship, target oid)] over live sources."""
        out = []
        for oid in sorted(st['objects']):
            if not self._live(st, oid):
                continue
            o = st['objects'][oid]
            eff = self._cls(o['class'])
            body = self.versions[o['head']]['body']
            for field, uri in self._refs(eff, body):
                rel = eff['fields'][field].get('relationship')
                tid = self._resolve(st, uri)
                if rel and tid:
                    out.append((oid, field, rel, tid))
        return out

    def _end_object(self, st, oid, d, result):
        o = st['objects'][oid]
        base = self.versions[o['head']]
        vh = self._version(oid, o['class'], o['slug'], base['body'], {'from': d, 'to': None},
                           predecessor=o['head'], tombstone=True)
        o['timeline'] = tl_apply(o['timeline'], 'end', vh, d)
        o['head'] = vh
        result['versions'][oid] = vh
        self._ended.append((oid, d))
        if d < self._default_valid(o['class']):
            result['retroactive'].append({'object_id': oid, 'from': d, 'to': None})

    def _relationships(self, st, result):
        """on_end rules, composition and cardinality (STD-CLS-040)."""
        done = set()
        while True:
            todo = [(x, d) for x, d in self._ended if x not in done]
            if not todo:
                break
            for x, d in todo:
                done.add(x)
                o = st['objects'][x]
                eff = self._cls(o['class'])
                for field, uri in self._refs(eff, self.versions[o['head']]['body']):
                    rel = eff['fields'][field].get('relationship') or {}
                    tid = self._resolve(st, uri)
                    if rel.get('ownership') == 'composition' and tid and self._live(st, tid):
                        self._end_object(st, tid, d, result)
                for src, field, rel, tid in self._relations(st):
                    if tid != x or src == x:
                        continue
                    rule = rel.get('on_end', 'restrict')
                    if rule == 'restrict':
                        raise BpaError('MODEL.REFERENCE_RESTRICTED', 'referenced by %s' % src)
                    if rule == 'cascade':
                        if self._live(st, src):
                            self._end_object(st, src, d, result)
                        continue
                    o = st['objects'][src]
                    body = dict(self.versions[o['head']]['body'])
                    uri = self._uri(st['objects'][x])
                    body[field] = [u for u in body[field] if u != uri] if isinstance(body[field], list) else None
                    if body[field] is None:
                        del body[field]
                    self._check_body(o['class'], body)
                    rng = tl_range(o['timeline'], 'supersede_from', d)
                    vh = self._version(src, o['class'], o['slug'], body, {'from': rng[0], 'to': rng[1]},
                                       predecessor=o['head'])
                    o['timeline'] = tl_apply(o['timeline'], 'supersede_from', vh, d)
                    o['head'] = vh
                    result['versions'][src] = vh
                    if d < self._default_valid(o['class']):
                        result['retroactive'].append({'object_id': src, 'from': rng[0], 'to': rng[1]})
        counts = {}
        for src, field, rel, tid in self._relations(st):
            if rel['cardinality'] in ('one_to_one', 'one_to_many'):
                counts.setdefault((field, tid), set()).add(src)
        if any(len(v) > 1 for v in counts.values()):
            raise BpaError('MODEL.CARDINALITY_VIOLATED')

    # ---- queries of the class system (STD-CLS-022, -030, -040) ----

    def list_objects(self, branch, cls, only=False, time=None):
        st = self.commits[self._commit_at(branch, time)]['state']
        if cls not in self.classes and cls not in GENESIS:
            raise BpaError('MODEL.REFERENCE_INVALID', 'unknown class %s' % cls)
        out = []
        for oid, o in st['objects'].items():
            if not self._live(st, oid):
                continue
            lin = self._cls(o['class'])['linearization']
            if (o['class'] == cls) if only else (cls in lin):
                out.append(oid)
        return {'objects': sorted(out)}

    def traverse(self, branch, object_id, relation, time=None):
        st = self.commits[self._commit_at(branch, time)]['state']
        o = st['objects'].get(object_id)
        if o is None:
            raise BpaError('IAM.NOT_FOUND', 'unknown object')
        eff = self._cls(o['class'])
        f = eff['fields'].get(relation)
        out = set()
        if f is not None and f.get('relationship'):
            rel = f['relationship']
            for fld, uri in self._refs(eff, self.versions[o['head']]['body']):
                if fld != relation:
                    continue
                tid = self._resolve(st, uri)
                if tid and rel.get('attributes_class'):
                    tid = self._resolve(st, self.versions[st['objects'][tid]['head']]['body'].get('target', ''))
                if tid and self._live(st, tid):
                    out.add(tid)
            return {'objects': sorted(out)}
        known = False
        for src, field, rel, tid in self._relations(st):
            if rel.get('inverse') != relation:
                continue
            known = True
            if rel.get('attributes_class'):
                link = self.versions[st['objects'][tid]['head']]['body']
                if self._resolve(st, link.get('target', '')) == object_id:
                    out.add(src)
            elif tid == object_id:
                out.add(src)
        if not known and not any((fd.get('relationship') or {}).get('inverse') == relation
                                 for c in self.classes.values() for fd in c['fields'].values()):
            raise BpaError('MODEL.REFERENCE_INVALID', 'unknown relation %s' % relation)
        return {'objects': sorted(out)}

    def class_at(self, branch, cls, time=None):
        st = self.commits[self._commit_at(branch, time)]['state']
        if cls not in self.classes and cls not in GENESIS:
            raise BpaError('MODEL.REFERENCE_INVALID', 'unknown class %s' % cls)
        return public(effective_class(list(self.decls.values()), cls, self._extensions(st)))

    # ---- reads (STD-TIME-012, -020) ----

    def read(self, branch, object_id, asof=None, time=None):
        st = self.commits[self._commit_at(branch, time)]['state']
        o = st['objects'].get(object_id)
        if o is None:
            raise BpaError('IAM.NOT_FOUND', 'unknown object')
        a = asof or self._default_valid(o['class'])
        seg = tl_at(o['timeline'], a)
        if seg is None:
            raise BpaError('QRY.NOT_FOUND_AT_TIME')
        v = self.versions[seg[2]]
        return {'version_hash': seg[2], 'body': v['body'], 'slug': v['slug'],
                'valid': {'from': seg[0], 'to': seg[1]}}

    def read_ledger(self, branch, cls, ledger_key, asof=None, time=None):
        st = self.commits[self._commit_at(branch, time)]['state']
        q = st['ledger'].get((cls, canonical(ledger_key)), [])
        out = []
        for e in q:
            if asof is not None and e['valid_at'] > asof:
                continue
            rec = {k: e[k] for k in ('entry_id', 'seq', 'valid_at', 'body', 'corrects', 'correction_type') if k in e}
            rec['entry_hash'] = e['entry_hash']
            out.append(rec)
        return {'entries': out}

    # ---- state tree (STD-PROOF-001) ----

    def state_entries(self, st):
        ent = [['o/' + oid, o['head']] for oid, o in st['objects'].items() if o['kind'] != 'system']
        for (cls, key), q in st['ledger'].items():
            ch = content_id('ubos.chunk.v1', cls).split(':')[1]
            kh = content_id('ubos.chunk.v1', q[0]['ledger_key']).split(':')[1] if q else ''
            for e in q:
                ent.append(['l/%s/%s/%020d' % (ch, kh, e['seq']), e['entry_hash']])
        return sorted(ent, key=lambda x: x[0].encode('utf-8'))

    def export_state(self, branch, time=None):
        st = self.commits[self._commit_at(branch, time)]['state']
        ent = self.state_entries(st)
        return {'root': tree_root(ent), 'entries': ent}

    # ---- merge (STD-VER-040…056, DEC-032) ----

    def _anc(self, cid):
        seen, stack = set(), [cid]
        while stack:
            c = stack.pop()
            if c not in seen:
                seen.add(c)
                stack.extend(self.commits[c]['parents'])
        return seen

    def _base_state(self, xs, ys):
        """Merge base of two sets of commits; a virtual base for several best ancestors."""
        ax = set().union(*(self._anc(x) for x in xs))
        ay = set().union(*(self._anc(y) for y in ys))
        common = ax & ay
        best = sorted(c for c in common if not any(c != d and c in self._anc(d) for d in common))
        acc, tips = self.commits[best[0]]['state'], {best[0]}
        for c in best[1:]:
            b = self._base_state(tips, {c})
            acc, _ = self._merge_states(b, acc, self.commits[c]['state'], virtual=True)
            tips.add(c)
        return acc

    def _merge_states(self, B, S, T, virtual=False, cseq=None, branch=None):
        conflicts, objects = [], {}
        for oid in sorted(set(B['objects']) | set(S['objects']) | set(T['objects'])):
            o, c = self._merge_object(oid, B['objects'].get(oid), S['objects'].get(oid), T['objects'].get(oid), virtual)
            conflicts += c
            if o is not None:
                objects[oid] = o
        ledger = copy.deepcopy(T['ledger'])
        for key, q in S['ledger'].items():
            tq = ledger.setdefault(key, [])
            for e in q:
                if any(x['entry_id'] == e['entry_id'] for x in tq):
                    continue
                for tgt in e.get('corrects', []):
                    if not any(x['entry_id'] == tgt for x in tq) and not virtual:
                        raise BpaError('LEDG.CORRECTION_TARGET_MISSING')
                ne = dict(e, seq=len(tq) + 1, source_seq={'branch': branch, 'seq': e['seq']})
                if cseq is not None:
                    ne['cseq'] = cseq
                tq.append(ne)
        return {'objects': objects, 'ledger': ledger}, conflicts

    @staticmethod
    def _same(a, b):
        return (a is None and b is None) or (a is not None and b is not None and
                                             a['timeline'] == b['timeline'] and a['head'] == b['head'])

    def _merge_object(self, oid, bo, so, to_, virtual):
        if self._same(so, to_) or self._same(so, bo):
            return copy.deepcopy(to_), []
        if self._same(to_, bo):
            return copy.deepcopy(so), []
        meta = to_ or so
        if meta['kind'] == 'system':
            return copy.deepcopy(to_), []
        tls = [x['timeline'] if x else [] for x in (bo, so, to_)]
        cuts = sorted({s[0] for tl in tls for s in tl} | {s[1] for tl in tls for s in tl if s[1] is not None})
        bounds = cuts + [None]
        pieces, conflicts = [], []
        for i in range(len(cuts)):
            a, b = bounds[i], bounds[i + 1]
            vb, vs, vt = (tl_at(tl, a) for tl in tls)
            vb, vs, vt = (x[2] if x else None for x in (vb, vs, vt))
            if vs == vt or vs == vb:
                r = vt
            elif vt == vb:
                r = vs
            elif vs is None or vt is None:
                conflicts.append({'object_id': oid, 'kind': 'ended_vs_changed'})
                r = vb
            elif meta['kind'] == 'raw_document':
                conflicts.append({'object_id': oid, 'kind': 'raw_document'})
                r = vb
            else:
                r, fc = self._merge_body(oid, vb, vs, vt, virtual)
                conflicts += fc
            pieces.append([a, b, r])
        if conflicts and not virtual:
            return None, conflicts
        timeline, created, head = [], [], None
        for a, b, r in pieces:
            if r is None:
                continue
            if timeline and timeline[-1][1] == a and timeline[-1][2] == r:
                timeline[-1][1] = b
            else:
                timeline.append([a, b, r])
        final = []
        for a, b, r in timeline:
            if isinstance(r, tuple):                      # a merged body: a new version
                body = dict(r)
                pred = tl_at(to_['timeline'], a) if to_ else None
                slug = self.versions[pred[2]]['slug'] if pred else meta['slug']
                vh = self._version(oid, meta['class'], slug, body, {'from': a, 'to': b},
                                   predecessor=pred[2] if pred else None)
                created.append(vh)
                final.append([a, b, vh])
            else:
                final.append([a, b, r])
        if created:
            head = created[-1]
        elif so and final == so['timeline']:
            head = so['head']
        else:
            head = to_['head'] if to_ else so['head']
        out = copy.deepcopy(meta)
        out['timeline'], out['head'] = final, head
        out['slug'] = self.versions[head]['slug']
        return out, conflicts

    def _merge_body(self, oid, vb, vs, vt, virtual):
        B = self.versions[vb]['body'] if vb else {}
        S, T = self.versions[vs]['body'], self.versions[vt]['body']
        absent = object()
        out, conflicts = {}, []
        for k in sorted(set(B) | set(S) | set(T)):
            b, s, t = B.get(k, absent), S.get(k, absent), T.get(k, absent)
            if s == t or s == b:
                r = t
            elif t == b:
                r = s
            else:
                conflicts.append({'object_id': oid, 'kind': 'field', 'path': '/' + k})
                r = b
            if r is not absent:
                out[k] = r
        return tuple(sorted(out.items(), key=lambda kv: kv[0])), conflicts

    def merge(self, source, target):
        s, t = self._branch(source)['head'], self._branch(target)['head']
        if s in self._anc(t):
            return {'cseq': self.commits[t]['cseq'], 'merged': False}
        B = self._base_state({t}, {s})
        cseq = self.commits[t]['cseq'] + 1
        st, conflicts = self._merge_states(B, self.commits[s]['state'], self.commits[t]['state'],
                                           cseq=cseq, branch=source)
        if conflicts:
            uniq = []
            for c in conflicts:
                if c not in uniq:
                    uniq.append(c)
            raise BpaError('VER.MERGE_CONFLICT', {'conflicts': uniq})
        cid = next(self.ids)
        self.commits[cid] = {'parents': [t, s], 'state': st, 'cseq': cseq}
        self.branches[target]['head'] = cid
        appended = {}
        for q in st['ledger'].values():
            for e in q:
                if e.get('cseq') == cseq and 'source_seq' in e:
                    appended[e['entry_id']] = {'seq': e['seq'], 'source_seq': e['source_seq']['seq']}
        return {'cseq': cseq, 'merged': True, 'appended': appended}

    # ---- diff (STD-VER-030) ----

    def _coord(self, c):
        return self.commits[self._commit_at(c['branch'], c.get('time'))]['state']

    def diff(self, a, b):
        A, B = self._coord(a), self._coord(b)
        out = []
        for oid in sorted(set(A['objects']) | set(B['objects'])):
            x, y = A['objects'].get(oid), B['objects'].get(oid)
            if x and y and x['head'] == y['head'] and x['timeline'] == y['timeline']:
                continue
            ent = {'object_id': oid, 'class': (y or x)['class'],
                   'versions': {'a': x['head'] if x else None, 'b': y['head'] if y else None}}
            if x is None:
                ent['op'] = 'added'
            elif self.versions[y['head']]['tombstone'] and not self.versions[x['head']]['tombstone']:
                ent['op'] = 'ended'
            else:
                vx, vy = self.versions[x['head']], self.versions[y['head']]
                fields = [{'path': '/' + k, 'old': vx['body'].get(k), 'new': vy['body'].get(k)}
                          for k in sorted(set(vx['body']) | set(vy['body'])) if vx['body'].get(k, ()) != vy['body'].get(k, ())]
                ent['op'] = 'moved' if vx['slug'] != vy['slug'] and not fields else 'changed'
                if fields:
                    ent['fields'] = fields
            out.append(ent)
        seen = {e['entry_id'] for q in A['ledger'].values() for e in q}
        for q in B['ledger'].values():
            for e in q:
                if e['entry_id'] not in seen:
                    out.append({'object_id': e['entry_id'], 'class': e['class'], 'op': 'appended',
                                'versions': {'a': None, 'b': e['entry_hash']}})
        return out
