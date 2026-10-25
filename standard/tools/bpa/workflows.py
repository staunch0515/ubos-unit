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
SAGA_STATE = ('status', 'completed', 'compensated', 'current', 'error', 'log', 'skipped')
SAGA_END = ('completed', 'compensated', 'failed')
SAGA_ACTIONS = ('retry', 'skip', 'cancel')      # STD-TXN-081 (1.0-rc12)
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
            if not isinstance(s.get('defer', False), bool):          # STD-TXN-093 (1.0-rc12)
                raise _invalid('step %s: defer is a boolean' % name)
            succ(s, 'on_reject', False)
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
            if (s.get('after') is None) == (s.get('at') is None):     # STD-TXN-094 (1.0-rc12)
                raise _incomplete('step %s: a timer names after or at' % name)
            cals = s.get('calendars')
            if cals is not None and (not isinstance(cals, dict) or not all(isinstance(c, dict) for c in cals.values())):
                raise _invalid('step %s: calendars map names to {weekend, holidays}' % name)
            if s.get('at') is not None:
                if not isinstance(s['at'], str):
                    raise _invalid('step %s: at is an expression' % name)
            elif not isinstance(s['after'], str):
                raise _invalid('step %s: after is a duration' % name)
            else:
                parse_duration(s['after'])
        elif t == 'human_task':
            if s.get('assignee') is not None and not isinstance(s['assignee'], str):
                raise _invalid('step %s: assignee is a principal' % name)
            for k in ('queue', 'task_type', 'subject'):              # STD-TXN-093 (1.0-rc12)
                if s.get(k) is not None and not isinstance(s[k], str):
                    raise _invalid('step %s: %s is text' % (name, k))
            p = s.get('priority', 0)
            if not isinstance(p, int) or isinstance(p, bool):
                raise _invalid('step %s: priority is an integer' % name)
            if s.get('data') is not None and (not isinstance(s['data'], list) or
                                              not all(x in names for x in s['data'])):
                raise _invalid('step %s: data lists variables' % name)
            succ(s, 'on_cancel', False)
        if t in ('wait_event', 'human_task') and (s.get('timeout') is not None or s.get('on_timeout') is not None):
            from .expr import parse_duration
            if s.get('timeout') is None or s.get('on_timeout') is None:   # STD-TXN-094 (1.0-rc12)
                raise _incomplete('step %s: a timeout names on_timeout' % name)
            if not isinstance(s['timeout'], str):
                raise _invalid('step %s: timeout is a duration' % name)
            parse_duration(s['timeout'])
            succ(s, 'on_timeout')
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
        if s.get('timeout') is not None:                            # STD-TXN-081 (1.0-rc12)
            from .expr import parse_duration
            if not isinstance(s['timeout'], str):
                raise _invalid('a saga step timeout is a duration')
            parse_duration(s['timeout'])
        if s.get('on_timeout', 'recover') not in ('recover', 'escalate'):
            raise _invalid('on_timeout is recover or escalate')
    ops = b.get('operators')
    if ops is not None and (not isinstance(ops, list) or not all(isinstance(x, str) for x in ops)):
        raise _invalid('operators are principals')
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
        self.input_requests = {}   # STD-ISA-101 (1.0-rc12): request id -> request
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
                inv = {'branch': branch, 'target': s['target'], 'port': s['port'], 'params': params, 'process': proc}
                if s.get('defer'):
                    inv['defer'] = True                 # STD-TXN-093 (1.0-rc12)
                try:
                    out = self.execute(inv)
                    if (out.get('pending') or {}).get('state') == 'pending':
                        self.wf_results[key] = ('pending', out['pending']['pending_change_id'])
                    else:
                        self.wf_results[key] = ('ok', out.get('result'))
                except BpaError as e:
                    self.wf_results[key] = ('error', e.code)
            kind, res = self.wf_results[key]
            if kind == 'pending':
                tok['wait'] = {'pending_change': res, 'since': since}
                self._log(body, tok, 'deferred', pending_change=res)
            elif kind == 'ok':
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
                self._wf_timeout(tok, s)
            else:
                if s.get('result'):
                    self._wf_set(d, body['variables'], s['result'], ev.get('payload'))
                self._log(body, tok, 'event', event_id=ev['event_id'], buffered=True)
                self._visit(body, tok, s['next'])
        elif t == 'timer':
            if s.get('at') is not None:                     # STD-TXN-094 (1.0-rc12)
                try:
                    due = self._wf_at(d, s, body['variables'], self.now)
                except BpaError as e:
                    self._end(body, 'failed', {'step': tok['step'], 'code': e.code})
                    self._wf_save(branch, iid, head, body)
                    return
                tok['wait'] = {'due': due, 'since': since, 'entered': self.now}
            else:
                tok['wait'] = {'due': _shift(self.now, s['after']), 'since': since}
        elif t == 'human_task':
            if s.get('queue') is not None:                  # STD-TXN-093 (1.0-rc12)
                try:
                    tid = self._wf_task(branch, iid, body, tok, s)
                except BpaError as e:
                    self._end(body, 'failed', {'step': tok['step'], 'code': e.code})
                    self._wf_save(branch, iid, head, body)
                    return
                tok['wait'] = {'task_id': tid, 'since': since}
            else:
                tok['wait'] = {'task': True, 'since': since}
            if s.get('assignee'):
                tok['wait']['assignee'] = s['assignee']
            self._wf_timeout(tok, s)
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

    # ---- tasks, approvals, timers and timeouts (STD-TXN-093, -094; 1.0-rc12) ----

    def _wf_at(self, d, s, variables, entered):
        """The due instant of a timer's `at` expression, entered at `entered`."""
        from .expr import eval_expr
        types = {v['name']: v for v in d.get('variables') or []}
        params = {k: v for k, v in variables.items() if v is not None and k in types}
        out = eval_expr({'expr': s['at'], 'params_types': types, 'params': params, 'ctx': {'now': entered},
                         'calendars': s.get('calendars') or {}})
        if out['type'] == 'instant':
            return out['value']
        if out['type'] == 'date':
            return out['value'] + 'T00:00:00.000000Z'
        raise _invalid('at gives an instant or a date')

    def _wf_timeout(self, tok, s):
        if s.get('timeout') is not None:
            tok['wait'].update(timeout_at=_shift(self.now, s['timeout']), entered=self.now)

    def _wf_task(self, branch, iid, body, tok, s):
        """The Task of a human_task step with a queue."""
        tid = derived_uuid('ubos.workflow.task', iid, tok['id'], tok['step'], tok['visit'])
        i = {'type': s.get('task_type') or tok['step'], 'queue': s['queue'], 'priority': s.get('priority', 0),
             'workflow': {'instance': iid, 'step': tok['step'], 'visit': tok['visit']}}
        if s.get('subject') is not None:
            i['subject'] = s['subject']
        if s.get('assignee'):
            i['assignee'] = {'principal': s['assignee']}
        if s.get('data') is not None:
            i['data'] = {k: body['variables'].get(k) for k in s['data']}
        self._task_create(branch, self._state(branch)[0], tid, body['initiator'], i)
        return tid

    def _wf_waiting(self, branch, match):
        """The running instance and token whose wait satisfies `match`."""
        for b, iid in self.wf_instances:
            if b != branch:
                continue
            head, body = self._wf_body(branch, iid)
            if body['status'] != 'running':
                continue
            tok = next((t for t in body['tokens'] if match(t.get('wait') or {})), None)
            if tok is not None:
                return iid, head, body, tok
        return None

    def _wf_task_done(self, branch, tid, task):
        """A workflow task that was completed or cancelled continues its instance."""
        w = self._wf_waiting(branch, lambda x: x.get('task_id') == tid)
        if w is None:
            return
        iid, head, body, tok = w
        d = self.versions[body['definition_version']]['body']
        s = d['steps'][tok['step']]
        if task['state'] == 'completed':
            if s.get('result'):
                self._wf_set(d, body['variables'], s['result'], task.get('outcome'))
            self._log(body, tok, 'completed', principal=task.get('owner'), task_id=tid)
            self._visit(body, tok, s['next'])
        elif s.get('on_cancel'):
            self._log(body, tok, 'cancelled', task_id=tid)
            self._visit(body, tok, s['on_cancel'])
        else:
            self._log(body, tok, 'cancelled', task_id=tid)
            self._end(body, 'failed', {'step': tok['step'], 'task_id': tid, 'state': 'cancelled'})
        self._wf_save(branch, iid, head, body, {'initiator': task.get('owner') or body['initiator']})
        self._wf_run(branch, iid)

    def _wf_pc_done(self, rec):
        """A decided pending change continues the step that waits for it."""
        if rec['state'] == 'pending':
            return
        pcid = rec['pending_change_id']
        w = self._wf_waiting(rec['target_branch'], lambda x: x.get('pending_change') == pcid)
        if w is None:
            return
        iid, head, body, tok = w
        branch = rec['target_branch']
        d = self.versions[body['definition_version']]['body']
        s = d['steps'][tok['step']]
        if rec['state'] == 'applied':
            if s.get('result'):
                self._wf_set(d, body['variables'], s['result'], {'pending_change_id': pcid, 'cseq': rec['cseq']})
            self._log(body, tok, 'applied', pending_change=pcid)
            self._visit(body, tok, s['next'])
        else:
            self._log(body, tok, rec['state'], pending_change=pcid)
            if s.get('on_reject'):
                self._visit(body, tok, s['on_reject'])
            else:
                self._end(body, 'failed', {'step': tok['step'], 'pending_change': pcid, 'state': rec['state']})
        self._wf_save(branch, iid, head, body)
        self._wf_run(branch, iid)

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
            w.pop('entered', None)
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
        qt = next((t for t in body['tokens'] if t['step'] == step and (t.get('wait') or {}).get('task_id')), None)
        if qt is not None:                                  # STD-TXN-093 (1.0-rc12)
            raise BpaError('FLOW.TRANSITION_NOT_ALLOWED', {'task_id': qt['wait']['task_id']})
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
                    due.append((w['due'], branch, iid, t['id'], 'due'))
                if w.get('timeout_at') and w['timeout_at'] <= self.now:     # STD-TXN-094 (1.0-rc12)
                    due.append((w['timeout_at'], branch, iid, t['id'], 'timeout_at'))
                pc = getattr(self, 'pending_changes', {}).get(w.get('pending_change'))
                if pc is not None and pc['state'] == 'pending' and pc.get('expires_at') and \
                        pc['expires_at'] <= self.now:                         # STD-TXN-093 (1.0-rc12)
                    due.append((pc['expires_at'], branch, iid, t['id'], 'pending_change'))
        for at, branch, iid, tid, kind in sorted(due):
            head, body = self._wf_body(branch, iid)
            tok = next((t for t in body['tokens'] if t['id'] == tid and kind in (t.get('wait') or {})), None)
            if tok is None or body['status'] != 'running':
                continue
            d = self.versions[body['definition_version']]['body']
            s = d['steps'][tok['step']]
            if kind == 'pending_change':
                rec = self.pending_changes[tok['wait']['pending_change']]
                self._pc_refresh(rec)
                self._wf_pc_done(rec)
                continue
            if kind == 'timeout_at':
                task = tok['wait'].get('task_id')
                self._log(body, tok, 'timeout', due=at)
                self._visit(body, tok, s['on_timeout'])
                self._wf_save(branch, iid, head, body)
                if task is not None:
                    self.task({'branch': branch, 'action': 'cancel', 'task_id': task, 'principal': body['initiator'],
                               'reason': 'timeout'})
                self._wf_run(branch, iid)
                continue
            self._log(body, tok, 'fired', due=tok['wait']['due'])
            self._visit(body, tok, s['next'])
            self._wf_save(branch, iid, head, body)
            self._wf_run(branch, iid)

    def _wf_redue(self, olds, new, step, tok, body):
        """Recompute the due times of a waiting token from its entry instant under the new
        version of its step."""
        w = tok.get('wait') or {}
        news = new['steps'][step]
        if w.get('due') and (olds.get('at') is not None or news.get('at') is not None or
                             olds.get('after') != news.get('after')):
            entered = w.get('entered') or _shift(w['due'], olds['after'], -1)
            if news.get('at') is not None:
                w['due'] = self._wf_at(new, news, body['variables'], entered)
                w['entered'] = entered
            else:
                w['due'] = _shift(entered, news['after'])
        if w.get('timeout_at'):
            if news.get('timeout') is not None:
                w['timeout_at'] = _shift(w['entered'], news['timeout'])
            else:
                w.pop('timeout_at')

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
            self._wf_redue(old['steps'][t['step']], new, smap[t['step']], t, body)   # STD-TXN-094 (1.0-rc12)
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

    def _saga_set(self, branch, sid, head, changes, effects=None):
        proc = {'initiator': self._saga_run['initiator']} if self._saga_run.get('initiator') else {}
        if effects:
            proc['effects'] = effects
        self._sys_commit(branch, [{'op': 'update', 'object_id': sid, 'base': head, 'body': changes}], (SAGA,),
                         proc or None)
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
            elif status in SAGA_END or status == 'stuck':
                return
            elif status == 'running':
                done = b['completed']
                if len(done) == len(order):
                    self._saga_set(branch, sid, head, {'status': 'completed', 'current': None})
                    continue
                name = order[len(done)]
                if cur.get('step') != name or cur.get('phase') != 'forward':
                    self._saga_set(branch, sid, head, {'current': self._saga_attempt(steps[name], 1)})
                    continue
                kind, res = self._saga_timed_out(sid, steps[name], cur)
                if kind is None:
                    kind, res = self._saga_call(branch, sid, steps[name]['process'], name, cur['attempt'], 'forward')
                entry = {'step': name, 'attempt': cur['attempt'], 'phase': 'forward', 'outcome': kind}
                if kind == 'ok':
                    self._saga_set(branch, sid, head, {'completed': done + [name], 'current': None, 'log': log + [entry]})
                    continue
                entry['code'] = res
                # STD-TXN-081 (1.0-rc12): a step that times out may escalate instead of recovering
                if res in ('FLOW.TIMEOUT', 'LOGIC.LIMIT_EXCEEDED') and steps[name].get('on_timeout') == 'escalate':
                    self._saga_set(branch, sid, head, {'status': 'stuck', 'log': log + [entry],
                                                       'error': {'step': name, 'code': res}},
                                   [{'type': 'SagaStuck', 'object_id': sid,
                                     'payload': {'saga': sid, 'step': name, 'code': res}}])
                    continue
                if b.get('recovery', 'backward') == 'forward':
                    if cur['attempt'] < b.get('max_attempts', 1):
                        self._saga_set(branch, sid, head, {'current': self._saga_attempt(steps[name], cur['attempt'] + 1),
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

    def _saga_attempt(self, step, attempt):
        """The step in progress; a step with a timeout records when its attempt started."""
        cur = {'step': step['name'], 'attempt': attempt, 'phase': 'forward'}
        if step.get('timeout'):
            cur['started_at'] = self.now
        return cur

    def _saga_timed_out(self, sid, step, cur):
        """STD-TXN-081: a step whose process has not committed by the start of its attempt plus
        its timeout has timed out."""
        if not step.get('timeout') or not cur.get('started_at') or \
                (sid, step['name'], cur['attempt'], 'forward') in self.saga_results:
            return None, None
        if self.now >= _shift(cur['started_at'], step['timeout']):
            return 'timeout', 'FLOW.TIMEOUT'
        return None, None

    def sagas(self, i):
        """STD-TXN-081: the sagas of a branch by state, age of the step in progress and step."""
        branch = i.get('branch', 'main')
        st = self._state(branch)[0]
        since = _shift(self.now, i['older_than'], -1) if i.get('older_than') is not None else None
        out = []
        for oid, o in sorted(st['objects'].items()):
            if o['class'] != SAGA or self.versions[o['head']]['tombstone']:
                continue
            b = self.versions[o['head']]['body']
            status = b.get('status') or 'pending'
            cur, err = b.get('current'), b.get('error')
            if i.get('status') is not None and status != i['status']:
                continue
            if since is not None and not (cur and cur.get('started_at') and cur['started_at'] <= since):
                continue
            if i.get('step') is not None and i['step'] not in ((cur or {}).get('step'), (err or {}).get('step')):
                continue
            out.append({'saga_id': oid, 'name': b['name'], 'status': status, 'current': cur, 'error': err})
        return {'sagas': out}

    def saga_action(self, i):
        """STD-TXN-081: an operator retries or skips the step of a stuck or failed saga, or
        cancels the saga; the saga then continues as run_saga."""
        branch = i.get('branch', 'main')
        self._check_active(branch)
        sid, o = self._ref_oid(self._state(branch)[0], i.get('saga'), SAGA)
        action, principal, reason = i.get('action'), i.get('principal'), i.get('reason')
        if action not in SAGA_ACTIONS:
            raise _invalid('a saga action is retry, skip or cancel')
        if not isinstance(principal, str):
            raise _incomplete('principal')
        if action == 'skip' and not (isinstance(reason, str) and reason):
            raise _incomplete('a skip states its reason')
        b = self.versions[o['head']]['body']
        if principal not in (b.get('operators') or []):
            raise BpaError('IAM.DENIED', 'the principal is not an operator of the saga')
        status, err, cur = b.get('status') or 'pending', b.get('error') or {}, b.get('current') or {}
        if not (status == 'stuck' or (status == 'failed' and 'compensation' not in err and err.get('step'))):
            raise BpaError('FLOW.TRANSITION_NOT_ALLOWED', {'status': status, 'action': action})
        name = cur.get('step') or err['step']
        attempt = cur.get('attempt') or err.get('attempts') or 1
        steps = {s['name']: s for s in b['steps']}
        entry = {'step': name, 'phase': 'operator', 'action': action, 'principal': principal}
        if reason is not None:
            entry['reason'] = reason
        log = (b.get('log') or []) + [entry]
        if action == 'retry':
            changes = {'status': 'running', 'current': self._saga_attempt(steps[name], attempt + 1), 'error': None,
                       'log': log}
        elif action == 'skip':
            changes = {'status': 'running', 'current': None, 'error': None, 'log': log,
                       'completed': (b.get('completed') or []) + [name],
                       'skipped': (b.get('skipped') or []) + [{'step': name, 'principal': principal,
                                                                 'reason': reason}]}
        else:
            changes = {'status': 'compensating', 'current': None, 'log': log}
        n = i.get('interrupt_after')
        self._saga_run = {'left': n, 'commits': 0, 'initiator': principal}
        try:
            self._saga_set(branch, sid, o['head'], changes)
            self._saga_loop(branch, sid)
        except _Interrupted:
            return {'saga_id': sid, 'status': 'interrupted', 'commits': self._saga_run['commits']}
        finally:
            run, self._saga_run = self._saga_run, None
        out = self._saga_view(branch, sid)
        out['commits'] = run['commits']
        return out

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
