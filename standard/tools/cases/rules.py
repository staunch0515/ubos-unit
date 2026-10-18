"""Vector cases of BPA 1.0-rc2 for validations, invariants, commit-enforced declarations
and decisions (PH-1 step 10c, DEC-067): STD-LGS-030, STD-LGS-031, the pin-out property of
BPU instances (STD-BPU-003) and the instructions `read.explain` (STD-ISA-035), `run.validate`
(STD-ISA-054) and `search.query` (STD-ISA-080). Not required of the PH-0 prototype."""
from .common import cls, field, uid, create, base, NOW
from .common import commit as _commit
from .model import port
from .sheets import sheet, prop, op, cascade  # noqa: F401
from .execution import asset, fn, SEED
from .history import cs

APPROVER = 'did:ubos:acme:01900000-0000-7000-8000-00000000000a'


def promote(n=40):
    """Merge the feature branch into main through a change set (STD-VER-012)."""
    return [cs('open', uid(n), source=BR, target='main', title='promote'), cs('submit', uid(n)),
            cs('approve', uid(n), approver=APPROVER), cs('merge', uid(n))]

BR = 'feature/rules'
FB = {'op': 'create_branch', 'input': {'name': BR, 'from': 'main'}}
SHEET = uid(900)

PARTY = cls('Party', [field('name', 'text', True), field('country', 'text', default='US'),
                      field('state', 'enum', constraints={'allowed_values': ['draft', 'active', 'closed']})])
CUSTOMER = cls('Customer', [field('email', 'text'), field('credit_limit', 'money'), field('rating', 'integer')],
               parent='Party',
               validations=[{'name': 'rating_range', 'rule': 'this.rating is null or (this.rating >= 1 and this.rating <= 5)',
                             'severity': 'error', 'message': 'rating is 1 to 5'}])
ACCOUNT = cls('Account', [field('name', 'text', True), field('balance', 'decimal'), field('status', 'text')],
              ports=[port('rename', 'acme.acct#rename', params=[field('name', 'text', True)]),
                     port('close', 'acme.acct#close'),
                     port('peek', 'acme.acct#peek', kind='query')])
MOVE = cls('CashMove', [field('account', 'reference', constraints={'target_class': 'Account'}),
                        field('amount', 'decimal', True)], kind='ledger',
           ledger={'key': ['account'], 'amounts': ['amount'], 'double_entry': False})
GUARDED = cls('Vault', [field('name', 'text', True), field('amount', 'decimal')], write_policy='change_set')
CLASSES = [PARTY, CUSTOMER, ACCOUNT, MOVE, GUARDED]

ACCT = asset(80, 'acme.acct', {
    'rename': fn([{'do': 'read.get', 'args': {'target': 'self_id'}},
                  {'do': 'commit.update', 'args': {'target': 'self_id', 'set': {'name': 'params.name'}}},
                  {'return': 'params.name'}], ['name']),
    'close': fn([{'do': 'read.get', 'args': {'target': 'self_id'}},
                 {'do': 'commit.update', 'args': {'target': 'self_id', 'set': {'status': '"closed"'}}},
                 {'return': '"closed"'}]),
    'peek': fn([{'do': 'read.get', 'args': {'target': 'self_id'}, 'as': 'o'}, {'return': 'o.fields.status'}])})


def commit(changes, branch=BR, **kw):
    return _commit(changes, branch=branch, **kw)


def validate(changes, label=None, error=None, branch=BR, process=None):
    inp = {'branch': branch, 'changes': changes}
    if process:
        inp['process'] = process
    return op('validate', label, error, **inp)


def decide(oid, port_, label=None, error=None, branch=BR, params=None, ctx=None):
    inp = {'branch': branch, 'object_id': oid, 'port': port_}
    if params is not None:
        inp['params'] = params
    if ctx is not None:
        inp['ctx'] = ctx
    return op('decide', label, error, **inp)


