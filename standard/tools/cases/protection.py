"""Vector cases of BPA 1.0-rc8 for protection (PH-2 step 1e, DEC-093): encrypted fields under
subject keys (STD-OBJ-010), erasure (STD-OBJ-011), authorization relations (STD-LGS-050),
relation-based access rules (STD-LGS-051) and field masking (STD-LGS-052). Not required of the
PH-0 prototype."""
from .common import NOW, cls, field, uid, create
from .common import commit as _commit
from .model import port
from .sheets import op, sheet
from .execution import asset, fn

BR = 'feature/sec'
ALICE, BOB, CAROL, DAVE = ('did:ubos:acme:01900000-0000-7000-8000-0000000000a%d' % n for n in range(1, 5))
P1, P2, P3, P4 = uid(1), uid(2), uid(3), uid(4)
T1, A1 = uid(10), uid(20)
U = 'ubos://acme/Person/%s'
PERSON_REF = {'target_class': 'Person'}
PERSON = cls('Person', [field('name', 'text', True), field('did', 'text'),
                        field('tax_id', 'text', classification='personal'),
                        field('email', 'text', classification='personal'),
                        field('salary', 'decimal', classification='confidential'),
                        field('region', 'text'),
                        field('manager', 'reference', constraints=PERSON_REF, authz_relation='manager')],
             subject_key='self', principal_field='did')
TEAM = cls('Team', [field('name', 'text', True),
                    field('members', 'list', constraints={'item_type': {'type': 'reference', 'constraints': PERSON_REF}},
                          authz_relation='member')])
AGREEMENT = cls('Agreement', [field('title', 'text', True), field('status', 'text'),
                              field('owner', 'reference', constraints=PERSON_REF, authz_relation='owner'),
                              field('team', 'reference', constraints={'target_class': 'Team'}, authz_relation='team'),
                              field('iban', 'text', classification='personal')],
                subject_key='owner', ports=[port('sign', 'acme.agr#sign')])
PLAIN = cls('Contact', [field('name', 'text', True), field('phone', 'text', classification='personal')])
CLASSES = [PERSON, TEAM, AGREEMENT, PLAIN]
SIGN = asset(80, 'acme.agr', {'sign': fn([{'do': 'read.get', 'args': {'target': 'self_id'}},
                                          {'do': 'commit.update', 'args': {'target': 'self_id',
                                                                           'set': {'status': '"signed"'}}},
                                          {'return': '"signed"'}])})
BOB_P = create(P2, 'Person', 'bob', {'name': 'Bob', 'did': BOB, 'region': 'east'})
ALICE_P = create(P1, 'Person', 'alice', {'name': 'Alice', 'did': ALICE, 'tax_id': '123-45-6789',
                                         'email': 'alice@example.com', 'salary': '5000', 'region': 'east',
                                         'manager': U % 'bob'})
CAROL_P = create(P3, 'Person', 'carol', {'name': 'Carol', 'did': CAROL, 'region': 'west'})
DAVE_P = create(P4, 'Person', 'dave', {'name': 'Dave', 'did': DAVE, 'region': 'west'})
TEAM_T = create(T1, 'Team', 'ops', {'name': 'Ops', 'members': [U % 'dave']})
AGR = create(A1, 'Agreement', 'lease', {'title': 'Lease', 'status': 'draft', 'owner': U % 'alice',
                                        'team': 'ubos://acme/Team/ops', 'iban': 'DE89370400440532013000'})
DATA = [SIGN, BOB_P, ALICE_P, CAROL_P, DAVE_P, TEAM_T, AGR]
KEY = '42' * 32


def commit(changes, branch=BR, **kw):
    return _commit(changes, branch=branch, **kw)


def rd(oid, label=None, error=None, viewer=None, **kw):
    if viewer is not None:
        kw['as'] = viewer
    return op('read', label, error, branch=BR, object_id=oid, **kw)


def who(principal, *clearances, **ctx):
    v = {'principal': principal}
    if clearances:
        v['clearances'] = list(clearances)
    if ctx:
        v['ctx'] = ctx
    return v


def access(oid, principal, action='read', label=None, **kw):
    return op('check_access', label, None, branch=BR, object_id=oid, principal=principal, action=action, **kw)


def rules(source, n=900):
    return commit([sheet(uid(n), 'access-%d' % n, source)])


