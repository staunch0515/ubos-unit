"""Vector cases of BPA 1.0-rc12 for durable work (PH-2 step 6a, DEC-119): saga timeouts,
monitoring and intervention (STD-TXN-081), workflow tasks and approvals (STD-TXN-093), workflow
timers, business calendars and timeouts (STD-TXN-094) and actions that ask for input
(STD-ISA-101). Not required of the PH-0 prototype."""
from bpa.canon import content_id

from .common import uid, create, NOW
from .sheets import sheet, op
from .execution import asset, fn, SEED
from .rules import CLASSES as RULE_CLASSES, BR, FB, SHEET
from .approvals import ACC, ACCT, MAKER, CHECKER, approve, commit, read
from .work import PEOPLE, ALICE, BOB, LEAD, CAL, queue, task, Q
from . import workflows as wf

T = 'ubos://acme/Account/a-1'
I1, I2 = uid(701), uid(702)
LATE = '2026-03-15T11:00:00.000000Z'


def V(name, type_):
    return {'name': name, 'type': type_}


VARS = [V('ref', 'text'), V('decision', 'json'), V('approval', 'json'), V('amount', 'integer')]
END = {'type': 'end'}


def wdef(n, slug, steps, start):
    return create(uid(n), 'WorkflowDefinition', slug, {'name': slug, 'variables': VARS, 'start': start,
                                                        'steps': steps, 'description': slug})


def start(defn, iid=I1, label=None, error=None, initiator=MAKER, **variables):
    return op('start_workflow', label, error, branch=BR, definition=uid(defn), instance_id=iid,
              variables=variables, process={'initiator': initiator})


def rd(oid, label=None):
    return op('read', label, None, branch=BR, object_id=oid)


def now(t):
    return op('set_now', None, None, now=t)


def tref(label='w', n=0):
    return {'$ref': '%s/waiting/%d/task_id' % (label, n)}


def pcref(label='w'):
    return {'$ref': '%s/waiting/0/pending_change' % label}


REVIEW = {'type': 'human_task', 'queue': Q, 'task_type': 'invoice_review', 'subject': 'Invoice 7', 'priority': 2,
          'data': ['ref'], 'result': 'decision', 'next': 'done'}
CHASE = {'type': 'human_task', 'assignee': LEAD, 'next': 'done'}
QUEUED = wdef(710, 'queued-review', {'review': REVIEW, 'done': END}, 'review')
CANCELLABLE = wdef(711, 'cancellable', {'review': dict(REVIEW, on_cancel='chase'), 'chase': CHASE, 'done': END},
                   'review')
CLOSE = {'type': 'action', 'target': T, 'port': 'close', 'defer': True, 'result': 'approval', 'next': 'done'}
APPROVED = wdef(712, 'close-approved', {'close': dict(CLOSE, on_reject='rejected'), 'done': END,
                                        'rejected': {'type': 'end', 'outcome': 'failed'}}, 'close')
STRICT = wdef(713, 'close-strict', {'close': CLOSE, 'done': END}, 'close')
NOWHERE = wdef(714, 'unknown-queue', {'review': dict(REVIEW, queue=uid(699)), 'done': END}, 'review')
BUSINESS = wdef(720, 'business-days', {'wait': {'type': 'timer', 'at': 'add_business_days(today(), 2, "us")',
                                                'calendars': CAL, 'next': 'done'}, 'done': END}, 'wait')
CUTOFF = wdef(721, 'cutoff', {'wait': {'type': 'timer', 'calendars': CAL, 'next': 'done',
                                       'at': 'cutoff(add_business_days(today(), 1, "us"), "12:00:00", "America/New_York", "us")'},
                              'done': END}, 'wait')
TEXT_AT = wdef(722, 'text-at', {'wait': {'type': 'timer', 'at': '"soon"', 'next': 'done'}, 'done': END}, 'wait')
SETTLE = {'type': 'wait_event', 'event_type': 'PaymentSettled', 'correlation': 'params.ref', 'result': 'decision',
          'timeout': 'PT1H', 'on_timeout': 'chase', 'next': 'done'}
