"""Stateful adapter operations and commit checks of BPA 1.0-rc9 for money, accounts, statements
and projections (PH-2 step 3a, DEC-098): currency reference data (STD-FND-028), FX rates and
conversion (STD-FND-029), the chart of accounts (STD-KIND-029), statement lines (STD-KIND-030)
and projections (STD-TIME-021, STD-ISA-037). Mixed into the reference store."""
import copy
import re
from decimal import Decimal, localcontext

from . import BpaError
from . import values
from .canon import canonical, content_id
from .periods import derived_uuid
from .values import parse_decimal, fmt_decimal, MINOR_UNITS, TABLE_UNITS

CODE = re.compile(r'\A[A-Z]{3}\Z')
NUMERIC = re.compile(r'\A[0-9]{3}\Z')
AGG_FNS = ('count', 'sum', 'min', 'max', 'last')
NUMERIC_TYPES = ('decimal', 'integer', 'money')
STATEMENT_KEYS = ('source', 'account', 'statement', 'line')


def _prefix_le(a, b):
    """a <= b, comparing a date and an instant at the shorter of the two (STD-TIME-020)."""
    n = min(len(a), len(b))
    return a[:n] <= b[:n]


def _scale(s):
    return len(s.split('.')[1]) if '.' in s else 0


def tl_at(tl, at):
    from .store import tl_at as _tl_at
    return _tl_at(tl, at)


