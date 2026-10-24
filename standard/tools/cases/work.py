"""Vector cases of BPA 1.0-rc10 for delegation and work (PH-2 step 4a, DEC-105): delegations
(STD-LGS-060), tasks (STD-LGS-061), work queues, assignment and inbox (STD-LGS-062), due dates
and escalation (STD-LGS-063), the service-level report (STD-LGS-064) and the governed
reopening of a period (STD-TXN-060, STD-KIND-025). Not required of the PH-0 prototype."""
from .common import cls, field, uid, create, NOW, PERIOD, GL
from .model import port
from .sheets import sheet, op
from .rules import CLASSES as RULE_CLASSES, BR, FB, SHEET
from .approvals import ACC, ACCT, MAKER, CHECKER, CHECKER2, ex, approve, pending, read, commit

ALICE = 'did:ubos:acme:01900000-0000-7000-8000-000000000031'
BOB = 'did:ubos:acme:01900000-0000-7000-8000-000000000032'
CAROL = 'did:ubos:acme:01900000-0000-7000-8000-000000000033'
DAVE = 'did:ubos:acme:01900000-0000-7000-8000-000000000034'
LEAD = 'did:ubos:acme:01900000-0000-7000-8000-000000000035'
Q = uid(600)
Q2 = uid(601)
D1 = uid(650)
T1, T2, T3, T4 = uid(610), uid(611), uid(612), uid(613)
LATER = '2026-03-18T09:00:00.000000Z'
CAL = {'us': {'weekend': [6, 7], 'holidays': ['2026-03-17']}}


def who(did, *roles):
    return op('principal', None, None, did=did, roles=list(roles))


PEOPLE = [who(ALICE, 'clerk'), who(BOB, 'clerk'), who(CAROL, 'clerk', 'senior'), who(DAVE, 'clerk'),
          who(LEAD, 'ap_lead'), who(CHECKER, 'controller'), who(CHECKER2), who(MAKER, 'clerk')]


def queue(oid=Q, slug='ap-review', members=(ALICE, BOB, CAROL), assignment='manual', **extra):
    body = {'name': slug, 'members': list(members), 'managers': ['ap_lead'], 'assignment': assignment,
            'description': slug}
    body.update(extra)
    return create(oid, 'WorkQueue', slug, body)


def task(action, tid=T1, principal=ALICE, label=None, error=None, details=False, branch=BR, **kw):
    s = op('task', label, error, branch=branch, action=action, task_id=tid, principal=principal, **kw)
    if details:
        s['details'] = True
    return s


def new(tid=T1, principal=LEAD, type_='invoice_review', queue_=Q, **kw):
    if queue_ is not None:
        kw['queue'] = queue_
    return task('create', tid, principal, type=type_, **kw)


def now(t):
    return op('set_now', None, None, now=t)


def delegate(did=D1, delegator=CHECKER, delegate_=CHECKER2, roles=('controller',), scope=None, error=None,
             details=False, valid_from='2026-03-15T00:00:00.000000Z', valid_to='2026-03-20T00:00:00.000000Z', **kw):
    s = op('delegate', None, error, branch=BR, delegation_id=did, delegator=delegator, delegate=delegate_,
           roles=list(roles), scope=scope if scope is not None else {'ports': ['close']},
           valid_from=valid_from, valid_to=valid_to, **kw)
    if details:
        s['details'] = True
    return s


def inbox(p, **kw):
    return op('inbox', None, None, branch=BR, principal=p, **kw)


