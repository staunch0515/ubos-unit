"""Delegations, tasks, work queues, due dates, escalation and the service-level report of
BPA 1.0-rc10 (PH-2 step 4a, DEC-105): STD-LGS-060…064. Mixed into the reference store."""
import copy
import datetime
from decimal import Decimal, ROUND_HALF_EVEN

from . import BpaError
from .approvals import _plus
from .values import INSTANT, DATE, parse_decimal, fmt_decimal

TASK = 'Task'
QUEUE = 'WorkQueue'
DELEGATION = 'Delegation'
TASK_ACTIONS = ('create', 'claim', 'release', 'reassign', 'complete', 'cancel')
ASSIGNMENT = ('manual', 'round_robin', 'least_loaded', 'skill')
ESCALATION = ('notify', 'reassign', 'raise_priority')
SCOPE_KEYS = ('ports', 'classes', 'task_types', 'limit')


def _incomplete(what):
    return BpaError('MODEL.DECLARATION_INCOMPLETE', what)


def _invalid(what):
    return BpaError('MODEL.INVALID_VALUE', what)


def _instant(v, what):
    if not isinstance(v, str) or not INSTANT.match(v):
        raise _invalid('%s is an instant' % what)
    return v


def _target(to, kinds):
    """A task target `{principal}`, `{role}` or `{queue}` (the kinds allowed)."""
    if not isinstance(to, dict) or len(to) != 1 or next(iter(to)) not in kinds or \
            not isinstance(next(iter(to.values())), str):
        raise _invalid('a target is one of %s' % ', '.join('{%s}' % k for k in kinds))
    return dict(to)


def check_queue(b):
    """STD-LGS-062, -063: the declaration of a work queue."""
    if not isinstance(b.get('members'), list) or any(not isinstance(m, str) for m in b['members']):
        raise _invalid('members are principals')
    if any(not isinstance(r, str) for r in b.get('managers') or []):
        raise _invalid('managers are roles')
    skills = b.get('skills') or {}
    if not isinstance(skills, dict) or any(not isinstance(v, list) for v in skills.values()):
        raise _invalid('skills map a member to a list of skills')
    if b.get('due_rule') is not None and not isinstance(b['due_rule'], str):
        raise _invalid('a due rule is an L1 expression')
    from .expr import parse_duration
    for s in b.get('escalation') or []:
        if not isinstance(s, dict) or s.get('action') not in ESCALATION or not isinstance(s.get('after'), str):
            raise _invalid('an escalation step is {after, action} with action notify, reassign or raise_priority')
        parse_duration(s['after'])
        if s['action'] == 'notify':
            _target(s.get('to'), ('principal', 'role'))
        elif s['action'] == 'reassign':
            _target(s.get('to'), ('principal', 'role', 'queue'))
        elif not isinstance(s.get('priority'), int) or isinstance(s['priority'], bool):
            raise _invalid('raise_priority names the priority')


