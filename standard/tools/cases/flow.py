"""Vector cases of BPA 1.0-rc3 for lifecycles (PH-1 step 12a, DEC-071): the lifecycle
declaration (STD-CLS-004), `commit.transition` (STD-ISA-017), `read.actions` (STD-ISA-036),
the availability and guards of ports (STD-ISA-050) and the lifecycle findings of the BPU
class verification (STD-BPU-011). Not required of the PH-0 prototype."""
from .common import cls, field, uid, create, base, NOW
from .common import commit as _commit
from .model import port
from .sheets import sheet, op
from .execution import asset, fn, SEED

BR = 'feature/flow'
FB = {'op': 'create_branch', 'input': {'name': BR, 'from': 'main'}}
LC = 'ubos://acme/Lifecycle/invoice'
INV = 'ubos://acme/Invoice/inv-1'
SHEET = uid(900)


def commit(changes, branch=BR, **kw):
    return _commit(changes, branch=branch, **kw)


def p(name, impl, transition=None, states=None, guards=None, kind='command', params=()):
    d = port(name, impl, kind=kind, params=params)
    if transition:
        d['transition'] = transition
    if states is not None:
        d['states'] = list(states)
    if guards is not None:
        d['guards'] = list(guards)
    return d


FIELDS = [field('number', 'text', True), field('amount', 'decimal'), field('approved_by', 'text'),
          field('note', 'text'), field('revision', 'integer', default=0)]
PORTS = [p('submit', 'acme.inv#submit', transition='submit'),
         p('approve', 'acme.inv#approve', transition='approve', params=[field('reason', 'text', True)]),
         p('pay', 'acme.inv#pay', transition='pay'),
         p('reopen', 'acme.inv#reopen', transition='reopen'),
         p('edit', 'acme.inv#edit', states=['draft'], params=[field('note', 'text', True)]),
         p('discount', 'acme.inv#discount', guards=['this.amount > 100'], params=[field('pct', 'integer', True)]),
         p('peek', 'acme.inv#peek', kind='query', states=['approved', 'paid'])]
INVOICE = cls('Invoice', FIELDS, ports=PORTS, lifecycle=LC)
PLAIN = cls('Memo', [field('text', 'text', True)], ports=[p('touch', 'acme.inv#touch')])
CLASSES = [INVOICE, PLAIN]


def lifecycle(oid=uid(70), slug='invoice', states=('draft', 'submitted', 'approved', 'paid'), initial='draft',
              final=('paid',), transitions=None, effects=None, field_rules=None, name='acme.invoice', **extra):
    body = {'name': name, 'states': list(states), 'initial': initial, 'final': list(final),
            'transitions': TRANSITIONS if transitions is None else transitions, 'description': 'invoice lifecycle'}
    if effects is not None:
        body['effects'] = effects
    if field_rules is not None:
        body['field_rules'] = field_rules
    body.update(extra)
    return create(oid, 'Lifecycle', slug, body)


def tr(name, from_, to, guards=None):
    t = {'name': name, 'from': list(from_), 'to': to, 'description': name}
    if guards is not None:
        t['guards'] = guards
    return t


TRANSITIONS = [tr('submit', ['draft'], 'submitted'),
               tr('approve', ['submitted'], 'approved',
                  [{'name': 'positive', 'rule': 'this.amount > 0', 'message': 'the amount must be positive'},
                   {'name': 'numbered', 'rule': 'len(this.number) > 2', 'message': 'the number is at least 3 characters'}]),
               tr('pay', ['approved'], 'paid'),
               tr('reopen', ['submitted', 'approved'], 'draft')]
EFFECTS = {'draft': {'exit': [{'set': {'revision': 'this.revision + 1'}}]},
           'approved': {'entry': [{'logic': 'acme.inv#on_approve'}]},
           'paid': {'entry': [{'set': {'note': '"paid"'}}]}}
FIELD_RULES = {'approved': {'required': ['approved_by']}, 'paid': {'read_only': ['amount', 'number'], 'hidden': ['note']}}


def transition(name, target='self_id'):
    return {'do': 'commit.transition', 'args': {'target': target, 'transition': '"%s"' % name}, 'as': 't'}