def ex(target, port_, params=None, label=None, error=None, branch=BR, pid=1, ctx=None, **kw):
    inp = {'branch': branch, 'target': target, 'port': port_, 'params': params or {},
           'process': {'process_id': uid(500 + pid)}, 'entropy_seed': SEED}
    if ctx is not None:
        inp['ctx'] = ctx
    inp.update(kw)
    s = {'op': 'execute', 'input': inp}
    if label:
        s['label'] = label
    if error:
        s['error'] = error
    return s


def prog(n, *body, caps=('commit', 'read', 'run', 'search')):
    return asset(n, 'acme.app%d' % n, {'main': fn(list(body), [])}, caps=caps)


def run(a, label=None, error=None, branch=BR, pid=1, **kw):
    inp = {'branch': branch, 'asset': a, 'function': 'main', 'args': [], 'process': {'process_id': uid(500 + pid)},
           'entropy_seed': SEED}
    inp.update(kw)
    s = {'op': 'execute', 'input': inp}
    if label:
        s['label'] = label
    if error:
        s['error'] = error
    return s


def cases(g):
    V = g.vector

    def S(clause, desc, steps, classes=None, now_=NOW, also=()):
        V(clause, desc, setup=[{'op': 'set_now', 'input': {'now': now_}},
                               {'op': 'load_genesis', 'input': {'classes': classes or CLASSES}}, FB],
          steps=steps, prototype=False, also=also)

    validations(S)
    enforcement(S)
    decisions(S)
    pinout(S)
    instructions(S)


def c(n, slug, body=None, **extra):
    b = {'name': slug.title()}
    b.update(body or {})
    return create(uid(n), 'Customer', slug, b, **extra)