def upd(oid, label, body, error=None):
    return commit([{'op': 'update', 'object_id': oid, 'base': {'$ref': '%s/version_hash' % label}, 'body': body}],
                  error=error)


def cases(g):
    def S(clause, desc, steps, data=True, classes=None):
        setup = [{'op': 'set_now', 'input': {'now': NOW}}, {'op': 'load_genesis', 'input': {'classes': classes or CLASSES}},
                 {'op': 'create_branch', 'input': {'name': BR, 'from': 'main'}}]
        if data:
            setup.append(commit(DATA, label='d'))
        g.vector(clause, desc, setup=setup, steps=steps, prototype=False)

    def P(clause, desc, op_, inp, error=None):
        g.pure(clause, desc, op_, inp, error=error, prototype=False)

    def G(clause, desc, classes, error):
        g.vector(clause, desc, setup=[{'op': 'set_now', 'input': {'now': NOW}}],
                 steps=[{'op': 'load_genesis', 'input': {'classes': classes}, 'error': error}], prototype=False)

    # ---- STD-OBJ-010: encrypted fields ----
    S('STD-OBJ-010', 'personal fields are stored encrypted under the subject key; reads return the plaintext',
      [op('stored_form', None, None, branch=BR, object_id=P1), rd(P1)])
    S('STD-OBJ-010', 'the subject of a class may be a referenced object',
      [op('stored_form', None, None, branch=BR, object_id=A1)])
    S('STD-OBJ-010', 'a disclosure gives the value and salt that match the field commitment',
      [op('disclose', 'x', None, branch=BR, object_id=P1, field='tax_id'),
       {'op': 'verify_disclosure', 'input': {'commitment': {'$ref': 'x/commitment'}, 'salt': {'$ref': 'x/salt'},
                                             'value': '123-45-6789'}},
       {'op': 'verify_disclosure', 'input': {'commitment': {'$ref': 'x/commitment'}, 'salt': {'$ref': 'x/salt'},
                                             'value': '987-65-4321'}}])
    S('STD-OBJ-010', 'a rotated subject key encrypts later writes; earlier versions stay readable',
      [op('subject_key', None, None, subject_id=P1, rotate=True), rd(P1, 'r'),
       upd(P1, 'r', {'email': 'alice@corp.example'}), op('stored_form', None, None, branch=BR, object_id=P1), rd(P1),
       rd(P1, time=1)])
    S('STD-OBJ-010', 'fields that are not personal are stored as they are', [
        commit([create(uid(30), 'Person', 'erin', {'name': 'Erin', 'region': 'north'})]),
        op('stored_form', None, None, branch=BR, object_id=uid(30))])
    S('STD-OBJ-010', 'a class without a subject key stores its fields as they are', [
        commit([create(uid(31), 'Contact', 'frank', {'name': 'Frank', 'phone': '+1 555 0100'})]),
        op('stored_form', None, None, branch=BR, object_id=uid(31))])
    S('STD-OBJ-010', 'an object whose subject key field has no value cannot store personal fields', [
        commit([create(uid(32), 'Agreement', 'loan', {'title': 'Loan', 'iban': 'DE89370400440532013000'})],
               error='MODEL.CONSTRAINT_VIOLATED')])
    G('STD-OBJ-010', 'the subject key names self or an unprotected text or reference field',
      [cls('Person', [field('name', 'text', True), field('tax_id', 'text', classification='personal')],
           subject_key='tax_id')], 'MODEL.INVALID_VALUE')
    S('STD-OBJ-010', 'only encrypted fields are disclosed',
      [op('disclose', None, 'MODEL.INVALID_VALUE', branch=BR, object_id=P1, field='region')])
    S('STD-OBJ-010', 'an archive carries the stored form; without the subject keys the fields are unavailable',
      [op('stored_form', 's', None, branch=BR, object_id=P1), op('export_archive', 'a'), op('reset'), {'op': 'set_now', 'input': {'now': NOW}},
       op('import_archive', None, None, archive={'$ref': 'a/archive'}),
       {'op': 'stored_form', 'input': {'branch': BR, 'object_id': P1}, 'same_as': 's'},
       rd(P1, error='IAM.KEY_UNAVAILABLE')])

    # ---- STD-OBJ-011: erasure ----
    erase = op('erase_subject', None, None, subject_id=P1, reason='GDPR erasure request')
    S('STD-OBJ-011', 'after the subject key is destroyed the fields read as erased markers',
      [erase, rd(P1), rd(A1)])
    S('STD-OBJ-011', 'an erased value cannot be disclosed',
      [erase, op('disclose', None, 'PROOF.VALUE_ERASED', branch=BR, object_id=P1, field='tax_id')])
    S('STD-OBJ-011', 'erasure leaves every hash and proof unchanged',
      [op('prove', 'p', None, branch=BR, object_id=P1), erase,
       {'op': 'prove', 'input': {'branch': BR, 'object_id': P1}, 'same_as': 'p'}])
    S('STD-OBJ-011', 'other fields stay writable; new values for the erased subject are refused',
      [erase, rd(P1, 'r'), upd(P1, 'r', {'region': 'south'}), rd(P1, 'r2'),
       upd(P1, 'r2', {'email': 'new@example.com'}, error='PROOF.VALUE_ERASED')])
    S('STD-OBJ-011', 'a key that is unavailable is not an erased key',
      [op('key_availability', None, None, subject_id=P1, available=False), rd(P1, error='IAM.KEY_UNAVAILABLE'),
       op('key_availability', None, None, subject_id=P1, available=True), rd(P1)])

    # ---- STD-LGS-050: relations ----
    S('STD-LGS-050', 'relations are derived from reference fields marked as authorization relations',
      [op('relations', None, None, branch=BR, object_id=A1), op('relations', None, None, branch=BR, object_id=P1)])
    S('STD-LGS-050', 'a list of references gives one relation per item',
      [op('relations', None, None, branch=BR, object_id=T1)])
    S('STD-LGS-050', 'a change of a relation is visible at the commit that makes it',
      [rd(A1, 'a'), upd(A1, 'a', {'owner': U % 'carol'}), op('relations', None, None, branch=BR, object_id=A1),
       op('relations', None, None, branch=BR, object_id=A1, time=1)])
    G('STD-LGS-050', 'an authorization relation is a reference field',
      [cls('Note', [field('author', 'text', authz_relation='author')])], 'MODEL.INVALID_VALUE')
    G('STD-LGS-050', 'the principal field is a text field',
      [cls('Member', [field('n', 'integer')], principal_field='n')], 'MODEL.INVALID_VALUE')
    S('STD-LGS-050', 'relations of an unknown object are not found',
      [op('relations', None, 'IAM.NOT_FOUND', branch=BR, object_id=uid(99))])

    # ---- STD-LGS-051: access rules ----
    owner_rule = rules('Agreement { access: [{action: "read", allow_if: "principal in this.owner"},'
                       ' {action: "read", allow_if: "principal in this.owner.manager"},'
                       ' {action: "sign", allow_if: "principal in this.owner"}]; }')
    S('STD-LGS-051', 'the owner and the owner\'s manager may read; a peer may not',
      [owner_rule, access(A1, ALICE), access(A1, BOB), access(A1, CAROL)])
    S('STD-LGS-051', 'a read as a principal that no rule allows is not found',
      [owner_rule, rd(A1, viewer=who(BOB, 'personal')), rd(A1, error='IAM.NOT_FOUND', viewer=who(CAROL))])
    S('STD-LGS-051', 'a port call that no rule allows is denied',
      [owner_rule,
       op('execute', None, 'IAM.DENIED', branch=BR, target='ubos://acme/Agreement/lease', port='sign',
          process={'process_id': uid(501), 'initiator': BOB}),
       op('execute', None, None, branch=BR, target='ubos://acme/Agreement/lease', port='sign',
          process={'process_id': uid(502), 'initiator': ALICE}), rd(A1)])
    S('STD-LGS-051', 'relation paths compose across objects',
      [rules('Agreement { access: [{action: "read", allow_if: "principal in this.team.member"}]; }'),
       access(A1, DAVE), access(A1, ALICE)])
    S('STD-LGS-051', 'a rule for every action applies to each of them',
      [rules('Agreement { access: [{action: "*", allow_if: "principal in this.owner"}]; }'),
       access(A1, ALICE, 'sign'), access(A1, CAROL, 'sign')])
    S('STD-LGS-051', 'without access rules relations do not restrict an action', [access(A1, CAROL)])
    S('STD-LGS-051', 'the principal of the object itself is the empty path',
      [rules('Person { access: [{action: "read", allow_if: "principal in this"}]; }'), access(P1, ALICE),
       access(P1, BOB)])
    S('STD-LGS-051', 'a relation path has at most four relations',
      [commit([sheet(uid(900), 'deep', 'Agreement { access: [{action: "read", allow_if: '
                                       '"principal in this.owner.manager.manager.manager.manager"}]; }')],
              error='RULE.PARSE_ERROR')])
    S('STD-LGS-051', 'an access rule names a principal and a path',
      [commit([sheet(uid(900), 'bad', 'Agreement { access: [{action: "read", allow_if: "this.owner == principal"}]; }')],
              error='RULE.PARSE_ERROR')])
    S('STD-LGS-051', 'access is checked at the snapshot of the request',
      [owner_rule, rd(A1, 'a'), upd(A1, 'a', {'owner': U % 'carol'}), access(A1, ALICE),
       access(A1, ALICE, time=2)])
    S('STD-LGS-051', 'a query as a principal returns only the objects it may read',
      [commit([create(uid(21), 'Agreement', 'loan', {'title': 'Loan', 'owner': U % 'carol'})]), owner_rule,
       op('query', None, None, branch=BR, **{'from': 'Agreement', 'as': who(ALICE)}),
       op('query', None, None, branch=BR, **{'from': 'Agreement', 'as': who(CAROL)}),
       op('query', None, None, branch=BR, **{'from': 'Agreement'})])

    # ---- STD-LGS-052: masking ----
    def masks(source):
        return [op('lookup_key', None, None, key=KEY), rules(source, 901)]

    S('STD-LGS-052', 'a partial mask keeps the last four characters',
      masks('Person { mask.tax_id: "partial"; }') + [rd(P1, viewer=who(BOB, 'personal'))])
    S('STD-LGS-052', 'a hash mask gives the keyed hash of the value',
      masks('Person { mask.email: "hash"; }') + [rd(P1, viewer=who(BOB, 'personal'))])
    S('STD-LGS-052', 'a hide mask removes the field',
      masks('Person { mask.region: "hide"; }') + [rd(P1, viewer=who(BOB, 'personal'))])
    S('STD-LGS-052', 'fields above the viewer\'s clearances are hidden',
      [rd(P1, viewer=who(BOB)), rd(P1, viewer=who(BOB, 'confidential')), rd(P1, viewer=who(BOB, 'personal'))])
    S('STD-LGS-052', 'masks with a context qualifier apply in that context only',
      masks('Person:ctx(role = "support") { mask.tax_id: "partial"; }') +
      [rd(P1, viewer=who(BOB, 'personal', role='support')), rd(P1, viewer=who(BOB, 'personal', role='auditor'))])
    S('STD-LGS-052', 'a read without a viewer is the system read and is not masked',
      masks('Person { mask.tax_id: "hide"; }') + [rd(P1)])
    S('STD-LGS-052', 'a query may not filter on a field masked for the viewer',
      [op('query', None, 'QRY.INVALID_QUERY', branch=BR, **{'from': 'Person', 'where': 'this.tax_id == "123-45-6789"',
                                                            'as': who(BOB)}),
       op('query', None, None, branch=BR, **{'from': 'Person', 'where': 'this.region == "east"', 'as': who(BOB)})])
    S('STD-LGS-052', 'a query may not sort on a field masked for the viewer',
      [op('query', None, 'QRY.INVALID_QUERY', branch=BR, **{'from': 'Person', 'order_by': [{'path': 'this.salary'}],
                                                            'as': who(BOB)})])
    S('STD-LGS-052', 'event payloads are masked for a subscriber like the object',
      [commit([], process={'effects': [{'type': 'PersonUpdated', 'object_id': P1,
                                        'payload': {'name': 'Alice', 'tax_id': '123-45-6789', 'region': 'east'}}]}),
       op('outbox', None, None, **{'as': who(BOB)}), op('outbox')])
    S('STD-LGS-052', 'an erased field is masked like any other',
      [op('erase_subject', None, None, subject_id=P1, reason='erasure'), rd(P1, viewer=who(BOB)),
       rd(P1, viewer=who(BOB, 'personal'))])
    S('STD-LGS-052', 'clearances are classification levels',
      [rd(P1, error='MODEL.INVALID_VALUE', viewer=who(BOB, 'secret'))])
