"""Vector cases of BPA 1.0-rc8 for workflows, sagas and quarantine (PH-2 step 1d, DEC-092):
sagas (STD-TXN-080), workflow definitions and instances (STD-TXN-090), events, timers and
human tasks (STD-TXN-091), workflow migration (STD-TXN-092) and the quarantine of logic asset
versions (STD-CTX-031). Not required of the PH-0 prototype."""
from .common import CLASSES, NOW, cls, field, uid, create, base
from .common import commit as _commit
from .model import port
from .sheets import op
from .execution import asset, fn

BR = 'feature/wf'
CHECKER = 'did:ubos:acme:01900000-0000-7000-8000-000000000021'
OTHER = 'did:ubos:acme:01900000-0000-7000-8000-000000000022'
OPERATOR = 'did:ubos:acme:01900000-0000-7000-8000-000000000030'
A1, A2 = uid(1), uid(2)
URI1, URI2 = 'ubos://acme/Account/a-1', 'ubos://acme/Account/a-2'
AMOUNT = [field('amount', 'decimal', True)]
ACCOUNT = cls('Account', [field('name', 'text', True), field('balance', 'decimal'), field('status', 'text')],
              ports=[port('credit', 'acme.wf#credit', params=AMOUNT), port('debit', 'acme.wf#debit', params=AMOUNT),
                     port('fail', 'acme.wf#fail'), port('spin', 'acme.wf#spin')])
WF_CLASSES = CLASSES + [ACCOUNT]
GET = {'do': 'read.get', 'args': {'target': 'self_id'}}
WFA = asset(80, 'acme.wf', {
    'credit': fn([GET, {'do': 'commit.update', 'args': {'target': 'self_id',
                                                        'set': {'balance': 'this.balance + params.amount'}}},
                  {'return': 'this.balance + params.amount'}], ['amount']),
    'debit': fn([GET, {'if': 'params.amount > this.balance', 'then': [{'fail': 'RULE.INVARIANT_VIOLATED'}]},
                 {'do': 'commit.update', 'args': {'target': 'self_id',
                                                  'set': {'balance': 'this.balance - params.amount'}}},
                 {'return': 'this.balance - params.amount'}], ['amount']),
    'fail': fn([{'fail': 'RULE.INVARIANT_VIOLATED'}]),
    'spin': fn([{'repeat': '20000', 'body': [GET]}]),
    'ok': fn([{'return': '1'}]),
    'loop': fn([{'repeat': '1000', 'body': [{'let': 'x', 'value': '1'}]}])})
ACC1 = create(A1, 'Account', 'a-1', {'name': 'One', 'balance': '100', 'status': 'open'})
ACC2 = create(A2, 'Account', 'a-2', {'name': 'Two', 'balance': '0', 'status': 'open'})


def commit(changes, branch=BR, **kw):
    return _commit(changes, branch=branch, **kw)


def V(name, type_):
    return {'name': name, 'type': type_}


VARS = [V('amount', 'integer'), V('ref', 'text'), V('approved', 'boolean'), V('balance', 'decimal'),
        V('payment', 'json'), V('route', 'text')]


def wfdef(n, slug, steps, start, variables=VARS):
    return create(uid(n), 'WorkflowDefinition', slug, {'name': slug, 'variables': variables, 'start': start,
                                                        'steps': steps, 'description': slug})


def pay(target=URI2, nxt='confirm', amount='params.amount'):
    return {'type': 'action', 'target': target, 'port': 'credit', 'params': {'amount': amount}, 'result': 'balance',
            'next': nxt}


CONFIRM = {'type': 'wait_event', 'event_type': 'PaymentSettled', 'correlation': 'params.ref', 'result': 'payment',
           'next': 'done'}
END = {'type': 'end'}
PAY_STEPS = {'check': {'type': 'decision', 'cases': [{'when': 'params.amount > 100', 'next': 'review'}],
                       'default': 'pay'},
             'review': {'type': 'human_task', 'assignee': CHECKER, 'result': 'approved', 'next': 'route'},
             'route': {'type': 'decision', 'cases': [{'when': 'params.approved', 'next': 'pay'}], 'default': 'rejected'},
             'pay': pay(), 'confirm': CONFIRM, 'done': END, 'rejected': {'type': 'end', 'outcome': 'failed'}}