WAITING = wdef(723, 'settle-or-chase', {'settle': SETTLE, 'chase': CHASE, 'done': END}, 'settle')
QUEUE_TIMEOUT = wdef(724, 'review-or-chase', {'review': dict(REVIEW, timeout='PT2H', on_timeout='chase'),
                                              'chase': CHASE, 'done': END}, 'review')
HOUR = wdef(725, 'hour', {'wait': {'type': 'timer', 'after': 'PT1H', 'next': 'done'}, 'done': END}, 'wait')


def event(eid, key, label=None):
    return op('deliver_event', label, None, branch=BR, event_id=eid, event_type='PaymentSettled', key=key,
              payload={'settled': key})


# ---- STD-ISA-101 ----

ASK = lambda request, fields, message='"Which account?"', **kw: {  # noqa: E731
    'do': 'ask.input', 'as': request, 'args': dict({'request': '"%s"' % request, 'fields': fields,
                                                     'message': message}, **kw)}
F_ACCOUNT = {'name': '"account"', 'type': '"text"', 'required': 'true'}
F_NOTE = {'name': '"note"', 'type': '"text"', 'required': 'false'}
F_COUNT = {'name': '"count"', 'type': '"integer"', 'required': 'true'}
PARTY = {'do': 'commit.create', 'args': {'class': '"Party"', 'fields': {'name': 'account.account'}}}
ASKER = asset(730, 'acme.ask', {
    'open': fn([ASK('account', [F_ACCOUNT, F_NOTE]), PARTY, {'return': 'account.account'}]),
    'two': fn([ASK('account', [F_ACCOUNT]), ASK('count', [F_COUNT], '"How many?"'), PARTY,
               {'return': 'count.count'}]),
    'later': fn([ASK('account', [F_ACCOUNT], timeout='"PT1H"'), {'return': 'account.account'}]),
    'writes': fn([{'do': 'commit.create', 'args': {'class': '"Party"', 'fields': {'name': '"draft"'}}},
                  ASK('account', [F_ACCOUNT]), {'return': 'account.account'}]),
    'bad': fn([ASK('account', [{'name': '"account"'}]), {'return': '1'}])},
    caps=('commit', 'read', 'run', 'emit', 'ask'))
NO_ASK = asset(731, 'acme.noask', {'open': fn([ASK('account', [F_ACCOUNT]), {'return': '1'}])})


def call(function, pid=None, answers=None, label=None, error=None, asset_='acme.ask', details=False):
    inp = {'branch': BR, 'asset': asset_, 'function': function, 'args': [], 'entropy_seed': SEED}
    if pid is not None:
        inp['process'] = {'process_id': uid(740 + pid), 'initiator': MAKER}
    if answers is not None:
        inp['answers'] = answers
    s = op('execute', label, error, **inp)
    if details:
        s['details'] = True
    return s


def rid(function, request='account', asset_='acme.ask'):
    """The request ID of STD-ISA-101 for a call made by `call`."""
    inv = {'branch': BR, 'asset': asset_, 'function': function, 'args': [], 'entropy_seed': SEED}
    return content_id('ubos.chunk.v1', {'invocation': inv, 'request': request})