def validations(S):
    limit = sheet(SHEET, 'limits', 'Customer { validations: [{name: "credit", rule: "this.credit_limit is null or '
                  'this.credit_limit <= 100000USD", severity: "error", message: "credit limit too high"}]; }')
    warn = sheet(uid(902), 'warn', 'Customer { validations: [{name: "email", rule: "this.email is not null", '
                 'severity: "warning", message: "no email"}]; }')
    S('STD-LGS-030', 'a validation declared in a sheet blocks a commit that violates it', [
        commit([limit]), commit([c(1, 'acme', {'credit_limit': {'amount': '200000.00', 'currency': 'USD'}})],
                                error='RULE.VALIDATION_FAILED', details=True)])
    S('STD-LGS-030', 'a validation that holds lets the commit through', [
        commit([limit]), commit([c(1, 'acme', {'credit_limit': {'amount': '50000.00', 'currency': 'USD'}})])])
    S('STD-LGS-030', 'a warning-severity validation commits and reports the warning', [
        commit([warn]), commit([c(1, 'acme')])])
    S('STD-LGS-030', 'an info-severity validation is reported with the warnings', [
        commit([sheet(SHEET, 'i', 'Customer { validations: [{name: "note", rule: "false", severity: "info", '
                      'message: "fyi"}]; }')]), commit([c(1, 'acme')])])
    S('STD-LGS-030', 'every error-level violation of a commit is reported together', [
        commit([limit, sheet(uid(903), 'names', 'Customer { validations: [{name: "name", rule: "this.name != \\"X\\"", '
                                               'severity: "error", message: "bad name"}]; }')]),
        commit([c(1, 'x', {'name': 'X', 'credit_limit': {'amount': '200000.00', 'currency': 'USD'}}), c(2, 'y', {'name': 'X'})],
               error='RULE.VALIDATION_FAILED', details=True)])
    S('STD-LGS-030', 'the number of violations reported is capped by the configured maximum', [
        {'op': 'set_limits', 'input': {'max_violations': 1}},
        commit([limit, sheet(uid(903), 'names', 'Customer { validations: [{name: "name", rule: "this.name != \\"X\\"", '
                                               'severity: "error", message: "bad name"}]; }')]),
        commit([c(1, 'x', {'name': 'X', 'credit_limit': {'amount': '200000.00', 'currency': 'USD'}})],
               error='RULE.VALIDATION_FAILED', details=True)])
    S('STD-LGS-030', 'a validation may compare with the base version of the object', [
        commit([sheet(SHEET, 'nodrop', 'Customer { validations: [{name: "no_drop", rule: "old.credit_limit is null or '
                      'this.credit_limit >= old.credit_limit", severity: "error", message: "limit cannot drop"}]; }')]),
        commit([c(1, 'acme', {'credit_limit': {'amount': '100.00', 'currency': 'USD'}})], label='c'),
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('c', uid(1)),
                 'body': {'credit_limit': {'amount': '50.00', 'currency': 'USD'}}}], error='RULE.VALIDATION_FAILED', details=True)])
    S('STD-LGS-030', 'a validation declared on the class applies without any sheet', [
        commit([c(1, 'acme', {'rating': 9})], error='RULE.VALIDATION_FAILED', details=True)])
    S('STD-LGS-030', 'a validation declared by an extension applies to the target', [
        commit([create(uid(90), 'ClassExtension', 'crm', {'target': 'Customer', 'namespace': 'acme.crm', 'order': 1,
                                                          'validations': [{'name': 'country', 'rule': 'this.country in ["US", "CA"]',
                                                                           'severity': 'error', 'message': 'country'}]})]),
        commit([c(1, 'acme', {'country': 'FR'})], error='RULE.VALIDATION_FAILED', details=True)])
    S('STD-LGS-030', 'validations apply to the objects the sheet selector matches', [
        commit([sheet(SHEET, 'vip', 'Customer[this.rating == 5] { validations: [{name: "email", rule: "this.email is not null", '
                      'severity: "error", message: "vip needs email"}]; }')]),
        commit([c(1, 'plain', {'rating': 3})]),
        commit([c(2, 'vip', {'rating': 5})], error='RULE.VALIDATION_FAILED', details=True)])
    S('STD-LGS-030', 'a validation whose rule cannot be evaluated counts as violated', [
        commit([sheet(SHEET, 'bad', 'Customer { validations: [{name: "len", rule: "length(this.email) > 3", '
                      'severity: "error", message: "short email"}]; }')]),
        commit([c(1, 'acme')], error='RULE.VALIDATION_FAILED', details=True)])
    S('STD-LGS-030', 'an invariant is evaluated for the written objects of its scope', [
        commit([sheet(SHEET, 'inv', 'Party { invariants: [{name: "us_only", scope: ["Customer"], rule: "this.country == \\"US\\"", '
                      'severity: "error", message: "customers are US"}]; }')]),
        commit([c(1, 'acme', {'country': 'CA'})], error='RULE.VALIDATION_FAILED', details=True)])
    S('STD-LGS-030', 'an invariant is not evaluated for objects outside its scope', [
        commit([sheet(SHEET, 'inv', 'Party { invariants: [{name: "us_only", scope: ["Account"], rule: "this.country == \\"US\\"", '
                      'severity: "error", message: "x"}]; }')]),
        commit([c(1, 'acme', {'country': 'CA'})])])
    S('STD-LGS-030', 'the validation preview reports what the commit would reject', [
        commit([limit]), validate([c(1, 'acme', {'credit_limit': {'amount': '200000.00', 'currency': 'USD'}})])])
    S('STD-LGS-030', 'the validation preview reports the warnings of a commit that would pass', [
        commit([warn]), validate([c(1, 'acme')])])
    S('STD-LGS-030', 'the validation preview commits nothing', [
        commit([limit]), validate([c(1, 'acme')]),
        op('read', branch=BR, object_id=uid(1), error='IAM.NOT_FOUND')])
    S('STD-LGS-030', 'the validation preview fails like a commit on other errors', [
        validate([c(1, 'acme', {'rating': 'high'})], error='MODEL.INVALID_VALUE')])
    S('STD-LGS-030', 'a validation item needs a name, a rule and a severity', [
        commit([sheet(SHEET, 'bad', 'Customer { validations: [{rule: "true"}]; }')], error='MODEL.CONSTRAINT_VIOLATED')])


