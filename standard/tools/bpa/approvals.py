"""Approvals, deferred commits and simulation of BPA 1.0-rc8 (PH-2 step 1c, DEC-091): pending
changes (STD-TXN-060), approval requirements (STD-LGS-041), separation of duties
(STD-LGS-042), approval evidence (STD-LGS-043) and historical rule simulation
(STD-LGS-044). Mixed into the reference store."""
import copy
import datetime

from . import BpaError
from .canon import canonical, content_id

SIM_CLASS = 'SimulationRun'


def _plus(now, duration):
    from .expr import parse_duration
    us = parse_duration(duration).us
    t = datetime.datetime.strptime(now, '%Y-%m-%dT%H:%M:%S.%fZ') + datetime.timedelta(microseconds=us)
    return t.strftime('%Y-%m-%dT%H:%M:%S.%fZ')


def requirements(decisions):
    """The requirements of the `require` outcomes of a deferred process (STD-LGS-041):
    one item per step, the approval items with their specification."""
    out = []
    for oid, port, r in decisions:
        for step in r.get('steps') or []:
            item = {'step': step, 'object_id': oid, 'port': port}
            if step == 'approval':
                spec = next((e.get('value') for e in r['explanations']
                             if e.get('name') == 'approvals' and isinstance(e.get('value'), dict)), None) or {}
                item['count'] = spec.get('count', 1)
                item['order'] = spec.get('order', 'any')
                for k in ('roles', 'principals', 'incompatible', 'expires_in'):
                    if spec.get(k) is not None:
                        item[k] = spec[k]
            if not any(x['step'] == item['step'] and x['object_id'] == oid for x in out):
                out.append(item)
    return out