READ = {'do': 'read.get', 'args': {'target': 'self_id'}, 'as': 'o'}
ASSET = asset(80, 'acme.inv', {
    'submit': fn([READ, transition('submit'), {'return': 't'}]),
    'approve': fn([READ, transition('approve'), {'return': 't'}], ['reason']),
    'pay': fn([READ, transition('pay'), {'return': 't'}]),
    'reopen': fn([READ, transition('reopen'), {'return': 't'}]),
    'edit': fn([READ, {'do': 'commit.update', 'args': {'target': 'self_id', 'set': {'note': 'params.note'}}},
                {'return': '"edited"'}], ['note']),
    'discount': fn([READ, {'do': 'commit.update', 'args': {'target': 'self_id', 'set': {'note': '"discounted"'}}},
                    {'return': 'params.pct'}], ['pct']),
    'peek': fn([READ, {'return': 'o.state'}]),
    'touch': fn([READ, {'return': 'o.fields.text'}]),
    'on_approve': fn([{'do': 'commit.update', 'args': {'target': 'self_id', 'set': {'approved_by': 'params.reason'}}}],
                     ['reason'])})
# variants of the host program for the error cases
WRONG = asset(80, 'acme.inv', {
    'submit': fn([READ, transition('submit'), {'return': 't'}]),
    'edit': fn([READ, transition('submit'), {'return': 't'}], ['note']),
    'reopen': fn([READ, transition('reopen'), transition('reopen'), {'return': 't'}]),
    'discount': fn([READ, {'do': 'commit.update', 'args': {'target': 'self_id', 'set': {'state': '"paid"'}}},
                    {'return': '"edited"'}], ['pct']),
    'approve': fn([transition('approve'), {'return': 't'}], ['reason'])})
FAILING = asset(80, 'acme.inv', {
    'submit': fn([READ, transition('submit'), {'return': 't'}]),
    'approve': fn([READ, transition('approve'), {'return': 't'}], ['reason']),
    'on_approve': fn([{'do': 'commit.update', 'args': {'target': 'self_id', 'set': {'approved_by': 'params.missing.x'}}}],
                     ['reason'])})
ACTIONS = asset(90, 'acme.app', {
    'main': fn([{'do': 'read.actions', 'args': {'target': '"%s"' % INV}, 'as': 'a'}, {'return': 'a'}])},
    caps=('read',))


def inv(oid=uid(1), slug='inv-1', number='INV-1', amount='250', **extra):
    body = {'number': number, 'amount': amount, 'revision': 0}
    body.update(extra)
    return create(oid, 'Invoice', slug, body)


def ex(port_, params=None, label=None, error=None, target=INV, branch=BR, pid=1, ctx=None, grants=None, details=False,
       **kw):
    inp = {'branch': branch, 'target': target, 'port': port_, 'params': params or {},
           'process': {'process_id': uid(500 + pid)}, 'entropy_seed': SEED}
    if ctx is not None:
        inp['ctx'] = ctx
    if grants is not None:
        inp['grants'] = grants
    inp.update(kw)
    s = {'op': 'execute', 'input': inp}
    if label:
        s['label'] = label
    if error:
        s['error'] = error
    if details:
        s['details'] = True
    return s


def run(label=None, error=None, branch=BR, pid=1, grants=None, **kw):
    inp = {'branch': branch, 'asset': 'acme.app', 'function': 'main', 'args': [], 'process': {'process_id': uid(500 + pid)},
           'entropy_seed': SEED}
    if grants is not None:
        inp['grants'] = grants
    inp.update(kw)
    s = {'op': 'execute', 'input': inp}
    if label:
        s['label'] = label
    if error:
        s['error'] = error
    return s


def read(oid=uid(1), label=None, branch=BR, **sel):
    inp = {'branch': branch, 'object_id': oid}
    inp.update(sel)
    return op('read', label, **inp)


def actions(oid=uid(1), label=None, error=None, branch=BR, ctx=None):
    inp = {'branch': branch, 'object_id': oid}
    if ctx is not None:
        inp['ctx'] = ctx
    return op('actions', label, error, **inp)


def deploy(*more, lc=None, effects=EFFECTS, field_rules=FIELD_RULES):
    return [commit([ASSET, lc or lifecycle(effects=effects, field_rules=field_rules)] + list(more), label='d')]


def cases(g):
    V = g.vector

    def S(clause, desc, steps, classes=None, also=()):
        V(clause, desc, setup=[{'op': 'set_now', 'input': {'now': NOW}},
                               {'op': 'load_genesis', 'input': {'classes': classes or CLASSES}}, FB],
          steps=steps, prototype=False, also=also)

    declaration(S)
    transitions(S)
    availability(S)
    read_actions(S)
    verification(S)


