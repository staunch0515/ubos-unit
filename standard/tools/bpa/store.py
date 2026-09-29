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
from .classes import effective_class
from .tree import tree_root
from .values import GRAMMARS, check_name, validate_value, parse_decimal, fmt_decimal, MINOR_UNITS

DEFAULT_NOW = '2026-01-01T00:00:00.000000Z'


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

    def _cls(self, name):
        if name not in self.classes:
            raise BpaError('MODEL.REFERENCE_INVALID', 'unknown class %s' % name)
        return self.classes[name]

    def _today(self):
        return self.now[:10]

    def _default_valid(self, cls):
        return self._today() if self.decls[cls].get('valid_time', 'date') == 'date' else self.now

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
        return out

    def _version(self, oid, cls, slug, body, valid, predecessor=None, tombstone=False):
        env = {'object_id': oid, 'class': cls, 'class_version': content_id('ubos.chunk.v1', self.decls[cls]),
               'schema_version': self.decls[cls]['schema_version'], 'kind': self.classes[cls]['kind'],
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

    def commit(self, branch, changes, process=None):
        br = self._branch(branch)
        head = self.commits[br['head']]
        st = copy.deepcopy(head['state'])
        cseq = head['cseq'] + 1
        result = {'cseq': cseq, 'versions': {}, 'entries': {}}
        if process and process.get('serializable'):
            for oid, vh in process.get('reads', {}).items():
                if st['objects'].get(oid, {}).get('head') != vh:
                    raise BpaError('TXN.READ_SET_CHANGED')
        appended = []
        stale = []
        for ch in changes:
            op = ch.get('op')
            if op in ('update', 'end') and ch.get('base') is not None:
                o = st['objects'].get(ch.get('object_id'))
                if o is not None and o['head'] != ch['base']:
                    stale.append(ch['object_id'])
        if stale:
            raise BpaError('TXN.HEAD_MOVED', sorted(stale))
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
            o['timeline'] = tl_apply(o['timeline'], kind, vh, a, b)
            o['slug'] = slug
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