class Work:

    # ---- helpers ----

    def _roles_of(self, did):
        return set((getattr(self, 'principals', {}).get(did) or {}).get('roles') or [])

    def _obj_of(self, st, oid, cls):
        o = st['objects'].get(oid) if isinstance(oid, str) else None
        if o is None or o['class'] != cls or self.versions[o['head']]['tombstone']:
            raise BpaError('IAM.NOT_FOUND', 'unknown %s' % cls)
        return o, copy.deepcopy(self.versions[o['head']]['body'])

    def _kernel_write(self, branch, cls, changes, principal=None):
        process = {'initiator': principal} if principal else None
        return self._sys_commit(branch, changes, (cls,), process)

    # ---- delegations (STD-LGS-060) ----

    def delegate(self, i):
        branch = i.get('branch', 'main')
        did, delegator, delegate = i.get('delegation_id'), i.get('delegator'), i.get('delegate')
        if not isinstance(did, str) or not isinstance(delegator, str) or not isinstance(delegate, str):
            raise _incomplete('a delegation has its id, delegator and delegate')
        if delegator == delegate:
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'a principal does not delegate to itself')
        scope = i.get('scope')
        if not isinstance(scope, dict) or not any(scope.get(k) is not None for k in SCOPE_KEYS):
            raise _incomplete('a delegation has a scope')
        if any(k not in SCOPE_KEYS for k in scope):
            raise _invalid('a scope has ports, classes, task_types and limit')
        for k in ('ports', 'classes', 'task_types'):
            if scope.get(k) is not None and (not isinstance(scope[k], list) or
                                             any(not isinstance(x, str) for x in scope[k])):
                raise _invalid('%s is a list of names' % k)
        if scope.get('limit') is not None:
            lim = scope['limit']
            if not isinstance(lim, dict) or not isinstance(lim.get('param'), str):
                raise _invalid('a limit is {param, max}')
            parse_decimal(lim.get('max'))
        frm, to = _instant(i.get('valid_from'), 'valid_from'), _instant(i.get('valid_to'), 'valid_to')
        if frm >= to:
            raise _invalid('valid_from is before valid_to')
        roles = i.get('roles') or []
        if not isinstance(roles, list) or any(not isinstance(r, str) for r in roles):
            raise _invalid('roles are a list of names')
        extra = sorted(set(roles) - self._roles_of(delegator))
        if extra:
            raise BpaError('IAM.DENIED', {'delegation': 'exceeds_authority', 'roles': extra})
        body = {'delegator': delegator, 'delegate': delegate, 'roles': sorted(set(roles)), 'scope': scope,
                'valid_from': frm, 'valid_to': to, 'state': 'active'}
        res = self._kernel_write(branch, DELEGATION, [{'op': 'create', 'object_id': did, 'class': DELEGATION,
                                                       'slug': 'delegation-%s' % did[:8], 'body': body}], delegator)
        return dict(body, delegation_id=did, cseq=res['cseq'])

    def revoke_delegation(self, i):
        branch = i.get('branch', 'main')
        st = self._state(branch)[0]
        o, body = self._obj_of(st, i.get('delegation_id'), DELEGATION)
        if i.get('principal') != body['delegator']:
            raise BpaError('IAM.DENIED', {'delegation': 'not_delegator'})
        if body['state'] == 'revoked':
            raise BpaError('FLOW.TRANSITION_NOT_ALLOWED', {'state': 'revoked'})
        body.update(state='revoked', revoked_at=self.now)
        if i.get('reason') is not None:
            body['reason'] = i['reason']
        self._kernel_write(branch, DELEGATION, [{'op': 'update', 'object_id': i['delegation_id'], 'base': o['head'],
                                                 'body': body}], body['delegator'])
        return dict(body, delegation_id=i['delegation_id'])

    def _delegation_state(self, body):
        if body['state'] == 'revoked':
            return 'revoked'
        if self.now < body['valid_from']:
            return 'scheduled'
        return 'active' if self.now < body['valid_to'] else 'expired'

    def delegations(self, i):
        branch = i.get('branch', 'main')
        p = i.get('principal')
        st = self._state(branch)[0]
        out = []
        for oid, _, body in self._live_of(st, DELEGATION):
            if p is None or p in (body['delegator'], body['delegate']):
                out.append(dict(body, delegation_id=oid, state=self._delegation_state(body)))
        return {'delegations': out}

    def _use_delegation(self, st, did, principal, on_behalf_of=None, **scope):
        """The delegation an action names, checked for the acting principal and the scope
        (`port`, `cls`, `task_type`, `params`)."""
        _, body = self._obj_of(st, did, DELEGATION)
        if body['delegate'] != principal:
            raise BpaError('IAM.DENIED', {'delegation': 'not_delegate'})
        if on_behalf_of is not None and on_behalf_of != body['delegator']:
            raise BpaError('IAM.DENIED', {'delegation': 'not_delegator'})
        state = self._delegation_state(body)
        if state == 'expired':
            raise BpaError('IAM.DELEGATION_EXPIRED', {'delegation_id': did, 'valid_to': body['valid_to']})
        if state != 'active':
            raise BpaError('IAM.DENIED', {'delegation': 'revoked' if state == 'revoked' else 'not_yet_valid'})
        sc = body['scope']
        ok = True
        if sc.get('ports') is not None and scope.get('port') not in sc['ports']:
            ok = False
        if sc.get('classes') is not None and scope.get('cls') not in sc['classes']:
            ok = False
        if sc.get('task_types') is not None and scope.get('task_type') not in sc['task_types']:
            ok = False
        if 'task_type' in scope and sc.get('task_types') is None:
            ok = False          # task authority is delegated by type only
        if sc.get('limit') is not None and 'task_type' not in scope:
            v = (scope.get('params') or {}).get(sc['limit']['param'])
            try:
                ok = ok and v is not None and parse_decimal(str(v)) <= parse_decimal(sc['limit']['max'])
            except BpaError:
                ok = False
        if not ok:
            raise BpaError('IAM.DENIED', {'delegation': 'out_of_scope'})
        return body

    def _approval_delegation(self, rec, req, i, approver):
        """STD-LGS-060: an approval through a delegation; returns (on_behalf_of, roles)."""
        st = self._state(rec['target_branch'])[0]
        o = st['objects'].get(req.get('object_id')) if req.get('object_id') else None
        inv = getattr(self, 'invocations', {}).get(rec.get('process_id')) or {}
        body = self._use_delegation(st, i['delegation'], approver, i.get('on_behalf_of'), port=req.get('port'),
                                    cls=o['class'] if o else None, params=inv.get('params'))
        return body['delegator'], body['roles']

    # ---- work queues and tasks (STD-LGS-061, -062) ----

    def _check_queue_body(self, b):
        check_queue(b)

    def _load(self, st, member):
        n = 0
        for _, _, b in self._live_of(st, TASK):
            if b['state'] in ('open', 'claimed') and (
                    b.get('owner') == member or (b.get('owner') is None and (b.get('assignee') or {}).get('principal') == member)):
                n += 1
        return n

    def _assign(self, st, qid, q, task):
        rule = q.get('assignment', 'manual')
        members = q['members']
        if rule == 'manual' or not members:
            return None
        if rule == 'round_robin':
            n = sum(1 for _, _, b in self._live_of(st, TASK) if b.get('queue') == qid and b.get('assigned_by') == 'round_robin')
            return members[n % len(members)]
        cands = members
        if rule == 'skill':
            need = set(task.get('skills') or [])
            cands = [m for m in members if need <= set((q.get('skills') or {}).get(m) or [])]
            if not cands:
                return None
        loads = [(self._load(st, m), k, m) for k, m in enumerate(cands)]
        return min(loads)[2]

    def _due_of(self, q, task):
        from .expr import eval_expr
        types = {'created': {'name': 'created', 'type': 'date'}, 'created_at': {'name': 'created_at', 'type': 'instant'},
                 'type': {'name': 'type', 'type': 'text'}, 'priority': {'name': 'priority', 'type': 'integer'}}
        out = eval_expr({'expr': q['due_rule'], 'params_types': types, 'calendars': q.get('calendars') or {},
                         'params': {'created': self.now[:10], 'created_at': self.now, 'type': task['type'],
                                    'priority': task['priority']}})
        v, t = out['value'], out['type']
        if t == 'date':
            d = datetime.date.fromisoformat(v) + datetime.timedelta(days=1)
            return d.isoformat() + 'T00:00:00.000000Z', v
        if t == 'instant':
            return v, None
        raise _invalid('a due rule gives a date or an instant')

    def _task_view(self, tid, body):
        return dict(body, task_id=tid)

    def _log_entry(self, action, principal, on_behalf_of=None, **extra):
        e = {'action': action, 'principal': principal, 'at': self.now}
        if on_behalf_of:
            e['on_behalf_of'] = on_behalf_of
        e.update({k: v for k, v in extra.items() if v is not None})
        return e

    def task(self, i):
        branch = i.get('branch', 'main')
        action = i.get('action')
        if action not in TASK_ACTIONS:
            raise _invalid('a task action is one of %s' % ', '.join(TASK_ACTIONS))
        principal = i.get('principal')
        if not isinstance(principal, str):
            raise _incomplete('principal')
        tid = i.get('task_id')
        if not isinstance(tid, str):
            raise _incomplete('task_id')
        st = self._state(branch)[0]
        if action == 'create':
            return self._task_create(branch, st, tid, principal, i)
        o, body = self._obj_of(st, tid, TASK)
        if i.get('base') is not None and i['base'] != o['head']:
            raise BpaError('TXN.HEAD_MOVED', {'object_id': tid})
        q = None
        if body.get('queue'):
            q = self._obj_of(st, body['queue'], QUEUE)[1]
        manager = bool(q and self._roles_of(principal) & set(q.get('managers') or []))
        acting, behalf = principal, None
        if i.get('delegation') is not None and action in ('claim', 'release', 'complete'):
            d = self._use_delegation(st, i['delegation'], principal, task_type=body['type'])
            acting, behalf = d['delegator'], d['delegator']
            roles = set(d['roles'])
        else:
            roles = self._roles_of(principal)

        def need(states):
            if body['state'] not in states:
                raise BpaError('FLOW.TRANSITION_NOT_ALLOWED', {'state': body['state'], 'action': action})

        if action == 'claim':
            need(('open',))
            a = body.get('assignee') or {}
            ok = (a.get('principal') == acting) or (a.get('role') in roles) or \
                (not a.get('principal') and not a.get('role') and q is not None and acting in q['members'])
            if not ok:
                raise BpaError('IAM.DENIED', {'task': 'not_eligible', 'principal': principal})
            body.update(state='claimed', owner=principal)
            if behalf:
                body['on_behalf_of'] = behalf
            body['log'].append(self._log_entry('claim', principal, behalf))
        elif action in ('release', 'complete'):
            need(('claimed',))
            if body.get('owner') != principal:
                raise BpaError('IAM.DENIED', {'task': 'not_owner', 'principal': principal})
            if action == 'release':
                body['state'] = 'open'
                body.pop('owner', None)
                body.pop('on_behalf_of', None)
                body['log'].append(self._log_entry('release', principal, behalf))
            else:
                body.update(state='completed', completed_at=self.now)
                if i.get('outcome') is not None:
                    body['outcome'] = i['outcome']
                body['log'].append(self._log_entry('complete', principal, behalf))
        elif action == 'reassign':
            need(('open', 'claimed'))
            if not manager:
                raise BpaError('IAM.DENIED', {'task': 'not_manager', 'principal': principal})
            to = _target(i.get('to'), ('principal', 'role', 'queue'))
            if 'queue' in to:
                self._obj_of(st, to['queue'], QUEUE)
                body['queue'] = to['queue']
                body.pop('assignee', None)
            else:
                body['assignee'] = to
            body['state'] = 'open'
            body.pop('owner', None)
            body.pop('on_behalf_of', None)
            body.pop('assigned_by', None)
            body['log'].append(self._log_entry('reassign', principal, to=to, reason=i.get('reason')))
        else:   # cancel
            need(('open', 'claimed'))
            if not manager and principal != body['created_by']:
                raise BpaError('IAM.DENIED', {'task': 'not_manager', 'principal': principal})
            body['state'] = 'cancelled'
            body['log'].append(self._log_entry('cancel', principal, reason=i.get('reason')))
        self._kernel_write(branch, TASK, [{'op': 'update', 'object_id': tid, 'base': o['head'], 'body': body}],
                           principal)
        return self._task_view(tid, body)

    def _task_create(self, branch, st, tid, principal, i):
        if not isinstance(i.get('type'), str):
            raise _incomplete('a task has a type')
        if i.get('queue') is None and i.get('assignee') is None:
            raise _incomplete('a task has a queue or an assignee')
        prio = i.get('priority', 0)
        if not isinstance(prio, int) or isinstance(prio, bool):
            raise _invalid('a priority is an integer')
        body = {'type': i['type'], 'state': 'open', 'priority': prio, 'created_by': principal,
                'created_at': self.now, 'escalations': [], 'log': [self._log_entry('create', principal)]}
        for k in ('subject', 'data'):
            if i.get(k) is not None:
                body[k] = i[k]
        if i.get('skills') is not None:
            if not isinstance(i['skills'], list) or any(not isinstance(s, str) for s in i['skills']):
                raise _invalid('skills are a list of names')
            body['skills'] = i['skills']
        if i.get('assignee') is not None:
            body['assignee'] = _target(i['assignee'], ('principal', 'role'))
        q = None
        if i.get('queue') is not None:
            q = self._obj_of(st, i['queue'], QUEUE)[1]
            body['queue'] = i['queue']
            if 'assignee' not in body:
                m = self._assign(st, i['queue'], q, body)
                if m is not None:
                    body['assignee'] = {'principal': m}
                    body['assigned_by'] = q['assignment']
        if i.get('due') is not None:
            body['due'] = _instant(i['due'], 'due')
        elif q is not None and q.get('due_rule'):
            due, due_date = self._due_of(q, body)
            body['due'] = due
            if due_date:
                body['due_date'] = due_date
        self._kernel_write(branch, TASK, [{'op': 'create', 'object_id': tid, 'class': TASK,
                                           'slug': 'task-%s' % tid[:8], 'body': body}], principal)
        if not hasattr(self, 'work_branches'):
            self.work_branches = []
        if branch not in self.work_branches:
            self.work_branches.append(branch)
        return self._task_view(tid, body)

    def _pc_eligible(self, rec, principal):
        """STD-LGS-062: whether an approval by `principal` would pass STD-LGS-041 and -042."""
        step = 'approval' if any(r['step'] == 'approval' for r in rec['requirements']) else \
            rec['requirements'][0]['step'] if rec['requirements'] else 'approval'
        req = next((r for r in rec['requirements'] if r['step'] == step), {'step': step, 'object_id': None})
        try:
            self._eligible(rec, req, {'roles': sorted(self._roles_of(principal))}, principal)
        except BpaError:
            return False
        return True

    def inbox(self, i):
        branch = i.get('branch', 'main')
        p = i.get('principal')
        if not isinstance(p, str):
            raise _incomplete('principal')
        st = self._state(branch)[0]
        roles = self._roles_of(p)
        queues = {oid: b for oid, _, b in self._live_of(st, QUEUE)}
        delegated = []
        for oid, _, d in self._live_of(st, DELEGATION):
            if d['delegate'] == p and self._delegation_state(d) == 'active' and d['scope'].get('task_types') is not None:
                delegated.append(d)
        tasks = []
        for tid, _, b in self._live_of(st, TASK):
            if b['state'] not in ('open', 'claimed'):
                continue
            a = b.get('assignee') or {}
            item = None
            if b.get('owner') == p or (b.get('owner') is None and a.get('principal') == p):
                item = self._task_view(tid, b)
            elif b['state'] == 'open' and a.get('role') in roles:
                item = self._task_view(tid, b)
            elif b['state'] == 'open' and not a and b.get('queue') in queues and p in queues[b['queue']]['members']:
                item = self._task_view(tid, b)
            else:
                for d in delegated:
                    if b['type'] not in d['scope']['task_types']:
                        continue
                    who = d['delegator']
                    if (b['state'] == 'open' and (a.get('principal') == who or a.get('role') in d['roles'] or
                                                  (not a and b.get('queue') in queues and who in queues[b['queue']]['members']))):
                        item = dict(self._task_view(tid, b), on_behalf_of=who)
                        break
            if item is not None:
                tasks.append(item)
        far = '9999-12-31T23:59:59.999999Z'
        if i.get('sort') == 'priority':
            tasks.sort(key=lambda t: (-t['priority'], t.get('due') or far, t['task_id']))
        else:
            tasks.sort(key=lambda t: (t.get('due') or far, -t['priority'], t['task_id']))
        approvals = []
        for pcid in sorted(getattr(self, 'pending_changes', {})):
            rec = self.pending_changes[pcid]
            if rec['target_branch'] != branch:
                continue
            if rec['state'] == 'pending':
                self._pc_refresh(rec)
            if rec['state'] == 'pending' and self._pc_eligible(rec, p):
                approvals.append({'pending_change_id': pcid, 'requirements': copy.deepcopy(rec['requirements']),
                                  'preview': rec['preview']})
        overdue = sum(1 for t in tasks if t.get('due') and self.now >= t['due'])
        return {'tasks': tasks, 'approvals': approvals,
                'counts': {'tasks': len(tasks), 'overdue': overdue, 'approvals': len(approvals)}}

    # ---- escalation (STD-LGS-063) ----

    def _task_escalations(self):
        due = []
        for branch in getattr(self, 'work_branches', []):
            if branch not in self.branches or self.branches[branch].get('archived'):
                continue
            st = self._state(branch)[0]
            queues = {oid: b for oid, _, b in self._live_of(st, QUEUE)}
            for tid, _, b in self._live_of(st, TASK):
                if b['state'] not in ('open', 'claimed') or not b.get('due') or b.get('queue') not in queues:
                    continue
                done = {e['step'] for e in b.get('escalations') or []}
                for k, s in enumerate(queues[b['queue']].get('escalation') or []):
                    at = _plus(b['due'], s['after'])
                    if k not in done and at <= self.now:
                        due.append((at, branch, tid, k))
        for at, branch, tid, k in sorted(due):
            st = self._state(branch)[0]
            o, b = self._obj_of(st, tid, TASK)
            if b['state'] not in ('open', 'claimed') or k in {e['step'] for e in b['escalations']}:
                continue
            s = self._obj_of(st, b['queue'], QUEUE)[1]['escalation'][k]
            rec = {'step': k, 'action': s['action'], 'at': at}
            if s['action'] == 'reassign':
                to = dict(s['to'])
                if 'queue' in to:
                    b['queue'] = to['queue']
                    b.pop('assignee', None)
                else:
                    b['assignee'] = to
                b['state'] = 'open'
                b.pop('owner', None)
                b.pop('on_behalf_of', None)
                b.pop('assigned_by', None)
            elif s['action'] == 'raise_priority':
                b['priority'] = s['priority']
            if s.get('to') is not None:
                rec['to'] = s['to']
            b['escalations'].append(rec)
            b['log'].append({'action': 'escalate', 'step': k, 'escalation': s['action'], 'at': at})
            res = self._kernel_write(branch, TASK, [{'op': 'update', 'object_id': tid, 'base': o['head'], 'body': b}])
            payload = {'task_id': tid, 'step': k, 'action': s['action']}
            if s.get('to') is not None:
                payload['to'] = s['to']
            self.outbox.append({'effect_id': 'task-escalation/%s/%d' % (tid, k), 'type': 'task.escalated',
                                'branch': branch, 'cseq': res['cseq'], 'object_id': tid, 'payload': payload})

    # ---- service-level report (STD-LGS-064) ----

    def sla_report(self, i):
        branch = i.get('branch', 'main')
        frm, to = i.get('from'), i.get('to')
        for v, n in ((frm, 'from'), (to, 'to')):
            if not isinstance(v, str) or not DATE.match(v):
                raise _invalid('%s is a date' % n)
        st = self._state(branch)[0]
        rows = {}

        def row(b):
            k = (b['type'], b.get('queue') or '')
            return rows.setdefault(k, {'type': b['type'], 'queue': b.get('queue'), 'completed': 0, 'on_time': 0,
                                       'late': 0, 'open_overdue': 0})
        for _, _, b in self._live_of(st, TASK):
            if not b.get('due'):
                continue
            if b['state'] == 'completed' and frm <= b['completed_at'][:10] < to:
                r = row(b)
                r['completed'] += 1
                r['on_time' if b['completed_at'] <= b['due'] else 'late'] += 1
            elif b['state'] in ('open', 'claimed') and self.now >= b['due']:
                row(b)['open_overdue'] += 1

        def rate(r):
            n = r['on_time'] + r['late']
            if not n:
                return None
            return fmt_decimal((Decimal(r['on_time']) / Decimal(n)).quantize(Decimal('0.0001'), ROUND_HALF_EVEN), 4)
        out = []
        total = {'completed': 0, 'on_time': 0, 'late': 0, 'open_overdue': 0}
        for k in sorted(rows):
            r = rows[k]
            r['attainment'] = rate(r)
            out.append(r)
            for f in total:
                total[f] += r[f]
        total['attainment'] = rate(total)
        return {'from': frm, 'to': to, 'rows': out, 'total': total}