def enforcement(S):
    acme = c(1, 'acme', {'email': 'a@x.com'})
    S('STD-LGS-030', 'read_only rejects an update of a matched object', [
        commit([acme], label='c'), commit([sheet(SHEET, 'ro', 'Customer { read_only: true; }')]),
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('c', uid(1)), 'body': {'email': 'b@x.com'}}],
               error='RULE.DECLARATION_UNSATISFIED', details=True)])
    S('STD-LGS-030', 'read_only rejects ending a matched object', [
        commit([acme], label='c'), commit([sheet(SHEET, 'ro', 'Customer { read_only: true; }')]),
        commit([{'op': 'end', 'object_id': uid(1), 'base': base('c', uid(1))}], error='RULE.DECLARATION_UNSATISFIED', details=True)])
    S('STD-LGS-030', 'read_only does not stop a create', [
        commit([sheet(SHEET, 'ro', 'Customer { read_only: true; }')]), commit([acme])])
    S('STD-LGS-030', 'field_required rejects a matched object without the field', [
        commit([sheet(SHEET, 'req', 'Customer { field_required: ["email"]; }')]),
        commit([c(1, 'acme')], error='RULE.DECLARATION_UNSATISFIED', details=True)])
    S('STD-LGS-030', 'field_required accepts an object with the field', [
        commit([sheet(SHEET, 'req', 'Customer { field_required: ["email"]; }')]), commit([acme])])
    S('STD-LGS-030', 'field_default fills absent fields on create', [
        commit([sheet(SHEET, 'def', 'Customer { field_default: {country: "CA", rating: 3}; }')]),
        commit([c(1, 'acme', {'country': 'US'})]), op('read', branch=BR, object_id=uid(1))])
    S('STD-LGS-030', 'audit_level strict requires a message and a reason with the commit', [
        commit([sheet(SHEET, 'strict', 'Customer { audit_level: "strict"; }')]),
        commit([acme], error='RULE.DECLARATION_UNSATISFIED', details=True),
        commit([acme], process={'message': 'new customer', 'reason': 'onboarding'})])
    S('STD-LGS-030', 'write_policy change_set from a sheet requires a change set on a protected branch', [
        commit([sheet(SHEET, 'wp', 'Customer { write_policy: "change_set"; }')])] + promote() + [
        commit([acme], branch='main', error='VER.CHANGE_SET_REQUIRED'), commit([acme])])
    S('STD-LGS-030', 'declarations are not enforced on the sheets and extensions themselves', [
        commit([sheet(SHEET, 'all', '* { audit_level: "strict"; field_required: ["email"]; }')]),
        commit([sheet(uid(902), 'more', 'Customer { read_only: false; }')])])
    S('STD-LGS-030', 'declarations enforce in the context of the branch', [
        commit([sheet(SHEET, 'sb', 'Customer:ctx(branch_kind = "sandbox") { read_only: true; }')]),
        commit([acme], label='c'), op('create_branch', name='sandbox/try', **{'from': BR}),
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('c', uid(1)), 'body': {'email': 'b@x.com'}}],
               branch='sandbox/try', error='RULE.DECLARATION_UNSATISFIED', details=True),
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('c', uid(1)), 'body': {'email': 'b@x.com'}}])])
    # since BPA 1.0-rc7 (DEC-084): the rule binding lists the sheets the enforcement evaluated
    S('STD-LGS-030', 'the rule binding of a commit lists the sheets its enforcement evaluated', [
        commit([sheet(SHEET, 'strict', 'Customer { audit_level: "strict"; }'),
                sheet(uid(902), 'acct', 'Account { read_only: true; }')]),
        commit([acme], process={'message': 'new customer', 'reason': 'onboarding'}), op('commit_info', branch=BR)],
      also=('STD-TIME-040',))