def declaration(S):
    C = 'STD-CLS-004'
    S(C, 'a lifecycle object declares states, the initial state and transitions', deploy() + [read(uid(70))])
    S(C, 'an object of a class with a lifecycle is created in the initial state', deploy(inv()) + [read()])
    S(C, 'the state of a version is read with the object', deploy(inv()) + [ex('submit'), read()])
    S(C, 'the initial state must be a declared state', [commit([lifecycle(initial='open')], error='MODEL.INVALID_VALUE')])
    S(C, 'a transition leaves and enters declared states', [
        commit([lifecycle(transitions=[tr('submit', ['draft'], 'sent')])], error='MODEL.INVALID_VALUE')])
    S(C, 'a transition has a name, from and to', [
        commit([lifecycle(transitions=[{'name': 'submit', 'to': 'submitted'}])], error='MODEL.DECLARATION_INCOMPLETE')])
    S(C, 'transition names are distinct', [
        commit([lifecycle(transitions=[tr('submit', ['draft'], 'submitted'), tr('submit', ['submitted'], 'paid')])],
               error='MODEL.INVALID_VALUE')])
    S(C, 'a guard rule is an L1 expression', [
        commit([lifecycle(transitions=[tr('submit', ['draft'], 'submitted', [{'name': 'g', 'rule': 'this.amount >'}])])],
               error='LOGIC.TYPE_ERROR')])
    S(C, 'effects name declared states and are set or logic', [
        commit([lifecycle(effects={'paid': {'entry': [{'notify': 'x'}]}})], error='MODEL.INVALID_VALUE')])
    S(C, 'field rules name declared states and are read_only, required or hidden', [
        commit([lifecycle(field_rules={'paid': {'frozen': ['amount']}})], error='MODEL.INVALID_VALUE')])
    S(C, 'the state is not written by a commit', deploy() + [
        commit([inv(state='paid')], error='FLOW.STATE_DIRECT_WRITE')])
    S(C, 'the state is not written by an update', deploy(inv()) + [
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('d', uid(1)), 'body': {'state': 'paid'}}],
               error='FLOW.STATE_DIRECT_WRITE')])
    S(C, 'a class with a lifecycle declares no field state', [
        {'op': 'load_genesis', 'input': {'classes': CLASSES + [cls('Bill', FIELDS + [field('state', 'text')], lifecycle=LC)]},
         'error': 'MODEL.INVALID_VALUE'}])
    S(C, 'the lifecycle of a class must resolve on the branch', [commit([ASSET, inv()], error='MODEL.REFERENCE_INVALID')])
    S(C, 'a read-only field of a state is not updated', deploy(inv()) + [
        ex('submit', pid=1), ex('approve', {'reason': 'ok'}, pid=2), ex('pay', pid=3), read(label='r'),
        commit([{'op': 'update', 'object_id': uid(1), 'base': {'$ref': 'r/version_hash'}, 'body': {'amount': '1'}}],
               error='FLOW.FIELD_RULE_VIOLATED')])
    S(C, 'other fields of a state with read-only fields are updated', deploy(inv()) + [
        ex('submit', pid=1), ex('approve', {'reason': 'ok'}, pid=2), ex('pay', pid=3), read(label='r'),
        commit([{'op': 'update', 'object_id': uid(1), 'base': {'$ref': 'r/version_hash'}, 'body': {'note': 'thanks'}}]), read()])
    S(C, 'a subclass lifecycle keeps the states and transitions of the parent lifecycle', deploy(
        lifecycle(uid(71), 'bill', states=('draft', 'submitted', 'approved', 'paid', 'disputed'),
                  transitions=TRANSITIONS + [tr('dispute', ['approved'], 'disputed')], name='acme.bill')) + [read(uid(71))],
        classes=CLASSES + [cls('Bill', [field('dispute', 'text')], parent='Invoice', lifecycle='ubos://acme/Lifecycle/bill')])
    S(C, 'a subclass lifecycle that drops a transition narrows the parent lifecycle', [
        commit([ASSET, lifecycle(effects=EFFECTS, field_rules=FIELD_RULES),
                lifecycle(uid(71), 'bill', transitions=TRANSITIONS[:3], name='acme.bill')], error='MODEL.NARROWING_VIOLATION')],
        classes=CLASSES + [cls('Bill', [field('dispute', 'text')], parent='Invoice', lifecycle='ubos://acme/Lifecycle/bill')])
    S(C, 'a subclass lifecycle with another initial state narrows the parent lifecycle', [
        commit([ASSET, lifecycle(effects=EFFECTS, field_rules=FIELD_RULES),
                lifecycle(uid(71), 'bill', states=('new', 'draft', 'submitted', 'approved', 'paid'), initial='new',
                          transitions=TRANSITIONS + [tr('start', ['new'], 'draft')], name='acme.bill')],
               error='MODEL.NARROWING_VIOLATION')],
        classes=CLASSES + [cls('Bill', [field('dispute', 'text')], parent='Invoice', lifecycle='ubos://acme/Lifecycle/bill')])
    S(C, 'a subclass without its own lifecycle inherits the nearest one', deploy(
        create(uid(2), 'Bill', 'b-1', {'number': 'B-1', 'amount': '10'})) + [read(uid(2))],
        classes=CLASSES + [cls('Bill', [field('dispute', 'text')], parent='Invoice')])