class Finance:

    def _fin_init(self):
        self.proj_lag = {}            # (branch, projection name) -> cseq (STD-TIME-021)
        values.ADDED.clear()

    # ---- helpers ----

    def _live_of(self, st, cls):
        """[(object id, object, head body)] of the live objects of a class."""
        out = []
        for oid, o in sorted(st['objects'].items()):
            if o['class'] != cls:
                continue
            v = self.versions[o['head']]
            if not v['tombstone']:
                out.append((oid, o, v['body']))
        return out

    def _at(self, o, date):
        """(version hash, body) of an object at a valid date, or None."""
        seg = tl_at(o['timeline'], date)
        if seg is None or self.versions[seg[2]]['tombstone']:
            return None
        return seg[2], self.versions[seg[2]]['body']

    # ---- currencies (STD-FND-028) ----

    def _added_currencies(self, st):
        return {b['code']: b['minor_units'] for _, _, b in self._live_of(st, 'Currency')
                if isinstance(b.get('code'), str) and isinstance(b.get('minor_units'), int)}

    def _use_currencies(self, st):
        values.ADDED.clear()
        values.ADDED.update(self._added_currencies(st))

    def currency(self, i):
        st = self._state(i.get('branch', 'main'), i)[0]
        code = i.get('code')
        if code in TABLE_UNITS:
            return {'code': code, 'minor_units': TABLE_UNITS[code], 'source': 'table'}
        for oid, _, b in self._live_of(st, 'Currency'):
            if b['code'] == code:
                return {'code': code, 'minor_units': b['minor_units'], 'source': oid}
        raise BpaError('MODEL.INVALID_VALUE', 'unknown ISO 4217 currency')

    # ---- FX rates (STD-FND-029) ----

    def _fx_rate(self, st, date, base, quote, rate_type, source=None):
        """(multiplier, object id, version hash, stored rate, inverse) of the rate converting
        `base` into `quote` at a valid date."""
        cands = []
        for oid, o in sorted(st['objects'].items()):
            if o['class'] != 'FxRate':
                continue
            at = self._at(o, date)
            if at is None:
                continue
            vh, b = at
            if b['rate_type'] != rate_type or (source is not None and b['source'] != source):
                continue
            if (b['base'], b['quote']) == (base, quote):
                cands.append((b['source'], False, oid, vh, b['rate']))
            elif (b['base'], b['quote']) == (quote, base):
                cands.append((b['source'], True, oid, vh, b['rate']))
        if not cands:
            raise BpaError('MODEL.CURRENCY_MISMATCH', {'base': base, 'quote': quote, 'rate_type': rate_type,
                                                       'date': date})
        if len({c[0] for c in cands}) > 1:
            raise BpaError('MODEL.INVALID_VALUE', 'several sources have the rate; name one')
        src, inverse, oid, vh, rate = next((c for c in cands if not c[1]), cands[0])
        r = parse_decimal(rate)
        if inverse:
            with localcontext() as c:
                c.prec = 200
                r = Decimal(1) / r
        return r, oid, vh, rate, inverse

    def fx_rate(self, i):
        st = self._state(i.get('branch', 'main'), i)[0]
        _, oid, vh, rate, inverse = self._fx_rate(st, i.get('date') or self._today(), i['base'], i['quote'],
                                                  i['rate_type'], i.get('source'))
        return {'object_id': oid, 'version_hash': vh, 'rate': rate, 'inverse': inverse}

    def _rate_object(self, st, ref, date):
        """The FxRate version a rate reference names at a date, or None (STD-FND-029)."""
        oid = self._resolve(st, ref) if isinstance(ref, str) and ref.startswith('ubos://') else ref
        o = st['objects'].get(oid) if isinstance(oid, str) else None
        if o is None or o['class'] != 'FxRate':
            return None
        at = self._at(o, date)
        if at is None:
            return None
        return oid, at[0], at[1]

    # ---- commit checks ----

    def _check_finance(self, st, appended):
        """STD-FND-028, -029, STD-KIND-029, -030, STD-TIME-021 (1.0-rc9): the declarations of
        currencies, rates, accounts and projections written by the commit, and the account and
        statement rules of the appended entries."""
        written = {}
        for oid, vh in self._written:
            v = self.versions[vh]
            if not v['tombstone'] and v['class'] in ('Currency', 'FxRate', 'LedgerAccount', 'Projection') \
                    and self._live(st, oid):
                written[oid] = v
        for oid, v in written.items():
            getattr(self, '_check_' + v['class'].lower())(st, oid, v['body'])
        for e in appended:
            spec = self._decl(e['class']).get('ledger') or {}
            if spec.get('account'):
                self._check_account_entry(st, e, spec['account'])
            if spec.get('statement') and not e.get('corrects'):
                eid = statement_id(spec['statement'], e['body'])
                if e['entry_id'] != eid:
                    raise BpaError('MODEL.INVALID_VALUE', {'entry_id': e['entry_id'], 'derived': eid})

    def _unique_key(self, st, cls, oid, key):
        for other, _, b in self._live_of(st, cls):
            if other != oid and key(b) == key(self.versions[st['objects'][oid]['head']]['body']):
                raise BpaError('MODEL.CONSTRAINT_VIOLATED', {'duplicate': cls, 'objects': sorted([oid, other])})

    def _check_currency(self, st, oid, b):
        code = b.get('code')
        if not isinstance(code, str) or not CODE.match(code) or code in TABLE_UNITS:
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', {'currency': code, 'reason': 'code'})
        if b.get('numeric') is not None and not NUMERIC.match(b['numeric']):
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', {'currency': code, 'reason': 'numeric'})
        if not 0 <= b['minor_units'] <= 4:
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', {'currency': code, 'reason': 'minor_units'})
        self._unique_key(st, 'Currency', oid, lambda x: x.get('code'))

    def _check_fxrate(self, st, oid, b):
        for c in (b['base'], b['quote']):
            if c not in MINOR_UNITS:
                raise BpaError('MODEL.INVALID_VALUE', 'unknown ISO 4217 currency')
        if b['base'] == b['quote'] or parse_decimal(b['rate']) <= 0:
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', {'rate': oid, 'reason': 'base' if b['base'] == b['quote']
                                                         else 'rate'})
        self._unique_key(st, 'FxRate', oid, lambda x: (x.get('source'), x.get('base'), x.get('quote'), x.get('rate_type')))

    def _check_ledgeraccount(self, st, oid, b):
        if b.get('currency') is not None and b['currency'] not in MINOR_UNITS:
            raise BpaError('MODEL.INVALID_VALUE', 'unknown ISO 4217 currency')
        seen, cur = {oid}, b
        while cur.get('parent'):
            p = self._resolve(st, cur['parent'])
            if p is None:
                return                                  # an unresolved reference is reported by the integrity check
            if p in seen:
                raise BpaError('MODEL.CONSTRAINT_VIOLATED', {'account': self._uri(st['objects'][oid]),
                                                             'reason': 'cycle'})
            seen.add(p)
            cur = self.versions[st['objects'][p]['head']]['body']

    def _check_account_entry(self, st, e, field):
        uri = e['ledger_key'].get(field, e['body'].get(field))
        if uri is None:
            return
        oid = self._resolve(st, uri)
        if oid is None or st['objects'][oid]['class'] != 'LedgerAccount':
            return                                      # reported by the reference checks
        at = self._at(st['objects'][oid], e['valid_at'])
        if at is None:
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', {'account': uri, 'reason': 'inactive'})
        for other, o, _ in self._live_of(st, 'LedgerAccount'):
            x = self._at(o, e['valid_at'])
            if x is not None and x[1].get('parent') and self._resolve(st, x[1]['parent']) == oid:
                raise BpaError('MODEL.CONSTRAINT_VIOLATED', {'account': uri, 'reason': 'not_leaf'})
        cur = at[1].get('currency')
        if cur is not None:
            for a in self._amounts(e['class']):
                v = e['body'].get(a)
                if isinstance(v, dict) and v.get('currency') != cur:
                    raise BpaError('MODEL.CONSTRAINT_VIOLATED', {'account': uri, 'reason': 'currency'})

    def _check_projection(self, st, oid, b):
        src = b['source']
        if src not in self.classes or self._cls(src)['kind'] != 'ledger':
            raise BpaError('MODEL.INVALID_VALUE', 'the source of a projection is a ledger class')
        fields = self._cls(src)['fields']
        gb = b.get('group_by') or []
        if not isinstance(gb, list) or any(not isinstance(f, str) or f not in fields for f in gb):
            raise BpaError('MODEL.INVALID_VALUE', 'group_by names fields of the source class')
        aggs = b['aggregates']
        if not isinstance(aggs, list) or not aggs:
            raise BpaError('MODEL.INVALID_VALUE', 'a projection has aggregates')
        names = []
        for a in aggs:
            if not isinstance(a, dict) or a.get('fn') not in AGG_FNS or not isinstance(a.get('as'), str):
                raise BpaError('MODEL.INVALID_VALUE', 'an aggregate is {fn, field, as}')
            if a['fn'] != 'count':
                f = fields.get(a.get('field'))
                if f is None or (a['fn'] != 'last' and f['type'] not in NUMERIC_TYPES):
                    raise BpaError('MODEL.INVALID_VALUE', 'the field of %s' % a['fn'])
            names.append(a['as'])
        if len(set(names)) != len(names):
            raise BpaError('MODEL.INVALID_VALUE', 'aggregate names are unique')
        self._unique_key(st, 'Projection', oid, lambda x: x.get('name'))

    def _check_ledger_key(self, cls, lk):
        """The validated ledger key of an entry of a ledger class (STD-KIND-020)."""
        spec = self._decl(cls)['ledger']
        if not isinstance(lk, dict) or sorted(lk) != sorted(spec['key']):
            raise BpaError('MODEL.INVALID_VALUE', 'ledger key fields')
        eff = self._cls(cls)
        return {k: values.validate_value(eff['fields'][k]['type'], eff['fields'][k].get('constraints'), lk[k])
                for k in spec['key']}

    # ---- statement lines (STD-KIND-030) ----

    def ingest_statement(self, i):
        """STD-KIND-030: append the new lines of a statement in one commit; repeated lines are
        duplicates, and a changed line fails the ingestion."""
        branch = i.get('branch', 'main')
        cls = i['class']
        spec = (self._decl(cls).get('ledger') or {}).get('statement')
        if not spec:
            raise BpaError('MODEL.INVALID_VALUE', 'the class declares no statement lines')
        st = self._state(branch)[0]
        known = {e['entry_id']: e['body'] for (c, _), q in st['ledger'].items() if c == cls for e in q}
        appended, dups, changes, seen = [], [], [], {}
        for line in i.get('lines') or []:
            body = self._check_body(cls, copy.deepcopy(line['body']))
            eid = statement_id(spec, body)
            prior = known.get(eid, seen.get(eid))
            if prior is not None:
                if content_id('ubos.chunk.v1', prior) != content_id('ubos.chunk.v1', body):
                    raise BpaError('TXN.IDEMPOTENCY_MISMATCH', {'entry_id': eid})
                dups.append(eid)
                continue
            seen[eid] = body
            ch = {'op': 'append', 'class': cls, 'entry_id': eid, 'ledger_key': line['ledger_key'],
                  'body': line['body']}
            if line.get('valid_at') is not None:
                ch['valid_at'] = line['valid_at']
            changes.append(ch)
            appended.append(eid)
        out = {'appended': appended, 'duplicates': dups}
        if changes:
            res = self.commit(branch, changes, i.get('process'))
            out['cseq'], out['commit_id'] = res['cseq'], res['commit_id']
        return out

    # ---- projections (STD-TIME-021, STD-ISA-037) ----

    def _projection(self, st, name):
        for oid, _, b in self._live_of(st, 'Projection'):
            if b['name'] == name:
                return oid, b
        raise BpaError('IAM.NOT_FOUND', 'unknown projection')

    def _watermark(self, branch, cid, decl):
        """(cseq, commit id) up to which a projection is maintained at a snapshot."""
        c = cid
        lag = self.proj_lag.get((branch, decl['name'])) if decl.get('mode') == 'async' else None
        if lag is not None:
            for c in self._first_parents(cid):
                if self.commits[c]['cseq'] <= lag:
                    break
        return self.commits[c]['cseq'], self.commits[c]['commit_id']

    def _fold(self, st, decl, wm, asof):
        """{canonical key: [key, entries, accumulators]} of the entries of the source class
        included at (asof, watermark)."""
        src = decl['source']
        fields = self._cls(src)['fields']
        entries = [e for (c, _), q in st['ledger'].items() if c == src for e in q
                   if e['cseq'] <= wm and (asof is None or _prefix_le(e['valid_at'], asof))]
        entries.sort(key=lambda e: (e['valid_at'], e['cseq'], e['seq']))
        groups = {}
        for e in entries:
            key = {f: e['ledger_key'][f] if f in e['ledger_key'] else e['body'].get(f)
                   for f in decl.get('group_by') or []}
            g = groups.setdefault(canonical(key), [key, 0, [_acc(a, fields) for a in decl['aggregates']]])
            g[1] += 1
            for acc, a in zip(g[2], decl['aggregates']):
                _add(acc, a, e['body'].get(a.get('field')) if a['fn'] != 'count' else None)
        return groups

    def _rollup(self, st, decl, groups, date):
        af = (self._decl(decl['source']).get('ledger') or {}).get('account')
        if not af or af not in (decl.get('group_by') or []) \
                or any(a['fn'] not in ('count', 'sum') for a in decl['aggregates']):
            raise BpaError('QRY.INVALID_QUERY', 'rollup needs the account field and count or sum aggregates')
        out = {k: [g[0], g[1], [copy.deepcopy(a) for a in g[2]]] for k, g in groups.items()}
        for key, n, accs in list(groups.values()):
            seen = set()
            uri = key[af]
            while True:
                oid = self._resolve(st, uri) if isinstance(uri, str) else None
                at = self._at(st['objects'][oid], date) if oid else None
                parent = at[1].get('parent') if at else None
                if not parent or parent in seen:
                    break
                seen.add(parent)
                pkey = dict(key, **{af: parent})
                g = out.setdefault(canonical(pkey), [pkey, 0, [_acc(a, None) for a in decl['aggregates']]])
                g[1] += n
                for acc, src in zip(g[2], accs):
                    _merge(acc, src)
                uri = parent
        return out

    def _read_projection(self, branch, i):
        st, _, _, cid = self._state(branch, i)
        _, decl = self._projection(st, i.get('projection'))
        wm, wid = self._watermark(branch, cid, decl)
        at_least = i.get('at_least')
        if at_least is not None and wm < at_least:
            raise BpaError('QRY.WATERMARK_TIMEOUT', {'watermark': wm, 'at_least': at_least})
        groups = self._fold(st, decl, wm, i.get('asof'))
        if i.get('rollup'):
            groups = self._rollup(st, decl, groups, i.get('asof') or self._today())
        want = i.get('group') or {}
        if any(f not in (decl.get('group_by') or []) for f in want):
            raise BpaError('QRY.INVALID_QUERY', 'group names fields of group_by')
        out = []
        for k in sorted(groups):
            key, n, accs = groups[k]
            if all(key.get(f) == v for f, v in want.items()):
                out.append({'key': key, 'entries': n,
                            'values': {a['as']: _value(acc) for a, acc in zip(decl['aggregates'], accs)}})
        return {'projection': decl['name'], 'groups': out, 'watermark': {'cseq': wm, 'commit_id': wid}}

    def read_projection(self, i):
        return self._read_projection(i.get('branch', 'main'), i)

    def projection_lag(self, i):
        branch = i.get('branch', 'main')
        st = self._state(branch)[0]
        _, decl = self._projection(st, i.get('projection'))
        if decl.get('mode') != 'async':
            raise BpaError('MODEL.INVALID_VALUE', 'only an async projection lags')
        if i.get('cseq') is None:
            self.proj_lag.pop((branch, decl['name']), None)
        else:
            self.proj_lag[(branch, decl['name'])] = i['cseq']
        return {'projection': decl['name'], 'cseq': i.get('cseq')}

    def rebuild_projection(self, i):
        """STD-TIME-021: a rebuild from the committed entries switches to the fold at the head;
        an async projection is then maintained up to the head."""
        branch = i.get('branch', 'main')
        st = self._state(branch)[0]
        _, decl = self._projection(st, i.get('projection'))
        head = self.commits[self._branch(branch)['head']]['cseq']
        if (branch, decl['name']) in self.proj_lag:
            self.proj_lag[(branch, decl['name'])] = head
        r = self._read_projection(branch, {'projection': decl['name']})
        return {'projection': decl['name'], 'groups': len(r['groups']), 'watermark': r['watermark']}


