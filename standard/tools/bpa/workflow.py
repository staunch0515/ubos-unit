"""Stateful adapter operations of BPA 1.0-rc1 for versioning, time and processes (PH-1 step
1c, DEC-041): change sets and merge previews (STD-VER-020, -021, -057), tags and releases
(STD-VER-060, -061), revert (STD-VER-070, -071), range queries and commit records
(STD-TIME-014, -015, -040), derivation records and impact sets (STD-TXN-030, STD-TIME-031),
the outbox with became-effective events (STD-TXN-050, STD-TIME-050) and bulk processes
(STD-TXN-070). Mixed into the reference store."""
import copy
import re

from . import BpaError
from .canon import canonical, content_id
from .values import GRAMMARS, check_name, validate_value

SEMVER = re.compile(r'\A(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)'
                    r'(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?(\+[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?\Z')
MODES = ('as_originally', 'as_effective', 'current')


def _instant_or_date(v):
    try:
        return validate_value('date', None, v)
    except BpaError:
        return validate_value('instant', None, v)


class Workflow:

    # ---- derivation records (STD-TXN-030) ----

    def _input_meta(self, x):
        v = self.versions.get(x['version'])
        if v is not None:
            if v['object_id'] != x['object']:
                raise BpaError('MODEL.REFERENCE_INVALID', 'version of another object')
            return v['class'], v['cseq']
        for c in self.commits.values():
            for q in c['state']['ledger'].values():
                for e in q:
                    if e['entry_id'] == x['object'] and e['entry_hash'] == x['version']:
                        return e['class'], e['cseq']
        raise BpaError('MODEL.REFERENCE_INVALID', 'unknown input version')

    def _derivation(self, ch):
        d = ch.get('derivation')
        if d is None:
            return None
        logic = d.get('logic') if isinstance(d, dict) else None
        if not isinstance(logic, list) or not logic or not all(isinstance(x, str) for x in logic):
            raise BpaError('MODEL.INVALID_VALUE', 'a derivation names the logic versions that produced it')
        mode = d.get('mode', 'as_effective')
        if mode not in MODES:
            raise BpaError('MODEL.INVALID_VALUE', 'mode is as_originally, as_effective or current')
        items, meta = [], {}
        for x in d.get('inputs') or []:
            if not isinstance(x, dict) or any(k not in x for k in ('object', 'version', 'asof')):
                raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'an input is {object, version, asof}')
            item = {'object': x['object'], 'version': x['version'], 'asof': _instant_or_date(x['asof'])}
            meta[canonical(item)] = self._input_meta(item)
            items.append(item)
        items = sorted({canonical(i): i for i in items}.values(),
                       key=lambda i: (i['object'], i['asof'], i['version']))
        rec = {'process_id': self._pid, 'logic': list(logic), 'mode': mode}
        if len(items) > self.max_inputs:
            per = {}
            for i in items:
                cls, cseq = meta[canonical(i)]
                p = per.setdefault(cls, {'class': cls, 'version_range': [cseq, cseq], 'count': 0})
                p['version_range'] = [min(p['version_range'][0], cseq), max(p['version_range'][1], cseq)]
                p['count'] += 1
            rec['inputs_summary'] = [per[c] for c in sorted(per)]
            rec['inputs_commitment'] = content_id('ubos.chunk.v1', items)
        elif items:
            rec['inputs'] = items
        rid = content_id('ubos.chunk.v1', rec)
        self.derivations[rid] = rec
        self.derivation_inputs[rid] = items
        return rid

    def derivation(self, branch, object_id=None, entry_id=None, sel=None):
        st = self._state(branch, sel)[0]
        if object_id is not None:
            o = st['objects'].get(object_id)
            if o is None:
                raise BpaError('IAM.NOT_FOUND', 'unknown object')
            rid = self.versions[o['head']].get('provenance_ref')
        else:
            _, e = self._entry_owner(st, entry_id)
            if e is None:
                raise BpaError('IAM.NOT_FOUND', 'unknown entry')
            rid = e.get('provenance_ref')
        if rid is None:
            raise BpaError('IAM.NOT_FOUND', 'written by direct input: no derivation record')
        out = dict(self.derivations[rid], provenance_ref=rid)
        if 'inputs' in out:
            # version hashes are named version_hash in adapter results (STD-CONF-005, rule 6)
            out['inputs'] = [{'object': x['object'], 'asof': x['asof'], 'version_hash': x['version']}
                             for x in out['inputs']]
        return out

    # ---- impact set (STD-TIME-031) ----

    def impact(self, branch, time):
        cid = self._commit_at(branch, time)
        c = self.commits[cid]
        if not c['parents']:
            raise BpaError('MODEL.INVALID_VALUE', 'the first commit changes nothing')
        before = self.commits[c['parents'][0]]['state']['objects']
        after = c['state']['objects']
        seeds = []
        for r in c.get('retroactive') or []:
            oid = r['object_id']
            tb = before.get(oid, {}).get('timeline', [])
            ta = after.get(oid, {}).get('timeline', [])
            seeds.append((oid, r['from'], r['to'], tb, ta))
        outputs = []
        for x in sorted(self._anc(self.branches[branch]['head']), key=lambda x: self.commits[x]['cseq']):
            outputs += self.commits[x].get('derived') or []
        hit_versions, hit_entries = set(), set()
        from .store import tl_at, _lt

        def seeded(i):
            for oid, a, b, tb, ta in seeds:
                if i['object'] != oid or _lt(i['asof'], a) or not _lt(i['asof'], b):
                    continue
                old, new = tl_at(tb, i['asof']), tl_at(ta, i['asof'])
                if old and old[2] == i['version'] and (new is None or new[2] != i['version']):
                    return True
            return False
        changed = True
        while changed:
            changed = False
            for kind, oid, h, rid in outputs:
                if (kind == 'object' and h in hit_versions) or (kind == 'entry' and oid in hit_entries):
                    continue
                ins = self.derivation_inputs.get(rid, [])
                if any(seeded(i) or i['version'] in hit_versions or i['object'] in hit_entries for i in ins):
                    if kind == 'object':
                        hit_versions.add(h)
                    else:
                        hit_entries.add(oid)
                    changed = True
        impacted = sorted(({'object_id': self.versions[h]['object_id'], 'version_hash': h} for h in hit_versions),
                          key=lambda v: (v['object_id'], self.versions[v['version_hash']]['cseq']))
        return {'impacted': impacted, 'entries': sorted(hit_entries)}

    # ---- outbox (STD-TXN-050) and became-effective events (STD-TIME-050) ----

    def _effects(self, p, branch, cid):
        rec = self.processes[p['process_id']]
        for e in p.get('effects') or []:
            rec['effects'] += 1
            item = {'effect_id': '%s/%d' % (p['process_id'], rec['effects']), 'type': e['type'],
                    'branch': branch, 'cseq': self.commits[cid]['cseq']}
            for k in ('object_id', 'payload'):
                if e.get(k) is not None:
                    item[k] = e[k]
            self.outbox.append(item)

    def _due(self, frm, cls):
        return frm <= (self._today() if self._decl(cls).get('valid_time', 'date') == 'date' else self.now)

    def _schedule(self, branch, cid, st, result):
        for oid in result['versions']:
            o = st['objects'].get(oid)
            if o is None or not self._decl(o['class']).get('emit_became_effective'):
                continue
            for f, _, vh in o['timeline']:
                key = (branch, oid, f, vh)
                if not self._due(f, o['class']) and key not in [s['key'] for s in self.schedule]:
                    self.schedule.append({'key': key, 'class': o['class']})

    def _tick(self):
        keep = []
        for s in getattr(self, 'schedule', []):
            branch, oid, f, vh = s['key']
            if not self._due(f, s['class']):
                keep.append(s)
                continue
            o = self._view(branch, self.branches[branch]['head'])[0]['objects'].get(oid)
            if o and any(sf == f and sv == vh for sf, _, sv in o['timeline']):
                self.outbox.append({'effect_id': 'became-effective/%s/%s/%s' % (branch, oid, f),
                                    'type': 'BecameEffective', 'branch': branch, 'object_id': oid,
                                    'payload': {'valid_from': f}})
        self.schedule = keep

    def outbox_list(self):
        self._tick()
        return {'effects': [e for e in self.outbox if e['effect_id'] not in self.acked]}

    def ack(self, effect_ids):
        known = {e['effect_id'] for e in self.outbox}
        for x in effect_ids:
            if x not in known:
                raise BpaError('IAM.NOT_FOUND', 'unknown effect')
        self.acked |= set(effect_ids)
        return True

    # ---- commit records and range queries (STD-TIME-014, -015, -040) ----

    def commit_info(self, branch, sel=None):
        cid, _ = self._select(branch, sel or {})
        c = self.commits[cid]
        out = {'branch': c['branch'], 'cseq': c['cseq'], 'commit_id': c['commit_id'], 'tx_time': c['tx_time'],
               'parents': [self.commits[p]['commit_id'] for p in c['parents']],
               'rule_binding': c.get('rule_binding') or {'classes': []}}
        for k in ('process_id', 'resolutions', 'change_set', 'reverts'):
            if c.get(k):
                out[k] = c[k]
        return out

    def read_range(self, branch, object_id, a, b=None, sel=None):
        st, _, ev, _ = self._state(branch, sel)
        o = st['objects'].get(object_id)
        if o is None:
            raise BpaError('IAM.NOT_FOUND', 'unknown object')
        from .store import _lt, _min
        a = _instant_or_date(a)
        if b is not None:
            b = _instant_or_date(b)
            if not _lt(a, b):
                raise BpaError('MODEL.INVALID_VALUE', 'empty range')
        out = []
        for f, t, vh in o['timeline']:
            if _lt(f, b) if b is not None else True:
                if _lt(a, t):
                    seg = self._segment([f if _lt(a, f) else a, _min(t, b), vh])
                    out.append(seg)
        return {'segments': out, 'evaluated_at': ev}

    def history(self, branch, object_id, c=None, d=None):
        for v in (c, d):
            if v is not None:
                validate_value('instant', None, v)
        head = self._branch(branch)['head']
        out = []
        for cid in self._first_parents(head):
            x = self.commits[cid]
            t = x['tx_time']
            if t is None or (c is not None and t < c) or (d is not None and t >= d):
                continue
            if object_id in x['ops']:
                out.append({'cseq': x['cseq'], 'commit_id': x['commit_id'], 'tx_time': t, 'ops': x['ops'][object_id]})
        return {'commits': sorted(out, key=lambda e: e['cseq']), 'evaluated_at': {branch: self.commits[head]['commit_id']}}

    def tree_diff(self, a, b):
        """Keys of the state tree whose entries differ between two coordinates (STD-VER-031)."""
        A = dict(self.state_entries(self.commits[self._select(a['branch'], a)[0]]['state']))
        B = dict(self.state_entries(self.commits[self._select(b['branch'], b)[0]]['state']))
        keys = sorted((k for k in set(A) | set(B) if A.get(k) != B.get(k)), key=lambda k: k.encode('utf-8'))
        return {'keys': keys}

    # ---- tags and releases (STD-VER-060, -061) ----

    def tag(self, name, branch, sel=None, signer=None):
        check_name('slug', name)
        cid, _ = self._select(branch, sel or {})
        if name in self.tags:
            raise BpaError('VER.TAG_IMMUTABLE', 'tag names are never reused')
        signer = signer or 'did:ubos:acme:01900000-0000-7000-8000-000000000002'
        rec = {'name': name, 'commit': self.commits[cid]['commit_id'], 'signer': signer}
        self.tags[name] = {'commit': cid, 'branch': branch, 'signer': signer, 'deleted': False,
                           'signature': content_id('ubos.chunk.v1', rec)}
        return {'name': name, 'branch': branch, 'cseq': self.commits[cid]['cseq'],
                'commit_id': self.commits[cid]['commit_id']}

    def delete_tag(self, name):
        t = self.tags.get(name)
        if t is None or t['deleted']:
            raise BpaError('IAM.NOT_FOUND', 'unknown tag')
        t['deleted'] = True
        return True

    def tag_info(self, name):
        t = self.tags.get(name)
        if t is None:
            raise BpaError('IAM.NOT_FOUND', 'unknown tag')
        return {'name': name, 'branch': t['branch'], 'cseq': self.commits[t['commit']]['cseq'],
                'commit_id': self.commits[t['commit']]['commit_id'], 'signer': t['signer'],
                'signature': t['signature'], 'deleted': t['deleted']}

    def release(self, i):
        branch, version, oid = i['branch'], i.get('version'), i.get('object_id')
        if not isinstance(version, str) or not SEMVER.match(version):
            raise BpaError('MODEL.INVALID_VALUE', 'a release version is a semantic version')
        st = self._state(branch)[0]
        for o in st['objects'].values():
            if o['class'] == 'Release' and not self.versions[o['head']]['tombstone'] \
                    and self.versions[o['head']]['body'].get('version') == version:
                raise BpaError('MODEL.KEY_CONFLICT', 'the version is already released on this branch')
        check_name('slug', i['tag'])
        if i['tag'] in self.tags:
            raise BpaError('VER.TAG_IMMUTABLE', 'tag names are never reused')
        cid, _ = self._select(branch, {k: i[k] for k in ('time', 'commit', 'tx_time') if k in i})
        body = {'version': version, 'tag': i['tag']}
        for k in ('notes', 'change_sets'):
            if i.get(k) is not None:
                body[k] = i[k]
        res = self.commit(branch, [{'op': 'create', 'object_id': oid, 'class': 'Release', 'slug': i['tag'],
                                    'body': body}])
        self.tags[i['tag']] = {'commit': cid, 'branch': branch, 'signer': i.get('signer') or 'did:ubos:acme:01900000-0000-7000-8000-000000000002',
                               'deleted': False,
                               'signature': content_id('ubos.chunk.v1', {'name': i['tag'],
                                                                         'commit': self.commits[cid]['commit_id']})}
        return {'tag': i['tag'], 'tagged': self.commits[cid]['cseq'], 'cseq': res['cseq'],
                'versions': res['versions']}

    # ---- merge previews (STD-VER-057) ----

    def merge_preview(self, source, target):
        self._branch(source)
        self._branch(target)
        s, t = self.branches[source]['head'], self.branches[target]['head']
        T = self.commits[t]['state']
        best = self._best({t}, {s})
        base = self.commits[best[0]]['commit_id'] if len(best) == 1 else \
            content_id('ubos.chunk.v1', [self.commits[c]['commit_id'] for c in best])
        conflicts, validation, st = [], [], T
        if s not in self._anc(t):
            cseq = self.commits[t]['cseq'] + 1
            self._cseq = cseq
            try:
                st, conflicts = self._merge_result(s, t, source, cseq, keep=True)
                conflicts = self._unique(conflicts)
                _, entries = self._changed(T, st)
                self._validate(T, st, entries)
            except BpaError as e:
                validation.append({'code': e.code})
        changes = self._diff_states(T, st)
        counts = {}
        for ch in changes:
            c = counts.setdefault(ch['class'], {})
            c[ch['op']] = c.get(ch['op'], 0) + 1
        summary = {'counts': counts}
        pid = content_id('ubos.chunk.v1', {'changes': changes, 'conflicts': conflicts, 'validation': validation,
                                           'summary': summary})
        return {'base': base, 'source_head': self.commits[s]['commit_id'], 'target_head': self.commits[t]['commit_id'],
                'changes': changes, 'conflicts': conflicts, 'validation': validation, 'summary': summary,
                'preview': pid}

    # ---- change sets (STD-VER-020, -021) ----

    def _cs(self, oid):
        cs = self.change_sets.get(oid)
        if cs is None:
            raise BpaError('IAM.NOT_FOUND', 'unknown change set')
        o = self._state(cs['target'])[0]['objects'][oid]
        return cs, o, dict(self.versions[o['head']]['body'])

    def _cs_write(self, oid, body):
        cs = self.change_sets[oid]
        o = self._state(cs['target'])[0]['objects'][oid]
        self.commit(cs['target'], [{'op': 'update', 'object_id': oid, 'base': o['head'], 'body': body}])

    def _cs_refresh(self, oid):
        """STD-VER-021: a changed preview invalidates every approval."""
        cs, _, body = self._cs(oid)
        if body['state'] not in ('submitted', 'approved'):
            return body
        preview = self.merge_preview(body['source'], body['target'])['preview']
        if preview != body.get('preview'):
            body.update(state='submitted', preview=preview, approvals=[])
            self._cs_write(oid, body)
        return body

    def _cs_result(self, body, extra=None):
        out = {'state': body['state']}
        for k in ('preview', 'approvals', 'merged_commit'):
            if body.get(k) is not None:
                out[k] = body[k]
        out.update(extra or {})
        return out

    def change_set(self, i):
        action, oid = i.get('action'), i.get('id')
        if action == 'open':
            source, target = i.get('source'), i.get('target')
            self._branch(source)
            self._check_active(target)
            if self.branches[source]['kind'] in ('main', 'release', 'overlay'):
                raise BpaError('VER.MERGE_NOT_ALLOWED', 'the source of a change set is not protected')
            self._merge_allowed(source, target, True)
            if not isinstance(oid, str) or not GRAMMARS['uuid'].match(oid):
                raise BpaError('MODEL.INVALID_VALUE', 'a change set id is a UUID')
            body = {'source': source, 'target': target, 'title': i.get('title'), 'state': 'open'}
            if i.get('description') is not None:
                body['description'] = i['description']
            self.commit(target, [{'op': 'create', 'object_id': oid, 'class': 'ChangeSet', 'slug': 'cs-' + oid,
                                  'body': body}])
            self.change_sets[oid] = {'target': target, 'merge': None}
            return self._cs_result(body)
        if action == 'get':
            return self._cs_result(self._cs_refresh(oid))
        body = self._cs_refresh(oid)
        allowed = {'submit': ('open',), 'approve': ('submitted',), 'reject': ('submitted',),
                   'merge': ('approved',), 'abandon': ('open', 'submitted')}
        if action not in allowed:
            raise BpaError('MODEL.INVALID_VALUE', 'unknown change set action')
        if body['state'] not in allowed[action]:
            raise BpaError('FLOW.TRANSITION_NOT_ALLOWED', '%s from %s' % (action, body['state']))
        if action == 'submit':
            p = self.merge_preview(body['source'], body['target'])
            if p['conflicts']:
                raise BpaError('VER.MERGE_CONFLICT', {'conflicts': p['conflicts']})
            if p['validation']:
                raise BpaError(p['validation'][0]['code'], 'the merge preview does not validate')
            body.update(state='submitted', preview=p['preview'], approvals=[])
        elif action in ('approve', 'reject'):
            if not i.get('approver'):
                raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'an approval names its approver')
            if action == 'reject':
                body['state'] = 'rejected'
            else:
                body['approvals'] = list(body.get('approvals') or []) + [
                    {'approver': i['approver'], 'decision': 'approve', 'preview': body['preview'], 'at': self.now}]
                body['state'] = 'approved'
        elif action == 'abandon':
            body['state'] = 'abandoned'
        else:
            res = self.merge(body['source'], body['target'], i.get('resolutions'), _change_set=oid)
            self.change_sets[oid]['merge'] = self.branches[body['target']]['head']
            body.update(state='merged', merged_commit=res.get('commit_id') or
                        self.commits[self.branches[body['target']]['head']]['commit_id'])
            self._cs_write(oid, body)
            return self._cs_result(body, {'cseq': res['cseq']})
        self._cs_write(oid, body)
        return self._cs_result(body)

    # ---- revert (STD-VER-070, -071) ----

    def _revert_targets(self, i):
        if i.get('process_id') is not None:
            p = self.processes.get(i['process_id'])
            if p is None:
                raise BpaError('IAM.NOT_FOUND', 'unknown process')
            return list(p['commits']), [i['process_id']]
        if i.get('change_set') is not None:
            cs = self.change_sets.get(i['change_set'])
            if cs is None:
                raise BpaError('IAM.NOT_FOUND', 'unknown change set')
            if cs['merge'] is None:
                raise BpaError('FLOW.TRANSITION_NOT_ALLOWED', 'the change set is not merged')
            targets = [(cs['target'], cs['merge'])]
        else:
            branch = i['branch']
            sel = {k: i[k] for k in ('time', 'commit', 'tag') if i.get(k) is not None}
            if not sel:
                raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'revert names a commit')
            cid = self._select(branch, sel)[0]
            targets = [(branch, cid)]
        pids = [self.commits[c].get('process_id') for _, c in targets]
        return targets, [p for p in pids if p]

    def _revert_one(self, T, cid, rev_ids, cseq, branch, result):
        C = self.commits[cid]
        if not C['parents']:
            raise BpaError('MODEL.INVALID_VALUE', 'the first commit of a branch cannot be reverted')
        P = self.commits[C['parents'][0]]['state']
        st, conflicts = self._merge_states(C['state'], P, T, cseq=cseq, branch=branch)
        if conflicts:
            raise BpaError('VER.MERGE_CONFLICT', {'conflicts': self._unique(conflicts)})
        for oid in sorted(set(T['objects']) - set(st['objects'])):
            st['objects'][oid] = copy.deepcopy(T['objects'][oid])
            tl = st['objects'][oid]['timeline']
            if tl and not self.versions[st['objects'][oid]['head']]['tombstone']:
                self._end_object(st, oid, tl[0][0], result)
        before = {e['entry_id'] for q in P['ledger'].values() for e in q}
        added = [e for q in C['state']['ledger'].values() for e in q if e['entry_id'] not in before]
        for e in sorted(added, key=lambda e: (e['cseq'], e['seq'], e['entry_id'])):
            if e['entry_id'] not in rev_ids:
                raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a reversal id is needed for entry %s' % e['entry_id'])
            self._append(st, {'entry_id': e['entry_id'], 'new_entry_id': rev_ids[e['entry_id']]}, result,
                         'reverse', cseq)
        return st

    def revert(self, i):
        targets, pids = self._revert_targets(i)
        dependents = sorted(self._descendants(set(pids)) - set(pids))
        reverted = sorted(({'branch': b, 'cseq': self.commits[c]['cseq']} for b, c in targets),
                          key=lambda x: (x['branch'], -x['cseq']))
        if i.get('dry_run'):
            return {'reverted': reverted, 'dependents': dependents}
        rev_ids = i.get('reversal_ids') or {}
        p = self._process(i.get('process'))
        plan = []
        for branch in sorted({b for b, _ in targets}):
            self._check_active(branch)
            head = self.branches[branch]['head']
            T = self.commits[head]['state']
            cseq = self.commits[head]['cseq'] + 1
            self._cseq = cseq
            st = copy.deepcopy(T)
            result = {'cseq': cseq, 'versions': {}, 'entries': {}, 'retroactive': []}
            self._begin(st, p)
            try:
                for _, cid in sorted(((b, c) for b, c in targets if b == branch),
                                     key=lambda x: -self.commits[x[1]]['cseq']):
                    st = self._revert_one(st, cid, rev_ids, cseq, branch, result)
                objs, entries = self._changed(T, st)
                for oid in objs:
                    result['versions'][oid] = st['objects'][oid]['head']
                binding = self._binding(st, result)
            finally:
                self._exts = None
                self._eff_cache = {}
            self._validate(T, st, entries)
            ops = {oid: [{'kind': 'revert'}] for oid in objs}
            plan.append((branch, head, st, cseq, result, ops, binding, entries,
                         [self.commits[c]['commit_id'] for b, c in targets if b == branch]))
        out = []
        for branch, head, st, cseq, result, ops, binding, entries, reverts in plan:
            cid = self._new_commit(branch, [head], st, cseq, ops=ops, rule_binding=binding, reverts=reverts,
                                   process_id=p['process_id'] if p['explicit'] else None,
                                   appended=[e['entry_id'] for e in entries])
            self._advance(branch, cid)
            self._record(p, 'committed', (branch, cid))
            out.append({'branch': branch, 'cseq': cseq, 'versions': result['versions'], 'entries': result['entries']})
        return {'reverted': reverted, 'dependents': dependents, 'commits': out}

    # ---- bulk processes (STD-TXN-070) ----

    def bulk(self, i):
        pid, branch = i.get('process_id'), i.get('branch')
        changes = i.get('changes') or []
        size = i.get('max_changes', 100000)
        mode = i.get('mode', 'independent')
        if mode not in ('independent', 'all_or_nothing'):
            raise BpaError('MODEL.INVALID_VALUE', 'mode is independent or all_or_nothing')
        if not isinstance(size, int) or isinstance(size, bool) or size < 1:
            raise BpaError('MODEL.INVALID_VALUE', 'max_changes is a positive integer')
        b = self.bulks.get(pid)
        if i.get('resume'):
            if b is None:
                raise BpaError('IAM.NOT_FOUND', 'unknown bulk process')
            if b['state'] != 'failed':
                raise BpaError('FLOW.TRANSITION_NOT_ALLOWED', 'only a failed bulk process is resumed')
        else:
            if b is not None:
                raise BpaError('TXN.ALREADY_EXISTS', 'bulk process exists')
            p = self._process(dict(i.get('process') or {}, process_id=pid, kind='job'))
            self._check_active(branch)
            b = {'branch': branch, 'mode': mode, 'staging': None, 'children': [], 'state': 'running'}
            if mode == 'all_or_nothing':
                staging = i.get('staging')
                self.create_branch(staging, branch)
                b['staging'] = staging
            self.bulks[pid] = b
            self._record(p, 'running')
        chunks = [changes[k:k + size] for k in range(0, len(changes), size)]
        target = b['staging'] or b['branch']
        for n in range(len(b['children']), len(chunks)):
            try:
                res = self.commit(target, chunks[n], {'kind': 'job',
                                                      'trigger': {'type': 'job', 'ref': pid},
                                                      'parent_process': pid, 'bulk': pid})
            except BpaError as e:
                b['state'] = 'failed'
                self.processes[pid]['state'] = 'failed'
                return {'state': 'failed', 'error': e.code, 'children': self._children(b)}
            b['children'].append({'n': n + 1, 'branch': target, 'cseq': res['cseq'], 'changes': len(chunks[n])})
        out = {'state': 'committed', 'children': self._children(b)}
        if b['staging']:
            res = self.merge(b['staging'], b['branch'], _change_set=None)
            out['merge'] = {'cseq': res['cseq']}
        b['state'] = 'committed'
        self.processes[pid]['state'] = 'committed'
        return out

    @staticmethod
    def _children(b):
        return [dict(c) for c in b['children']]

