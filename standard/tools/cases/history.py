"""Vector cases of BPA 1.0-rc1 for versioning, time and processes (PH-1 step 1c, DEC-041):
branch kinds and write policy (STD-VER-011, -012), change sets and approvals (STD-VER-020,
-021), tree diff (STD-VER-031), merge validation, previews and resolutions (STD-VER-055,
-057, -058), tags and releases (STD-VER-060, -061), revert (STD-VER-070, -071), branch
inheritance (STD-VER-080, -081), snapshot selection (STD-VER-090), bitemporal reads and
evidence (STD-TIME-013, -014, -015), impact sets (STD-TIME-031), rule binding (STD-TIME-040),
became-effective events (STD-TIME-050), derivation records, trigger lineage, idempotency,
the outbox and bulk processes (STD-TXN-030, -031, -040, -050, -070). Not required of the PH-0
prototype."""
from .common import CLASSES, cls, field, uid, commit, create, base, usd, gl, stock, NOW, PERIOD, CUSTOMER

TAX = cls('TaxRule', [field('rate', 'decimal', True), field('note', 'text')], parent='ConfigAsset')
BUDGET = cls('BudgetLine', [field('account', 'text', True), field('amount', 'decimal', True)], kind='ledger',
             ledger={'key': ['account'], 'amounts': ['amount'], 'double_entry': False}, write_policy='change_set')
PRICE = cls('PriceList', [field('price', 'decimal', True)], emit_became_effective=True)
ALL = CLASSES + [TAX, BUDGET, PRICE]


def uri(klass, slug):
    return 'ubos://acme/%s/%s' % (klass, slug)


def ref(name, target, relationship=None):
    f = field(name, 'reference', constraints={'target_class': target})
    if relationship:
        f['relationship'] = relationship
    return f


def op(o, label=None, error=None, **inp):
    s = {'op': o, 'input': inp}
    if label:
        s['label'] = label
    if error:
        s['error'] = error
    return s


def br(name, frm='main', **kw):
    return op('create_branch', name=name, **dict({'from': frm}, **kw))


def read(oid, branch='main', label=None, error=None, **kw):
    return op('read', label, error, branch=branch, object_id=oid, **kw)


def upd(oid, label, body, **kw):
    ch = {'op': 'update', 'object_id': oid, 'base': base(label, oid), 'body': body}
    ch.update(kw)
    return ch


def end(oid, label, **kw):
    ch = {'op': 'end', 'object_id': oid, 'base': base(label, oid)}
    ch.update(kw)
    return ch


def now(t):
    return {'op': 'set_now', 'input': {'now': t}}


def ref_(label, ptr):
    return {'$ref': '%s/%s' % (label, ptr)}


def cs(action, id_, label=None, error=None, **kw):
    return op('change_set', label, error, action=action, id=id_, **kw)


def P(n, **kw):
    """A process with an explicit identifier."""
    d = {'process_id': uid(500 + n)}
    d.update(kw)
    return d


def cases(g):
    V = g.vector

    def S(clause, desc, steps, classes=None, now_=NOW, also=()):
        V(clause, desc, setup=[{'op': 'set_now', 'input': {'now': now_}},
                               {'op': 'load_genesis', 'input': {'classes': classes or ALL}}],
          steps=steps, prototype=False, also=also)

    c1 = create(uid(1), 'Customer', 'c-1', {'name': 'Acme'})
    c2 = create(uid(2), 'Customer', 'c-2', {'name': 'Beta'})
    t1 = create(uid(3), 'TaxRule', 'vat', {'rate': '0.2'})

    ver(S, c1, c2, t1)
    time_(S, c1)
    txn(S, c1, c2)