def statement_id(spec, body):
    """STD-KIND-030: the entry id derived from the source, account, statement and line."""
    return derived_uuid('ubos.statement.v1', [body.get(spec[k]) for k in STATEMENT_KEYS])


# ---- accumulators of the projection aggregates ----

def _acc(a, fields):
    t = (fields[a['field']]['type'] if fields and a.get('field') in fields else None)
    return {'fn': a['fn'], 'type': t, 'n': 0, 'cur': {}, 'scale': 0, 'value': None}


def _add(acc, a, v):
    acc['n'] += 1
    fn = acc['fn']
    if fn == 'count' or v is None:
        return
    if fn == 'last':
        acc['value'] = v
        return
    if isinstance(v, dict):
        cur, amt = v['currency'], parse_decimal(v['amount'])
    elif isinstance(v, int) and not isinstance(v, bool):
        cur, amt = None, Decimal(v)
    else:
        cur, amt = '', parse_decimal(v)
        acc['scale'] = max(acc['scale'], _scale(v))
    acc['int'] = cur is None
    k = cur or ''
    if fn == 'sum':
        acc['cur'][k] = acc['cur'].get(k, Decimal(0)) + amt
    else:
        old = acc['cur'].get(k)
        if old is None or (amt < old if fn == 'min' else amt > old):
            acc['cur'][k] = amt
    acc['money'] = acc.get('money') or isinstance(v, dict)


def _merge(acc, src):
    acc['n'] += src['n']
    acc['scale'] = max(acc['scale'], src['scale'])
    for k in ('int', 'money'):
        if src.get(k):
            acc[k] = True
    for k, v in src['cur'].items():
        acc['cur'][k] = acc['cur'].get(k, Decimal(0)) + v


def _value(acc):
    fn = acc['fn']
    if fn == 'count':
        return acc['n']
    if fn == 'last':
        return acc['value']
    if not acc['cur']:
        return None
    if acc.get('money'):
        return [{'amount': fmt_decimal(acc['cur'][c], MINOR_UNITS[c]), 'currency': c} for c in sorted(acc['cur'])]
    v = acc['cur']['']
    if acc.get('int'):
        return int(v)
    return fmt_decimal(v, acc['scale'])