def cases(g):
    def S(clause, desc, steps, also=()):
        g.vector(clause, desc, setup=[{'op': 'set_now', 'input': {'now': NOW}},
                                      {'op': 'load_genesis', 'input': {'classes': RULE_CLASSES}}, FB] + PEOPLE,
                 steps=steps, prototype=False, also=also)

    work = [commit([ACCT, ACC, queue(), QUEUED, CANCELLABLE, APPROVED, STRICT, NOWHERE, QUEUE_TIMEOUT])]

    def needs(src):
        return work + [commit([sheet(SHEET, 'd', src)])]

    # ---- STD-TXN-093: workflow tasks ----
    S('STD-TXN-093', 'a human task with a queue creates a Task linked to the instance step',
      work + [start(710, label='w', ref='inv-7'), op('read', None, None, branch=BR, object_id=tref())])
    S('STD-TXN-093', 'completing the task continues the instance with its outcome',
      work + [start(710, label='w', ref='inv-7'), task('claim', tref(), ALICE),
              task('complete', tref(), ALICE, outcome={'approved': True}), rd(I1)])
    S('STD-TXN-093', 'complete_task on a step that waits for a task is refused',
      work + [start(710, label='w', ref='inv-7'),
              dict(op('complete_task', None, 'FLOW.TRANSITION_NOT_ALLOWED', branch=BR, instance=I1, step='review',
                      principal=ALICE, data=True), details=True)])
    S('STD-TXN-093', 'a cancelled task continues to on_cancel',
      work + [start(711, label='w', ref='inv-7'), task('cancel', tref(), LEAD, reason='duplicate'), rd(I1)])
    S('STD-TXN-093', 'a cancelled task without on_cancel fails the instance',
      work + [start(710, label='w', ref='inv-7'), task('cancel', tref(), LEAD, reason='duplicate'), rd(I1)])
    S('STD-TXN-093', 'the queue assigns the task and its due rule sets the due time',
      [commit([ACCT, ACC, queue(assignment='round_robin', due_rule='add_business_days(params.created, 2, "us")',
                                calendars=CAL), QUEUED]),
       start(710, label='w', ref='inv-7'), op('read', None, None, branch=BR, object_id=tref())])
    S('STD-TXN-093', 'the workflow task appears in the inbox of a queue member',
      work + [start(710, ref='inv-7'), op('inbox', None, None, branch=BR, principal=BOB)])
    S('STD-TXN-093', 'an unknown queue fails the instance when the step is entered',
      work + [start(714, ref='inv-7')])
    S('STD-TXN-093', 'a task step whose data names an unknown variable is refused',
      [commit([wdef(790, 'broken', {'review': dict(REVIEW, data=['colour']), 'done': END}, 'review')],
              error='MODEL.INVALID_VALUE')])
    S('STD-TXN-093', 'on_cancel names a step',
      [commit([wdef(790, 'broken', {'review': dict(REVIEW, on_cancel='nowhere'), 'done': END}, 'review')],
              error='MODEL.INVALID_VALUE')])
    S('STD-TXN-093', 'a completed workflow task is not completed again',
      work + [start(710, label='w', ref='inv-7'), task('claim', tref(), ALICE),
              task('complete', tref(), ALICE, outcome='ok'),
              task('complete', tref(), ALICE, outcome='ok', error='FLOW.TRANSITION_NOT_ALLOWED')])

    # ---- STD-TXN-093: approvals ----
    one = needs('Account { approvals: 1; }')
    S('STD-TXN-093', 'a deferred action waits for its pending change',
      one + [start(712, label='w'), read()])
    S('STD-TXN-093', 'the final approval continues the instance with the applied change',
      one + [start(712, label='w'), approve(pc=pcref()), rd(I1), read()])
    S('STD-TXN-093', 'a rejection continues to on_reject',
      one + [start(712, label='w'), approve(pc=pcref(), decision='reject'), rd(I1), read()])
    S('STD-TXN-093', 'a rejection without on_reject fails the instance',
      one + [start(713, label='w'), approve(pc=pcref(), decision='reject'), rd(I1)])
    S('STD-TXN-093', 'an expired pending change is noticed when the clock advances',
      needs('Account { approvals: {count: 1, expires_in: "P1D"}; }') +
      [start(712, label='w'), now('2026-03-16T10:00:00.000000Z'), rd(I1)])
    S('STD-TXN-093', 'an invalidated pending change continues to on_reject at the next decision',
      one + [start(712, label='w'), rd(uid(1), 'r'),
             commit([{'op': 'update', 'object_id': uid(1), 'base': {'$ref': 'r/version_hash'},
                      'body': {'balance': '90'}}]),
             approve(pc=pcref()), rd(I1)])
    S('STD-TXN-093', 'a deferred action that needs no approval continues at once',
      work + [start(712), rd(I1), read()])
    S('STD-TXN-093', 'on_reject names a step',
      [commit([wdef(790, 'broken', {'close': dict(CLOSE, on_reject='nowhere'), 'done': END}, 'close')],
              error='MODEL.INVALID_VALUE')])
    S('STD-TXN-093', 'defer is a boolean',
      [commit([wdef(790, 'broken', {'close': dict(CLOSE, defer='yes'), 'done': END}, 'close')],
              error='MODEL.INVALID_VALUE')])

    # ---- STD-TXN-094: timers, business calendars and timeouts ----
    timers = [commit([ACCT, ACC, queue(), BUSINESS, CUTOFF, TEXT_AT, WAITING, QUEUE_TIMEOUT, HOUR])]
    S('STD-TXN-094', 'a timer due two business days later skips the weekend and a holiday',
      timers + [start(720), now('2026-03-17T23:59:59.999999Z'), rd(I1), now('2026-03-18T00:00:00.000000Z'),
                rd(I1)])
    S('STD-TXN-094', 'a timer expression may give an instant with a cut-off time',
      timers + [start(720), start(721, I2)])
    S('STD-TXN-094', 'an expression of another type fails the instance when the step is entered',
      timers + [start(722)])
    S('STD-TXN-094', 'a timer with both after and at is refused',
      [commit([wdef(790, 'broken', {'wait': {'type': 'timer', 'after': 'PT1H', 'at': 'now()', 'next': 'done'},
                                    'done': END}, 'wait')], error='MODEL.DECLARATION_INCOMPLETE')])
    S('STD-TXN-094', 'a timer with neither after nor at is refused',
      [commit([wdef(790, 'broken', {'wait': {'type': 'timer', 'next': 'done'}, 'done': END}, 'wait')],
              error='MODEL.DECLARATION_INCOMPLETE')])
    S('STD-TXN-094', 'a waiting event step shows its timeout',
      timers + [start(723, ref='p-1')])
    S('STD-TXN-094', 'a timeout continues the wait to on_timeout',
      timers + [start(723, ref='p-1'), now(LATE), rd(I1)])
    S('STD-TXN-094', 'an event before the timeout ends the wait, and the timeout does not fire',
      timers + [start(723, ref='p-1'), event('e-1', 'p-1'), now('2026-03-15T12:00:00.000000Z'), rd(I1)])
    S('STD-TXN-094', 'an event after the timeout finds no waiting step',
      timers + [start(723, ref='p-1'), now(LATE), event('e-1', 'p-1')])
    S('STD-TXN-094', 'a timeout cancels the task of the step and continues to on_timeout',
      timers + [start(724, label='w', ref='inv-7'), now('2026-03-15T12:00:00.000000Z'),
                op('read', None, None, branch=BR, object_id=tref()), rd(I1)])
    S('STD-TXN-094', 'a timeout without on_timeout is refused',
      [commit([wdef(790, 'broken', {'settle': dict(SETTLE, on_timeout=None), 'done': END}, 'settle')],
              error='MODEL.DECLARATION_INCOMPLETE')])
    S('STD-TXN-094', 'timers and timeouts fire in the order of their due times',
      timers + [start(723, I1, ref='p-1'), start(725, I2), now('2026-03-15T12:00:00.000000Z'),
                op('history', None, None, branch=BR, object_id=I2)])
    longer = commit([{'op': 'update', 'object_id': uid(725), 'base': {'$ref': 'd/version_hash'},
                      'body': {'steps': {'wait': {'type': 'timer', 'after': 'PT3H', 'next': 'done'}, 'done': END}}}])
    S('STD-TXN-094', 'a migration recomputes the due time from the entry instant',
      timers + [rd(uid(725), 'd'), start(725), now('2026-03-15T10:30:00.000000Z'), longer,
                op('migrate_workflow', None, None, branch=BR, instance=I1, step_map={'wait': 'wait'}),
                now(LATE), rd(I1)])
    shorter = commit([{'op': 'update', 'object_id': uid(723), 'base': {'$ref': 'd/version_hash'},
                       'body': {'steps': {'settle': dict(SETTLE, timeout='PT30M'), 'chase': CHASE, 'done': END}}}])
    S('STD-TXN-094', 'a migration recomputes a timeout under the new version',
      timers + [rd(uid(723), 'd'), start(723, ref='p-1'), now('2026-03-15T10:20:00.000000Z'), shorter,
                op('migrate_workflow', None, None, branch=BR, instance=I1, step_map={'settle': 'settle'}),
                now('2026-03-15T10:30:00.000000Z'), rd(I1)])

    # ---- STD-ISA-101: ask.input ----
    ask = [commit([ACCT, ACC, ASKER, NO_ASK], label='d')]
    S('STD-ISA-101', 'a process that needs input ends with the request and commits nothing',
      ask + [call('open')])
    S('STD-ISA-101', 'the repeated call with the answers completes in one commit',
      ask + [call('open'), call('open', answers={rid('open'): {'account': 'a-7'}}),
             op('outbox', None, None)])
    S('STD-ISA-101', 'repeating the call without answers returns the same request and expiry',
      ask + [call('open'), now('2026-03-15T10:05:00.000000Z'), call('open')])
    S('STD-ISA-101', 'answers after the expiry are refused',
      ask + [call('open'), now('2026-03-15T10:15:00.000000Z'),
             call('open', answers={rid('open'): {'account': 'a-7'}}, error='FLOW.INPUT_EXPIRED')])
    S('STD-ISA-101', 'after the expiry a call without answers asks again with a new expiry',
      ask + [call('open'), now('2026-03-15T10:20:00.000000Z'), call('open')])
    S('STD-ISA-101', 'a request may declare its timeout',
      ask + [call('later')])
    S('STD-ISA-101', 'a missing required field is refused',
      ask + [call('open'), call('open', answers={rid('open'): {'note': 'x'}}, error='MODEL.INVALID_VALUE')])
    S('STD-ISA-101', 'an answer of another type is refused',
      ask + [call('two'), call('two', answers={rid('two'): {'account': 'a-7'}}),
             call('two', answers={rid('two'): {'account': 'a-7'}, rid('two', 'count'): {'count': 'many'}},
                  error='MODEL.INVALID_VALUE')])
    S('STD-ISA-101', 'an unknown field is refused',
      ask + [call('open'), call('open', answers={rid('open'): {'account': 'a-7', 'colour': 'red'}},
                                error='MODEL.INVALID_VALUE')])
    S('STD-ISA-101', 'answers for a request that was never made are not found',
      ask + [call('open', answers={rid('open'): {'account': 'a-7'}}, error='IAM.NOT_FOUND')])
    S('STD-ISA-101', 'answered requests are consumed when the process commits',
      ask + [call('open'), call('open', answers={rid('open'): {'account': 'a-7'}}),
             call('open', answers={rid('open'): {'account': 'a-7'}}, error='IAM.NOT_FOUND')])
    S('STD-ISA-101', 'the first request without an answer ends the process',
      ask + [call('two'), call('two', answers={rid('two'): {'account': 'a-7'}}),
             call('two', answers={rid('two'): {'account': 'a-7'}, rid('two', 'count'): {'count': 3}})])
    S('STD-ISA-101', 'changes made before the request are not committed and the process is aborted',
      ask + [call('writes', 3), op('process_info', None, None, process_id=uid(743)), op('outbox', None, None)])
    S('STD-ISA-101', 'fields that are not {name, type} items are refused',
      ask + [call('bad', error='MODEL.INVALID_VALUE')])
    S('STD-ISA-101', 'an asset without the ask capability cannot ask',
      ask + [call('open', asset_='acme.noask', error='LOGIC.FORBIDDEN_CAPABILITY')])
    S('STD-ISA-101', 'the answers are journaled and the process replays',
      ask + [call('open', 1), call('open', 2, answers={rid('open'): {'account': 'a-7'}}),
             op('replay', None, None, process_id=uid(742))])

    # ---- STD-TXN-081: saga timeouts, monitoring and intervention ----
    def SG(desc, sg, steps):
        g.vector('STD-TXN-081', desc,
                 setup=[wf.now(NOW), {'op': 'load_genesis', 'input': {'classes': wf.WF_CLASSES}},
                        {'op': 'create_branch', 'input': {'name': wf.BR, 'from': 'main'}},
                        wf.commit([wf.WFA, wf.ACC1, wf.ACC2], label='d'), wf.commit([sg], label='s')],
                 steps=steps, prototype=False)

    OP = wf.OPERATOR
    timed = [wf.SAGA_STEPS[0], dict(wf.SAGA_STEPS[1], timeout='PT10M')]
    esc = [wf.SAGA_STEPS[0], dict(wf.SAGA_STEPS[1], timeout='PT10M', on_timeout='escalate')]
    spin = wf.SAGA_STEPS + [dict(wf.step('wait', 'spin'), on_timeout='escalate')]
    later = wf.now('2026-03-15T10:11:00.000000Z')
    S1 = wf.S1

    def act(action, principal=OP, error=None, **kw):
        return op('saga_action', None, error, branch=wf.BR, saga=S1, action=action, principal=principal, **kw)

    def sagas(**kw):
        return op('sagas', None, None, branch=wf.BR, **kw)

    SG('a step with a timeout records when its attempt started', wf.saga(timed, operators=[OP]),
       [wf.run(interrupt_after=5), wf.rd(S1)])
    SG('a step whose process has not committed by its timeout times out and is compensated',
       wf.saga(timed), [wf.run(interrupt_after=5), later, wf.run(), wf.rd(wf.A1), wf.rd(S1)])
    SG('a step resumed before its timeout runs', wf.saga(timed),
       [wf.run(interrupt_after=5), wf.now('2026-03-15T10:05:00.000000Z'), wf.run(), wf.rd(wf.A2)])
    SG('a timeout with on_timeout escalate stops the saga stuck and releases SagaStuck', wf.saga(esc),
       [wf.run(interrupt_after=5), later, wf.run(), op('outbox')])
    SG('a limit termination with on_timeout escalate stops the saga stuck', wf.saga(spin), [wf.run(), wf.rd(wf.A2)])
    SG('running a stuck saga returns its state without commits', wf.saga(esc),
       [wf.run(interrupt_after=5), later, wf.run(), wf.run()])
    SG('sagas are listed by state and by failing step', wf.saga(esc),
       [wf.run(interrupt_after=5), later, wf.run(), sagas(status='stuck'), sagas(status='running'),
        sagas(step='transfer')])
    SG('sagas are listed by the age of the step in progress', wf.saga(esc),
       [wf.run(interrupt_after=5), wf.now('2026-03-15T10:04:00.000000Z'), sagas(older_than='PT5M'),
        wf.now('2026-03-15T10:06:00.000000Z'), sagas(older_than='PT5M')])
    SG('an operator retries the stuck step as a new attempt', wf.saga(esc, operators=[OP]),
       [wf.run(interrupt_after=5), later, wf.run(), act('retry'), wf.rd(wf.A2), wf.rd(S1)])
    SG('an operator skips the stuck step with a reason', wf.saga(esc, operators=[OP]),
       [wf.run(interrupt_after=5), later, wf.run(), act('skip', reason='settled by phone'), wf.rd(S1)])
    SG('a skip without a reason is refused', wf.saga(esc, operators=[OP]),
       [wf.run(interrupt_after=5), later, wf.run(), act('skip', error='MODEL.DECLARATION_INCOMPLETE')])
    SG('an operator cancels a stuck saga, which compensates its completed steps', wf.saga(esc, operators=[OP]),
       [wf.run(interrupt_after=5), later, wf.run(), act('cancel', reason='customer withdrew'), wf.rd(wf.A1)])
    SG('a principal who is not an operator cannot act', wf.saga(esc, operators=[OP]),
       [wf.run(interrupt_after=5), later, wf.run(), act('retry', wf.OTHER, error='IAM.DENIED')])
    SG('an action on a saga that is not stuck or failed is refused', wf.saga(wf.SAGA_STEPS, operators=[OP]),
       [wf.run(), act('retry', error='FLOW.TRANSITION_NOT_ALLOWED')])
    fail_last = wf.SAGA_STEPS + [wf.step('notify', 'fail')]
    SG('an operator skips the step of a saga that failed under forward recovery',
       wf.saga(fail_last, recovery='forward', operators=[OP]),
       [wf.run(), act('skip', reason='notified by mail'), wf.rd(S1)])
    S('STD-TXN-081', 'on_timeout is recover or escalate',
      [wf.commit([wf.saga([dict(wf.step('a', 'credit', amount=1), on_timeout='ignore')])], branch=BR,
                 error='MODEL.INVALID_VALUE')])
    S('STD-TXN-081', 'a step timeout is a duration',
      [wf.commit([wf.saga([dict(wf.step('a', 'credit', amount=1), timeout='ten minutes')])], branch=BR,
                 error='MODEL.INVALID_VALUE')])