def decisions(S):
    acct = create(uid(1), 'Account', 'a-1', {'name': 'Acme', 'balance': '100', 'status': 'open'})
    T = 'ubos://acme/Account/a-1'
    deploy = [commit([ACCT, acct])]
    S('STD-LGS-031', 'without applicable decisions a command port is allowed', deploy + [decide(uid(1), 'close')])
    S('STD-LGS-031', 'a deny decision denies', deploy + [
        commit([sheet(SHEET, 'd', 'Account { decisions: [{name: "no_close", outcome: "deny", message: "closing is off"}]; }')]),
        decide(uid(1), 'close')])
    S('STD-LGS-031', 'a decision with a condition applies when it holds', deploy + [
        commit([sheet(SHEET, 'd', 'Account { decisions: [{name: "rich", when: "this.balance > 50", outcome: "deny"}]; }')]),
        decide(uid(1), 'close')])
    S('STD-LGS-031', 'a decision with a condition does not apply when it fails', deploy + [
        commit([sheet(SHEET, 'd', 'Account { decisions: [{name: "rich", when: "this.balance > 500", outcome: "deny"}]; }')]),
        decide(uid(1), 'close')])
    S('STD-LGS-031', 'a condition may use the port parameters', deploy + [
        commit([sheet(SHEET, 'd', 'Account { decisions: [{name: "short", when: "length(params.name) < 3", outcome: "deny"}]; }')]),
        decide(uid(1), 'rename', 'short', params={'name': 'ab'}), decide(uid(1), 'rename', 'long', params={'name': 'abcd'})])
    S('STD-LGS-031', 'a condition may use the context, including the principal kind', deploy + [
        commit([sheet(SHEET, 'd', 'Account { decisions: [{name: "agents", when: "ctx.principal_kind == \\"agent\\"", '
                      'outcome: "require", steps: ["approval"]}]; }')]),
        decide(uid(1), 'close', 'agent', ctx={'principal_kind': 'agent'}), decide(uid(1), 'close', 'human')])
    S('STD-LGS-031', 'require collects the union of the steps', deploy + [
        commit([sheet(SHEET, 'd', 'Account { decisions: [{name: "a", outcome: "require", steps: ["approval", "reason"]}, '
                      '{name: "b", outcome: "require", steps: ["signature", "reason"]}]; }')]),
        decide(uid(1), 'close')])
    S('STD-LGS-031', 'any deny wins over require', deploy + [
        commit([sheet(SHEET, 'd', 'Account { decisions: [{name: "a", outcome: "require", steps: ["approval"]}, '
                      '{name: "b", outcome: "deny"}]; }')]),
        decide(uid(1), 'close')])
    S('STD-LGS-031', 'an allow decision adds nothing', deploy + [
        commit([sheet(SHEET, 'd', 'Account { decisions: [{name: "ok", outcome: "allow"}]; }')]), decide(uid(1), 'close')])
    S('STD-LGS-031', 'the approvals property requires an approval step', deploy + [
        commit([sheet(SHEET, 'd', 'Account { approvals: 2; }')]), decide(uid(1), 'close')])
    S('STD-LGS-031', 'require_signature and maker_checker require signature and second-person steps', deploy + [
        commit([sheet(SHEET, 'd', 'Account { require_signature: true; maker_checker: true; }')]), decide(uid(1), 'close')])
    S('STD-LGS-031', 'decisions do not apply to query ports', deploy + [
        commit([sheet(SHEET, 'd', 'Account { decisions: [{name: "no", outcome: "deny"}]; approvals: 1; }')]),
        decide(uid(1), 'peek')])
    S('STD-LGS-031', 'decisions from several sheets combine', deploy + [
        commit([sheet(SHEET, 'a', 'Account { decisions: [{name: "a", outcome: "require", steps: ["approval"]}]; }', layer='platform'),
                sheet(uid(902), 'b', 'Account { decisions: [{name: "b", outcome: "require", steps: ["signature"]}]; }')]),
        decide(uid(1), 'close')])
    S('STD-LGS-031', 'the result binds the sheet versions evaluated', deploy + [
        commit([sheet(SHEET, 'd', 'Account { decisions: [{name: "no", outcome: "deny"}]; }')]),
        decide(uid(1), 'close', 'd'), op('read', 'r', branch=BR, object_id=SHEET),
        {'op': 'decide', 'input': {'branch': BR, 'object_id': uid(1), 'port': 'close'}, 'same_as': 'd'}])
    S('STD-LGS-031', 'a denied port call fails before the port runs', deploy + [
        commit([sheet(SHEET, 'd', 'Account { decisions: [{name: "no_close", outcome: "deny"}]; }')]),
        ex(T, 'close', error='RULE.DECISION_DENIED', details=True), op('read', branch=BR, object_id=uid(1))])
    S('STD-LGS-031', 'a port call that requires steps stops with the steps', deploy + [
        commit([sheet(SHEET, 'd', 'Account { approvals: 1; }')]),
        ex(T, 'close', error='RULE.DECISION_REQUIRES', details=True)])
    S('STD-LGS-031', 'an allowed port call runs', deploy + [
        commit([sheet(SHEET, 'd', 'Account { decisions: [{name: "rich", when: "this.balance > 500", outcome: "deny"}]; }')]),
        ex(T, 'close'), op('read', branch=BR, object_id=uid(1))])
    S('STD-LGS-031', 'the decision context of a port call carries the principal kind of the process', deploy + [
        commit([sheet(SHEET, 'd', 'Account { decisions: [{name: "agents", when: "ctx.principal_kind == \\"agent\\"", '
                      'outcome: "deny"}]; }')]),
        ex(T, 'close', label='agent', error='RULE.DECISION_DENIED', ctx={'principal_kind': 'agent'}), ex(T, 'close', label='human', pid=2)])
    S('STD-LGS-031', 'a decision may condition on the role in the context', deploy + [
        commit([sheet(SHEET, 'd', 'Account { decisions: [{name: "cfo_only", when: "ctx.role != \\"cfo\\"", outcome: "deny"}]; }')]),
        decide(uid(1), 'close', 'clerk', ctx={'role': 'clerk'}), decide(uid(1), 'close', 'cfo', ctx={'role': 'cfo'})])
    S('STD-LGS-031', 'the decisions of an unknown port fail', deploy + [decide(uid(1), 'explode', error='MODEL.PORT_NOT_IMPLEMENTED')])