PAY = wfdef(60, 'payment', PAY_STEPS, 'check')
# version 2 of the payment workflow: the review step is renamed and pays into a-1
PAY_V2 = dict(PAY_STEPS, approve=dict(PAY_STEPS['review']), pay=pay(URI1))
del PAY_V2['review']
PAY_V2['check'] = {'type': 'decision', 'cases': [{'when': 'params.amount > 100', 'next': 'approve'}], 'default': 'pay'}
PAR = wfdef(61, 'four-eyes', {'fork': {'type': 'parallel', 'branches': ['a', 'b'], 'join': 'j'},
                              'a': {'type': 'human_task', 'next': 'j'}, 'b': {'type': 'human_task', 'next': 'j'},
                              'j': {'type': 'join', 'next': 'pay'}, 'pay': pay(nxt='done'), 'done': END}, 'fork')
RETRY = wfdef(62, 'retrying', {'try': {'type': 'action', 'target': URI1, 'port': 'fail', 'retries': 2,
                                       'on_error': 'handle', 'next': 'done'},
                               'handle': {'type': 'human_task', 'next': 'done'}, 'done': END}, 'try')
NO_HANDLER = wfdef(63, 'failing', {'try': {'type': 'action', 'target': URI1, 'port': 'fail', 'next': 'done'},
                                   'done': END}, 'try')
CHILD = wfdef(64, 'child-credit', {'pay': pay(nxt='done'), 'done': END}, 'pay')
PARENT = wfdef(65, 'parent', {'sub': {'type': 'sub_workflow', 'definition': uid(64), 'inputs': {'amount': 'params.amount'},
                                      'outputs': {'balance': 'balance'}, 'next': 'done'}, 'done': END}, 'sub')
PARENT_FAIL = wfdef(66, 'parent-failing', {'sub': {'type': 'sub_workflow', 'definition': uid(63), 'next': 'done'},
                                           'done': END}, 'sub')
ROUTING = create(uid(70), 'DecisionTable', 'routing', {
    'name': 'routing', 'description': 'routing', 'hit_policy': 'first',
    'inputs': [{'name': 'amount', 'type': 'integer'}], 'outputs': [{'name': 'route', 'type': 'text'}],
    'rows': [{'when': {'amount': 'params.amount < 1000'}, 'then': {'route': 'auto'}},
             {'when': {'amount': '-'}, 'then': {'route': 'manual'}}]})
TABLED = wfdef(67, 'tabled', {'route': {'type': 'decision', 'table': uid(70), 'inputs': {'amount': 'params.amount'},
                                        'output': 'route', 'branches': {'auto': 'pay', 'manual': 'review'},
                                        'default': 'review'},
                              'review': {'type': 'human_task', 'next': 'done'}, 'pay': pay(nxt='done'), 'done': END},
               'route')
TIMED = wfdef(68, 'timed', {'wait': {'type': 'timer', 'after': 'PT1H', 'next': 'pay'}, 'pay': pay(nxt='done'),
                            'done': END}, 'wait')
BUFFERED = wfdef(69, 'buffered', {'confirm': dict(CONFIRM, buffer='PT1H'), 'done': END}, 'confirm')
LOOP = wfdef(71, 'ask-again', {'ask': {'type': 'human_task', 'result': 'approved', 'next': 'route'},
                               'route': {'type': 'decision', 'cases': [{'when': 'params.approved', 'next': 'done'}],
                                         'default': 'ask'},
                               'done': END}, 'ask')
DEFS = [PAY, PAR, RETRY, NO_HANDLER, CHILD, PARENT, PARENT_FAIL, ROUTING, TABLED, TIMED, BUFFERED, LOOP]
I1, I2 = uid(201), uid(202)
LATER = '2026-03-15T11:00:00.000000Z'


def start(defn, iid=I1, label=None, error=None, **variables):
    return op('start_workflow', label, error, branch=BR, definition=defn, instance_id=iid, variables=variables)


def event(eid, key, payload=None, label=None, error=None, event_type='PaymentSettled'):
    return op('deliver_event', label, error, branch=BR, event_id=eid, event_type=event_type, key=key,
              payload=payload if payload is not None else {'settled': key})


def task(step, iid=I1, principal=CHECKER, data=None, label=None, error=None, details=False):
    s = op('complete_task', label, error, branch=BR, instance=iid, step=step, principal=principal, data=data)
    if details:
        s['details'] = True
    return s