def transitions(S):
    C = 'STD-ISA-017'
    S(C, 'a transition moves the object to the target state, runs the exit effects and returns from and to', deploy(inv()) + [ex('submit'), read()])
    S(C, 'the port must name the transition', [
        commit([WRONG, lifecycle(), inv()], label='d'), ex('edit', {'note': 'x'}, error='FLOW.TRANSITION_NOT_ALLOWED', details=True)])
    S(C, 'the transition must leave the current state', deploy(inv()) + [ex('pay', error='FLOW.PORT_UNAVAILABLE')])
    S(C, 'the second transition of a process leaves the state the first entered', [
        commit([WRONG, lifecycle(), inv()], label='d'), ex('submit', pid=1),
        ex('reopen', pid=2, error='FLOW.TRANSITION_NOT_ALLOWED', details=True)])
    S(C, 'logic does not write the state', [
        commit([WRONG, lifecycle(), inv()], label='d'), ex('discount', {'pct': 1}, error='FLOW.STATE_DIRECT_WRITE')])
    S(C, 'the target must have been read', [
        commit([WRONG, lifecycle(), inv()], label='d'),
        ex('submit', pid=1), ex('approve', {'reason': 'ok'}, pid=2, error='MODEL.DECLARATION_INCOMPLETE')])
    S(C, 'a transition of a later process leaves the state the earlier one entered', deploy(inv()) + [
        ex('submit', pid=1), ex('approve', {'reason': 'ok'}, pid=2), read()])
    S(C, 'a failing guard raises FLOW.GUARD_FAILED with every failing guard and stages nothing',
      deploy(inv(amount='0', number='X')) + [ex('submit', pid=1), ex('approve', {'reason': 'ok'}, pid=2, error='FLOW.GUARD_FAILED', details=True),
                                           read()])
    S(C, 'guards see the pending changes of the object', deploy(inv(amount='0')) + [
        ex('submit', pid=1), read(label='r'),
        commit([{'op': 'update', 'object_id': uid(1), 'base': {'$ref': 'r/version_hash'}, 'body': {'amount': '5'}}]),
        ex('approve', {'reason': 'ok'}, pid=2), read()])
    S(C, 'a logic entry effect runs in the process with the port parameters', deploy(inv()) + [
        ex('submit', pid=1), ex('approve', {'reason': 'cfo'}, pid=2), read()])
    S(C, 'a set entry effect runs after the state change', deploy(inv()) + [
        ex('submit', pid=1), ex('approve', {'reason': 'cfo'}, pid=2), ex('pay', pid=3), read()])
    S(C, 'a failing effect raises FLOW.EFFECT_FAILED', [
        commit([FAILING, lifecycle(effects={'approved': {'entry': [{'logic': 'acme.inv#on_approve'}]}}), inv()], label='d'),
        ex('submit', pid=1), ex('approve', {'reason': 'cfo'}, pid=2, error='FLOW.EFFECT_FAILED', details=True)])
    S(C, 'a required field of the entered state must be present', deploy(
        inv(), lc=lifecycle(effects={}, field_rules=FIELD_RULES)) + [
        ex('submit', pid=1), ex('approve', {'reason': 'cfo'}, pid=2, error='FLOW.FIELD_RULE_VIOLATED', details=True)])
    S(C, 'the commit of a transition records the guards in the rule binding', deploy(inv()) + [
        ex('submit', pid=1), ex('approve', {'reason': 'cfo'}, pid=2), op('commit_info', branch=BR)])
    S(C, 'a transition back to the initial state', deploy(inv()) + [ex('submit', pid=1), ex('reopen', pid=2), read()])


