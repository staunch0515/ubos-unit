"""Workflows, sagas and quarantine of BPA 1.0-rc8 (PH-2 step 1d, DEC-092): workflow
definitions and instances with events, timers, human tasks, decisions, parallel branches,
sub-workflows and migration (STD-TXN-090, -091), sagas (STD-TXN-080) and the quarantine of
logic asset versions (STD-CTX-031). Mixed into the reference store."""
import copy
import datetime

from . import BpaError
from .canon import canonical, content_id
from .values import validate_value
from .periods import derived_uuid

WF_DEF, WF_INST = 'WorkflowDefinition', 'WorkflowInstance'
SAGA = 'Saga'
QUAR, QREL = 'Quarantine', 'QuarantineRelease'
STEP_TYPES = ('action', 'wait_event', 'timer', 'human_task', 'decision', 'parallel', 'join', 'sub_workflow', 'end')
OUTCOMES = ('completed', 'failed')
TERMINAL = ('completed', 'failed')
MAX_STEPS = 1000
# STD-TXN-080: the members of a saga that only the saga's execution writes
SAGA_STATE = ('status', 'completed', 'compensated', 'current', 'error', 'log')
SAGA_END = ('completed', 'compensated', 'failed')
FMT = '%Y-%m-%dT%H:%M:%S.%fZ'


def _shift(now, duration, sign=1):
    from .expr import parse_duration
    us = parse_duration(duration).us
    t = datetime.datetime.strptime(now, FMT) + sign * datetime.timedelta(microseconds=us)
    return t.strftime(FMT)


def _incomplete(what):
    return BpaError('MODEL.DECLARATION_INCOMPLETE', what)


def _invalid(what):
    return BpaError('MODEL.INVALID_VALUE', what)


def check_definition(d):
    """DAT-WorkflowDefinition (STD-TXN-090): typed variables, a start step and steps whose
    successors exist."""
    vs = d.get('variables') or []
    if not isinstance(vs, list) or not all(isinstance(v, dict) and isinstance(v.get('name'), str)
                                           and isinstance(v.get('type'), str) for v in vs):
        raise _incomplete('variables are {name, type} items')
    names = {v['name'] for v in vs}
    steps = d.get('steps')
    if not isinstance(steps, dict) or not steps:
        raise _incomplete('steps')
    if d.get('start') not in steps:
        raise _invalid('start names a step')

    def succ(s, k, required=True):
        if s.get(k) is None and not required:
            return
        if s.get(k) not in steps:
            raise _invalid('%s names a step' % k)

    for name, s in sorted(steps.items()):
        if not isinstance(s, dict) or s.get('type') not in STEP_TYPES:
            raise _invalid('step %s: the type is one of %s' % (name, ', '.join(STEP_TYPES)))
        t = s['type']
        if s.get('result') is not None and s['result'] not in names:
            raise _invalid('step %s: result names a variable' % name)
        if t in ('action', 'wait_event', 'timer', 'human_task', 'join', 'sub_workflow'):
            succ(s, 'next')
        if t == 'action':
            if not isinstance(s.get('target'), str) or not isinstance(s.get('port'), str):
                raise _incomplete('step %s: an action names target and port' % name)
            if not isinstance(s.get('params') or {}, dict):
                raise _invalid('step %s: params are a map of expressions' % name)
            r = s.get('retries', 0)
            if not isinstance(r, int) or isinstance(r, bool) or r < 0:
                raise _invalid('step %s: retries is a non-negative integer' % name)
            succ(s, 'on_error', False)
        elif t == 'wait_event':
            if not isinstance(s.get('event_type'), str) or not isinstance(s.get('correlation'), str):
                raise _incomplete('step %s: a wait names event_type and correlation' % name)
            if s.get('buffer') is not None:
                from .expr import parse_duration
                if not isinstance(s['buffer'], str):
                    raise _invalid('step %s: buffer is a duration' % name)
                parse_duration(s['buffer'])
        elif t == 'timer':
            from .expr import parse_duration
            if not isinstance(s.get('after'), str):
                raise _incomplete('step %s: a timer names after' % name)
            parse_duration(s['after'])
        elif t == 'human_task':
            if s.get('assignee') is not None and not isinstance(s['assignee'], str):
                raise _invalid('step %s: assignee is a principal' % name)
        elif t == 'decision':
            succ(s, 'default')
            if s.get('table') is not None:
                if not isinstance(s.get('inputs'), dict) or not isinstance(s.get('output'), str) \
                        or not isinstance(s.get('branches'), dict):
                    raise _incomplete('step %s: a table decision names inputs, output and branches' % name)
                for nxt in s['branches'].values():
                    if nxt not in steps:
                        raise _invalid('step %s: a branch names a step' % name)
            else:
                cs = s.get('cases')
                if not isinstance(cs, list) or not cs or not all(
                        isinstance(c, dict) and isinstance(c.get('when'), str) for c in cs):
                    raise _incomplete('step %s: a decision has cases {when, next} or a table' % name)
                for c in cs:
                    succ(c, 'next')
        elif t == 'parallel':
            br = s.get('branches')
            if not isinstance(br, list) or len(br) < 2 or not all(b in steps for b in br):
                raise _invalid('step %s: a parallel step names two or more branch steps' % name)
            if s.get('join') not in steps or steps[s['join']].get('type') != 'join':
                raise _invalid('step %s: a parallel step names its join step' % name)
        elif t == 'sub_workflow':
            if not isinstance(s.get('definition'), str):
                raise _incomplete('step %s: a sub-workflow names its definition' % name)
            for k in ('inputs', 'outputs'):
                if not isinstance(s.get(k) or {}, dict):
                    raise _invalid('step %s: %s are a map' % (name, k))
        elif t == 'end':
            if s.get('outcome', 'completed') not in OUTCOMES:
                raise _invalid('step %s: the outcome is completed or failed' % name)