def cases(g):
    def S(clause, desc, steps, classes=None, also=()):
        g.vector(clause, desc, setup=[{'op': 'set_now', 'input': {'now': NOW}},
                                      {'op': 'load_genesis', 'input': {'classes': classes or RULE_CLASSES}}, FB]
                 + PEOPLE, steps=steps, prototype=False, also=also)

    ctrl = [commit([ACCT, ACC]),
            commit([sheet(SHEET, 'd', 'Account { approvals: {count: 1, roles: ["controller"]}; }')])]

    # ---- STD-LGS-060 delegations ----
    S('STD-LGS-060', 'a delegation records the delegator, the delegate, the roles, the scope and the validity', [
        delegate(), op('delegations', None, None, branch=BR, principal=CHECKER2)])
    S('STD-LGS-060', 'a delegation does not grant a role the delegator does not hold', [
        delegate(roles=('controller', 'treasurer'), error='IAM.DENIED', details=True)])
    S('STD-LGS-060', 'a principal does not delegate to itself', [
        delegate(delegate_=CHECKER, error='MODEL.CONSTRAINT_VIOLATED')])
    S('STD-LGS-060', 'a delegation has a scope', [delegate(scope={}, error='MODEL.DECLARATION_INCOMPLETE')])
    S('STD-LGS-060', 'the validity starts before it ends', [
        delegate(valid_from='2026-03-20T00:00:00.000000Z', error='MODEL.INVALID_VALUE')])
    S('STD-LGS-060', 'the delegate approves on behalf of the delegator with the delegated role', ctrl + [
        delegate(), ex(), approve(CHECKER2, delegation=D1, comment='for the controller'), pending(), read()])
    S('STD-LGS-060', 'without the delegation the delegate is not eligible', ctrl + [
        ex(), approve(CHECKER2, error='IAM.DENIED', details=True)])
    S('STD-LGS-060', 'an expired delegation fails with IAM.DELEGATION_EXPIRED', ctrl + [
        delegate(), ex(), now('2026-03-21T00:00:00.000000Z'),
        approve(CHECKER2, delegation=D1, error='IAM.DELEGATION_EXPIRED')])
    S('STD-LGS-060', 'a delegation not yet valid is refused', ctrl + [
        delegate(valid_from='2026-03-16T00:00:00.000000Z'), ex(),
        approve(CHECKER2, delegation=D1, error='IAM.DENIED', details=True)])
    S('STD-LGS-060', 'a revoked delegation is refused', ctrl + [
        delegate(), op('revoke_delegation', None, None, branch=BR, delegation_id=D1, principal=CHECKER,
                       reason='back from leave'),
        ex(), approve(CHECKER2, delegation=D1, error='IAM.DENIED', details=True)])
    S('STD-LGS-060', 'only the delegator revokes', [
        delegate(), op('revoke_delegation', None, 'IAM.DENIED', branch=BR, delegation_id=D1, principal=CHECKER2)])
    S('STD-LGS-060', 'only the delegate acts through a delegation', ctrl + [
        delegate(), ex(), approve(MAKER, delegation=D1, error='IAM.DENIED', details=True)])
    S('STD-LGS-060', 'a delegation for other ports does not cover the call', ctrl + [
        delegate(scope={'ports': ['rename']}), ex(), approve(CHECKER2, delegation=D1, error='IAM.DENIED', details=True)])
    S('STD-LGS-060', 'separation of duties holds for the delegator: the maker does not approve through a delegate',
      ctrl + [delegate(delegator=MAKER, roles=('clerk',)), ex(),
              approve(CHECKER2, delegation=D1, error='IAM.DENIED', details=True)])
    S('STD-LGS-060', 'a limit covers port calls whose parameter stays at or below the maximum', [
        commit([ACCT, ACC]),
        commit([sheet(SHEET, 'd', 'Account { approvals: {count: 1, roles: ["controller"]}; }')]),
        delegate(scope={'ports': ['rename'], 'limit': {'param': 'name', 'max': '5'}}),
        ex('rename', params={'name': 'Acme Ltd'}), approve(CHECKER2, delegation=D1, error='IAM.DENIED', details=True)])
    S('STD-LGS-060', 'the delegations of a principal for an access review, with their state', [
        delegate(), delegate(uid(651), delegator=LEAD, delegate_=CHECKER2, roles=('ap_lead',),
                             scope={'task_types': ['invoice_review']}, valid_from='2026-03-16T00:00:00.000000Z'),
        delegate(uid(652), delegator=CAROL, delegate_=DAVE, roles=('senior',), scope={'task_types': ['x']}),
        op('revoke_delegation', None, None, branch=BR, delegation_id=D1, principal=CHECKER),
        op('delegations', None, None, branch=BR, principal=CHECKER2)])

    # ---- STD-LGS-061 tasks ----
    S('STD-LGS-061', 'a task is created open in its queue with its log', [
        commit([queue()]), new(subject=uid(1), priority=2, data={'invoice': 'INV-7'})])
    S('STD-LGS-061', 'a task has a queue or an assignee', [
        new(queue_=None, error='MODEL.DECLARATION_INCOMPLETE')])
    S('STD-LGS-061', 'a task in an unknown queue is not created', [new(queue_=uid(699), error='IAM.NOT_FOUND')])
    S('STD-LGS-061', 'a member claims an unassigned task of the queue', [
        commit([queue()]), new(), task('claim', principal=BOB)])
    S('STD-LGS-061', 'a principal outside the queue does not claim', [
        commit([queue()]), new(), task('claim', principal=DAVE, error='IAM.DENIED', details=True)])
    S('STD-LGS-061', 'of two claims the second fails: exactly one principal owns the task', [
        commit([queue()]), new(), task('claim', principal=BOB),
        task('claim', principal=ALICE, error='FLOW.TRANSITION_NOT_ALLOWED', details=True)])
    S('STD-LGS-061', 'an action on a version the task has left fails with TXN.HEAD_MOVED', [
        commit([queue()]), new(), op('read', 'v', None, branch=BR, object_id=T1), task('claim', principal=BOB),
        task('release', principal=BOB), task('claim', principal=ALICE, base={'$ref': 'v/version_hash'},
                                             error='TXN.HEAD_MOVED')])
    S('STD-LGS-061', 'the owner releases a claimed task back to open', [
        commit([queue()]), new(), task('claim', principal=BOB), task('release', principal=BOB)])
    S('STD-LGS-061', 'only the owner releases', [
        commit([queue()]), new(), task('claim', principal=BOB), task('release', principal=ALICE, error='IAM.DENIED')])
    S('STD-LGS-061', 'the owner completes a claimed task with its outcome', [
        commit([queue()]), new(), task('claim', principal=BOB), now('2026-03-15T11:00:00.000000Z'),
        task('complete', principal=BOB, outcome={'decision': 'pay'})])
    S('STD-LGS-061', 'an open task is not completed', [
        commit([queue()]), new(), task('complete', principal=BOB, error='FLOW.TRANSITION_NOT_ALLOWED')])
    S('STD-LGS-061', 'a manager reassigns a task to a principal, who then claims it', [
        commit([queue()]), new(), task('claim', principal=BOB),
        task('reassign', principal=LEAD, to={'principal': DAVE}, reason='Bob is out'),
        task('claim', principal=DAVE)])
    S('STD-LGS-061', 'reassigning needs a manager role of the queue', [
        commit([queue()]), new(), task('reassign', principal=BOB, to={'principal': DAVE},
                                       error='IAM.DENIED', details=True)])
    S('STD-LGS-061', 'a task assigned to a role is claimed by a holder of the role', [
        new(queue_=None, assignee={'role': 'senior'}), task('claim', principal=ALICE, error='IAM.DENIED'),
        task('claim', principal=CAROL)])
    S('STD-LGS-061', 'the creator cancels an open task; a cancelled task is not claimed', [
        commit([queue()]), new(principal=ALICE), task('cancel', principal=ALICE, reason='duplicate'),
        task('claim', principal=BOB, error='FLOW.TRANSITION_NOT_ALLOWED')])
    S('STD-LGS-061', 'a commit does not change a task', [
        commit([queue()]), new(), op('read', 'v', None, branch=BR, object_id=T1),
        commit([{'op': 'update', 'object_id': T1, 'base': {'$ref': 'v/version_hash'}, 'body': {'priority': 9}}],
               error='LOGIC.FORBIDDEN_TARGET')])
    S('STD-LGS-061', 'a delegate claims and completes a task of the delegator; the log records both principals', [
        commit([queue()]), new(assignee={'principal': ALICE}),
        delegate(delegator=ALICE, delegate_=DAVE, roles=(), scope={'task_types': ['invoice_review']}),
        task('claim', principal=DAVE, delegation=D1), task('complete', principal=DAVE, delegation=D1,
                                                          outcome={'decision': 'hold'})])
    S('STD-LGS-061', 'a delegation for other task types does not cover the task', [
        commit([queue()]), new(assignee={'principal': ALICE}),
        delegate(delegator=ALICE, delegate_=DAVE, roles=(), scope={'task_types': ['vendor_setup']}),
        task('claim', principal=DAVE, delegation=D1, error='IAM.DENIED', details=True)])

    # ---- STD-LGS-062 assignment and inbox ----
    S('STD-LGS-062', 'round robin assigns tasks to the members in turn', [
        commit([queue(assignment='round_robin')]), new(T1), new(T2), new(T3), new(T4)])
    S('STD-LGS-062', 'least loaded assigns to the member with the fewest open or claimed tasks', [
        commit([queue(assignment='least_loaded')]), new(T1), new(T2), task('claim', T1, principal=ALICE),
        task('complete', T1, principal=ALICE), new(T3)])
    S('STD-LGS-062', 'skill matching assigns among the members with every skill of the task', [
        commit([queue(assignment='skill', skills={ALICE: ['usd'], BOB: ['usd', 'eur'], CAROL: ['eur']})]),
        new(T1, skills=['usd', 'eur']), new(T2, skills=['eur']), new(T3, skills=['jpy'])])
    S('STD-LGS-062', 'a queue with an unknown assignment rule is not committed', [
        commit([queue(assignment='random')], error='MODEL.CONSTRAINT_VIOLATED')])
    S('STD-LGS-062', 'the inbox combines assigned, queue and role tasks, ordered by due time then priority', [
        commit([queue()]), new(T1, due='2026-03-17T00:00:00.000000Z', priority=1),
        new(T2, assignee={'principal': BOB}, priority=5),
        new(T3, queue_=None, assignee={'role': 'clerk'}, due='2026-03-16T00:00:00.000000Z'),
        new(T4, assignee={'principal': CAROL}), inbox(BOB)])
    S('STD-LGS-062', 'the inbox sorted by priority', [
        commit([queue()]), new(T1, due='2026-03-17T00:00:00.000000Z', priority=1), new(T2, priority=5),
        inbox(ALICE, sort='priority')])
    S('STD-LGS-062', 'the inbox lists the pending changes the principal may decide', [
        commit([ACCT, ACC]), commit([sheet(SHEET, 'd', 'Account { approvals: {count: 1, roles: ["controller"]}; }')]),
        ex(), inbox(CHECKER), inbox(CHECKER2), inbox(MAKER)])
    S('STD-LGS-062', 'the inbox counts overdue tasks', [
        commit([queue()]), new(T1, due='2026-03-15T09:00:00.000000Z'), new(T2, due='2026-03-16T09:00:00.000000Z'),
        inbox(ALICE)])
    S('STD-LGS-062', 'the inbox of a delegate shows the delegator\'s tasks on behalf of the delegator', [
        commit([queue()]), new(T1, assignee={'principal': ALICE}),
        delegate(delegator=ALICE, delegate_=DAVE, roles=(), scope={'task_types': ['invoice_review']}),
        inbox(DAVE)])

    # ---- STD-LGS-063 due dates and escalation ----
    rule = 'add_business_days(params.created, 2, "us")'
    S('STD-LGS-063', 'a two-business-day rule on a Friday gives Tuesday', [
        now('2026-03-13T15:00:00.000000Z'), commit([queue(due_rule=rule, calendars={'us': {'weekend': [6, 7]}})]),
        new()])
    S('STD-LGS-063', 'the due rule skips the holidays of the queue calendar', [
        commit([queue(due_rule=rule, calendars=CAL)]), new()])
    S('STD-LGS-063', 'a due rule over the priority', [
        commit([queue(due_rule='params.priority >= 5 ? params.created : add_business_days(params.created, 3, "us")',
                      calendars=CAL)]),
        new(T1, priority=5), new(T2, priority=1)])
    S('STD-LGS-063', 'a due rule that gives no date fails the creation', [
        commit([queue(due_rule='params.priority')]), new(error='MODEL.INVALID_VALUE')])
    S('STD-LGS-063', 'a given due time overrides the rule', [
        commit([queue(due_rule=rule, calendars=CAL)]), new(due='2026-03-15T18:00:00.000000Z')])
    chain = [{'after': 'PT0S', 'action': 'notify', 'to': {'role': 'ap_lead'}},
             {'after': 'P1D', 'action': 'raise_priority', 'priority': 9},
             {'after': 'P2D', 'action': 'reassign', 'to': {'principal': CAROL}}]
    S('STD-LGS-063', 'escalation steps apply in order as the clock reaches them', [
        commit([queue(escalation=chain)]), new(due='2026-03-16T00:00:00.000000Z'), task('claim', principal=BOB),
        now('2026-03-16T00:00:00.000000Z'), op('read', None, None, branch=BR, object_id=T1),
        now('2026-03-18T12:00:00.000000Z'), op('read', None, None, branch=BR, object_id=T1),
        op('outbox', None, None)])
    S('STD-LGS-063', 'an escalation step applies once per task', [
        commit([queue(escalation=chain[:1])]), new(due='2026-03-16T00:00:00.000000Z'),
        now('2026-03-16T01:00:00.000000Z'), now('2026-03-16T02:00:00.000000Z'), op('outbox', None, None)])
    S('STD-LGS-063', 'a completed task is not escalated', [
        commit([queue(escalation=chain[:1])]), new(due='2026-03-16T00:00:00.000000Z'), task('claim', principal=BOB),
        task('complete', principal=BOB), now('2026-03-16T01:00:00.000000Z'), op('outbox', None, None)])
    S('STD-LGS-063', 'a queue with an unknown escalation action is not committed', [
        commit([queue(escalation=[{'after': 'PT1H', 'action': 'page'}])], error='MODEL.INVALID_VALUE')])

    # ---- STD-LGS-064 service-level report ----
    S('STD-LGS-064', 'attainment per type and queue, with late and open overdue tasks', [
        commit([queue(), queue(Q2, 'vendor-setup', members=(DAVE,))]),
        new(T1, due='2026-03-15T12:00:00.000000Z'), new(T2, due='2026-03-15T10:30:00.000000Z'),
        new(T3, due='2026-03-15T11:00:00.000000Z', type_='vendor_setup', queue_=Q2),
        new(T4, due='2026-03-15T10:00:00.000000Z'),
        task('claim', T1, principal=ALICE), task('claim', T2, principal=BOB), task('claim', T3, principal=DAVE),
        now('2026-03-15T11:00:00.000000Z'),
        task('complete', T1, principal=ALICE), task('complete', T2, principal=BOB), task('complete', T3, principal=DAVE),
        op('sla_report', None, None, branch=BR, **{'from': '2026-03-01', 'to': '2026-04-01'})])
    S('STD-LGS-064', 'tasks completed outside the window are not counted', [
        commit([queue()]), new(T1, due='2026-03-15T12:00:00.000000Z'), task('claim', T1, principal=ALICE),
        task('complete', T1, principal=ALICE),
        op('sla_report', None, None, branch=BR, **{'from': '2026-04-01', 'to': '2026-05-01'})])

    # ---- STD-TXN-060 / STD-KIND-025 governed reopening ----
    period = cls('FiscalPeriod', PERIOD['fields'], ports=[port('reopen', 'acme.period#reopen',
                                                              params=[field('reason', 'text', True)])])
    per = create(uid(50), 'FiscalPeriod', '2026-03', {'start': '2026-03-01', 'end': '2026-04-01', 'state': 'closing'})
    closed = [commit([per]), op('close_period', None, None, branch=BR, period=uid(50), artefact_id=uid(60))]
    proc = {'process_id': uid(520), 'initiator': MAKER}
    gov = [commit([sheet(SHEET, 'd', 'FiscalPeriod { approvals: {count: 1, roles: ["controller"]}; }')])]
    S('STD-TXN-060', 'a reopening that requires approval is refused without defer', closed + gov + [
        op('reopen_period', None, 'RULE.DECISION_REQUIRES', branch=BR, period=uid(50), reason='late invoice',
           process=proc)], classes=RULE_CLASSES + [period, GL], also=('STD-KIND-025',))
    S('STD-TXN-060', 'with defer a reopening waits as a pending change; the approval reopens the period with the reason',
      closed + gov + [
          op('reopen_period', None, None, branch=BR, period=uid(50), reason='late invoice', process=proc, defer=True),
          op('read', None, None, branch=BR, object_id=uid(50)),
          op('approve', None, None, pending_change_id=uid(520), approver=CHECKER, roles=['controller']),
          op('read', None, None, branch=BR, object_id=uid(50))], classes=RULE_CLASSES + [period, GL], also=('STD-KIND-025',))