def pinout(S):
    acct = create(uid(1), 'Account', 'a-1', {'name': 'Acme', 'balance': '100', 'status': 'open'})
    vault = create(uid(5), 'Vault', 'v-1', {'name': 'Main', 'amount': '10'})
    T = 'ubos://acme/Account/a-1'
    S('STD-BPU-003', 'a port changes the state of a BPU instance', [
        commit([ACCT, acct]), ex(T, 'close'), op('read', branch=BR, object_id=uid(1))])
    S('STD-BPU-003', 'a governed Definition edit changes the state under the write policy of the class', [
        commit([acct], label='c'),
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('c', uid(1)), 'body': {'status': 'closed'}}]),
        op('read', branch=BR, object_id=uid(1))])
    S('STD-BPU-003', 'a direct edit on a protected branch needs a change set when the class requires one', [
        commit([vault], label='c')] + promote() + [
        commit([{'op': 'update', 'object_id': uid(5), 'base': base('c', uid(5)), 'body': {'amount': '20'}}],
               branch='main', error='VER.CHANGE_SET_REQUIRED')])
    S('STD-BPU-003', 'a sheet may raise the write policy of a BPU class to change sets', [
        commit([acct], label='c'), commit([sheet(SHEET, 'wp', 'Account { write_policy: "change_set"; }')])] + promote() + [
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('c', uid(1)), 'body': {'status': 'closed'}}],
               branch='main', error='VER.CHANGE_SET_REQUIRED')])
    S('STD-BPU-003', 'ledger entries referencing a BPU instance are governed postings', [
        commit([acct]),
        commit([{'op': 'append', 'class': 'CashMove', 'entry_id': uid(20), 'ledger_key': {'account': T},
                 'valid_at': '2026-03-15', 'body': {'account': T, 'amount': '5'}}]),
        op('read_ledger', branch=BR, **{'class': 'CashMove', 'ledger_key': {'account': T}})])
    S('STD-BPU-003', 'a denied decision leaves the state untouched', [
        commit([ACCT, acct]), commit([sheet(SHEET, 'd', 'Account { decisions: [{name: "no", outcome: "deny"}]; }')]),
        ex(T, 'close', error='RULE.DECISION_DENIED'), op('read', branch=BR, object_id=uid(1))])


