"""Stateful adapter operations of BPA 1.0-rc8 for money and periods (PH-2 step 1b, DEC-090):
period transitions and close artefacts (STD-KIND-025, -026), matching and match records
(STD-KIND-027, -028) and restatement on a correction branch (STD-TIME-041, -042). Mixed into
the reference store."""
import copy
import hashlib

from . import BpaError
from .canon import canonical, content_id
from .values import parse_decimal, fmt_decimal, MINOR_UNITS, check_name

STATES = ('open', 'closing', 'closed')
# STD-KIND-025 (1.0-rc8): the transitions an update may make; `closed` is entered by
# close_period and left by reopen_period only.
UPDATE_TRANSITIONS = {('open', 'closing'), ('closing', 'open')}
RULE_KINDS = ('one_to_one', 'one_to_many', 'many_to_one')
MATCH_CLASS = 'MatchRecord'
CLOSE_CLASS = 'CloseArtefact'


def derived_uuid(*parts):
    """A UUID (version 8) derived from content (STD-KIND-028): the identifier of a match
    record is fixed by what it records."""
    h = hashlib.sha256(canonical(list(parts)).encode('utf-8')).hexdigest()
    return '%s-%s-8%s-%x%s-%s' % (h[:8], h[8:12], h[13:16], 8 | (int(h[16], 16) & 3), h[17:20], h[20:32])


def _add(acc, v):
    if isinstance(v, dict):
        cur = acc.setdefault(v['currency'], 0)
        acc[v['currency']] = cur + parse_decimal(v['amount'])
    else:
        acc[''] = acc.get('', 0) + parse_decimal(v)


def _total(acc):
    if list(acc) == ['']:
        return fmt_decimal(acc[''])
    return [{'amount': fmt_decimal(acc[c], MINOR_UNITS[c]), 'currency': c} for c in sorted(acc)]


def _amount(v):
    """(currency, decimal) of a decimal or money value."""
    if isinstance(v, dict):
        return v['currency'], parse_decimal(v['amount'])
    return '', parse_decimal(v)


def _days(a, b):
    import datetime
    d = lambda s: datetime.date.fromisoformat(s[:10])
    return abs((d(a) - d(b)).days)