def ver(S, c1, c2, t1):
    # ---- STD-VER-011 branch-kind policies ----
    S('STD-VER-011', 'a sandbox branch cannot be merged into main', [
        br('sandbox/try'), commit([c1], branch='sandbox/try'),
        op('merge', error='VER.MERGE_NOT_ALLOWED', source='sandbox/try', target='main')])
    S('STD-VER-011', 'a sandbox branch cannot be merged into a working branch either', [
        br('sandbox/try'), br('feature/a'), commit([c1], branch='sandbox/try'),
        op('merge', error='VER.MERGE_NOT_ALLOWED', source='sandbox/try', target='feature/a')])
    S('STD-VER-011', 'an overlay branch only receives merges', [
        br('overlay/acme'), commit([c1], branch='overlay/acme'),
        op('merge', error='VER.MERGE_NOT_ALLOWED', source='overlay/acme', target='main')])
    S('STD-VER-011', 'main is not merged into a release branch', [
        br('release/v1'), commit([c1]),
        op('merge', error='VER.MERGE_NOT_ALLOWED', source='main', target='release/v1')])
    S('STD-VER-011', 'a release branch merges fixes into main', [
        br('release/v1'), commit([c1], branch='release/v1'),
        op('merge', source='release/v1', target='main'), read(uid(1))])
    S('STD-VER-011', 'a feature branch merges into a release branch', [
        br('release/v1'), br('feature/fix', 'release/v1'), commit([c1], branch='feature/fix'),
        op('merge', source='feature/fix', target='release/v1')])
    S('STD-VER-011', 'a feature branch merges into a draft branch', [
        br('draft/plan'), br('feature/a'), commit([c1], branch='feature/a'),
        op('merge', source='feature/a', target='draft/plan')])
    S('STD-VER-011', 'a working branch is refreshed from main', [
        br('sandbox/try'), commit([c1]), op('merge', source='main', target='sandbox/try'),
        read(uid(1), 'sandbox/try')])
    S('STD-VER-011', 'an agent branch reaches main by change set only', [
        br('agent/run-1'), commit([c1], branch='agent/run-1'),
        op('merge', error='VER.CHANGE_SET_REQUIRED', source='agent/run-1', target='main')])
    S('STD-VER-011', 'an agent branch reaches main through an approved change set', [
        br('agent/run-1'), commit([c1], branch='agent/run-1'),
        cs('open', uid(40), source='agent/run-1', target='main', title='agent result'),
        cs('submit', uid(40)), cs('approve', uid(40), approver='did:ubos:acme:01900000-0000-7000-8000-00000000000a'),
        cs('merge', uid(40)), read(uid(1))])
    S('STD-VER-011', 'branch kinds and default lifetimes are reported', [
        br('sandbox/try'), br('agent/run-1'), br('evolution/e1'), br('feature/a'), br('overlay/acme'),
        op('branch_info', branch='main'), op('branch_info', branch='sandbox/try'),
        op('branch_info', branch='agent/run-1'), op('branch_info', branch='evolution/e1'),
        op('branch_info', branch='feature/a'), op('branch_info', branch='overlay/acme')])
    S('STD-VER-011', 'a sandbox branch is archived after 14 days and refuses commits', [
        br('sandbox/try'), now('2026-03-29T10:00:00.000000Z'),
        op('branch_info', branch='sandbox/try'),
        commit([c1], branch='sandbox/try', error='VER.BRANCH_ARCHIVED')])
    S('STD-VER-011', 'a sandbox branch is still active before 14 days', [
        br('sandbox/try'), now('2026-03-29T09:59:59.999999Z'), commit([c1], branch='sandbox/try')])
    S('STD-VER-011', 'a commit keeps a draft branch active', [
        br('draft/plan'), now('2026-08-01T10:00:00.000000Z'), commit([c1], branch='draft/plan'),
        now('2027-01-20T10:00:00.000000Z'), op('branch_info', branch='draft/plan'),
        commit([c2], branch='draft/plan')])
    S('STD-VER-011', 'an inactive draft branch is archived', [
        br('draft/plan'), now('2026-09-11T10:00:00.000000Z'), op('branch_info', branch='draft/plan'),
        commit([c1], branch='draft/plan', error='VER.BRANCH_ARCHIVED')])
    S('STD-VER-011', 'main is created by bootstrap only', [br('main', error='TXN.ALREADY_EXISTS')])

    # ---- STD-VER-012 write policy on protected branches ----
    S('STD-VER-012', 'a ConfigAsset class is written on main only by a change set', [
        commit([t1], error='VER.CHANGE_SET_REQUIRED')])
    S('STD-VER-012', 'a ConfigAsset class is written directly on a working branch', [
        br('feature/tax'), commit([t1], branch='feature/tax'), read(uid(3), 'feature/tax')])
    S('STD-VER-012', 'a direct merge that changes a change_set class into main fails', [
        br('feature/tax'), commit([t1], branch='feature/tax'),
        op('merge', error='VER.CHANGE_SET_REQUIRED', source='feature/tax', target='main')])
    S('STD-VER-012', 'the merge commit of an approved change set writes a change_set class', [
        br('feature/tax'), commit([t1], branch='feature/tax'),
        cs('open', uid(40), source='feature/tax', target='main', title='VAT'),
        cs('submit', uid(40)), cs('approve', uid(40), approver='did:ubos:acme:01900000-0000-7000-8000-00000000000a'),
        cs('merge', uid(40)), read(uid(3))])
    S('STD-VER-012', 'overlay branches are protected', [
        br('overlay/acme'), commit([t1], branch='overlay/acme', error='VER.CHANGE_SET_REQUIRED')])
    S('STD-VER-012', 'release branches are protected', [
        br('release/v1'), commit([t1], branch='release/v1', error='VER.CHANGE_SET_REQUIRED')])
    S('STD-VER-012', 'direct classes are written on main', [commit([c1]), read(uid(1))])
    S('STD-VER-012', 'ledger appends follow the ledger class write policy: direct by default', [
        commit([stock(uid(10), 'bolt', '5')])])
    S('STD-VER-012', 'a ledger class with write_policy change_set refuses direct appends on main', [
        commit([{'op': 'append', 'class': 'BudgetLine', 'entry_id': uid(10), 'ledger_key': {'account': '4000'},
                 'valid_at': '2026-03-15', 'body': {'account': '4000', 'amount': '100'}}],
               error='VER.CHANGE_SET_REQUIRED')])
    S('STD-VER-012', 'an update of a change_set object on main needs a change set', [
        br('feature/tax'), commit([t1], branch='feature/tax'),
        cs('open', uid(40), source='feature/tax', target='main', title='VAT'),
        cs('submit', uid(40)), cs('approve', uid(40), approver='did:ubos:acme:01900000-0000-7000-8000-00000000000a'),
        cs('merge', uid(40)), read(uid(3), label='h'),
        commit([{'op': 'update', 'object_id': uid(3), 'base': ref_('h', 'version_hash'), 'body': {'rate': '0.3'}}],
               error='VER.CHANGE_SET_REQUIRED')])

    # ---- STD-VER-020 change set lifecycle ----
    fa = [br('feature/a'), commit([c1], branch='feature/a')]
    open_ = cs('open', uid(40), source='feature/a', target='main', title='new customer')
    S('STD-VER-020', 'opening a change set writes a ChangeSet object on the target', fa + [
        open_, read(uid(40))])
    S('STD-VER-020', 'the source of a change set is not protected', [
        br('feature/a'), cs('open', uid(40), error='VER.MERGE_NOT_ALLOWED', source='main', target='feature/a',
                            title='x')])
    S('STD-VER-020', 'the source and target of a change set exist', [
        cs('open', uid(40), error='IAM.NOT_FOUND', source='feature/none', target='main', title='x')])
    S('STD-VER-020', 'the change set identifier is a UUID', [
        br('feature/a'), cs('open', 'cs-1', error='MODEL.INVALID_VALUE', source='feature/a', target='main',
                            title='x')])
    S('STD-VER-020', 'submit records the merge preview', fa + [open_, cs('submit', uid(40)), cs('get', uid(40))])
    S('STD-VER-020', 'submit fails when the merge preview has conflicts', [
        commit([c1], label='m'), br('feature/a'),
        commit([upd(uid(1), 'm', {'email': 'a@x.com'})], branch='feature/a'),
        commit([upd(uid(1), 'm', {'email': 'b@x.com'})]),
        cs('open', uid(40), source='feature/a', target='main', title='email'),
        cs('submit', uid(40), error='VER.MERGE_CONFLICT'), cs('get', uid(40))])
    S('STD-VER-020', 'submit fails when the merged result does not validate', [
        commit([c1], label='m'), br('feature/a'),
        commit([create(uid(5), 'Invoice', 'i-1', {'customer': uri('Customer', 'c-1')})], branch='feature/a'),
        commit([end(uid(1), 'm')]),
        cs('open', uid(40), source='feature/a', target='main', title='invoice'),
        cs('submit', uid(40), error='MODEL.REFERENCE_INVALID')],
      classes=ALL + [cls('Invoice', [ref('customer', 'Customer')])])
    S('STD-VER-020', 'approve before submit is not a transition', fa + [
        open_, cs('approve', uid(40), error='FLOW.TRANSITION_NOT_ALLOWED', approver='did:ubos:acme:01900000-0000-7000-8000-00000000000a')])
    S('STD-VER-020', 'merge before approval is not a transition', fa + [
        open_, cs('submit', uid(40)), cs('merge', uid(40), error='FLOW.TRANSITION_NOT_ALLOWED')])
    S('STD-VER-020', 'the final approval makes the change set approved', fa + [
        open_, cs('submit', uid(40)), cs('approve', uid(40), approver='did:ubos:acme:01900000-0000-7000-8000-00000000000a')])
    S('STD-VER-020', 'a merged change set records its merge commit', fa + [
        open_, cs('submit', uid(40)), cs('approve', uid(40), approver='did:ubos:acme:01900000-0000-7000-8000-00000000000a'),
        cs('merge', uid(40), 'mg'), op('commit_info', 'ci', branch='main', time=ref_('mg', 'cseq')),
        cs('get', uid(40)), read(uid(1))])
    S('STD-VER-020', 'a rejected change set cannot be merged', fa + [
        open_, cs('submit', uid(40)), cs('reject', uid(40), approver='did:ubos:acme:01900000-0000-7000-8000-00000000000a'),
        cs('merge', uid(40), error='FLOW.TRANSITION_NOT_ALLOWED')])
    S('STD-VER-020', 'a submitted change set can be abandoned', fa + [
        open_, cs('submit', uid(40)), cs('abandon', uid(40)),
        cs('approve', uid(40), error='FLOW.TRANSITION_NOT_ALLOWED', approver='did:ubos:acme:01900000-0000-7000-8000-00000000000a')])
    S('STD-VER-020', 'an approved change set cannot be abandoned', fa + [
        open_, cs('submit', uid(40)), cs('approve', uid(40), approver='did:ubos:acme:01900000-0000-7000-8000-00000000000a'),
        cs('abandon', uid(40), error='FLOW.TRANSITION_NOT_ALLOWED')])
    S('STD-VER-020', 'a merged change set is final', fa + [
        open_, cs('submit', uid(40)), cs('approve', uid(40), approver='did:ubos:acme:01900000-0000-7000-8000-00000000000a'),
        cs('merge', uid(40)), cs('merge', uid(40), error='FLOW.TRANSITION_NOT_ALLOWED')])
    S('STD-VER-020', 'a sandbox branch cannot be the source of a change set', [
        br('sandbox/try'), cs('open', uid(40), error='VER.MERGE_NOT_ALLOWED', source='sandbox/try', target='main',
                              title='x')])

    # ---- STD-VER-021 approval invalidation ----
    approved = fa + [open_, cs('submit', uid(40), 'sub'), cs('approve', uid(40), 'ap', approver='did:ubos:acme:01900000-0000-7000-8000-00000000000a')]
    S('STD-VER-021', 'an approval records the preview it approved', approved + [
        op('merge_preview', 'mp', source='feature/a', target='main'), cs('get', uid(40), 'g'),
        {'op': 'normalize_text', 'input': {'text': ref_('g', 'approvals/0/preview')}, 'same_as': 'mp/preview'}])
    S('STD-VER-021', 'a change of the source head invalidates the approvals', [
        br('feature/a'), commit([c1], branch='feature/a', label='f'), open_, cs('submit', uid(40)),
        cs('approve', uid(40), approver='did:ubos:acme:01900000-0000-7000-8000-00000000000a'),
        commit([upd(uid(1), 'f', {'email': 'a@x.com'})], branch='feature/a'), cs('get', uid(40))])
    S('STD-VER-021', 'an unrelated change of the target keeps the approvals', approved + [
        commit([c2]), cs('get', uid(40)), cs('merge', uid(40)), read(uid(2))])
    S('STD-VER-021', 'a target change that alters the preview invalidates the approvals', [
        commit([c1], label='m'), br('feature/a'),
        commit([upd(uid(1), 'm', {'email': 'a@x.com'})], branch='feature/a'),
        cs('open', uid(40), source='feature/a', target='main', title='email'), cs('submit', uid(40)),
        cs('approve', uid(40), approver='did:ubos:acme:01900000-0000-7000-8000-00000000000a'),
        commit([upd(uid(1), 'm', {'country': 'CA'})]), cs('get', uid(40))])
    S('STD-VER-021', 'an invalidated change set is approved again before it merges', [
        br('feature/a'), commit([c1], branch='feature/a', label='f'), open_, cs('submit', uid(40)),
        cs('approve', uid(40), approver='did:ubos:acme:01900000-0000-7000-8000-00000000000a'),
        commit([upd(uid(1), 'f', {'email': 'a@x.com'})], branch='feature/a'),
        cs('merge', uid(40), error='FLOW.TRANSITION_NOT_ALLOWED'),
        cs('approve', uid(40), approver='did:ubos:acme:01900000-0000-7000-8000-00000000000b'), cs('merge', uid(40)), read(uid(1))])

    # ---- STD-VER-031 diff over the state tree ----
    S('STD-VER-031', 'the tree difference lists exactly the keys of changed objects', [
        commit([c1, c2], label='m'), commit([upd(uid(2), 'm', {'email': 'b@x.com'})]),
        op('tree_diff', **{'from': {'branch': 'main', 'time': 1}, 'to': {'branch': 'main', 'time': 2}})])
    S('STD-VER-031', 'the tree difference of equal states is empty', [
        commit([c1]), br('feature/a'),
        op('tree_diff', **{'from': {'branch': 'main'}, 'to': {'branch': 'feature/a'}})])
    S('STD-VER-031', 'the tree difference includes appended entries', [
        commit([c1]), commit([stock(uid(10), 'bolt', '5')]),
        op('tree_diff', **{'from': {'branch': 'main', 'time': 1}, 'to': {'branch': 'main', 'time': 2}})])

    # ---- STD-VER-055 validation of the merged result ----
    inv = cls('Invoice', [ref('customer', 'Customer')])
    badge = cls('Badge', [ref('holder', 'Customer', {'target': 'Customer', 'cardinality': 'one_to_one',
                                                     'inverse': 'badge'})])
    S('STD-VER-055', 'a merged reference to an object ended on the target fails', [
        commit([c1], label='m'), br('feature/a'),
        commit([create(uid(5), 'Invoice', 'i-1', {'customer': uri('Customer', 'c-1')})], branch='feature/a'),
        commit([end(uid(1), 'm')]),
        op('merge', error='MODEL.REFERENCE_INVALID', source='feature/a', target='main')], classes=ALL + [inv])
    S('STD-VER-055', 'a merged reference to a live object passes', [
        commit([c1]), br('feature/a'),
        commit([create(uid(5), 'Invoice', 'i-1', {'customer': uri('Customer', 'c-1')})], branch='feature/a'),
        commit([c2]), op('merge', source='feature/a', target='main'), read(uid(5))], classes=ALL + [inv])
    S('STD-VER-055', 'relationship cardinality is checked on the merged state', [
        commit([c1]), br('feature/a'),
        commit([create(uid(6), 'Badge', 'b-1', {'holder': uri('Customer', 'c-1')})], branch='feature/a'),
        commit([create(uid(7), 'Badge', 'b-2', {'holder': uri('Customer', 'c-1')})]),
        op('merge', error='MODEL.CARDINALITY_VIOLATED', source='feature/a', target='main')], classes=ALL + [badge])
    per = create(uid(20), 'FiscalPeriod', 'p-2026-03', {'start': '2026-03-01', 'end': '2026-04-01', 'state': 'open'})
    S('STD-VER-055', 'entries merged into a period closed on the target fail', [
        commit([per], label='m'), br('feature/a'),
        commit([gl(uid(11), '1000', '10.00'), gl(uid(12), '4000', '-10.00')], branch='feature/a'),
        commit([upd(uid(20), 'm', {'state': 'closed'})]),
        op('merge', error='LEDG.PERIOD_CLOSED', source='feature/a', target='main')])
    S('STD-VER-055', 'balanced entries merge into an open period', [
        commit([per]), br('feature/a'),
        commit([gl(uid(11), '1000', '10.00'), gl(uid(12), '4000', '-10.00')], branch='feature/a'),
        op('merge', source='feature/a', target='main')])
    S('STD-VER-055', 'a merge that ends a referenced object fails when the target references it', [
        commit([c1], label='m'), br('feature/a'), commit([end(uid(1), 'm')], branch='feature/a'),
        commit([create(uid(5), 'Invoice', 'i-1', {'customer': uri('Customer', 'c-1')})]),
        op('merge', error='MODEL.REFERENCE_INVALID', source='feature/a', target='main')], classes=ALL + [inv])
    S('STD-VER-055', 'the merge preview reports a validation failure instead of failing', [
        commit([c1], label='m'), br('feature/a'),
        commit([create(uid(5), 'Invoice', 'i-1', {'customer': uri('Customer', 'c-1')})], branch='feature/a'),
        commit([end(uid(1), 'm')]),
        op('merge_preview', source='feature/a', target='main')], classes=ALL + [inv])

    # ---- STD-VER-057 merge preview ----
    S('STD-VER-057', 'the preview lists the changes relative to the target and counts them', [
        commit([c1], label='m'), br('feature/a'),
        commit([upd(uid(1), 'm', {'email': 'a@x.com'}), c2], branch='feature/a'),
        op('merge_preview', source='feature/a', target='main')])
    S('STD-VER-057', 'the preview lists conflicts without failing', [
        commit([c1], label='m'), br('feature/a'),
        commit([upd(uid(1), 'm', {'email': 'a@x.com'})], branch='feature/a'),
        commit([upd(uid(1), 'm', {'email': 'b@x.com'})]),
        op('merge_preview', source='feature/a', target='main')])
    S('STD-VER-057', 'equal inputs give equal preview identifiers', [
        commit([c1]), br('feature/a'), commit([c2], branch='feature/a'),
        op('merge_preview', 'p1', source='feature/a', target='main'),
        {'op': 'merge_preview', 'input': {'source': 'feature/a', 'target': 'main'}, 'same_as': 'p1'}])
    S('STD-VER-057', 'an unrelated target change keeps the preview identifier', [
        br('feature/a'), commit([c2], branch='feature/a'),
        op('merge_preview', 'p1', source='feature/a', target='main'), commit([c1]),
        op('merge_preview', 'p2', source='feature/a', target='main'),
        {'op': 'normalize_text', 'input': {'text': ref_('p2', 'preview')}, 'same_as': 'p1/preview'}])
    S('STD-VER-057', 'a source already contained in the target previews no changes', [
        commit([c1]), br('feature/a'), op('merge_preview', source='feature/a', target='main')])
    S('STD-VER-057', 'ledger entries appear as appended in the preview', [
        br('feature/a'), commit([stock(uid(10), 'bolt', '5')], branch='feature/a'),
        op('merge_preview', source='feature/a', target='main')])

    # ---- STD-VER-058 recorded resolutions ----
    conflict = [commit([c1], label='m'), br('feature/a'),
                commit([upd(uid(1), 'm', {'email': 'a@x.com', 'country': 'CA'})], branch='feature/a'),
                commit([upd(uid(1), 'm', {'email': 'b@x.com', 'country': 'MX'})])]
    S('STD-VER-058', 'resolutions choose values and are recorded with the merge commit', conflict + [
        op('merge', 'mg', source='feature/a', target='main', resolutions=[
            {'object_id': uid(1), 'path': '/email', 'choice': 'source', 'resolver': 'did:ubos:acme:01900000-0000-7000-8000-00000000000a'},
            {'object_id': uid(1), 'path': '/country', 'choice': 'target', 'resolver': 'did:ubos:acme:01900000-0000-7000-8000-00000000000a',
             'reason': 'billing country'}]),
        read(uid(1)), op('commit_info', branch='main', time=ref_('mg', 'cseq'))])
    S('STD-VER-058', 'a custom value is recorded by its hash', conflict + [
        op('merge', 'mg', source='feature/a', target='main', resolutions=[
            {'object_id': uid(1), 'path': '/email', 'choice': 'custom', 'value': 'ops@x.com',
             'resolver': 'did:ubos:acme:01900000-0000-7000-8000-00000000000a'},
            {'object_id': uid(1), 'path': '/country', 'choice': 'base', 'resolver': 'did:ubos:acme:01900000-0000-7000-8000-00000000000a'}]),
        read(uid(1)),
        {'op': 'content_id', 'input': {'domain': 'ubos.chunk.v1', 'value': 'ops@x.com'},
         'same_as': 'mg/resolutions/1/value_hash'}])
    S('STD-VER-058', 'unresolved conflicts still fail the merge', conflict + [
        op('merge', error='VER.MERGE_CONFLICT', source='feature/a', target='main', resolutions=[
            {'object_id': uid(1), 'path': '/email', 'choice': 'source', 'resolver': 'did:ubos:acme:01900000-0000-7000-8000-00000000000a'}])])
    S('STD-VER-058', 'a resolution that matches no conflict is rejected', conflict + [
        op('merge', error='MODEL.INVALID_VALUE', source='feature/a', target='main', resolutions=[
            {'object_id': uid(1), 'path': '/email', 'choice': 'source', 'resolver': 'did:ubos:acme:01900000-0000-7000-8000-00000000000a'},
            {'object_id': uid(1), 'path': '/country', 'choice': 'source', 'resolver': 'did:ubos:acme:01900000-0000-7000-8000-00000000000a'},
            {'object_id': uid(1), 'path': '/name', 'choice': 'source', 'resolver': 'did:ubos:acme:01900000-0000-7000-8000-00000000000a'}])])
    S('STD-VER-058', 'a resolution names its resolver', conflict + [
        op('merge', error='MODEL.DECLARATION_INCOMPLETE', source='feature/a', target='main', resolutions=[
            {'object_id': uid(1), 'path': '/email', 'choice': 'source'}])])

    # ---- STD-VER-060 tags ----
    S('STD-VER-060', 'a tag binds a name to a commit and records its signer', [
        commit([c1]), op('tag', name='q1-close', branch='main', signer='did:ubos:acme:01900000-0000-7000-8000-00000000000a'),
        op('tag_info', name='q1-close')])
    S('STD-VER-060', 'a tag name is not reused', [
        commit([c1]), op('tag', name='q1-close', branch='main'), commit([c2]),
        op('tag', error='VER.TAG_IMMUTABLE', name='q1-close', branch='main')])
    S('STD-VER-060', 'a deleted tag leaves a tombstone that blocks the name', [
        commit([c1]), op('tag', name='q1-close', branch='main'), op('delete_tag', name='q1-close'),
        op('tag_info', name='q1-close'), op('tag', error='VER.TAG_IMMUTABLE', name='q1-close', branch='main')])
    S('STD-VER-060', 'a tag name is a slug', [
        commit([c1]), op('tag', error='MODEL.INVALID_NAME', name='Q1 Close', branch='main')])
    S('STD-VER-060', 'a tag keeps pointing to its commit after later commits', [
        commit([c1], label='m'), op('tag', name='v1', branch='main'),
        commit([upd(uid(1), 'm', {'name': 'Acme Corp'})]), read(uid(1), tag='v1'), op('tag_info', name='v1')])
    S('STD-VER-060', 'a tag names an existing commit', [
        op('tag', error='IAM.NOT_FOUND', name='v1', branch='main', time=4)])

    # ---- STD-VER-061 releases ----
    S('STD-VER-061', 'a release is a tag plus a Release object', [
        commit([c1]), op('release', branch='main', version='1.0.0', tag='v1-0-0', object_id=uid(60),
                         notes='first release'),
        op('tag_info', name='v1-0-0'), read(uid(60))])
    S('STD-VER-061', 'a semantic version is released once per branch line', [
        commit([c1]), op('release', branch='main', version='1.0.0', tag='v1-0-0', object_id=uid(60)),
        op('release', error='MODEL.KEY_CONFLICT', branch='main', version='1.0.0', tag='v1-0-0b', object_id=uid(61))])
    S('STD-VER-061', 'the same version may be released on another branch line', [
        commit([c1]), br('release/v1'),
        op('release', branch='main', version='1.0.0', tag='main-1-0-0', object_id=uid(60)),
        op('release', branch='release/v1', version='1.0.0', tag='rel-1-0-0', object_id=uid(61))])
    S('STD-VER-061', 'a release version is a semantic version', [
        commit([c1]), op('release', error='MODEL.INVALID_VALUE', branch='main', version='1.0', tag='v1',
                         object_id=uid(60))])
    S('STD-VER-061', 'a release does not reuse a tag name', [
        commit([c1]), op('tag', name='v1', branch='main'),
        op('release', error='VER.TAG_IMMUTABLE', branch='main', version='1.0.0', tag='v1', object_id=uid(60))])

    # ---- STD-VER-070 revert of a commit ----
    S('STD-VER-070', 'revert restores the fields a commit changed', [
        commit([c1], label='m'), commit([upd(uid(1), 'm', {'email': 'a@x.com'})]),
        op('revert', branch='main', time=2), read(uid(1))])
    S('STD-VER-070', 'revert conflicts when the field changed since', [
        commit([c1], label='m'), commit([upd(uid(1), 'm', {'email': 'a@x.com'})], label='u'),
        commit([upd(uid(1), 'u', {'email': 'b@x.com'})]),
        op('revert', error='VER.MERGE_CONFLICT', branch='main', time=2)])
    S('STD-VER-070', 'revert keeps later changes of other fields', [
        commit([c1], label='m'), commit([upd(uid(1), 'm', {'email': 'a@x.com'})], label='u'),
        commit([upd(uid(1), 'u', {'country': 'CA'})]), op('revert', branch='main', time=2), read(uid(1))])
    S('STD-VER-070', 'revert ends the objects a commit created', [
        commit([c1]), commit([c2]), op('revert', branch='main', time=2),
        read(uid(2), error='QRY.NOT_FOUND_AT_TIME'), read(uid(1))])
    S('STD-VER-070', 'revert of a creation conflicts when the object changed since', [
        commit([c1], label='m'), commit([upd(uid(1), 'm', {'email': 'a@x.com'})]),
        op('revert', error='VER.MERGE_CONFLICT', branch='main', time=1)])
    S('STD-VER-070', 'revert restores the last version of an ended object', [
        commit([c1], label='m'), commit([end(uid(1), 'm')]), op('revert', branch='main', time=2), read(uid(1))])
    S('STD-VER-070', 'revert appends reversals of the entries a commit appended', [
        commit([stock(uid(10), 'bolt', '5')]),
        op('revert', branch='main', time=1, reversal_ids={uid(10): uid(11)}),
        op('read_ledger', branch='main', **{'class': 'StockMove'}, ledger_key={'item': 'bolt'})])
    S('STD-VER-070', 'revert of appended entries needs reversal identifiers', [
        commit([stock(uid(10), 'bolt', '5')]),
        op('revert', error='MODEL.DECLARATION_INCOMPLETE', branch='main', time=1)])
    S('STD-VER-070', 'revert of a merge commit is relative to its first parent', [
        commit([c1]), br('feature/a'), commit([c2], branch='feature/a'), commit([stock(uid(10), 'bolt', '5')]),
        op('merge', 'mg', source='feature/a', target='main'),
        op('revert', branch='main', time=ref_('mg', 'cseq')), read(uid(2), error='QRY.NOT_FOUND_AT_TIME'),
        op('read_ledger', branch='main', **{'class': 'StockMove'}, ledger_key={'item': 'bolt'})])
    S('STD-VER-070', 'revert restores a timeline changed from a future date', [
        commit([c1], label='m'),
        commit([upd(uid(1), 'm', {'email': 'a@x.com'}, valid={'kind': 'supersede_from', 'from': '2026-06-01'})]),
        op('revert', branch='main', time=2), read(uid(1), asof='2026-07-01')])
    S('STD-VER-070', 'revert names a commit on the history of the branch', [
        commit([c1]), op('revert', error='IAM.NOT_FOUND', branch='main', time=5)])
    # since BPA 1.0-rc6 (DEC-083): resolutions of a revert
    rv = lambda choice: [{'object_id': uid(1), 'path': '/email', 'choice': choice, 'resolver': 'u-1'}]  # noqa: E731
    S('STD-VER-070', 'a resolution choosing target keeps the later change of a conflicting field', [
        commit([c1], label='m'), commit([upd(uid(1), 'm', {'email': 'a@x.com'})], label='u'),
        commit([upd(uid(1), 'u', {'email': 'b@x.com'})]),
        op('revert', branch='main', time=2, resolutions=rv('target')), read(uid(1)), op('commit_info', branch='main')])
    S('STD-VER-070', 'a resolution choosing source restores the value before the reverted commit', [
        commit([c1], label='m'), commit([upd(uid(1), 'm', {'email': 'a@x.com'})], label='u'),
        commit([upd(uid(1), 'u', {'email': 'b@x.com'})]),
        op('revert', branch='main', time=2, resolutions=rv('source')), read(uid(1))])
    S('STD-VER-070', 'a resolution that matches no conflict fails the revert', [
        commit([c1], label='m'), commit([upd(uid(1), 'm', {'email': 'a@x.com'})]),
        op('revert', error='MODEL.INVALID_VALUE', branch='main', time=2, resolutions=rv('target')), read(uid(1))])

    # ---- STD-VER-071 revert of a process or change set ----
    S('STD-VER-071', 'reverting a process reverts all its commits in reverse order as one commit', [
        commit([c1], process=P(1), label='a'),
        commit([upd(uid(1), 'a', {'email': 'a@x.com'})], process=P(1)),
        op('revert', process_id=uid(501)), read(uid(1), error='QRY.NOT_FOUND_AT_TIME'),
        op('process_info', process_id=uid(501))])
    S('STD-VER-071', 'reverting a process that wrote two branches gives one commit per branch', [
        br('feature/a'), commit([c1], process=P(1)), commit([c2], branch='feature/a', process=P(1)),
        op('revert', process_id=uid(501)), read(uid(1), error='QRY.NOT_FOUND_AT_TIME'),
        read(uid(2), 'feature/a', error='QRY.NOT_FOUND_AT_TIME')])
    S('STD-VER-071', 'dependent processes are reported before execution', [
        commit([c1], process=P(1)),
        commit([c2], process=P(2, trigger={'type': 'event', 'ref': 'CustomerCreated'}, parent_process=uid(501))),
        commit([stock(uid(10), 'bolt', '5')],
               process=P(3, trigger={'type': 'job', 'ref': 'restock'}, parent_process=uid(502))),
        op('revert', process_id=uid(501), dry_run=True), read(uid(1))])
    S('STD-VER-071', 'reverting a merged change set reverts its merge commit', fa + [
        open_, cs('submit', uid(40)), cs('approve', uid(40), approver='did:ubos:acme:01900000-0000-7000-8000-00000000000a'), cs('merge', uid(40)),
        op('revert', change_set=uid(40)), read(uid(1), error='QRY.NOT_FOUND_AT_TIME'), read(uid(1), 'feature/a')])
    S('STD-VER-071', 'a change set that is not merged cannot be reverted', fa + [
        open_, op('revert', error='FLOW.TRANSITION_NOT_ALLOWED', change_set=uid(40))])

    # ---- STD-VER-080 resolution through the parent line ----
    ov = [commit([c1], label='m'), br('overlay/acme', parent_line={'branch': 'main', 'mode': 'live'})]
    S('STD-VER-080', 'a branch reads objects of its parent line', ov + [read(uid(1), 'overlay/acme')])
    S('STD-VER-080', 'an override on the branch wins and leaves the parent unchanged', ov + [
        commit([upd(uid(1), 'm', {'email': 'a@x.com'})], branch='overlay/acme'),
        read(uid(1), 'overlay/acme'), read(uid(1))])
    S('STD-VER-080', 'a hide marker makes the object not found on the branch only', ov + [
        commit([{'op': 'hide', 'object_id': uid(1), 'base': base('m', uid(1))}], branch='overlay/acme'),
        read(uid(1), 'overlay/acme', error='IAM.NOT_FOUND'), read(uid(1))])
    S('STD-VER-080', 'unhide shows the parent version again', ov + [
        commit([{'op': 'hide', 'object_id': uid(1), 'base': base('m', uid(1))}], branch='overlay/acme'),
        commit([{'op': 'unhide', 'object_id': uid(1)}], branch='overlay/acme'), read(uid(1), 'overlay/acme')])
    S('STD-VER-080', 'a live parent line shows later parent changes', ov + [
        commit([upd(uid(1), 'm', {'email': 'a@x.com'})]), read(uid(1), 'overlay/acme')])
    S('STD-VER-080', 'a pinned parent line keeps the pinned versions', [
        commit([c1], label='m'), br('overlay/acme', parent_line={'branch': 'main', 'mode': 'pinned', 'pin': 1}),
        commit([upd(uid(1), 'm', {'email': 'a@x.com'})]), read(uid(1), 'overlay/acme'),
        op('branch_info', branch='overlay/acme')])
    S('STD-VER-080', 'a snapshot of the branch resolves a live parent as of its transaction time', ov + [
        commit([c2], branch='overlay/acme'), now('2026-03-16T10:00:00.000000Z'),
        commit([upd(uid(1), 'm', {'email': 'a@x.com'})]),
        read(uid(1), 'overlay/acme', time=1), read(uid(1), 'overlay/acme')])
    S('STD-VER-080', 'a parent-line cycle is rejected', [
        br('overlay/a', parent_line={'branch': 'main'}), br('overlay/b', parent_line={'branch': 'overlay/a'}),
        op('set_parent_line', error='VER.INHERITANCE_CYCLE', branch='overlay/a',
           parent_line={'branch': 'overlay/b'})])
    chain = [commit([c1])] + [br('overlay/l%d' % n, parent_line={'branch': 'main' if n == 1 else 'overlay/l%d' % (n - 1)})
                              for n in range(1, 9)]
    S('STD-VER-080', 'a parent line of depth 8 resolves', chain + [read(uid(1), 'overlay/l8')])
    S('STD-VER-080', 'a hide marker in the middle of a chain hides the object below it', chain[:4] + [
        read(uid(1), 'overlay/l2', 'x'),
        commit([{'op': 'hide', 'object_id': uid(1), 'base': {'$ref': 'x/version_hash'}}], branch='overlay/l2'),
        read(uid(1), 'overlay/l3', error='IAM.NOT_FOUND'), read(uid(1), 'overlay/l1')])
    S('STD-VER-080', 'only objects of the parent line can be hidden', ov + [
        commit([c2], branch='overlay/acme', label='o'),
        commit([{'op': 'hide', 'object_id': uid(2), 'base': base('o', uid(2))}], branch='overlay/acme',
               error='MODEL.INVALID_VALUE')])
    S('STD-VER-080', 'a hide marker names its base', ov + [
        commit([upd(uid(1), 'm', {'email': 'a@x.com'})]),
        commit([{'op': 'hide', 'object_id': uid(1), 'base': base('m', uid(1))}], branch='overlay/acme',
               error='TXN.HEAD_MOVED', details=True)])
    S('STD-VER-080', 'class filters include inherited objects and exclude hidden ones', ov + [
        commit([c2], branch='overlay/acme'), op('list_objects', branch='overlay/acme', **{'class': 'Customer'}),
        commit([{'op': 'hide', 'object_id': uid(1), 'base': base('m', uid(1))}], branch='overlay/acme'),
        op('list_objects', branch='overlay/acme', **{'class': 'Customer'})])
    S('STD-VER-080', 'a branch without a parent line has nothing to hide', [
        commit([c1], label='m'), br('feature/a'),
        commit([{'op': 'hide', 'object_id': uid(1), 'base': base('m', uid(1))}], branch='feature/a',
               error='MODEL.INVALID_VALUE')])

    # ---- STD-VER-081 provenance of resolution ----
    S('STD-VER-081', 'an inherited version reports the parent branch and commit', ov + [
        commit([c2], branch='overlay/acme'), read(uid(1), 'overlay/acme', provenance=True)])
    S('STD-VER-081', 'an override reports the branch itself', ov + [
        commit([upd(uid(1), 'm', {'email': 'a@x.com'})], branch='overlay/acme'),
        read(uid(1), 'overlay/acme', provenance=True)])
    S('STD-VER-081', 'a read of a plain branch reports the branch and the snapshot commit', [
        commit([c1]), commit([c2]), read(uid(1), provenance=True), read(uid(1), provenance=True, time=1)])
    S('STD-VER-081', 'provenance names the commit resolved on the parent', ov + [
        op('commit_info', 'ci', branch='main'),
        {'op': 'read', 'input': {'branch': 'overlay/acme', 'object_id': uid(1), 'provenance': True}, 'label': 'r'},
        op('read', branch='main', object_id=uid(1), commit=ref_('r', 'provenance/commit_id'))])

    # ---- STD-VER-090 snapshot selection ----
    two = [commit([c1], label='m'), now('2026-03-15T11:00:00.000000Z'),
           commit([upd(uid(1), 'm', {'email': 'a@x.com'})], label='u')]
    S('STD-VER-090', 'a commit selector reads that commit', two + [
        op('commit_info', 'ci', branch='main', time=1), read(uid(1), commit=ref_('ci', 'commit_id'))])
    S('STD-VER-090', 'a commit off the history of the branch is not found', two + [
        br('feature/a', time=1), commit([c2], branch='feature/a', label='f'),
        read(uid(1), commit=ref_('f', 'commit_id'), error='IAM.NOT_FOUND')])
    S('STD-VER-090', 'a merged source commit is not on the first-parent history', [
        commit([c1]), br('feature/a'), commit([c2], branch='feature/a', label='f'),
        op('merge', source='feature/a', target='main'), read(uid(2)),
        read(uid(2), commit=ref_('f', 'commit_id'), error='IAM.NOT_FOUND')])
    S('STD-VER-090', 'a tag selector reads the tagged commit', two[:1] + [
        op('tag', name='before', branch='main')] + two[1:] + [read(uid(1), tag='before')])
    S('STD-VER-090', 'a time selector reads the latest commit at or before it', two + [
        read(uid(1), tx_time='2026-03-15T10:30:00.000000Z'), read(uid(1), tx_time=ref_('u', 'tx_time')),
        read(uid(1), tx_time='2026-03-15T12:00:00.000000Z')])
    S('STD-VER-090', 'a time before the first commit selects the empty start', two + [
        read(uid(1), tx_time='2026-03-15T09:00:00.000000Z', error='IAM.NOT_FOUND')])
    S('STD-VER-090', 'at most one selector is given', two + [
        read(uid(1), time=1, tag='x', error='QRY.INVALID_QUERY')])
    S('STD-VER-090', 'a deleted tag no longer selects', two + [
        op('tag', name='t1', branch='main'), op('delete_tag', name='t1'),
        read(uid(1), tag='t1', error='IAM.NOT_FOUND')])
    S('STD-VER-090', 'no selector reads the head', two + [read(uid(1))])