def instructions(S):
    acct = create(uid(1), 'Account', 'a-1', {'name': 'Acme', 'balance': '100', 'status': 'open'})
    acct2 = create(uid(2), 'Account', 'a-2', {'name': 'Beta', 'balance': '20', 'status': 'open'})
    T = '"ubos://acme/Account/a-1"'
    strict = sheet(SHEET, 's', 'Account { audit_level: "strict"; }', layer='platform')
    tenant = sheet(uid(902), 't', 'Account[this.balance > 50] { audit_level: "basic"; }')
    # STD-ISA-035 read.explain
    S('STD-ISA-035', 'read.explain cascade returns the explanation of a property', [
        commit([acct, strict, tenant, prog(90, {'do': 'read.explain', 'args': {'kind': '"cascade"', 'target': T, 'property': '"audit_level"'}, 'as': 'e'},
                                             {'return': 'e'})]), run('acme.app90')])
    S('STD-ISA-035', 'read.explain cascade needs the property', [
        commit([acct, prog(90, {'do': 'read.explain', 'args': {'kind': '"cascade"', 'target': T}})]),
        run('acme.app90', error='MODEL.DECLARATION_INCOMPLETE')])
    S('STD-ISA-035', 'read.explain decision returns the decision of a port', [
        commit([acct, sheet(SHEET, 'd', 'Account { approvals: 1; }'),
                prog(90, {'do': 'read.explain', 'args': {'kind': '"decision"', 'target': T, 'property': '"close"'}, 'as': 'e'},
                     {'return': 'e'})]), run('acme.app90')])
    S('STD-ISA-035', 'read.explain validation reports the findings of the pending changes', [
        commit([acct, sheet(SHEET, 'v', 'Account { validations: [{name: "pos", rule: "this.balance >= 0", severity: "warning", message: "negative"}]; }'),
                prog(90, {'do': 'read.get', 'args': {'target': T}}, {'do': 'commit.update', 'args': {'target': T, 'set': {'balance': '"-1"'}}},
                     {'do': 'read.explain', 'args': {'kind': '"validation"', 'target': T}, 'as': 'e'},
                     {'return': 'e'})]), run('acme.app90')])
    S('STD-ISA-035', 'read.explain lineage comes with PH-2', [
        commit([acct, prog(90, {'do': 'read.explain', 'args': {'kind': '"lineage"', 'target': T}})]),
        run('acme.app90', error='LOGIC.NOT_AVAILABLE')])
    S('STD-ISA-035', 'read.explain needs the explain capability', [
        commit([acct, prog(90, {'do': 'read.explain', 'args': {'kind': '"cascade"', 'target': T, 'property': '"audit_level"'}},
                            caps=('commit',))]), run('acme.app90', error='LOGIC.FORBIDDEN_CAPABILITY')])
    # STD-ISA-054 run.validate
    S('STD-ISA-054', 'run.validate previews the unit of work on a target without committing', [
        commit([acct, sheet(SHEET, 'v', 'Account { validations: [{name: "pos", rule: "this.balance >= 0", severity: "error", message: "negative"}]; }'),
                prog(90, {'do': 'read.get', 'args': {'target': T}}, {'do': 'commit.update', 'args': {'target': T, 'set': {'balance': '"-1"'}}},
                     {'do': 'run.validate', 'args': {'target': T}, 'as': 'v'}, {'return': 'v'})]),
        run('acme.app90', error='RULE.VALIDATION_FAILED')])
    S('STD-ISA-054', 'run.validate reports no violation for a unit of work that would pass', [
        commit([acct, sheet(SHEET, 'v', 'Account { validations: [{name: "pos", rule: "this.balance >= 0", severity: "error", message: "negative"}]; }'),
                prog(90, {'do': 'read.get', 'args': {'target': T}}, {'do': 'commit.update', 'args': {'target': T, 'set': {'balance': '"5"'}}},
                     {'do': 'run.validate', 'args': {'target': T}, 'as': 'v'}, {'return': 'v'})]),
        run('acme.app90'), op('read', branch=BR, object_id=uid(1))])
    S('STD-ISA-054', 'run.validate reports warnings', [
        commit([acct, sheet(SHEET, 'v', 'Account { validations: [{name: "low", rule: "this.balance >= 10", severity: "warning", message: "low"}]; }'),
                prog(90, {'do': 'read.get', 'args': {'target': T}}, {'do': 'commit.update', 'args': {'target': T, 'set': {'balance': '"5"'}}},
                     {'do': 'run.validate', 'args': {'target': T}, 'as': 'v'}, {'return': 'v'})]),
        run('acme.app90')])
    # STD-ISA-080 search.query
    S('STD-ISA-080', 'search.query evaluates a criteria query at the process snapshot', [
        commit([acct, acct2, prog(90, {'do': 'search.query', 'args': {'query': {'from': '"Account"', 'where': '"this.balance > 50"'}}, 'as': 'r'},
                                  {'return': 'r.items'})]), run('acme.app90')])
    S('STD-ISA-080', 'search.query accepts the textual form', [
        commit([acct, acct2, prog(90, {'do': 'search.query', 'args': {'query': '"from Account where this.balance > 50 order by this.name"'}, 'as': 'r'},
                                  {'return': 'r.items'})]), run('acme.app90')])
    S('STD-ISA-080', 'search.query orders and limits', [
        commit([acct, acct2, prog(90, {'do': 'search.query', 'args': {'query': {'from': '"Account"', 'order_by': [{'path': '"this.balance"', 'dir': '"desc"'}], 'limit': '1'}}, 'as': 'r'},
                                  {'return': 'r.items'})]), run('acme.app90')])
    S('STD-ISA-080', 'search.query rejects a malformed query', [
        commit([acct, prog(90, {'do': 'search.query', 'args': {'query': '"select * from Account"'}})]),
        run('acme.app90', error='QRY.INVALID_QUERY')])
    S('STD-ISA-080', 'search.query needs the search capability', [
        commit([acct, prog(90, {'do': 'search.query', 'args': {'query': {'from': '"Account"'}}}, caps=('read',))]),
        run('acme.app90', error='LOGIC.FORBIDDEN_CAPABILITY')])
    S('STD-ISA-080', 'search.query reads the process snapshot, not the pending changes of the process', [
        commit([acct, acct2, prog(90, {'do': 'read.get', 'args': {'target': T}}, {'do': 'commit.update', 'args': {'target': T, 'set': {'balance': '"0"'}}},
                                  {'do': 'search.query', 'args': {'query': {'from': '"Account"', 'where': '"this.balance > 50"'}}, 'as': 'r'},
                                  {'return': 'r.items'})]), run('acme.app90')])