class Periods:

    # ---- periods (STD-KIND-025) ----

    def _period_classes(self):
        """{period class: [ledger classes it scopes]}."""
        out = {}
        for n, d in self.decls.items():
            scope = (d.get('ledger') or {}).get('period_scope')
            if scope:
                out.setdefault(scope, []).append(n)
        return out

    def _check_period_transition(self, cls, old, new):
        """STD-KIND-025 (1.0-rc8): an update of a period object moves between `open` and
        `closing` only; `closed` is entered and left by the period operations."""
        if old == new or cls not in self._period_classes():
            return
        if (old, new) in UPDATE_TRANSITIONS:
            return
        if getattr(self, '_period_op', None) == (old, new):
            return
        raise BpaError('MODEL.CONSTRAINT_VIOLATED', {'period_transition': [old, new]})

    def _period(self, st, oid):
        o = st['objects'].get(oid)
        if o is None:
            raise BpaError('IAM.NOT_FOUND', 'unknown period')
        if o['class'] not in self._period_classes():
            raise BpaError('MODEL.INVALID_VALUE', 'not an object of a period class')
        v = self.versions[o['head']]
        if v['tombstone']:
            raise BpaError('IAM.NOT_FOUND', 'the period is ended')
        return o, v['body']

    def _posting_dates(self, branch, cid):
        """{cseq: posting date} of the commits of a branch's lineage (STD-KIND-025: the date
        of the commit's transaction time)."""
        out = {}
        for x in self._anc(cid):
            c = self.commits[x]
            if c.get('tx_time'):
                out.setdefault(c['cseq'], c['tx_time'][:10])
        return out

    def _balances(self, branch, cid, st, classes, end):
        """The balances per ledger key of the entries of `classes` with a posting date before
        `end`, as of commit `cid` (STD-KIND-026)."""
        dates = self._posting_dates(branch, cid)
        out = []
        for (cls, key), q in sorted(st['ledger'].items()):
            if cls not in classes:
                continue
            sums = {a: {} for a in self._amounts(cls)}
            n = 0
            for e in q:
                if dates.get(e['cseq'], '9999-12-31') >= end:
                    continue
                n += 1
                for a in sums:
                    _add(sums[a], e['body'][a])
            if n:
                out.append({'class': cls, 'ledger_key': q[0]['ledger_key'], 'entries': n,
                            'amounts': {a: _total(s) for a, s in sums.items() if s}})
        return out

    def close_period(self, i):
        """STD-KIND-026: close a period in `closing` — the period becomes `closed`, a close
        artefact records the balances at close, and a signed tag names the closing commit."""
        branch = i.get('branch', 'main')
        self._check_active(branch)
        st = self._state(branch)[0]
        o, body = self._period(st, i['period'])
        if body.get('state') != 'closing':
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', {'period_transition': [body.get('state'), 'closed']})
        aid = i.get('artefact_id')
        if not isinstance(aid, str):
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'artefact_id')
        n = 1 + sum(1 for x in st['objects'].values() if x['class'] == CLOSE_CLASS
                    and self.versions[x['head']]['body'].get('period') == i['period'])
        head = self._branch(branch)['head']
        classes = self._period_classes()[o['class']]
        tag = 'close-%s-%d' % (o['slug'], n)
        check_name('slug', tag)
        if tag in self.tags:
            raise BpaError('VER.TAG_IMMUTABLE', 'tag names are never reused')
        art = {'period': i['period'], 'period_slug': o['slug'], 'sequence': n, 'start': body['start'],
               'end': body['end'], 'tag': tag,
               'as_of': {'cseq': self.commits[head]['cseq'], 'commit_id': self.commits[head]['commit_id'],
                         'root': self.commits[head]['record']['root']},
               'balances': self._balances(branch, head, st, classes, body['end'])}
        changes = [{'op': 'update', 'object_id': i['period'], 'base': o['head'], 'body': {'state': 'closed'}},
                   {'op': 'create', 'object_id': aid, 'class': CLOSE_CLASS, 'slug': '%s-close-%d' % (o['slug'], n),
                    'body': art}]
        self._period_op = ('closing', 'closed')
        try:
            res = self.commit(branch, changes, i.get('process'))
        finally:
            self._period_op = None
        self.tag(tag, branch, None, i.get('signer'))
        return {'cseq': res['cseq'], 'commit_id': res['commit_id'], 'artefact_id': aid, 'tag': tag,
                'sequence': n, 'balances': art['balances']}

    def reopen_period(self, i):
        """STD-KIND-025 (1.0-rc8): a closed period is reopened with a reason; its close
        artefacts stay."""
        branch = i.get('branch', 'main')
        reason = i.get('reason')
        if not isinstance(reason, str) or not reason:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a reopening states its reason')
        st = self._state(branch)[0]
        o, body = self._period(st, i['period'])
        if body.get('state') != 'closed':
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', {'period_transition': [body.get('state'), 'open']})
        process = dict(i.get('process') or {}, reason=reason)
        change = {'op': 'update', 'object_id': i['period'], 'base': o['head'], 'body': {'state': 'open'}}
        pend = self._governed_reopen(st, branch, i, o, reason, process, change)
        if pend is not None:
            return pend
        self._period_op = ('closed', 'open')
        try:
            res = self.commit(branch, [change], process)
        finally:
            self._period_op = None
        return {'cseq': res['cseq'], 'commit_id': res['commit_id'], 'state': 'open', 'reason': reason}

    def _governed_reopen(self, st, branch, i, o, reason, process, change):
        """STD-KIND-025 (1.0-rc10): with a command port `reopen` on the period's class, its
        decisions govern the reopening; `require` with `defer` stores a pending change
        (STD-TXN-060). None when the reopening goes ahead."""
        port = (self._cls(o['class']).get('_ports') or self._cls(o['class']).get('ports') or {}).get('reopen')
        if port is None or port.get('kind') == 'query':
            return None
        r = self._decide_in(st, branch, i['period'], 'reopen', {'reason': reason}, i.get('ctx'))
        if r['outcome'] == 'deny':
            raise BpaError('RULE.DECISION_DENIED', r)
        if r['outcome'] != 'require':
            return None
        if not i.get('defer'):
            raise BpaError('RULE.DECISION_REQUIRES', r)
        pid = process.get('process_id')
        if not isinstance(pid, str) or not isinstance(process.get('initiator'), str):
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a deferred reopening names its process and initiator')
        from .approvals import requirements, _plus
        from .canon import content_id
        if not hasattr(self, 'pending_changes'):
            self.pending_changes = {}
        reqs = requirements([(i['period'], 'reopen', r)])
        rec = {'pending_change_id': pid, 'target_branch': branch, 'changes': [change],
               'preview': content_id('ubos.chunk.v1', {'branch': branch, 'changes': [change]}),
               'requirements': reqs, 'approvals': [], 'state': 'pending', 'initiator': process['initiator'],
               'process_id': pid, 'process': {'reason': reason}, 'period_op': ('closed', 'open')}
        exp = [q['expires_in'] for q in reqs if q.get('expires_in')]
        if exp:
            rec['expires_at'] = min(_plus(self.now, e) for e in exp)
        self.pending_changes[pid] = rec
        return {'state': 'closed', 'reason': reason, 'pending': self._pc_view(rec)}

    # ---- matching (STD-KIND-027, -028) ----

    def _check_rule(self, r):
        if not isinstance(r, dict) or not isinstance(r.get('id'), str) or r.get('kind') not in RULE_KINDS:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a matching rule has an id and a kind')
        amt = r.get('amount')
        if not (isinstance(amt, list) and len(amt) == 2 and all(isinstance(x, str) for x in amt)):
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a matching rule names the amount field of each side')
        for pair in r.get('equal') or []:
            if not (isinstance(pair, list) and len(pair) == 2 and all(isinstance(x, str) for x in pair)):
                raise BpaError('MODEL.INVALID_VALUE', 'equal is a list of [left field, right field]')
        tol = parse_decimal(r.get('tolerance', '0'))
        if tol < 0:
            raise BpaError('MODEL.INVALID_VALUE', 'a tolerance is not negative')
        w = r.get('window_days', 0)
        if not isinstance(w, int) or isinstance(w, bool) or w < 0:
            raise BpaError('MODEL.INVALID_VALUE', 'window_days is a non-negative integer')

    def _side(self, st, side):
        if not isinstance(side, dict) or not isinstance(side.get('class'), str):
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a side names a ledger class')
        cls = side['class']
        if self._cls(cls)['kind'] != 'ledger':
            raise BpaError('MODEL.INVALID_VALUE', 'a side is a ledger class')
        key = canonical(side['ledger_key']) if side.get('ledger_key') is not None else None
        out = []
        for (c, k), q in st['ledger'].items():
            if c == cls and (key is None or k == key):
                out += q
        return sorted(out, key=lambda e: (e['valid_at'], e['entry_id']))

    def _active_matches(self, st, match_set):
        """The match records of a set that no unmatch supersedes (STD-KIND-028)."""
        q = st['ledger'].get((MATCH_CLASS, canonical({'match_set': match_set})), [])
        undone = {e['body']['unmatches'] for e in q if e['body'].get('unmatches')}
        return [e for e in q if not e['body'].get('unmatches') and e['entry_id'] not in undone], q

    def _run_rules(self, rules, left, right, used_l, used_r):
        matches = []

        def eq(l, r, rule):
            return all(l['body'].get(a) == r['body'].get(b) for a, b in rule.get('equal') or [])

        def near(l, r, rule):
            return _days(l['valid_at'], r['valid_at']) <= rule.get('window_days', 0)

        def within(ls, rs, rule):
            a, b = rule['amount']
            tl, tr = {}, {}
            for e in ls:
                _add(tl, e['body'][a])
            for e in rs:
                _add(tr, e['body'][b])
            if rule.get('negate'):
                tr = {c: -v for c, v in tr.items()}
            if set(tl) != set(tr):
                return False
            tol = parse_decimal(rule.get('tolerance', '0'))
            return all(abs(tl[c] - tr[c]) <= tol for c in tl)

        for rule in rules:
            ref = content_id('ubos.chunk.v1', rule)
            if rule['kind'] == 'one_to_one':
                for l in left:
                    if l['entry_id'] in used_l:
                        continue
                    for r in right:
                        if r['entry_id'] in used_r or not eq(l, r, rule) or not near(l, r, rule):
                            continue
                        if within([l], [r], rule):
                            used_l.add(l['entry_id'])
                            used_r.add(r['entry_id'])
                            matches.append({'rule': rule['id'], 'rule_ref': ref, 'left': [l['entry_id']],
                                            'right': [r['entry_id']]})
                            break
            else:
                one, many, used_one, used_many = (left, right, used_l, used_r) if rule['kind'] == 'one_to_many' \
                    else (right, left, used_r, used_l)
                for x in one:
                    if x['entry_id'] in used_one:
                        continue
                    if rule['kind'] == 'one_to_many':
                        group = [y for y in many if y['entry_id'] not in used_many and eq(x, y, rule) and near(x, y, rule)]
                        ok = group and within([x], group, rule)
                    else:
                        group = [y for y in many if y['entry_id'] not in used_many and eq(y, x, rule) and near(y, x, rule)]
                        ok = group and within(group, [x], rule)
                    if not ok:
                        continue
                    used_one.add(x['entry_id'])
                    used_many.update(y['entry_id'] for y in group)
                    ids = sorted(y['entry_id'] for y in group)
                    m = {'rule': rule['id'], 'rule_ref': ref}
                    m['left'], m['right'] = ([x['entry_id']], ids) if rule['kind'] == 'one_to_many' else (ids, [x['entry_id']])
                    matches.append(m)
        return matches

    def match(self, i):
        """STD-KIND-027: run matching rules in priority order over the unmatched entries of
        two sides; with `commit`, append one match record per match (STD-KIND-028)."""
        branch = i.get('branch', 'main')
        match_set = i.get('match_set')
        if not isinstance(match_set, str) or not match_set:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'match_set')
        rules = i.get('rules')
        if not isinstance(rules, list) or not rules:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'rules')
        for r in rules:
            self._check_rule(r)
        if len({r['id'] for r in rules}) != len(rules):
            raise BpaError('MODEL.INVALID_VALUE', 'rule ids are unique')
        st = self._state(branch)[0]
        left, right = self._side(st, i.get('left')), self._side(st, i.get('right'))
        active, _ = self._active_matches(st, match_set)
        used_l = {x for e in active for x in e['body']['left']}
        used_r = {x for e in active for x in e['body']['right']}
        matches = self._run_rules(rules, left, right, used_l, used_r)
        out = {'matches': matches,
               'breaks': {'left': [e['entry_id'] for e in left if e['entry_id'] not in used_l],
                          'right': [e['entry_id'] for e in right if e['entry_id'] not in used_r]}}
        if i.get('commit') and matches:
            actor = (i.get('process') or {}).get('initiator') or 'did:ubos:acme:01900000-0000-7000-8000-000000000002'
            changes = []
            for m in matches:
                body = {'match_set': match_set, 'left': m['left'], 'right': m['right'], 'rule': m['rule'],
                        'rule_ref': m['rule_ref'], 'actor': actor}
                eid = derived_uuid('ubos.match.v1', body)
                m['entry_id'] = eid
                changes.append({'op': 'append', 'class': MATCH_CLASS, 'entry_id': eid,
                                'ledger_key': {'match_set': match_set}, 'body': body})
            self._match_op = True
            try:
                res = self.commit(branch, changes, i.get('process'))
            finally:
                self._match_op = False
            out['cseq'] = res['cseq']
        return out

    def unmatch(self, i):
        """STD-KIND-028: undo a match by appending a record that supersedes it."""
        branch = i.get('branch', 'main')
        st = self._state(branch)[0]
        match_set = i.get('match_set')
        active, q = self._active_matches(st, match_set)
        target = next((e for e in q if e['entry_id'] == i.get('entry_id')), None)
        if target is None:
            raise BpaError('IAM.NOT_FOUND', 'unknown match record')
        if target not in active:
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'the record is an unmatch or is already undone')
        actor = (i.get('process') or {}).get('initiator') or 'did:ubos:acme:01900000-0000-7000-8000-000000000002'
        body = {'match_set': match_set, 'left': target['body']['left'], 'right': target['body']['right'],
                'rule': target['body'].get('rule'), 'rule_ref': target['body'].get('rule_ref'), 'actor': actor,
                'unmatches': target['entry_id']}
        if i.get('reason') is not None:
            body['reason'] = i['reason']
        eid = derived_uuid('ubos.match.v1', body)
        self._match_op = True
        try:
            res = self.commit(branch, [{'op': 'append', 'class': MATCH_CLASS, 'entry_id': eid,
                                        'ledger_key': {'match_set': match_set}, 'body': body}], i.get('process'))
        finally:
            self._match_op = False
        return {'cseq': res['cseq'], 'entry_id': eid, 'unmatches': target['entry_id']}

    # ---- restatement (STD-TIME-041, -042) ----

    def restate(self, i):
        """STD-TIME-042: re-run the processes that produced the impact set of a correcting
        commit on a correction branch, with the logic of the chosen mode (STD-TIME-041), and
        report old and new results."""
        branch = i.get('branch', 'main')
        name = i.get('name')
        mode = i.get('mode', 'as_effective')
        if mode not in ('as_originally', 'as_effective', 'current'):
            raise BpaError('MODEL.INVALID_VALUE', 'mode is as_originally, as_effective or current')
        if not isinstance(name, str) or name.split('/')[0] != 'correction':
            raise BpaError('MODEL.INVALID_VALUE', 'a restatement runs on a correction branch')
        policy = None
        if i.get('proposals') is not None:                 # STD-TIME-043 (1.0-rc9)
            req = i['proposals'] if isinstance(i['proposals'], dict) else {}
            policy = req.get('date_policy', 'open_period')
            if policy not in ('open_period', 'original'):
                raise BpaError('MODEL.INVALID_VALUE', 'date_policy is open_period or original')
        imp = self.impact(branch, i['time'])
        pids = []
        for v in imp['impacted']:
            rid = self.versions[v['version_hash']].get('provenance_ref')
            if rid:
                pids.append(self.derivations[rid]['process_id'])
        st0 = self._state(branch)[0]
        for k, q in st0['ledger'].items():
            for e in q:
                if e['entry_id'] in imp['entries'] and e.get('provenance_ref'):
                    pids.append(self.derivations[e['provenance_ref']]['process_id'])
        order = []
        for p in pids:
            if p not in order:
                order.append(p)
        rerun = [p for p in order if p in self.invocations]
        manual = [p for p in order if p not in self.invocations]
        self.create_branch(name, branch)
        processes = []
        postings = []
        for pid in rerun:
            inv = copy.deepcopy(self.invocations[pid])
            inv['branch'] = name
            j = self.journals[pid]
            d = (inv.get('ctx') or {}).get('asof') or j['context'].get('asof') or j['context']['now']
            self._logic_mode = {'mode': mode, 'date': d[:10], 'original': list(j['logic'])}
            self._restating = []
            try:
                out = self.execute(inv)
            finally:
                self._logic_mode = None
                got, self._restating = self._restating, None
            postings += [dict({'process_id': pid}, **x) for x in got]
            processes.append({'process_id': pid, 'cseq': out.get('cseq')})
        # old and new results of the objects the restatement wrote
        new_st = self._state(name)[0]
        changes = []
        for oid in sorted(new_st['objects']):
            o = new_st['objects'][oid]
            old = st0['objects'].get(oid)
            if old is not None and old['head'] == o['head']:
                continue
            ob = self.versions[old['head']]['body'] if old else None
            nb = self.versions[o['head']]['body']
            c = {'object_id': oid, 'class': o['class'], 'old': ob, 'new': nb}
            if i.get('materiality') is not None and ob is not None:
                from .expr import eval_expr
                c['material'] = bool(eval_expr({'expr': i['materiality'], 'classes': list(self.decls.values()),
                                                'class': o['class'], 'this': nb, 'old': ob})['value'])
            changes.append(c)
        out = {'branch': name, 'mode': mode, 'processes': processes, 'not_rerun': manual, 'changes': changes}
        if policy is not None:
            out['proposals'] = self._proposals(branch, st0, rerun, postings, policy)
            out['postings'] = postings
        return out

    def _proposals(self, branch, st0, pids, postings, policy):
        """STD-TIME-043 (1.0-rc9): per ledger class, key and currency, the restated postings
        minus the entries the original processes appended."""
        ids = set()
        for pid in pids:
            for b, cid in self.processes[pid]['commits']:
                if b == branch:
                    ids.update(self.commits[cid].get('appended') or [])
        groups = {}

        def acc(cls, lk, body, valid_at, side, eid=None):
            for a in self._amounts(cls):
                if body.get(a) is None:
                    continue
                cur, amt = _amount(body[a])
                g = groups.setdefault((cls, canonical(lk), cur), {'ledger_key': lk, 'old': {}, 'new': {},
                                                                  'corrects': [], 'valid': []})
                g[side][a] = g[side].get(a, 0) + amt
                if side == 'old':
                    if eid not in g['corrects']:
                        g['corrects'].append(eid)
                    g['valid'].append(valid_at)

        for (cls, _), q in sorted(st0['ledger'].items()):
            for e in q:
                if e['entry_id'] in ids:
                    acc(cls, e['ledger_key'], e['body'], e['valid_at'], 'old', e['entry_id'])
        for p in postings:
            acc(p['class'], p['ledger_key'], p['body'], p['valid_at'], 'new')
        out = []
        for (cls, _, cur), g in sorted(groups.items()):
            delta = {a: g['new'].get(a, 0) - g['old'].get(a, 0) for a in sorted(set(g['new']) | set(g['old']))}
            if all(v == 0 for v in delta.values()):
                continue
            amounts = {a: {'amount': fmt_decimal(v, MINOR_UNITS[cur]), 'currency': cur} if cur else fmt_decimal(v)
                       for a, v in delta.items()}
            valid = self._today() if policy == 'open_period' else \
                max(g['valid'] or [p['valid_at'] for p in postings if p['class'] == cls])
            out.append({'class': cls, 'ledger_key': g['ledger_key'], 'valid_at': valid, 'amounts': amounts,
                        'corrects': g['corrects']})
        return out