def time_(S, c1):
    # ---- STD-TIME-013 consistent coordinates across references ----
    inv = cls('Invoice', [ref('customer', 'Customer')])
    base_ = [commit([create(uid(1), 'Customer', 'c-1', {'name': 'Acme'}, valid={'from': '2026-01-01'})], label='m'),
             commit([upd(uid(1), 'm', {'name': 'Acme Corp'}, valid={'kind': 'supersede_from', 'from': '2026-06-01'})],
                    label='u2'),
             commit([create(uid(5), 'Invoice', 'i-1', {'customer': uri('Customer', 'c-1')},
                            valid={'from': '2026-02-01'})])]
    S('STD-TIME-013', 'a followed reference resolves at the valid time of the source', base_ + [
        read(uid(5), asof='2026-03-01', follow='customer'), read(uid(5), asof='2026-07-01', follow='customer')],
      classes=ALL + [inv])
    S('STD-TIME-013', 'a followed reference resolves at the snapshot of the source', base_ + [
        commit([upd(uid(1), 'u2', {'email': 'a@x.com'}, valid={'kind': 'replace_future', 'from': '2026-03-01'})]),
        read(uid(5), time=3, follow='customer'), read(uid(5), follow='customer')], classes=ALL + [inv])
    S('STD-TIME-013', 'following an unknown field is rejected', base_ + [
        read(uid(5), follow='vendor', error='MODEL.INVALID_VALUE')], classes=ALL + [inv])

    # ---- STD-TIME-014 range queries ----
    tl = [commit([create(uid(1), 'Customer', 'c-1', {'name': 'Acme'}, valid={'from': '2026-01-01'})], label='m'),
          now('2026-03-15T11:00:00.000000Z'),
          commit([upd(uid(1), 'm', {'email': 'a@x.com'}, valid={'kind': 'supersede_from', 'from': '2026-04-01'})],
                 label='u'),
          now('2026-03-15T12:00:00.000000Z'), commit([create(uid(2), 'Customer', 'c-2', {'name': 'Beta'})]),
          now('2026-03-15T13:00:00.000000Z'),
          commit([{'op': 'update', 'object_id': uid(1), 'base': {'$ref': 'u/versions/%s' % uid(1)},
                   'body': {'country': 'CA'}, 'valid': {'kind': 'supersede_from', 'from': '2026-09-01'}}],
                 label='w')]
    S('STD-TIME-014', 'a valid-range query returns the segments clipped to the range', tl + [
        op('read_range', branch='main', object_id=uid(1), **{'from': '2026-02-01', 'to': '2026-05-01'})])
    S('STD-TIME-014', 'an open valid range reaches the end of the timeline', tl + [
        op('read_range', branch='main', object_id=uid(1), **{'from': '2026-11-01'})])
    S('STD-TIME-014', 'a valid range before the timeline is empty', tl + [
        op('read_range', branch='main', object_id=uid(1), **{'from': '2025-01-01', 'to': '2025-12-31'})])
    S('STD-TIME-014', 'a valid-range query at an earlier snapshot uses that timeline', tl + [
        op('read_range', branch='main', object_id=uid(1), time=2, **{'from': '2026-01-01'})])
    S('STD-TIME-014', 'a transaction-range query returns the commits that changed the object', tl + [
        op('history', branch='main', object_id=uid(1))])
    S('STD-TIME-014', 'a transaction range is half open', tl + [
        op('history', branch='main', object_id=uid(1), **{'from': ref_('u', 'tx_time'), 'to': ref_('w', 'tx_time')})])
    S('STD-TIME-014', 'commits of other objects are not listed', tl + [
        op('history', branch='main', object_id=uid(2))])
    S('STD-TIME-014', 'an empty valid range is rejected', tl + [
        op('read_range', error='MODEL.INVALID_VALUE', branch='main', object_id=uid(1),
           **{'from': '2026-05-01', 'to': '2026-05-01'})])

    # ---- STD-TIME-015 evaluation evidence ----
    S('STD-TIME-015', 'repeating a read with its evaluated commits returns the same answer', [
        commit([c1], label='m'), read(uid(1), label='r'),
        commit([upd(uid(1), 'm', {'email': 'a@x.com'})]),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(1), 'evaluated_at': ref_('r', 'evaluated_at')},
         'same_as': 'r'}])
    S('STD-TIME-015', 'evidence of an inheriting branch names the parent commit too', [
        commit([c1], label='m'), br('overlay/acme', parent_line={'branch': 'main'}),
        read(uid(1), 'overlay/acme', 'r'), commit([upd(uid(1), 'm', {'email': 'a@x.com'})]),
        {'op': 'read', 'input': {'branch': 'overlay/acme', 'object_id': uid(1),
                                 'evaluated_at': ref_('r', 'evaluated_at')}, 'same_as': 'r'},
        read(uid(1), 'overlay/acme')])
    S('STD-TIME-015', 'ledger reads carry evidence', [
        commit([stock(uid(10), 'bolt', '5')]),
        op('read_ledger', 'r', branch='main', **{'class': 'StockMove'}, ledger_key={'item': 'bolt'}),
        commit([stock(uid(11), 'bolt', '3')]),
        {'op': 'read_ledger', 'input': {'branch': 'main', 'class': 'StockMove', 'ledger_key': {'item': 'bolt'},
                                        'evaluated_at': ref_('r', 'evaluated_at')}, 'same_as': 'r'}])
    S('STD-TIME-015', 'evidence and selectors are not combined', [
        commit([c1]), read(uid(1), label='r'),
        read(uid(1), time=1, evaluated_at=ref_('r', 'evaluated_at'), error='QRY.INVALID_QUERY')])

    # ---- STD-TIME-031 impact set ----
    rate = create(uid(1), 'Customer', 'c-1', {'name': 'Acme'}, valid={'from': '2026-01-01'})

    def derived(n, oid_in, label, asof, logic='logic:score@1'):
        return create(uid(n), 'Customer', 'd-%d' % n, {'name': 'derived'}, derivation={
            'logic': [logic], 'inputs': [{'object': oid_in, 'version': base(label, oid_in), 'asof': asof}]})
    retro = commit([upd(uid(1), 'm', {'email': 'a@x.com'}, valid={'kind': 'supersede_from', 'from': '2026-02-01'})],
                   label='r')
    S('STD-TIME-031', 'a version derived from a superseded input is impacted', [
        commit([rate], label='m'), commit([derived(2, uid(1), 'm', '2026-02-15')]), retro,
        op('impact', branch='main', time=3)])
    S('STD-TIME-031', 'a derivation that used the input outside the changed range is not impacted', [
        commit([rate], label='m'), commit([derived(2, uid(1), 'm', '2026-01-15')]), retro,
        op('impact', branch='main', time=3)])
    S('STD-TIME-031', 'the impact set is closed over derivation records', [
        commit([rate], label='m'), commit([derived(2, uid(1), 'm', '2026-02-15')], label='d2'),
        commit([derived(3, uid(2), 'd2', '2026-03-15')], label='d3'),
        commit([derived(4, uid(3), 'd3', '2026-03-15')]), retro, op('impact', branch='main', time=5)])
    S('STD-TIME-031', 'entries derived from a superseded input are impacted', [
        commit([rate], label='m'),
        commit([dict(stock(uid(10), 'bolt', '5'), derivation={
            'logic': ['logic:reorder@1'], 'inputs': [{'object': uid(1), 'version': base('m', uid(1)),
                                                      'asof': '2026-02-15'}]})]),
        retro, op('impact', branch='main', time=3)])
    S('STD-TIME-031', 'a change that is not retroactive has an empty impact set', [
        commit([rate], label='m'), commit([derived(2, uid(1), 'm', '2026-04-15')]),
        commit([upd(uid(1), 'm', {'email': 'a@x.com'}, valid={'kind': 'supersede_from', 'from': '2026-04-01'})]),
        op('impact', branch='main', time=3)])

    # ---- STD-TIME-040 rule binding ----
    vip = cls('VipCustomer', [field('tier', 'text')], parent='Customer')
    S('STD-TIME-040', 'the rule binding lists the class versions of the changed objects', [
        commit([create(uid(1), 'VipCustomer', 'v-1', {'name': 'Acme', 'tier': 'gold'})]),
        op('commit_info', branch='main', time=1)], classes=ALL + [vip])
    S('STD-TIME-040', 'the rule binding includes the extensions applied', [
        br('feature/crm'),
        commit([{'op': 'create', 'object_id': uid(90), 'class': 'ClassExtension', 'slug': 'crm-customer',
                 'body': {'target': 'Customer', 'namespace': 'acme.crm', 'order': 1,
                          'fields': [field('segment', 'text')]}}], branch='feature/crm'),
        commit([c1], branch='feature/crm'), op('commit_info', branch='feature/crm', time=2)])
    S('STD-TIME-040', 'the rule binding includes the logic of derived values', [
        commit([rate], label='m'), commit([derived(2, uid(1), 'm', '2026-02-15')]),
        op('commit_info', branch='main', time=2)])
    S('STD-TIME-040', 'a merge commit binds the classes of the merged changes', [
        br('feature/a'), commit([c1, stock(uid(10), 'bolt', '5')], branch='feature/a'),
        op('merge', source='feature/a', target='main'), op('commit_info', branch='main', time=1)])
    S('STD-TIME-040', 'the class versions of the binding are the declared class versions', [
        commit([c1]), op('commit_info', 'ci', branch='main', time=1),
        op('content_id', domain='ubos.chunk.v1', value=CUSTOMER),
        {'op': 'content_id', 'input': {'domain': 'ubos.chunk.v1', 'value': CUSTOMER},
         'same_as': 'ci/rule_binding/classes/0/class_version'}])

    # ---- STD-TIME-050 became-effective events ----
    pl = create(uid(30), 'PriceList', 'pl-1', {'price': '10'}, valid={'from': '2026-01-01'})
    fut = commit([upd(uid(30), 'm', {'price': '12'}, valid={'kind': 'supersede_from', 'from': '2026-04-01'})],
                 label='u')
    S('STD-TIME-050', 'a future segment start emits BecameEffective when it is reached', [
        commit([pl], label='m'), fut, op('outbox'), now('2026-04-01T00:00:00.000000Z'), op('outbox')])
    S('STD-TIME-050', 'no event is emitted before the start', [
        commit([pl], label='m'), fut, now('2026-03-31T23:59:59.999999Z'), op('outbox')])
    S('STD-TIME-050', 'a segment removed before its start emits nothing; its replacement does', [
        commit([pl], label='m'), fut,
        commit([{'op': 'update', 'object_id': uid(30), 'base': {'$ref': 'u/versions/%s' % uid(30)},
                 'body': {'price': '11'}, 'valid': {'kind': 'replace_future', 'from': '2026-03-20'}}]),
        now('2026-04-02T00:00:00.000000Z'), op('outbox')])
    S('STD-TIME-050', 'classes without emit_became_effective emit nothing', [
        commit([create(uid(1), 'Customer', 'c-1', {'name': 'Acme'}, valid={'from': '2026-01-01'})], label='m'),
        commit([upd(uid(1), 'm', {'email': 'a@x.com'}, valid={'kind': 'supersede_from', 'from': '2026-04-01'})]),
        now('2026-04-02T00:00:00.000000Z'), op('outbox')])
    S('STD-TIME-050', 'a segment that starts at commit time emits nothing', [
        commit([pl], label='m'), commit([upd(uid(30), 'm', {'price': '12'})]),
        now('2026-04-02T00:00:00.000000Z'), op('outbox')])
    S('STD-TIME-050', 'the event is delivered at least once until acknowledged', [
        commit([pl], label='m'), fut, now('2026-04-01T00:00:00.000000Z'), op('outbox', 'o'),
        op('ack', effect_ids=[ref_('o', 'effects/0/effect_id')]), op('outbox')])