class Approvals:

    # ---- deferred commits (STD-TXN-060) ----

    def _defer(self, run, i, p, result, attempt, decisions):
        """Store the process's changes as a pending change instead of committing them."""
        if not hasattr(self, 'pending_changes'):
            self.pending_changes = {}
        changes = copy.deepcopy(run.uow)
        reqs = requirements(decisions)
        pid = p['process_id']
        preview = content_id('ubos.chunk.v1', {'branch': run.branch, 'changes': [
            {k: v for k, v in c.items() if k != 'derivation'} for c in changes]})
        rec = {'pending_change_id': pid, 'target_branch': run.branch, 'changes': changes, 'preview': preview,
               'requirements': reqs, 'approvals': [], 'state': 'pending',
               'initiator': run.context['principal'], 'process_id': pid}
        exp = [r['expires_in'] for r in reqs if r.get('expires_in')]
        if exp:
            rec['expires_at'] = min(_plus(self.now, e) for e in exp)
        self.pending_changes[pid] = rec
        out = self._finish(run, i, p, result, None, 'ok', attempt)
        out['pending'] = self._pc_view(rec)
        return out

    def _pc_view(self, rec):
        out = {k: copy.deepcopy(rec[k]) for k in ('pending_change_id', 'target_branch', 'preview', 'requirements',
                                                  'approvals', 'state') if k in rec}
        for k in ('expires_at', 'voided', 'changed_since', 'invalid_reason', 'cseq'):
            if rec.get(k) is not None:
                out[k] = copy.deepcopy(rec[k])
        return out

    def _pc(self, pcid):
        rec = getattr(self, 'pending_changes', {}).get(pcid)
        if rec is None:
            raise BpaError('IAM.NOT_FOUND', 'unknown pending change')
        if rec['state'] == 'pending':
            self._pc_refresh(rec)
        return rec

    def _pc_refresh(self, rec):
        """FR-FLOW-045: expiry, and invalidation when a subject changed since the request."""
        if rec.get('expires_at') and self.now >= rec['expires_at']:
            rec['state'] = 'expired'
            return
        st = self._state(rec['target_branch'])[0]
        changed = []
        for c in rec['changes']:
            b = c.get('base')
            oid = c.get('object_id')
            if b is None or oid is None:
                continue
            o = st['objects'].get(oid)
            if o is None or o['head'] != b:
                changed.append({'object_id': oid, 'approved': self.versions[b]['body'],
                                'current': self.versions[o['head']]['body'] if o else None})
        if changed:
            rec['state'] = 'invalidated'
            rec['invalid_reason'] = 'TXN.HEAD_MOVED'
            rec['changed_since'] = changed
            if rec['approvals']:
                rec['voided'] = rec['approvals']
                rec['approvals'] = []

    def pending_change(self, i):
        return self._pc_view(self._pc(i.get('pending_change_id')))

    def _eligible(self, rec, req, i, approver):
        """STD-LGS-041 eligibility and STD-LGS-042 separation of duties."""
        actors = [approver] + ([i['on_behalf_of']] if i.get('on_behalf_of') else [])
        if rec['initiator'] in actors:
            raise BpaError('IAM.DENIED', {'sod': 'maker_checker', 'principal': rec['initiator']})
        done = {a['approver'] for a in rec['approvals']} | {a['on_behalf_of'] for a in rec['approvals']
                                                            if a.get('on_behalf_of')}
        for a in actors:
            if a in done:
                raise BpaError('IAM.DENIED', {'sod': 'one_decision_per_principal', 'principal': a})
        for port in req.get('incompatible') or []:
            for pid, inv in getattr(self, 'invocations', {}).items():
                prec = self.processes.get(pid)
                if inv.get('port') == port and prec and prec['state'] == 'committed' and \
                        self._target_oid(inv) == req['object_id'] and prec['initiator'] in actors:
                    raise BpaError('IAM.DENIED', {'sod': 'incompatible_duty', 'duty': port,
                                                  'principal': prec['initiator']})
        if req.get('principals') or req.get('roles'):
            ok = approver in (req.get('principals') or []) or \
                bool(set(i.get('roles') or []) & set(req.get('roles') or []))
            if not ok:
                raise BpaError('IAM.DENIED', {'approval': 'not_eligible', 'principal': approver})
        if req.get('order') == 'sequence' and req.get('principals'):
            n = sum(1 for a in rec['approvals'] if a.get('step') == 'approval')
            if req['principals'][n:n + 1] != [approver]:
                raise BpaError('IAM.DENIED', {'approval': 'out_of_sequence', 'principal': approver})

    def _target_oid(self, inv):
        t = inv.get('target')
        if isinstance(t, str) and t.startswith('ubos://'):
            parts = t.split('/')
            st = self._state(inv.get('branch', 'main'))[0]
            return next((k for k, o in st['objects'].items() if o['class'] == parts[3] and o['slug'] == parts[4]), None)
        return t

    def _satisfied(self, rec):
        approvals = [a for a in rec['approvals'] if a['decision'] == 'approve']
        for req in rec['requirements']:
            if req['step'] == 'approval' and sum(1 for a in approvals if a['step'] == 'approval') < req['count']:
                return False
            if req['step'] == 'second_person' and not approvals:
                return False
            if req['step'] == 'signature' and not any(a.get('signature') for a in approvals):
                return False
        return True

    def approve(self, i):
        """STD-LGS-043: record an approval or a rejection of a pending change; the final
        approval applies the change in a new process (STD-TXN-060)."""
        rec = self._pc(i.get('pending_change_id'))
        if rec['state'] != 'pending':
            return self._pc_view(rec)
        approver = i.get('approver')
        decision = i.get('decision', 'approve')
        if not isinstance(approver, str) or not approver:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'an approval names its approver')
        if decision not in ('approve', 'reject'):
            raise BpaError('MODEL.INVALID_VALUE', 'decision is approve or reject')
        step = 'approval' if any(r['step'] == 'approval' for r in rec['requirements']) else \
            rec['requirements'][0]['step'] if rec['requirements'] else 'approval'
        req = next((r for r in rec['requirements'] if r['step'] == step), {'step': step, 'object_id': None})
        self._eligible(rec, req, i, approver)
        a = {'approver': approver, 'decision': decision, 'preview': rec['preview'], 'at': self.now,
             'auth_level': i.get('auth_level', 'basic'), 'step': step}
        for k in ('on_behalf_of', 'comment', 'signature'):
            if i.get(k) is not None:
                a[k] = i[k]
        rec['approvals'].append(a)
        if decision == 'reject':
            rec['state'] = 'rejected'
            return self._pc_view(rec)
        if self._satisfied(rec):
            self._apply(rec)
        return self._pc_view(rec)

    def _apply(self, rec):
        """Final approval: a new process re-validates and commits the changes with the
        approvals referenced, or the pending change becomes invalidated."""
        try:
            res = self.commit(rec['target_branch'], copy.deepcopy(rec['changes']),
                              {'kind': 'action', 'initiator': rec['initiator']})
        except BpaError as e:
            rec['state'] = 'invalidated'
            rec['invalid_reason'] = e.code
            if e.code == 'TXN.HEAD_MOVED':
                self._pc_refresh(rec)
                rec['state'] = 'invalidated'
            return
        cid = self.branches[rec['target_branch']]['head']
        self.commits[cid]['approvals'] = [{'pending_change_id': rec['pending_change_id'],
                                           'preview': rec['preview'], 'approvers': [a['approver'] for a in rec['approvals']]}]
        rec['state'] = 'applied'
        rec['cseq'] = res['cseq']

    # ---- simulation (STD-LGS-044) ----

    RULE_CLASSES = ('GovernanceSheet', 'PropertyDecl', 'DecisionTable')

    def simulate(self, i):
        """FR-RULE-071…073: re-run the historical port calls of a window on sandbox branches
        with the candidate rules of the source branch, compare the outcomes with the actual
        ones, within a budget, and store the run as an object."""
        source, target = i.get('source'), i.get('target', 'main')
        self._branch(source)
        frm, to = i.get('from'), i.get('to')
        budget = (i.get('budget') or {}).get('processes')
        if not isinstance(budget, int) or budget < 1:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a simulation has a budget of processes')
        rid = i.get('run_id')
        if not isinstance(rid, str):
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'run_id')
        sel = i.get('select') or {}
        # the historical port calls of the window on the target, in commit order
        procs = []
        for pid, inv in getattr(self, 'invocations', {}).items():
            prec = self.processes.get(pid)
            if inv.get('port') is None or inv.get('branch', 'main') != target or prec is None:
                continue
            if sel.get('port') and inv['port'] != sel['port']:
                continue
            commits = [c for b, c in prec['commits'] if b == target]
            when = self.journals[pid]['context']['now'][:10]
            if (frm and when < frm) or (to and when >= to):
                continue
            procs.append((when, commits[0] if commits else None, pid))
        procs.sort(key=lambda x: (x[0], self.commits[x[1]]['cseq'] if x[1] is not None else -1, x[2]))
        src = self._state(source)[0]
        candidates = sorted((oid, o) for oid, o in src['objects'].items() if o['class'] in self.RULE_CLASSES)
        rule_versions = [o['head'] for _, o in candidates]
        outbox, results, covered = len(self.outbox), [], []
        saved = (copy.deepcopy(self.processes), dict(getattr(self, 'journals', {})),
                 dict(getattr(self, 'invocations', {})), dict(getattr(self, 'meterings', {})))
        try:
            for n, (when, cid, pid) in enumerate(procs[:budget], 1):
                actual = self._outcome_of(pid)
                name = 'sandbox/sim-%s-%d' % (rid[:8], n)
                cseq = self.commits[cid]['cseq'] - 1 if cid is not None else self.commits[self.branches[target]['head']]['cseq']
                self.create_branch(name, target, cseq)
                st = self._state(name)[0]
                changes = []
                for oid, o in candidates:
                    body = self.versions[o['head']]['body']
                    cur = st['objects'].get(oid)
                    if cur is None:
                        changes.append({'op': 'create', 'object_id': oid, 'class': o['class'], 'slug': o['slug'], 'body': body})
                    elif cur['head'] != o['head']:
                        changes.append({'op': 'update', 'object_id': oid, 'base': cur['head'], 'body': body})
                if changes:
                    self.commit(name, changes)
                inv = copy.deepcopy(self.invocations[pid])
                inv['branch'] = name
                inv['ctx'] = dict(inv.get('ctx') or {}, profile='simulation')
                try:
                    out = self.execute(inv)
                    sim = {'outcome': 'committed', 'result_hash': content_id('ubos.chunk.v1', out.get('result'))}
                except BpaError as e:
                    sim = {'outcome': 'failed', 'error': e.code}
                item = {'process_id': pid, 'date': when, 'actual': actual, 'simulated': sim,
                        'changed': actual != sim}
                results.append(item)
                covered.append(when)
        finally:
            del self.outbox[outbox:]
            self.processes, self.journals, self.invocations, self.meterings = saved
        coverage = {'processes': len(results), 'selected': len(procs), 'complete': len(results) == len(procs)}
        if covered:
            coverage['from'], coverage['to'] = covered[0], covered[-1]
        body = {'source': source, 'target': target, 'window': {'from': frm, 'to': to}, 'budget': {'processes': budget},
                'rule_versions': rule_versions, 'coverage': coverage,
                'results': results, 'changed': sum(1 for r in results if r['changed'])}
        if i.get('change_set') is not None:
            body['change_set'] = i['change_set']
        self._sim_op = True
        try:
            self.commit(target, [{'op': 'create', 'object_id': rid, 'class': SIM_CLASS, 'slug': 'sim-%s' % rid[:8],
                                  'body': body}])
        finally:
            self._sim_op = False
        if i.get('change_set') is not None:
            cs, _, cbody = self._cs(i['change_set'])
            cbody = dict(cbody, simulations=list(cbody.get('simulations') or []) + [rid])
            self._cs_write(i['change_set'], cbody)
        out = dict(body, run_id=rid)
        out.pop('rule_versions')
        return out

    def _outcome_of(self, pid):
        j = self.journals[pid]
        if self.processes[pid]['state'] == 'committed':
            return {'outcome': 'committed', 'result_hash': j['outputs']['result_hash']}
        return {'outcome': 'failed'}