def availability(S):
    C = 'STD-ISA-050'
    S(C, 'a port with states is unavailable outside them', deploy(inv()) + [
        ex('submit', pid=1), ex('edit', {'note': 'x'}, pid=2, error='FLOW.PORT_UNAVAILABLE')])
    S(C, 'a port with states runs in them', deploy(inv()) + [ex('edit', {'note': 'x'}), read()])
    S(C, 'a transition port is unavailable when its transition does not leave the state', deploy(inv()) + [
        ex('pay', error='FLOW.PORT_UNAVAILABLE', details=True)])
    S(C, 'a query port is subject to states', deploy(inv()) + [
        ex('peek', pid=1, error='FLOW.PORT_UNAVAILABLE'), ex('submit', pid=2), ex('approve', {'reason': 'x'}, pid=3),
        ex('peek', pid=4)])
    S(C, 'availability is checked before the permission', deploy(inv()) + [
        ex('pay', grants=['read'], error='FLOW.PORT_UNAVAILABLE')])
    S(C, 'a failing port guard raises FLOW.GUARD_FAILED', deploy(inv(amount='50')) + [
        ex('discount', {'pct': 5}, error='FLOW.GUARD_FAILED', details=True)])
    S(C, 'port guards see the parameters', deploy(inv()) + [ex('discount', {'pct': 5})])
    S(C, 'the port guards passed are recorded in the rule binding', deploy(inv()) + [
        ex('discount', {'pct': 5}), op('commit_info', branch=BR)])
    S(C, 'a class without a lifecycle has no state', deploy(create(uid(3), 'Memo', 'm-1', {'text': 'hi'})) + [
        ex('touch', target='ubos://acme/Memo/m-1'), read(uid(3))])


def read_actions(S):
    C = 'STD-ISA-036'
    S(C, 'read.actions lists the ports of an object with their availability', deploy(inv(), ACTIONS) + [run()])
    S(C, 'a port excluded by the state carries reason state', deploy(inv(), ACTIONS) + [ex('submit'), run(pid=2)])
    S(C, 'a port the principal lacks carries reason permission', deploy(inv(), ACTIONS) + [
        run(grants=['read', 'submit', 'edit'])])
    S(C, 'a port a decision denies carries reason decision', deploy(inv(), ACTIONS, sheet(
        SHEET, 'd', 'Invoice { decisions: [{name: "no_edit", outcome: "deny", message: "no edits"}]; }')) + [run()])
    S(C, 'a port whose guard fails without parameters carries reason guard', deploy(inv(amount='50'), ACTIONS) + [run()])
    S(C, 'a transition guard that fails carries reason guard', deploy(inv(amount='0'), ACTIONS) + [ex('submit'), run(pid=2)])
    S(C, 'the actions of an unknown object are not found', deploy(ACTIONS) + [run(error='IAM.NOT_FOUND')])
    S(C, 'the adapter operation actions permits every port', deploy(inv()) + [actions()])
    S(C, 'the adapter operation actions takes the context the decisions see', deploy(inv(), sheet(
        SHEET, 'd', 'Invoice { decisions: [{name: "humans", when: "ctx.principal_kind == \\"agent\\"", outcome: "deny"}]; }')) + [
        actions(label='human'), actions(label='agent', ctx={'principal_kind': 'agent'})])
    S(C, 'an object without a lifecycle lists its ports with a null state', deploy(
        create(uid(3), 'Memo', 'm-1', {'text': 'hi'})) + [actions(uid(3))])


def verification(S):
    C = 'STD-BPU-011'
    S(C, 'a lifecycle whose states are all reachable and leave passes the lifecycle check', deploy() + [
        op('verify_asset', branch=BR, object_id=uid(80)), op('verify_bpu', branch=BR, **{'class': 'Invoice'})])
    S(C, 'an unreachable state and a dead end are lifecycle findings', deploy(lc=lifecycle(
        states=('draft', 'submitted', 'approved', 'paid', 'void'), transitions=TRANSITIONS[:3])) + [
        op('verify_asset', branch=BR, object_id=uid(80)), op('verify_bpu', branch=BR, **{'class': 'Invoice'})])