def txn(S, c1, c2):
    rate = create(uid(1), 'Customer', 'c-1', {'name': 'Acme'}, valid={'from': '2026-01-01'})
    # ---- STD-TXN-030 derivation records ----
    der = {'logic': ['logic:score@1', 'table:risk@3'],
           'inputs': [{'object': uid(1), 'version': base('m', uid(1)), 'asof': '2026-02-15'}]}
    S('STD-TXN-030', 'a derived version references its derivation record', [
        commit([rate], label='m'),
        commit([create(uid(2), 'Customer', 'd-2', {'name': 'score'}, derivation=der)], process=P(1)),
        op('derivation', 'd', branch='main', object_id=uid(2)),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(2)}, 'label': 'r'},
        {'op': 'normalize_text', 'input': {'text': ref_('r', 'provenance_ref')}, 'same_as': 'd/provenance_ref'}])
    S('STD-TXN-030', 'the provenance reference of the version is the record identifier', [
        commit([rate], label='m'),
        commit([create(uid(2), 'Customer', 'd-2', {'name': 'score'}, derivation=der)], process=P(1)),
        read(uid(2), label='r'),
        op('content_id', domain='ubos.chunk.v1', value={
            'process_id': uid(501), 'logic': ['logic:score@1', 'table:risk@3'], 'mode': 'as_effective',
            'inputs': [{'object': uid(1), 'version': base('m', uid(1)), 'asof': '2026-02-15'}]}) | {
            'same_as': 'r/provenance_ref'}])
    S('STD-TXN-030', 'a version written by direct input has no derivation record', [
        commit([c1]), op('derivation', error='IAM.NOT_FOUND', branch='main', object_id=uid(1))])
    S('STD-TXN-030', 'inputs over the configured maximum are summarised per class', [
        op('set_limits', max_derivation_inputs=2),
        commit([rate, c2, create(uid(4), 'FiscalPeriod', 'p-1', {'start': '2026-01-01', 'end': '2026-02-01',
                                                                  'state': 'open'})], label='m'),
        commit([create(uid(5), 'Customer', 'd-5', {'name': 'score'}, derivation={
            'logic': ['logic:score@1'], 'mode': 'as_originally',
            'inputs': [{'object': uid(1), 'version': base('m', uid(1)), 'asof': '2026-02-15'},
                       {'object': uid(2), 'version': base('m', uid(2)), 'asof': '2026-03-15'},
                       {'object': uid(4), 'version': base('m', uid(4)), 'asof': '2026-01-15'}]})], process=P(1)),
        op('derivation', branch='main', object_id=uid(5))])
    S('STD-TXN-030', 'the derivation mode is one of the re-evaluation modes', [
        commit([rate], label='m'),
        commit([create(uid(2), 'Customer', 'd-2', {'name': 'score'}, derivation=dict(der, mode='latest'))],
               error='MODEL.INVALID_VALUE')])
    S('STD-TXN-030', 'an input names an existing version of the object', [
        commit([rate, c2], label='m'),
        commit([create(uid(5), 'Customer', 'd-5', {'name': 'score'}, derivation={
            'logic': ['logic:score@1'],
            'inputs': [{'object': uid(1), 'version': base('m', uid(2)), 'asof': '2026-02-15'}]})],
               error='MODEL.REFERENCE_INVALID')])
    S('STD-TXN-030', 'a derivation names its logic', [
        commit([rate], label='m'),
        commit([create(uid(2), 'Customer', 'd-2', {'name': 'score'}, derivation=dict(der, logic=[]))],
               error='MODEL.INVALID_VALUE')])
    S('STD-TXN-030', 'derived entries reference their derivation record', [
        commit([rate], label='m'),
        commit([dict(stock(uid(10), 'bolt', '5'), derivation=der)], process=P(1)),
        op('derivation', branch='main', entry_id=uid(10)),
        op('read_ledger', branch='main', **{'class': 'StockMove'}, ledger_key={'item': 'bolt'})])

    # ---- STD-TXN-031 trigger lineage ----
    chain = [commit([c1], process=P(1)),
             commit([c2], process=P(2, kind='handler', trigger={'type': 'event', 'ref': 'CustomerCreated'},
                                     parent_process=uid(501))),
             commit([stock(uid(10), 'bolt', '5')],
                    process=P(3, kind='job', trigger={'type': 'job', 'ref': 'restock'}, parent_process=uid(502))),
             commit([stock(uid(11), 'nut', '5')],
                    process=P(4, kind='handler', trigger={'type': 'event', 'ref': 'CustomerCreated'},
                              parent_process=uid(501)))]
    S('STD-TXN-031', 'a process records its trigger and parent process', chain + [
        op('process_info', process_id=uid(502))])
    S('STD-TXN-031', 'a process triggered by an event names its parent', [
        commit([c1], process=P(1, trigger={'type': 'event', 'ref': 'CustomerCreated'}),
               error='MODEL.DECLARATION_INCOMPLETE')])
    S('STD-TXN-031', 'ancestors are listed from the nearest', chain + [
        op('lineage', process_id=uid(503), direction='ancestors')])
    S('STD-TXN-031', 'descendants are the transitive closure', chain + [
        op('lineage', process_id=uid(501), direction='descendants')])
    S('STD-TXN-031', 'the parent process exists', [
        commit([c1], process=P(2, trigger={'type': 'event', 'ref': 'x'}, parent_process=uid(599)),
               error='IAM.NOT_FOUND')])
    S('STD-TXN-031', 'a user-triggered process has no ancestors', chain + [
        op('lineage', process_id=uid(501))])
    S('STD-TXN-031', 'the trigger type is one of the declared types', [
        commit([c1], process=P(1, trigger={'type': 'webhook'}), error='MODEL.INVALID_VALUE')])

    # ---- STD-TXN-040 idempotency keys ----
    k1 = P(1, idempotency_key='order-7', initiator='did:ubos:acme:01900000-0000-7000-8000-00000000000a')
    S('STD-TXN-040', 'a known key with an equal payload returns the stored result without effect', [
        commit([c1], process=k1, label='a'), commit([c1], process=dict(k1, process_id=uid(502)), same_as='a'),
        commit([c2]), op('commit_info', branch='main')])
    S('STD-TXN-040', 'a known key with another payload fails', [
        commit([c1], process=k1), commit([c2], process=k1, error='TXN.IDEMPOTENCY_MISMATCH')])
    S('STD-TXN-040', 'keys are scoped by the initiator', [
        commit([c1], process=k1), commit([c2], process=P(2, idempotency_key='order-7',
                                                         initiator='did:ubos:acme:01900000-0000-7000-8000-00000000000b'))])
    S('STD-TXN-040', 'records are kept for at least seven days', [
        commit([c1], process=k1, label='a'), now('2026-03-22T09:59:59.000000Z'),
        commit([c1], process=k1, same_as='a')])
    S('STD-TXN-040', 'a failed request stores no record', [
        commit([c1]), commit([c1], process=k1, error='TXN.ALREADY_EXISTS'),
        commit([c2], process=k1)])
    S('STD-TXN-040', 'a replayed request writes no second outbox effect', [
        commit([c1], process=dict(k1, effects=[{'type': 'CustomerCreated', 'object_id': uid(1)}])),
        commit([c1], process=dict(k1, effects=[{'type': 'CustomerCreated', 'object_id': uid(1)}])),
        op('outbox')])

    # ---- STD-TXN-050 effects after commit ----
    fx = [{'type': 'CustomerCreated', 'object_id': uid(1)}, {'type': 'SendWelcome', 'payload': {'to': 'a@x.com'}}]
    S('STD-TXN-050', 'effects are written with the commit under stable identifiers', [
        commit([c1], process=P(1, effects=fx)), op('outbox')])
    S('STD-TXN-050', 'an aborted process releases nothing', [
        commit([c1], process=P(1, effects=fx, abort=True)), op('outbox'), op('process_info', process_id=uid(501)),
        read(uid(1), error='IAM.NOT_FOUND')])
    S('STD-TXN-050', 'a failed commit releases nothing', [
        commit([c1]), commit([c1], process=P(1, effects=fx), error='TXN.ALREADY_EXISTS'), op('outbox'),
        op('process_info', process_id=uid(501))])
    S('STD-TXN-050', 'effects are ordered by commit order', [
        commit([c1], label='m', process=P(1, effects=[{'type': 'CustomerCreated', 'object_id': uid(1)}])),
        commit([upd(uid(1), 'm', {'email': 'a@x.com'})],
               process=P(2, effects=[{'type': 'CustomerChanged', 'object_id': uid(1)}])), op('outbox')])
    S('STD-TXN-050', 'unacknowledged effects are delivered again', [
        commit([c1], process=P(1, effects=fx)), op('outbox'), op('ack', effect_ids=[uid(501) + '/1']),
        op('outbox'), op('outbox')])
    S('STD-TXN-050', 'an unknown effect cannot be acknowledged', [
        op('ack', error='IAM.NOT_FOUND', effect_ids=['x/1'])])
    S('STD-TXN-050', 'an effect names its type', [
        commit([c1], process=P(1, effects=[{'object_id': uid(1)}]), error='MODEL.DECLARATION_INCOMPLETE')])

    # ---- STD-TXN-070 bulk processes ----
    rows = [create(uid(100 + n), 'Customer', 'b-%d' % n, {'name': 'Bulk %d' % n}) for n in range(5)]
    S('STD-TXN-070', 'a bulk process divides its work into children of at most the configured size', [
        op('bulk', process_id=uid(550), branch='main', changes=rows, max_changes=2),
        op('process_info', process_id=uid(550)), read(uid(104))])
    bad = rows[:3] + [create(uid(100), 'Customer', 'b-dup', {'name': 'dup'})] + rows[4:]
    fixed = rows[:3] + [rows[3]] + rows[4:]
    S('STD-TXN-070', 'resumption continues after the last committed child', [
        op('bulk', process_id=uid(550), branch='main', changes=bad, max_changes=2),
        read(uid(101)), op('bulk', process_id=uid(550), branch='main', changes=fixed, max_changes=2, resume=True),
        read(uid(104))])
    S('STD-TXN-070', 'an all_or_nothing bulk process leaves the target unchanged on failure', [
        op('bulk', process_id=uid(550), branch='main', changes=bad, max_changes=2, mode='all_or_nothing',
           staging='loader/bulk-1'),
        read(uid(101), error='IAM.NOT_FOUND'), read(uid(101), 'loader/bulk-1')])
    S('STD-TXN-070', 'an all_or_nothing bulk process merges the staging branch once on completion', [
        op('bulk', process_id=uid(550), branch='main', changes=bad, max_changes=2, mode='all_or_nothing',
           staging='loader/bulk-1'),
        op('bulk', process_id=uid(550), branch='main', changes=fixed, max_changes=2, resume=True),
        read(uid(104)), op('commit_info', branch='main')])
    S('STD-TXN-070', 'a resumed bulk process keeps one child per committed chunk', [
        op('bulk', process_id=uid(550), branch='main', changes=bad, max_changes=2),
        op('process_info', process_id=uid(550)),
        op('bulk', process_id=uid(550), branch='main', changes=fixed, max_changes=2, resume=True),
        op('process_info', process_id=uid(550))])
    S('STD-TXN-070', 'a committed bulk process is not resumed', [
        op('bulk', process_id=uid(550), branch='main', changes=rows, max_changes=2),
        op('bulk', error='FLOW.TRANSITION_NOT_ALLOWED', process_id=uid(550), branch='main', changes=rows,
           resume=True)])