def check_saga(b):
    """DAT-Saga (STD-TXN-080): named steps, each a process spec with an optional compensation."""
    steps = b.get('steps')
    if not isinstance(steps, list) or not steps:
        raise _incomplete('a saga lists its steps')
    seen = set()
    for s in steps:
        if not isinstance(s, dict) or not isinstance(s.get('name'), str) or s['name'] in seen:
            raise _invalid('saga steps have unique names')
        seen.add(s['name'])
        for k in ('process', 'compensation'):
            spec = s.get(k)
            if spec is None and k == 'compensation':
                continue
            if not isinstance(spec, dict) or not isinstance(spec.get('target'), str) or \
                    not isinstance(spec.get('port'), str) or not isinstance(spec.get('params') or {}, dict):
                raise _incomplete('a saga step %s is {target, port, params}' % k)
    if b.get('recovery', 'backward') not in ('backward', 'forward'):
        raise _invalid('recovery is backward or forward')
    m = b.get('max_attempts', 1)
    if not isinstance(m, int) or isinstance(m, bool) or m < 1:
        raise _invalid('max_attempts is a positive integer')


class _Interrupted(Exception):
    pass


class Workflows:

    def _orch_init(self):
        self.wf_events = {}        # STD-TXN-091: event id -> {hash, result}
        self.wf_results = {}       # (instance, token, step, visit, attempt) -> outcome of an action
        self.wf_instances = []     # (branch, instance id) in start order
        self.wf_buffer = {}        # STD-TXN-091: branch -> events that arrived before a wait
        self.saga_results = {}     # (saga, step, attempt, phase) -> outcome of a step process
        self.qpolicy = None        # STD-CTX-031
        self.qfailures = {}        # asset version -> [instants]
        self.quarantined = {}      # asset version -> quarantine object id
        self._sys_ok = ()

    # ---- kernel-written objects ----

    def _sys_commit(self, branch, changes, classes, process=None):
        prev = self._sys_ok
        self._sys_ok = tuple(classes)
        try:
            return self.commit(branch, changes, dict(process or {}, kind='kernel'))
        finally:
            self._sys_ok = prev

    def _kernel_fields(self, cls, body):
        """STD-TXN-080: the state of a saga is written by its execution only."""
        if cls == SAGA and SAGA not in self._sys_ok and isinstance(body, dict) and \
                any(k in body for k in SAGA_STATE):
            raise BpaError('FLOW.STATE_DIRECT_WRITE', 'the state of a saga is written by its execution')

    def _check_special(self, cls, body):
        if cls == WF_DEF:
            check_definition(body)
        elif cls == SAGA:
            check_saga(body)
        elif cls == 'Lens':
            from .lenses import check_lens
            check_lens(body)
        elif cls == 'Connector':        # STD-ISA-090 (1.0-rc8)
            from .integration import check_connector
            check_connector(body)
            self._check_egress(body)
        elif cls == 'NotificationTemplate':
            from .integration import check_template
            check_template(body)
        elif cls == 'RuleTest':         # STD-LGS-072 (1.0-rc11)
            from .ruleops import check_test
            check_test(body)
        elif cls == 'WorkQueue':        # STD-LGS-062 (1.0-rc10)
            from .work import check_queue
            check_queue(body)

    def _ref_oid(self, st, ref, cls):
        oid = ref
        if isinstance(ref, str) and ref.startswith('ubos://'):
            parts = ref.split('/')
            oid = next((k for k, o in sorted(st['objects'].items())
                        if o['class'] == parts[3] and o['slug'] == parts[4]), None)
        o = st['objects'].get(oid) if isinstance(oid, str) else None
        if o is None or o['class'] != cls or self.versions[o['head']]['tombstone']:
            raise BpaError('IAM.NOT_FOUND', 'unknown %s' % cls)
        return oid, o

    # ---- workflows (STD-TXN-090, -091) ----

    def _wx(self, d, text, variables):
        """An L1 expression of a workflow over its variables (`params`)."""
        from .expr import eval_expr
        types = {v['name']: v for v in d.get('variables') or []}
        return eval_expr({'expr': text, 'params_types': types,
                          'params': {k: v for k, v in variables.items() if v is not None and k in types}})['value']

    def _wf_set(self, d, variables, name, value):
        f = next(v for v in d.get('variables') or [] if v['name'] == name)
        variables[name] = None if value is None else validate_value(f['type'], f.get('constraints'), value)

    def _wf_vars(self, d, given):
        if not isinstance(given, dict):
            raise _invalid('variables are a map')
        decl = {v['name']: v for v in d.get('variables') or []}
        if any(k not in decl for k in given):
            raise _invalid('unknown variable')
        out = {}
        for k in sorted(decl):
            self._wf_set(d, out, k, given.get(k, decl[k].get('default')))
        return out

    def _wf_body(self, branch, iid):
        st = self._state(branch)[0]
        o = st['objects'].get(iid)
        if o is None or o['class'] != WF_INST:
            raise BpaError('IAM.NOT_FOUND', 'unknown workflow instance')
        return o['head'], copy.deepcopy(self.versions[o['head']]['body'])

    def _wf_save(self, branch, iid, head, body, process=None):
        self._sys_commit(branch, [{'op': 'update', 'object_id': iid, 'base': head, 'body': body}], (WF_INST,), process)

    def _token(self, body, step, forks=()):
        n = body['next_token']
        body['next_token'] = n + 1
        tok = {'id': 't%d' % n, 'step': step, 'attempt': 1, 'forks': list(forks)}
        self._visit(body, tok, step)
        body['tokens'].append(tok)
        return tok

    def _visit(self, body, tok, step):
        visit = body['visits'].get(step, 0) + 1
        body['visits'][step] = visit
        body['seq'] = body.get('seq', 0) + 1
        tok.update(step=step, visit=visit, attempt=1, seq=body['seq'])
        tok.pop('wait', None)

    def _log(self, body, tok, outcome, **extra):
        e = {'step': tok['step'], 'visit': tok['visit'], 'token': tok['id'], 'outcome': outcome}
        e.update(extra)
        body['log'].append(e)

    def _end(self, body, status, error=None):
        body['status'] = status
        body['tokens'] = []
        if error is not None:
            body['error'] = error

    def start_workflow(self, i):
        """STD-TXN-090: start an instance pinned to the definition's current version."""
        branch = i.get('branch', 'main')
        self._check_active(branch)
        iid = self._wf_create(branch, i.get('definition'), i.get('instance_id'), i.get('variables') or {},
                              (i.get('process') or {}).get('initiator'), None)
        self._wf_run(branch, iid)
        return self._wf_view(branch, iid)

    def _wf_create(self, branch, ref, iid, variables, initiator, parent):
        from . import store as _store
        st = self._state(branch)[0]
        doid, o = self._ref_oid(st, ref, WF_DEF)
        d = self.versions[o['head']]['body']
        check_definition(d)
        if not isinstance(iid, str):
            raise _incomplete('instance_id')
        body = {'definition': doid, 'definition_version': o['head'], 'status': 'running',
                'variables': self._wf_vars(d, variables), 'tokens': [], 'forks': {}, 'visits': {}, 'done': {},
                'log': [], 'next_token': 1, 'initiator': initiator or _store.PRINCIPAL, 'migrations': []}
        if parent:
            body['parent'] = parent
        self._token(body, d['start'])
        self._sys_commit(branch, [{'op': 'create', 'object_id': iid, 'class': WF_INST, 'slug': 'wf-' + iid,
                                   'body': body}], (WF_INST,))
        self.wf_instances.append((branch, iid))
        return iid

    def _wf_run(self, branch, iid):
        """Advance an instance until each of its tokens waits or it ends; every step is one
        committed state change."""
        for _ in range(MAX_STEPS):
            head, body = self._wf_body(branch, iid)
            if body['status'] != 'running':
                break
            tok = next((t for t in body['tokens'] if 'wait' not in t), None)
            if tok is None:
                return
            self._wf_step(branch, iid, head, body, tok)
        else:
            raise BpaError('LOGIC.LIMIT_EXCEEDED', 'workflow steps')
        if body.get('parent'):
            self._wf_child_done(branch, iid, body)

    def _wf_step(self, branch, iid, head, body, tok):
        d = self.versions[body['definition_version']]['body']
        since = self.commits[self._branch(branch)['head']]['cseq'] + 1
        s = d['steps'][tok['step']]
        t = s['type']
        if t == 'action':
            key = (iid, tok['id'], tok['step'], tok['visit'], tok['attempt'])
            if key not in self.wf_results:
                params = {k: self._wx(d, e, body['variables']) for k, e in sorted((s.get('params') or {}).items())}
                proc = {'kind': 'job', 'initiator': body['initiator'],
                        'workflow': {'instance': iid, 'step': tok['step'], 'visit': tok['visit'],
                                     'attempt': tok['attempt']}}
                try:
                    out = self.execute({'branch': branch, 'target': s['target'], 'port': s['port'],
                                        'params': params, 'process': proc})
                    self.wf_results[key] = ('ok', out['result'])
                except BpaError as e:
                    self.wf_results[key] = ('error', e.code)
            kind, res = self.wf_results[key]
            if kind == 'ok':
                if s.get('result'):
                    self._wf_set(d, body['variables'], s['result'], res)
                self._log(body, tok, 'ok', attempt=tok['attempt'])
                self._visit(body, tok, s['next'])
            else:
                self._log(body, tok, 'error', attempt=tok['attempt'], code=res)
                if tok['attempt'] <= s.get('retries', 0):
                    tok['attempt'] += 1
                elif s.get('on_error'):
                    self._visit(body, tok, s['on_error'])
                else:
                    self._end(body, 'failed', {'step': tok['step'], 'code': res})
        elif t == 'decision':
            if s.get('table') is not None:
                from .tables import evaluate
                from .store import tl_at
                st = self._state(branch)[0]
                _, o = self._table(st, s['table'])
                seg = tl_at(o['timeline'], self._today())
                if seg is None:
                    raise BpaError('QRY.NOT_FOUND_AT_TIME', 'no version of the table today')
                inputs = {k: self._wx(d, e, body['variables']) for k, e in s['inputs'].items()}
                out = evaluate(self.versions[seg[2]]['body'], inputs)['outputs']
                value = out.get(s['output']) if isinstance(out, dict) else None
                nxt = s['branches'].get(value if isinstance(value, str) else canonical(value), s['default'])
            else:
                nxt = next((c['next'] for c in s['cases'] if self._wx(d, c['when'], body['variables']) is True),
                           s['default'])
            self._log(body, tok, 'ok', next=nxt)
            self._visit(body, tok, nxt)
        elif t == 'parallel':
            fid = '%s/%d' % (tok['id'], tok['visit'])
            body['forks'][fid] = {'join': s['join'], 'n': len(s['branches']), 'arrived': [], 'fired': False}
            self._log(body, tok, 'ok', fork=fid)
            body['tokens'].remove(tok)
            for b in s['branches']:
                self._token(body, b, tok['forks'] + [fid])
        elif t == 'join':
            fid = tok['forks'][-1] if tok['forks'] else None
            fork = body['forks'].get(fid)
            body['tokens'].remove(tok)
            if fork is None or fork['join'] != tok['step']:
                self._end(body, 'failed', {'step': tok['step'], 'code': 'MODEL.INVALID_VALUE'})
            else:
                fork['arrived'].append(tok['id'])
                self._log(body, tok, 'arrived', fork=fid)
                if len(fork['arrived']) == fork['n'] and not fork['fired']:
                    fork['fired'] = True
                    self._token(body, s['next'], tok['forks'][:-1])
        elif t == 'end':
            self._log(body, tok, s.get('outcome', 'completed'))
            self._end(body, s.get('outcome', 'completed'))
        elif t == 'wait_event':
            key = self._wx(d, s['correlation'], body['variables'])
            ev = self._buffered(branch, s, key)
            if ev is None:
                tok['wait'] = {'event_type': s['event_type'], 'key': key, 'since': since}
            else:
                if s.get('result'):
                    self._wf_set(d, body['variables'], s['result'], ev.get('payload'))
                self._log(body, tok, 'event', event_id=ev['event_id'], buffered=True)
                self._visit(body, tok, s['next'])
        elif t == 'timer':
            tok['wait'] = {'due': _shift(self.now, s['after']), 'since': since}
        elif t == 'human_task':
            tok['wait'] = {'task': True, 'since': since}
            if s.get('assignee'):
                tok['wait']['assignee'] = s['assignee']
        elif t == 'sub_workflow':
            child = derived_uuid('ubos.workflow.child', iid, tok['id'], tok['step'], tok['visit'])
            tok['wait'] = {'child': child, 'since': since}
            self._wf_save(branch, iid, head, body)
            inputs = {k: self._wx(d, e, body['variables']) for k, e in sorted((s.get('inputs') or {}).items())}
            self._wf_create(branch, s['definition'], child, inputs, body['initiator'],
                            {'instance': iid, 'token': tok['id']})
            self._wf_run(branch, child)
            return
        self._wf_save(branch, iid, head, body)

    def _buffered(self, branch, s, key):
        """STD-TXN-091: the oldest event of the step's type and key that arrived before the
        wait within the step's buffer window; it is consumed."""
        if s.get('buffer') is None:
            return None
        since = _shift(self.now, s['buffer'], -1)
        for ev in self.wf_buffer.get(branch, []):
            if not ev.get('consumed') and ev['event_type'] == s['event_type'] and \
                    canonical(ev['key']) == canonical(key) and ev['at'] >= since:
                ev['consumed'] = True
                self.wf_events[ev['event_id']]['result'] = {'woken': None, 'buffered': True, 'consumed': True}
                return ev
        return None

    def _wf_child_done(self, branch, cid, child):
        """A sub-workflow that ended moves its parent's waiting token."""
        pid = child['parent']['instance']
        head, body = self._wf_body(branch, pid)
        tok = next((t for t in body['tokens'] if (t.get('wait') or {}).get('child') == cid), None)
        if tok is None or body['status'] != 'running':
            return
        d = self.versions[body['definition_version']]['body']
        s = d['steps'][tok['step']]
        if child['status'] == 'completed':
            for pv, cv in sorted((s.get('outputs') or {}).items()):
                self._wf_set(d, body['variables'], pv, child['variables'].get(cv))
            self._log(body, tok, 'ok', child=cid)
            self._visit(body, tok, s['next'])
        else:
            self._log(body, tok, 'error', child=cid)
            self._end(body, 'failed', {'step': tok['step'], 'child': cid})
        self._wf_save(branch, pid, head, body)
        self._wf_run(branch, pid)

    def _wf_view(self, branch, iid):
        _, body = self._wf_body(branch, iid)
        d = self.versions[body['definition_version']]['body']
        waiting = []
        for t in body['tokens']:
            w = dict(t.get('wait') or {})
            w.pop('task', None)
            waiting.append(dict(w, step=t['step'], type=d['steps'][t['step']]['type'], visit=t['visit']))
        out = {'instance_id': iid, 'status': body['status'], 'definition_version': body['definition_version'],
               'variables': body['variables'], 'waiting': waiting}
        if body.get('error'):
            out['error'] = body['error']
        return out

    def deliver_event(self, i):
        """STD-TXN-091: an event wakes at most one step waiting for its type and correlation
        key, the one waiting longest; a repeated event id returns the first result."""
        branch = i.get('branch', 'main')
        eid = i.get('event_id')
        if not isinstance(eid, str) or not isinstance(i.get('event_type'), str) or 'key' not in i:
            raise _incomplete('an event has event_id, event_type and key')
        h = content_id('ubos.chunk.v1', {k: i.get(k) for k in ('branch', 'event_type', 'key', 'payload')})
        known = self.wf_events.get(eid)
        if known is not None:
            if known['hash'] != h:
                raise BpaError('TXN.IDEMPOTENCY_MISMATCH')
            return dict(known['result'], duplicate=True)
        self._check_active(branch)
        best = None
        for b, iid in self.wf_instances:
            if b != branch:
                continue
            _, body = self._wf_body(branch, iid)
            if body['status'] != 'running':
                continue
            for t in body['tokens']:
                w = t.get('wait') or {}
                if w.get('event_type') == i['event_type'] and canonical(w.get('key')) == canonical(i['key']):
                    order = (w['since'], iid, t['id'])
                    if best is None or order < best:
                        best = order
        result = {'woken': None, 'buffered': True}
        if best is None:
            self.wf_buffer.setdefault(branch, []).append({'event_id': eid, 'event_type': i['event_type'],
                                                          'key': i['key'], 'payload': i.get('payload'),
                                                          'at': self.now})
        else:
            _, iid, tid = best
            head, body = self._wf_body(branch, iid)
            d = self.versions[body['definition_version']]['body']
            tok = next(t for t in body['tokens'] if t['id'] == tid)
            s = d['steps'][tok['step']]
            if s.get('result'):
                self._wf_set(d, body['variables'], s['result'], i.get('payload'))
            result = {'woken': {'instance_id': iid, 'step': tok['step'], 'visit': tok['visit']}}
            self._log(body, tok, 'event', event_id=eid)
            self._visit(body, tok, s['next'])
            self._wf_save(branch, iid, head, body)
            self._wf_run(branch, iid)
        self.wf_events[eid] = {'hash': h, 'result': result}
        return result

    def complete_task(self, i):
        """STD-TXN-091: complete a human task; the completion of (instance, step, visit) is
        idempotent."""
        branch = i.get('branch', 'main')
        iid, step = i.get('instance'), i.get('step')
        principal = i.get('principal')
        if not isinstance(principal, str):
            raise _incomplete('principal')
        head, body = self._wf_body(branch, iid)
        h = content_id('ubos.chunk.v1', {'principal': principal, 'data': i.get('data')})
        tok = next((t for t in body['tokens'] if t['step'] == step and (t.get('wait') or {}).get('task')
                    and i.get('visit', t['visit']) == t['visit']), None)
        visit = tok['visit'] if tok else i.get('visit', body['visits'].get(step))
        key = '%s/%s' % (step, visit)
        done = body['done'].get(key)
        if done is not None:
            if done['hash'] != h:
                raise BpaError('TXN.IDEMPOTENCY_MISMATCH')
            return dict(self._wf_view(branch, iid), duplicate=True)
        if tok is None or body['status'] != 'running':
            raise BpaError('FLOW.TRANSITION_NOT_ALLOWED', {'step': step, 'waiting': False})
        if tok['wait'].get('assignee') and tok['wait']['assignee'] != principal:
            raise BpaError('IAM.DENIED', 'the task is assigned to another principal')
        d = self.versions[body['definition_version']]['body']
        s = d['steps'][step]
        if s.get('result'):
            self._wf_set(d, body['variables'], s['result'], i.get('data'))
        body['done'][key] = {'principal': principal, 'hash': h}
        self._log(body, tok, 'completed', principal=principal)
        self._visit(body, tok, s['next'])
        self._wf_save(branch, iid, head, body, {'initiator': principal})
        self._wf_run(branch, iid)
        return self._wf_view(branch, iid)

    def _wf_timers(self):
        """STD-TXN-091: timers whose due time has come fire when the clock advances, in the
        order of their due time."""
        due = []
        for branch, iid in self.wf_instances:
            if branch not in self.branches or self.branches[branch].get('archived'):
                continue
            _, body = self._wf_body(branch, iid)
            if body['status'] != 'running':
                continue
            for t in body['tokens']:
                w = t.get('wait') or {}
                if w.get('due') and w['due'] <= self.now:
                    due.append((w['due'], branch, iid, t['id']))
        for _, branch, iid, tid in sorted(due):
            head, body = self._wf_body(branch, iid)
            tok = next((t for t in body['tokens'] if t['id'] == tid and 'wait' in t), None)
            if tok is None or body['status'] != 'running':
                continue
            d = self.versions[body['definition_version']]['body']
            self._log(body, tok, 'fired', due=tok['wait']['due'])
            self._visit(body, tok, d['steps'][tok['step']]['next'])
            self._wf_save(branch, iid, head, body)
            self._wf_run(branch, iid)

    def migrate_workflow(self, i):
        """STD-TXN-090: move a running instance to another version of its definition; every
        active step is mapped explicitly."""
        branch = i.get('branch', 'main')
        iid = i.get('instance')
        head, body = self._wf_body(branch, iid)
        if body['status'] != 'running':
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'the instance has ended')
        st = self._state(branch)[0]
        o = st['objects'][body['definition']]
        vh = i.get('version') or o['head']
        v = self.versions.get(vh)
        if v is None or v['object_id'] != body['definition'] or v['tombstone']:
            raise _invalid('version is a version of the instance definition')
        old = self.versions[body['definition_version']]['body']
        new = v['body']
        check_definition(new)
        smap = i.get('step_map') or {}
        if not isinstance(smap, dict):
            raise _invalid('step_map is a map')
        active = sorted({t['step'] for t in body['tokens']} |
                        {f['join'] for f in body['forks'].values() if not f['fired']})
        unmapped = [s for s in active if s not in smap]
        if unmapped:
            raise BpaError('MODEL.MIGRATION_REQUIRED', {'unmapped': unmapped})
        for s in active:
            if smap[s] not in new['steps'] or new['steps'][smap[s]]['type'] != old['steps'][s]['type']:
                raise _invalid('step %s maps to a step of the same type' % s)
        for t in body['tokens']:
            t['step'] = smap[t['step']]
            body['visits'][t['step']] = max(body['visits'].get(t['step'], 0), t['visit'])
        for f in body['forks'].values():
            if not f['fired']:
                f['join'] = smap[f['join']]
        body['migrations'].append({'from': body['definition_version'], 'to': vh,
                                   'step_map': {s: smap[s] for s in active}})
        body['definition_version'] = vh
        self._wf_save(branch, iid, head, body)
        self._wf_run(branch, iid)
        return self._wf_view(branch, iid)

    # ---- sagas (STD-TXN-080) ----

    def run_saga(self, i):
        """Run or resume a saga: each step is a process linked to the saga, every state
        change of the saga is committed, and a resumed saga repeats no step whose process
        committed. `interrupt_after` stops the run after that many commits (a simulated
        crash)."""
        branch = i.get('branch', 'main')
        self._check_active(branch)
        sid, o = self._ref_oid(self._state(branch)[0], i.get('saga'), SAGA)
        n = i.get('interrupt_after')
        if n is not None and (not isinstance(n, int) or isinstance(n, bool) or n < 1):
            raise _invalid('interrupt_after is a positive integer')
        self._saga_run = {'left': n, 'commits': 0, 'initiator': (i.get('process') or {}).get('initiator')}
        try:
            self._saga_loop(branch, sid)
        except _Interrupted:
            return {'saga_id': sid, 'status': 'interrupted', 'commits': self._saga_run['commits']}
        finally:
            run, self._saga_run = self._saga_run, None
        out = self._saga_view(branch, sid)
        out['commits'] = run['commits']
        return out

    def _saga_counted(self):
        r = self._saga_run
        r['commits'] += 1
        if r['left'] is not None:
            r['left'] -= 1
            if r['left'] == 0:
                raise _Interrupted()

    def _saga_body(self, branch, sid):
        o = self._state(branch)[0]['objects'][sid]
        return o['head'], copy.deepcopy(self.versions[o['head']]['body'])

    def _saga_set(self, branch, sid, head, changes):
        proc = {'initiator': self._saga_run['initiator']} if self._saga_run.get('initiator') else None
        self._sys_commit(branch, [{'op': 'update', 'object_id': sid, 'base': head, 'body': changes}], (SAGA,), proc)
        self._saga_counted()

    def _saga_call(self, branch, sid, spec, step, attempt, phase):
        key = (sid, step, attempt, phase)
        if key not in self.saga_results:
            proc = {'kind': 'job', 'saga': sid, 'saga_step': {'step': step, 'attempt': attempt, 'phase': phase}}
            if self._saga_run.get('initiator'):
                proc['initiator'] = self._saga_run['initiator']
            try:
                out = self.execute({'branch': branch, 'target': spec['target'], 'port': spec['port'],
                                    'params': spec.get('params') or {}, 'process': proc})
                self.saga_results[key] = ('ok', out['result'])
                if out.get('cseq') is not None:
                    self._saga_counted()
            except BpaError as e:
                self.saga_results[key] = ('error', e.code)
        return self.saga_results[key]

    def _saga_loop(self, branch, sid):
        for _ in range(MAX_STEPS):
            head, b = self._saga_body(branch, sid)
            steps = {s['name']: s for s in b['steps']}
            order = [s['name'] for s in b['steps']]
            status = b.get('status') or 'pending'
            log = b.get('log') or []
            cur = b.get('current') or {}
            if status == 'pending':
                self._saga_set(branch, sid, head, {'status': 'running', 'completed': [], 'compensated': [], 'log': []})
            elif status in SAGA_END:
                return
            elif status == 'running':
                done = b['completed']
                if len(done) == len(order):
                    self._saga_set(branch, sid, head, {'status': 'completed', 'current': None})
                    continue
                name = order[len(done)]
                if cur.get('step') != name or cur.get('phase') != 'forward':
                    self._saga_set(branch, sid, head, {'current': {'step': name, 'attempt': 1, 'phase': 'forward'}})
                    continue
                kind, res = self._saga_call(branch, sid, steps[name]['process'], name, cur['attempt'], 'forward')
                entry = {'step': name, 'attempt': cur['attempt'], 'phase': 'forward', 'outcome': kind}
                if kind == 'ok':
                    self._saga_set(branch, sid, head, {'completed': done + [name], 'current': None, 'log': log + [entry]})
                    continue
                entry['code'] = res
                if b.get('recovery', 'backward') == 'forward':
                    if cur['attempt'] < b.get('max_attempts', 1):
                        self._saga_set(branch, sid, head, {'current': dict(cur, attempt=cur['attempt'] + 1),
                                                           'log': log + [entry]})
                    else:
                        self._saga_set(branch, sid, head, {'status': 'failed', 'current': None, 'log': log + [entry],
                                                           'error': {'step': name, 'code': res,
                                                                     'attempts': cur['attempt']}})
                else:
                    self._saga_set(branch, sid, head, {'status': 'compensating', 'current': None, 'log': log + [entry],
                                                       'error': {'step': name, 'code': res}})
            elif status == 'compensating':
                todo = [n for n in reversed(b['completed']) if n not in b['compensated']]
                if not todo:
                    self._saga_set(branch, sid, head, {'status': 'compensated'})
                    continue
                name = todo[0]
                comp = steps[name].get('compensation')
                if comp is None:
                    self._saga_set(branch, sid, head, {'compensated': b['compensated'] + [name], 'log': log + [
                        {'step': name, 'phase': 'compensate', 'outcome': 'none'}]})
                    continue
                if cur.get('step') != name or cur.get('phase') != 'compensate':
                    self._saga_set(branch, sid, head, {'current': {'step': name, 'attempt': 1, 'phase': 'compensate'}})
                    continue
                kind, res = self._saga_call(branch, sid, comp, name, cur['attempt'], 'compensate')
                entry = {'step': name, 'attempt': cur['attempt'], 'phase': 'compensate', 'outcome': kind}
                if kind == 'ok':
                    self._saga_set(branch, sid, head, {'compensated': b['compensated'] + [name], 'current': None,
                                                       'log': log + [entry]})
                else:
                    entry['code'] = res
                    self._saga_set(branch, sid, head, {'status': 'failed', 'current': None, 'log': log + [entry],
                                                       'error': dict(b.get('error') or {}, compensation={
                                                           'step': name, 'code': res})})
        raise BpaError('LOGIC.LIMIT_EXCEEDED', 'saga steps')

    def _saga_view(self, branch, sid):
        _, b = self._saga_body(branch, sid)
        out = {'saga_id': sid, 'status': b.get('status') or 'pending', 'completed': b.get('completed') or [],
               'compensated': b.get('compensated') or []}
        if b.get('error'):
            out['error'] = b['error']
        return out

    # ---- quarantine (STD-CTX-031) ----

    def quarantine_policy(self, i):
        th, window, rel = i.get('threshold'), i.get('window'), i.get('releasers')
        if not isinstance(th, int) or isinstance(th, bool) or th < 1:
            raise _invalid('threshold is a positive integer')
        if not isinstance(window, str):
            raise _incomplete('window')
        from .expr import parse_duration
        parse_duration(window)
        if not isinstance(rel, list) or not rel or not all(isinstance(x, str) for x in rel):
            raise _incomplete('releasers are principals')
        self.qpolicy = {'threshold': th, 'window': window, 'releasers': sorted(rel)}
        return dict(self.qpolicy)

    def _q_check(self, vh):
        qid = self.quarantined.get(vh)
        if qid is not None:
            raise BpaError('LOGIC.ASSET_QUARANTINED', {'asset_version': vh, 'quarantine': qid})

    def _q_failure(self, run, code, outcome):
        """Count a failure or limit termination of the process's entry asset version and
        quarantine the version when the count in the window reaches the threshold."""
        if self.qpolicy is None or not run.logic or outcome not in ('error', 'limit') or \
                code == 'LOGIC.ASSET_QUARANTINED' or code.startswith('TXN.'):
            return
        vh = run.logic[0]
        if vh in self.quarantined:
            return
        since = _shift(self.now, self.qpolicy['window'], -1)
        q = [(t, k) for t, k in self.qfailures.get(vh, []) if t > since] + [(self.now, outcome)]
        self.qfailures[vh] = q
        if len(q) < self.qpolicy['threshold']:
            return
        n = sum(1 for x in self.versions.values() if x['class'] == QUAR and x['body'].get('asset_version') == vh)
        qid = derived_uuid('ubos.quarantine', vh, n)
        recent = [k for _, k in q]
        body = {'asset': self.versions[vh]['object_id'], 'asset_version': vh, 'failures': recent.count('error'),
                'limit_terminations': recent.count('limit'), 'window': self.qpolicy['window'],
                'threshold': self.qpolicy['threshold'], 'since': self.now, 'last_error': code}
        self._sys_commit(run.branch, [{'op': 'create', 'object_id': qid, 'class': QUAR, 'slug': 'q-' + qid,
                                       'body': body}], (QUAR,),
                         {'effects': [{'type': 'AssetQuarantined', 'object_id': qid,
                                       'payload': {'asset_version': vh, 'failures': len(q)}}]})
        self.quarantined[vh] = qid

    def release_quarantine(self, i):
        """STD-CTX-031: an authorised principal releases a quarantined asset version."""
        branch = i.get('branch', 'main')
        qid, o = self._ref_oid(self._state(branch)[0], i.get('quarantine'), QUAR)
        q = self.versions[o['head']]['body']
        if self.quarantined.get(q['asset_version']) != qid:
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'the quarantine is released')
        principal, reason = i.get('principal'), i.get('reason')
        if not isinstance(reason, str) or not reason or not isinstance(i.get('release_id'), str):
            raise _incomplete('a release states its release_id and reason')
        if self.qpolicy is None or principal not in self.qpolicy['releasers']:
            raise BpaError('IAM.DENIED', 'the principal may not release a quarantine')
        res = self._sys_commit(branch, [{'op': 'create', 'object_id': i['release_id'], 'class': QREL,
                                         'slug': 'r-' + i['release_id'],
                                         'body': {'quarantine': qid, 'asset_version': q['asset_version'],
                                                  'principal': principal, 'reason': reason,
                                                  'released_at': self.now}}], (QREL,), {'initiator': principal})
        del self.quarantined[q['asset_version']]
        self.qfailures[q['asset_version']] = []
        return {'quarantine': qid, 'asset_version': q['asset_version'], 'released': True, 'cseq': res['cseq']}