def rd(oid, label=None):
    return op('read', label, None, branch=BR, object_id=oid)


def now(t):
    return {'op': 'set_now', 'input': {'now': t}}


SAGA_STEPS = [{'name': 'reserve', 'process': {'target': URI1, 'port': 'debit', 'params': {'amount': 30}},
               'compensation': {'target': URI1, 'port': 'credit', 'params': {'amount': 30}}},
              {'name': 'transfer', 'process': {'target': URI2, 'port': 'credit', 'params': {'amount': 30}},
               'compensation': {'target': URI2, 'port': 'debit', 'params': {'amount': 30}}}]
S1 = uid(300)


def saga(steps, n=300, slug='transfer', **extra):
    body = {'name': slug, 'steps': steps}
    body.update(extra)
    return create(uid(n), 'Saga', slug, body)


def step(name, port_, target=URI1, amount=None, comp=None):
    s = {'name': name, 'process': {'target': target, 'port': port_, 'params': {} if amount is None else {'amount': amount}}}
    if comp:
        s['compensation'] = comp
    return s


def run(sid=S1, label=None, error=None, **kw):
    return op('run_saga', label, error, branch=BR, saga=sid, **kw)


def cases(g):
    def S(clause, desc, steps, setup_extra=(), also=()):
        g.vector(clause, desc, setup=[now(NOW), {'op': 'load_genesis', 'input': {'classes': WF_CLASSES}},
                                      {'op': 'create_branch', 'input': {'name': BR, 'from': 'main'}},
                                      commit([WFA, ACC1, ACC2] + DEFS, label='d')] + list(setup_extra),
                 steps=steps, prototype=False, also=also)

    # ---- STD-TXN-090: definitions and instances ----
    S('STD-TXN-090', 'an instance runs its decision and action steps and waits for an event',
      [start(uid(60), amount=40, ref='p-1'), rd(A2), rd(I1)])
    S('STD-TXN-090', 'an amount above the limit waits for review; a rejection ends the instance failed',
      [start(uid(60), amount=500, ref='p-1'), task('review', data=False), rd(A2)])
    S('STD-TXN-090', 'a definition whose start names no step is refused',
      [commit([wfdef(90, 'broken', {'done': END}, 'begin')], error='MODEL.INVALID_VALUE')])
    S('STD-TXN-090', 'a step of an unknown type is refused',
      [commit([wfdef(90, 'broken', {'go': {'type': 'loop', 'next': 'done'}, 'done': END}, 'go')],
              error='MODEL.INVALID_VALUE')])
    S('STD-TXN-090', 'a successor that names no step is refused',
      [commit([wfdef(90, 'broken', {'go': {'type': 'human_task', 'next': 'nowhere'}, 'done': END}, 'go')],
              error='MODEL.INVALID_VALUE')])
    S('STD-TXN-090', 'a parallel step names its join step',
      [commit([wfdef(90, 'broken', {'fork': {'type': 'parallel', 'branches': ['a', 'b'], 'join': 'a'},
                                    'a': {'type': 'human_task', 'next': 'done'},
                                    'b': {'type': 'human_task', 'next': 'done'}, 'done': END}, 'fork')],
              error='MODEL.INVALID_VALUE')])
    S('STD-TXN-090', 'a workflow instance is not written by a change',
      [commit([create(uid(91), 'WorkflowInstance', 'wf-x', {'definition': uid(60), 'definition_version': 'x',
                                                             'status': 'running', 'variables': {}, 'tokens': []})],
              error='LOGIC.FORBIDDEN_TARGET')])
    S('STD-TXN-090', 'an update of a workflow instance by a change is refused',
      [start(uid(60), amount=40, ref='p-1'), rd(I1, 'r'),
       commit([{'op': 'update', 'object_id': I1, 'base': {'$ref': 'r/version_hash'}, 'body': {'status': 'completed'}}],
              error='LOGIC.FORBIDDEN_TARGET')])
    S('STD-TXN-090', 'variables are typed: a value of the wrong type is refused',
      [start(uid(60), error='MODEL.INVALID_VALUE', amount='forty', ref='p-1')])
    S('STD-TXN-090', 'an undeclared variable is refused',
      [start(uid(60), error='MODEL.INVALID_VALUE', amount=40, ref='p-1', colour='red')])
    S('STD-TXN-090', 'a join fires once, after both parallel branches arrived',
      [start(uid(61), amount=25), task('a', principal=OTHER), rd(I1), task('b', principal=OTHER), rd(A2), rd(I1)])
    S('STD-TXN-090', 'a join fires once whichever branch arrives first',
      [start(uid(61), amount=25), task('b', principal=OTHER), task('a', principal=OTHER), rd(A2)])
    S('STD-TXN-090', 'a failing action is retried, then follows on_error',
      [start(uid(62)), rd(I1)])
    S('STD-TXN-090', 'a failing action without on_error ends the instance failed',
      [start(uid(63)), rd(A1)])
    S('STD-TXN-090', 'a sub-workflow runs to its end and returns its outputs',
      [start(uid(65), amount=15), rd(A2)])
    S('STD-TXN-090', 'a failing sub-workflow fails its parent',
      [start(uid(66))])
    S('STD-TXN-090', 'a decision follows the branch of a decision table output',
      [start(uid(67), amount=5000), start(uid(67), I2, amount=50), rd(A2)])
    S('STD-TXN-090', 'every step is one committed state change of the instance',
      [start(uid(60), amount=40, ref='p-1'), op('history', None, None, branch=BR, object_id=I1)])

    # ---- STD-TXN-091: events, timers and tasks ----
    S('STD-TXN-091', 'an event completes the waiting step and carries its payload',
      [start(uid(60), amount=40, ref='p-1'), event('e-1', 'p-1', {'bank_ref': 'B-77'}), rd(I1)])
    S('STD-TXN-091', 'one event wakes one of two steps waiting for the same key: the one waiting longest',
      [start(uid(60), I1, amount=40, ref='p-1'), start(uid(60), I2, amount=60, ref='p-1'),
       event('e-1', 'p-1'), event('e-2', 'p-1')])
    S('STD-TXN-091', 'an event with another key wakes nothing and is buffered',
      [start(uid(60), amount=40, ref='p-1'), event('e-1', 'p-9'), rd(I1)])
    S('STD-TXN-091', 'an event that arrived before the wait is delivered within the buffer window',
      [event('e-1', 'p-1'), now('2026-03-15T10:30:00.000000Z'), start(uid(69), ref='p-1'), rd(I1)])
    S('STD-TXN-091', 'an event older than the buffer window is not delivered',
      [event('e-1', 'p-1'), now(LATER), now('2026-03-15T11:00:00.000001Z'), start(uid(69), ref='p-1')])
    S('STD-TXN-091', 'a step without a buffer window does not take an earlier event',
      [event('e-1', 'p-1'), start(uid(60), amount=40, ref='p-1')])
    S('STD-TXN-091', 'an event delivered again under its ID returns the first result',
      [start(uid(60), amount=40, ref='p-1'), event('e-1', 'p-1', label='e'), event('e-1', 'p-1')])
    S('STD-TXN-091', 'another event under a delivered event ID is refused',
      [start(uid(60), amount=40, ref='p-1'), event('e-1', 'p-1'),
       event('e-1', 'p-1', {'bank_ref': 'other'}, error='TXN.IDEMPOTENCY_MISMATCH')])
    S('STD-TXN-091', 'a timer fires when the clock reaches its due time',
      [start(uid(68), amount=10), now('2026-03-15T10:59:59.999999Z'), rd(I1, 'w'), now(LATER), rd(I1), rd(A2)])
    S('STD-TXN-091', 'a human task is completed by its assignee only',
      [start(uid(60), amount=500, ref='p-1'), task('review', principal=OTHER, data=True, error='IAM.DENIED')])
    S('STD-TXN-091', 'completing a task again with the same data returns the first result',
      [start(uid(60), amount=500, ref='p-1'), task('review', data=True), task('review', data=True), rd(A2)])
    S('STD-TXN-091', 'completing a task again with other data is refused',
      [start(uid(60), amount=500, ref='p-1'), task('review', data=True),
       task('review', data=False, error='TXN.IDEMPOTENCY_MISMATCH')])
    S('STD-TXN-091', 'a task that does not wait cannot be completed',
      [start(uid(60), amount=40, ref='p-1'), task('review', data=True, error='FLOW.TRANSITION_NOT_ALLOWED',
                                                    details=True)])
    S('STD-TXN-091', 'a step entered again in a loop is a new visit with its own completion',
      [start(uid(71)), task('ask', principal=OTHER, data=False), task('ask', principal=OTHER, data=True), rd(I1)])

    # ---- STD-TXN-092: migration ----
    upd = commit([{'op': 'update', 'object_id': uid(60), 'base': base('d', uid(60)), 'body': {'steps': PAY_V2}}],
                 label='v2')
    S('STD-TXN-092', 'a running instance continues on its pinned definition version',
      [start(uid(60), amount=500, ref='p-1'), upd, task('review', data=True), rd(A1), rd(A2)])
    S('STD-TXN-092', 'a migration that maps the waiting step moves the instance to the new version',
      [start(uid(60), amount=500, ref='p-1'), upd,
       op('migrate_workflow', None, None, branch=BR, instance=I1, step_map={'review': 'approve'}),
       task('approve', data=True), rd(A1), rd(I1)])
    S('STD-TXN-092', 'a migration that leaves a waiting step unmapped is refused',
      [start(uid(60), amount=500, ref='p-1'), upd,
       dict(op('migrate_workflow', None, 'MODEL.MIGRATION_REQUIRED', branch=BR, instance=I1, step_map={}),
            details=True)])
    S('STD-TXN-092', 'a step mapped to a step of another type is refused',
      [start(uid(60), amount=500, ref='p-1'), upd,
       op('migrate_workflow', None, 'MODEL.INVALID_VALUE', branch=BR, instance=I1, step_map={'review': 'pay'})])
    S('STD-TXN-092', 'a join not yet fired is an active step of the migration',
      [start(uid(61), amount=25), task('a', principal=OTHER),
       dict(op('migrate_workflow', None, 'MODEL.MIGRATION_REQUIRED', branch=BR, instance=I1, step_map={'b': 'b'}),
            details=True)])
    S('STD-TXN-092', 'an instance that has ended is not migrated',
      [start(uid(60), amount=40, ref='p-1'), event('e-1', 'p-1'), upd,
       op('migrate_workflow', None, 'MODEL.CONSTRAINT_VIOLATED', branch=BR, instance=I1, step_map={})])

    # ---- STD-TXN-080: sagas ----
    def SG(desc, sg, steps):
        S('STD-TXN-080', desc, steps, setup_extra=[commit([sg], label='s')])

    fail_last = SAGA_STEPS + [step('notify', 'fail', comp={'target': URI1, 'port': 'credit',
                                                              'params': {'amount': 1}})]
    SG('a saga whose steps succeed completes', saga(SAGA_STEPS), [run(), rd(A1), rd(A2)])
    SG('a failed step compensates the completed steps in reverse order', saga(fail_last),
       [run(), rd(A1), rd(A2), rd(S1)])
    SG('a step that ends by a limit termination has timed out and is compensated',
       saga(SAGA_STEPS + [step('wait', 'spin')]), [run(), rd(A1), rd(A2)])
    SG('forward recovery retries the failed step up to max_attempts and does not compensate',
       saga(fail_last, recovery='forward', max_attempts=3), [run(), rd(A1), rd(S1)])
    SG('a step without a compensation counts as compensated',
       saga([step('mark', 'credit', URI2, 5), step('stop', 'fail')]), [run(), rd(A2)])
    SG('a failed compensation ends the saga failed',
       saga([step('give', 'credit', URI2, 5, {'target': URI2, 'port': 'fail', 'params': {}}), step('stop', 'fail')]),
       [run(), rd(A2)])
    SG('a saga interrupted before a step resumes and runs each step once', saga(SAGA_STEPS),
       [run(interrupt_after=2), rd(A1), run(), rd(A1), rd(A2)])
    SG('a saga interrupted after a step process committed does not run the step again', saga(SAGA_STEPS),
       [run(interrupt_after=3), run(interrupt_after=1), run(), rd(A1), rd(A2)])
    SG('running a saga that ended returns its state without commits', saga(SAGA_STEPS),
       [run(), run(label='again'), rd(A1)])
    S('STD-TXN-080', 'the state of a saga is not written by a change',
      [commit([saga(SAGA_STEPS, status='completed')], error='FLOW.STATE_DIRECT_WRITE')])
    SG('an update of the saga state by a change is refused', saga(SAGA_STEPS),
       [commit([{'op': 'update', 'object_id': S1, 'base': base('s', S1), 'body': {'completed': ['reserve']}}],
               error='FLOW.STATE_DIRECT_WRITE')])
    S('STD-TXN-080', 'saga steps have unique names',
      [commit([saga([step('a', 'credit', amount=1), step('a', 'credit', amount=2)])],
              error='MODEL.INVALID_VALUE')])
    S('STD-TXN-080', 'a saga step is a port call',
      [commit([saga([{'name': 'a', 'process': {'target': URI1}}])], error='MODEL.DECLARATION_INCOMPLETE')])

    # ---- STD-CTX-031: quarantine ----
    def ex(function, error=None, label=None, **kw):
        inp = {'branch': BR, 'asset': 'acme.wf', 'function': function, 'args': []}
        inp.update(kw)
        return op('execute', label, error, **inp)

    policy = op('quarantine_policy', None, None, threshold=2, window='PT1H', releasers=[OPERATOR])
    S('STD-CTX-031', 'reaching the failure threshold quarantines the asset version; calls then fail fast',
      [policy, ex('fail', 'RULE.INVARIANT_VIOLATED'), ex('fail', 'RULE.INVARIANT_VIOLATED'),
       ex('ok', 'LOGIC.ASSET_QUARANTINED')])
    S('STD-CTX-031', 'a quarantine is a System object and releases an alert effect',
      [policy, ex('fail', 'RULE.INVARIANT_VIOLATED'), ex('fail', 'RULE.INVARIANT_VIOLATED'), op('outbox')])
    S('STD-CTX-031', 'limit terminations count towards the threshold',
      [policy, ex('loop', 'LOGIC.LIMIT_EXCEEDED', limits={'units': 10}),
       ex('loop', 'LOGIC.LIMIT_EXCEEDED', limits={'units': 10}), ex('ok', 'LOGIC.ASSET_QUARANTINED')])
    S('STD-CTX-031', 'failures outside the window do not count',
      [policy, ex('fail', 'RULE.INVARIANT_VIOLATED'), now('2026-03-15T11:30:00.000000Z'),
       ex('fail', 'RULE.INVARIANT_VIOLATED'), ex('ok')])
    S('STD-CTX-031', 'without a policy nothing is quarantined',
      [ex('fail', 'RULE.INVARIANT_VIOLATED'), ex('fail', 'RULE.INVARIANT_VIOLATED'),
       ex('fail', 'RULE.INVARIANT_VIOLATED'), ex('ok')])
    qid = {'$ref': 'o/effects/0/object_id'}
    quarantined = [policy, ex('fail', 'RULE.INVARIANT_VIOLATED'), ex('fail', 'RULE.INVARIANT_VIOLATED'),
                   op('outbox', 'o')]
    S('STD-CTX-031', 'an authorised principal releases the quarantine and calls run again',
      quarantined + [op('release_quarantine', None, None, branch=BR, quarantine=qid, principal=OPERATOR,
                        reason='fixed upstream', release_id=uid(400)), ex('ok'), rd(uid(400))])
    S('STD-CTX-031', 'another principal cannot release a quarantine',
      quarantined + [op('release_quarantine', None, 'IAM.DENIED', branch=BR, quarantine=qid, principal=OTHER,
                        reason='try', release_id=uid(400))])
    S('STD-CTX-031', 'a released quarantine is not released again',
      quarantined + [op('release_quarantine', None, None, branch=BR, quarantine=qid, principal=OPERATOR,
                        reason='fixed', release_id=uid(400)),
                     op('release_quarantine', None, 'MODEL.CONSTRAINT_VIOLATED', branch=BR, quarantine=qid,
                        principal=OPERATOR, reason='again', release_id=uid(401))])
    S('STD-CTX-031', 'a quarantine is not written by a change',
      [commit([create(uid(92), 'Quarantine', 'q-x', {'asset': uid(80), 'asset_version': 'x', 'failures': 1,
                                                     'limit_terminations': 0, 'window': 'PT1H', 'threshold': 1,
                                                     'since': NOW})], error='LOGIC.FORBIDDEN_TARGET')])
    S('STD-CTX-031', 'a threshold is a positive integer',
      [op('quarantine_policy', None, 'MODEL.INVALID_VALUE', threshold=0, window='PT1H', releasers=[OPERATOR])])
