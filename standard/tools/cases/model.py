"""Vector cases of BPA 1.0-rc1 for the object and class model (PH-1 step 1b, DEC-040):
blind indexes (STD-OBJ-012), port dispatch and overrides (STD-CLS-020, -021), polymorphic
selectors (STD-CLS-022), extensions (STD-CLS-030), relationships (STD-CLS-040) and
referential integrity at valid time (STD-CLS-041). Not required of the PH-0 prototype."""
from .common import cls, field, uid, commit, create, base, NOW

KEY = '00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff'
KEY2 = 'ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100'


def uri(klass, slug):
    return 'ubos://acme/%s/%s' % (klass, slug)


def port(name, impl=None, kind='command', params=(), result=None, errors=None):
    p = {'name': name, 'kind': kind, 'permission': name, 'description': name, 'params': list(params)}
    if impl:
        p['impl'] = impl
    if result:
        p['result'] = result
    if errors is not None:
        p['errors'] = errors
    return p


def trait(name, applies_to, fields=(), **extra):
    d = {'name': name, 'namespace': 'acme.finance', 'trait': True, 'applies_to': applies_to,
         'label': name, 'description': name, 'schema_version': 1, 'fields': list(fields)}
    d.update(extra)
    return d


def ref(name, target_class=None, relationship=None, required=False):
    f = field(name, 'reference', required)
    if target_class:
        f['constraints'] = {'target_class': target_class}
    if relationship:
        f['relationship'] = relationship
    return f


def refs(name, target_class=None, relationship=None):
    item = {'type': 'reference'}
    if target_class:
        item['constraints'] = {'target_class': target_class}
    f = field(name, 'list', constraints={'item_type': item})
    if relationship:
        f['relationship'] = relationship
    return f


def ext(target, namespace='acme.crm', order=1, **members):
    e = {'target': target, 'namespace': namespace, 'order': order}
    e.update(members)
    return e


def st(classes, now=NOW):
    return [{'op': 'set_now', 'input': {'now': now}}, {'op': 'load_genesis', 'input': {'classes': classes}}]


def cases(g):
    V, P = g.vector, g.pure

    def pure(clause, desc, op, inp, error=None):
        P(clause, desc, op, inp, error=error, prototype=False)

    def S(clause, desc, classes, steps, now=NOW):
        V(clause, desc, setup=st(classes, now), steps=steps, prototype=False)

    # ---- STD-OBJ-012 blind indexes ----
    pure('STD-OBJ-012', 'the blind index of a text value is HMAC-SHA-256 over its canonical JSON', 'blind_index',
         {'key': KEY, 'type': 'text', 'value': 'US0378331005'})
    V('STD-OBJ-012', 'the value is normalised (NFC) before the index is computed', prototype=False, steps=[
        {'op': 'blind_index', 'input': {'key': KEY, 'type': 'text', 'value': 'José'}, 'label': 'a'},
        {'op': 'blind_index', 'input': {'key': KEY, 'type': 'text', 'value': 'José'}, 'same_as': 'a'}])
    pure('STD-OBJ-012', 'another tenant key gives another index for the same value', 'blind_index',
         {'key': KEY2, 'type': 'text', 'value': 'US0378331005'})
    pure('STD-OBJ-012', 'a key that is not 32 bytes of hexadecimal is rejected', 'blind_index',
         {'key': 'abc', 'type': 'text', 'value': 'x'}, error='MODEL.INVALID_VALUE')
    pure('STD-OBJ-012', 'a value invalid for its type is rejected before indexing', 'blind_index',
         {'key': KEY, 'type': 'integer', 'value': 'twelve'}, error='MODEL.INVALID_VALUE')

    # ---- STD-CLS-020 port dispatch ----
    party = cls('Party', [field('name', 'text', True)], ports=[port('describe', impl='logic/party-describe'),
                                                                 port('archive')])
    cust = cls('Customer', [], parent='Party', ports=[port('describe', impl='logic/customer-describe')])
    vip = cls('VipCustomer', [], parent='Customer')
    audited = trait('Audited', ['definition'], ports=[port('describe', impl='logic/audited-describe')])
    vip_t = cls('VipCustomer', [], parent='Customer', traits=['Audited'])
    base_cls = cls('Party', [field('name', 'text', True)], kind='definition')
    D = [party, cust, vip]

    def disp(desc, classes, name, p, error=None, after=None, extensions=None):
        inp = {'classes': classes, 'class': name, 'port': p}
        if after:
            inp['after'] = after
        if extensions:
            inp['extensions'] = extensions
        pure('STD-CLS-020', desc, 'dispatch', inp, error)

    disp('a class without its own implementation runs the inherited one', D, 'VipCustomer', 'describe')
    disp('an override in the class itself is chosen first', D, 'Customer', 'describe')
    disp('the declaring base class runs its own implementation', D, 'Party', 'describe')
    disp('a trait implementation comes before the parent implementation', [party, cust, audited, vip_t],
         'VipCustomer', 'describe')
    disp('run.super continues after the current class', D, 'VipCustomer', 'describe', after='Customer')
    disp('run.super from the most general implementation finds none', D, 'Customer', 'describe',
         error='MODEL.PORT_NOT_IMPLEMENTED', after='Party')
    disp('a port declared without an implementation anywhere is not implemented', D, 'Customer', 'archive',
         error='MODEL.PORT_NOT_IMPLEMENTED')
    disp('an undeclared port is not implemented', D, 'Customer', 'refund', error='MODEL.PORT_NOT_IMPLEMENTED')
    disp('run.super from a class outside the linearization is rejected', D, 'Customer', 'describe',
         error='MODEL.REFERENCE_INVALID', after='Invoice')
    E1 = [ext('Party', ports=[port('score', impl='logic/crm-score', kind='query')])]
    disp('a port added by an extension dispatches under its namespaced name', D, 'VipCustomer', 'acme.crm.score',
         extensions=E1)
    disp('an extension port is not reachable under its bare name', D, 'VipCustomer', 'score',
         error='MODEL.PORT_NOT_IMPLEMENTED', extensions=E1)

    # ---- STD-CLS-021 port override compatibility ----
    amount = field('amount', 'decimal', True, constraints={'max': '1000'})

    def over(desc, gen, spec, error=None):
        g1 = cls('Party', [field('name', 'text', True)], ports=[gen])
        s1 = cls('Customer', [], parent='Party', ports=[spec])
        pure('STD-CLS-021', desc, 'effective_class', {'classes': [g1, s1], 'class': 'Customer'}, error)

    gen = port('refund', impl='logic/a', params=[amount], result='decimal', errors=['LEDG.PERIOD_CLOSED', 'MODEL.CONSTRAINT_VIOLATED'])
    over('an override with the same signature is accepted', gen, dict(gen, impl='logic/b'))
    over('an override may declare a parameter more generally', gen,
         dict(gen, impl='logic/b', params=[field('amount', 'decimal', False, constraints={'max': '5000'})]))
    over('an override that narrows a parameter is rejected', gen,
         dict(gen, params=[field('amount', 'decimal', True, constraints={'max': '10'})]), 'MODEL.PORT_SIGNATURE_INCOMPATIBLE')
    over('an override with other parameter names is rejected', gen,
         dict(gen, params=[field('value', 'decimal', True)]), 'MODEL.PORT_SIGNATURE_INCOMPATIBLE')
    over('an override that changes a parameter type is rejected', gen,
         dict(gen, params=[field('amount', 'integer', True)]), 'MODEL.PORT_SIGNATURE_INCOMPATIBLE')
    over('an override with another result type is rejected', gen, dict(gen, result='integer'),
         'MODEL.PORT_SIGNATURE_INCOMPATIBLE')
    over('an override may declare fewer error codes', gen, dict(gen, errors=['LEDG.PERIOD_CLOSED']))
    over('an override that adds an error code is rejected', gen,
         dict(gen, errors=['LEDG.PERIOD_CLOSED', 'MODEL.CONSTRAINT_VIOLATED', 'TXN.HEAD_MOVED']),
         'MODEL.PORT_SIGNATURE_INCOMPATIBLE')
    over('an override that changes a command into a query is rejected', gen, dict(gen, kind='query'),
         'MODEL.PORT_SIGNATURE_INCOMPATIBLE')
    g1 = cls('Party', [field('name', 'text', True)], ports=[gen])
    V('STD-CLS-021', 'loading a class with an incompatible override fails', prototype=False, steps=[
        {'op': 'load_genesis', 'input': {'classes': [g1, cls('Customer', [], parent='Party',
                                                            ports=[dict(gen, result='integer')])]},
         'error': 'MODEL.PORT_SIGNATURE_INCOMPATIBLE'}])

    # ---- STD-CLS-022 polymorphic selectors and queries ----
    tagged = trait('Tagged', ['definition'])
    P2 = [base_cls, cls('Customer', [], parent='Party'), cls('VipCustomer', [], parent='Customer', traits=['Tagged']),
          tagged, cls('Supplier', [], parent='Party')]

    def sel(desc, name, selector, only=False, error=None):
        inp = {'classes': P2, 'class': name, 'selector': selector}
        if only:
            inp['only'] = True
        pure('STD-CLS-022', desc, 'type_selector', inp, error)

    sel('a selector on a base class matches a subclass instance', 'VipCustomer', 'Party')
    sel('with only, a selector on a base class does not match a subclass instance', 'VipCustomer', 'Party', only=True)
    sel('with only, a selector on the class itself matches', 'VipCustomer', 'VipCustomer', only=True)
    sel('a selector on a trait matches every class that includes it', 'VipCustomer', 'Tagged')
    sel('a selector on a sibling class does not match', 'Supplier', 'Customer')
    sel('a selector on Object matches everything with specificity 0', 'Supplier', 'Object')
    sel('the specificity of a deeper class is its depth below Object', 'VipCustomer', 'VipCustomer')
    sel('an unknown selector class is rejected', 'Supplier', 'Vendor', error='MODEL.REFERENCE_INVALID')
    body = {'name': 'A'}
    S('STD-CLS-022', 'a class filter returns instances of subclasses', P2, [
        commit([create(uid(1), 'Customer', 'c-1', body), create(uid(2), 'VipCustomer', 'v-1', body),
                create(uid(3), 'Supplier', 's-1', body)]),
        {'op': 'list_objects', 'input': {'branch': 'main', 'class': 'Customer'}}])
    S('STD-CLS-022', 'with only, a class filter returns direct instances only', P2, [
        commit([create(uid(1), 'Customer', 'c-1', body), create(uid(2), 'VipCustomer', 'v-1', body)]),
        {'op': 'list_objects', 'input': {'branch': 'main', 'class': 'Customer', 'only': True}}])
    S('STD-CLS-022', 'a class filter on a trait returns the instances of classes that include it', P2, [
        commit([create(uid(1), 'Customer', 'c-1', body), create(uid(2), 'VipCustomer', 'v-1', body)]),
        {'op': 'list_objects', 'input': {'branch': 'main', 'class': 'Tagged'}}])
    S('STD-CLS-022', 'ended objects are not returned by a class filter', P2, [
        commit([create(uid(1), 'Customer', 'c-1', body, valid={'from': '2026-01-01'}),
                create(uid(2), 'Supplier', 's-1', body)], label='c1'),
        commit([{'op': 'end', 'object_id': uid(1), 'base': base('c1', uid(1)), 'valid': {'from': '2026-02-01'}}]),
        {'op': 'list_objects', 'input': {'branch': 'main', 'class': 'Party'}}])

    # ---- STD-CLS-030 extensions ----
    C3 = [base_cls, cls('Customer', [], parent='Party'), cls('VipCustomer', [], parent='Customer')]
    seg = field('segment', 'text', constraints={'max_length': 20})

    def ee(desc, name, exts, error=None):
        pure('STD-CLS-030', desc, 'effective_class', {'classes': C3, 'class': name, 'extensions': exts}, error)

    ee('an extension adds a field under the key namespace.member', 'Customer', [ext('Customer', fields=[seg])])
    ee('an extension of a base class applies to its subclasses', 'VipCustomer', [ext('Party', fields=[seg])])
    ee('an extension of a subclass does not apply to its base class', 'Party', [ext('Customer', fields=[seg])])
    ee('a later extension refines a member added by an earlier one', 'Customer', [
        ext('Customer', fields=[seg]),
        ext('Customer', namespace='acme.risk', order=2,
            refinements=[{'name': 'acme.crm.segment', 'constraints': {'max_length': 10}}])])
    ee('a refinement ordered before the member it refines is rejected', 'Customer', [
        ext('Customer', order=2, fields=[seg]),
        ext('Customer', namespace='acme.risk', order=1,
            refinements=[{'name': 'acme.crm.segment', 'constraints': {'max_length': 10}}])],
       'MODEL.NARROWING_VIOLATION')
    ee('an extension refinement that widens a member is rejected', 'Customer', [
        ext('Customer', refinements=[{'name': 'name', 'required': False}])], 'MODEL.NARROWING_VIOLATION')
    ee('two extensions adding the same namespaced member are rejected', 'Customer', [
        ext('Customer', fields=[seg]), ext('Customer', order=2, fields=[seg])], 'MODEL.DECLARATION_INCOMPLETE')
    ee('an extension namespace outside the grammar is rejected', 'Customer', [
        ext('Customer', namespace='Acme CRM', fields=[seg])], 'MODEL.INVALID_NAME')
    xbody = ext('Customer', fields=[seg])
    mk = {'op': 'create', 'object_id': uid(90), 'class': 'ClassExtension', 'slug': 'crm-customer', 'body': xbody}
    S('STD-CLS-030', 'an extension committed on a branch changes the effective class there only', C3, [
        {'op': 'create_branch', 'input': {'name': 'feature/crm', 'from': 'main'}},
        commit([mk], branch='feature/crm'),
        {'op': 'class_at', 'input': {'branch': 'feature/crm', 'class': 'Customer'}, 'label': 'x'},
        {'op': 'class_at', 'input': {'branch': 'main', 'class': 'Customer'}}])
    S('STD-CLS-030', 'objects on the branch may use the extension member', C3, [
        {'op': 'create_branch', 'input': {'name': 'feature/crm', 'from': 'main'}},
        commit([mk], branch='feature/crm'),
        commit([create(uid(1), 'Customer', 'c-1', {'name': 'A', 'acme.crm.segment': 'retail'})], branch='feature/crm'),
        {'op': 'read', 'input': {'branch': 'feature/crm', 'object_id': uid(1)}}])
    S('STD-CLS-030', 'the extension member is undeclared on a branch where the extension is not visible', C3, [
        {'op': 'create_branch', 'input': {'name': 'feature/crm', 'from': 'main'}},
        commit([mk], branch='feature/crm'),
        commit([create(uid(1), 'Customer', 'c-1', {'name': 'A', 'acme.crm.segment': 'retail'})],
               error='MODEL.INVALID_VALUE')])
    # ClassExtension is a ConfigAsset class, written on protected branches only by change sets
    # (STD-VER-012, DEC-041), so these cases commit extensions on working branches.
    fb = {'op': 'create_branch', 'input': {'name': 'feature/crm', 'from': 'main'}}
    S('STD-CLS-030', 'a branch created after the extension inherits it', C3, [
        {'op': 'create_branch', 'input': {'name': 'draft/crm', 'from': 'main'}},
        commit([mk], branch='draft/crm'),
        {'op': 'create_branch', 'input': {'name': 'feature/later', 'from': 'draft/crm'}},
        {'op': 'class_at', 'input': {'branch': 'feature/later', 'class': 'VipCustomer'}}])
    S('STD-CLS-030', 'ending the extension object removes its members', C3, [
        fb, commit([mk], branch='feature/crm', label='c1'),
        commit([{'op': 'end', 'object_id': uid(90), 'base': base('c1', uid(90))}], branch='feature/crm'),
        {'op': 'class_at', 'input': {'branch': 'feature/crm', 'class': 'Customer'}}])
    S('STD-CLS-030', 'an extension of an unknown class is rejected', C3, [
        fb, commit([dict(mk, body=ext('Vendor', fields=[seg]))], branch='feature/crm', error='MODEL.REFERENCE_INVALID')])
    S('STD-CLS-030', 'an extension that breaks the narrowing rules is rejected on commit', C3, [
        fb, commit([dict(mk, body=ext('Customer', refinements=[{'name': 'name', 'type': 'integer'}]))],
                   branch='feature/crm', error='MODEL.NARROWING_VIOLATION')])

    # ---- STD-CLS-040 relationships ----
    customer = cls('Customer', [field('name', 'text', True)])
    order_rule = {'target': 'Customer', 'cardinality': 'many_to_one', 'inverse': 'orders'}
    order = cls('SalesOrder', [ref('customer', 'Customer', order_rule, True)])
    account = cls('Account', [field('number', 'text', True)])
    contact = cls('Contact', [ref('account', 'Account', {'target': 'Account', 'cardinality': 'one_to_one',
                                                          'inverse': 'contact'})])
    line = cls('InvoiceLine', [field('text', 'text')])
    invoice = cls('Invoice', [refs('lines', 'InvoiceLine', {'target': 'InvoiceLine', 'cardinality': 'one_to_many',
                                                            'ownership': 'composition', 'inverse': 'invoice'})])
    tag = cls('Tag', [field('label', 'text')])
    item = cls('Item', [refs('tags', 'Tag', {'target': 'Tag', 'cardinality': 'many_to_many', 'on_end': 'detach',
                                             'inverse': 'items'})])
    R = [customer, order, account, contact, line, invoice, tag, item]
    c1 = create(uid(1), 'Customer', 'c-1', {'name': 'Acme'}, valid={'from': '2026-01-01'})
    o1 = create(uid(2), 'SalesOrder', 'o-1', {'customer': uri('Customer', 'c-1')}, valid={'from': '2026-01-01'})
    o2 = create(uid(3), 'SalesOrder', 'o-2', {'customer': uri('Customer', 'c-1')}, valid={'from': '2026-01-01'})

    def decl(desc, f, error):
        pure('STD-CLS-040', desc, 'effective_class', {'classes': [customer, cls('X', [f])], 'class': 'X'}, error)

    decl('a to-many cardinality on a single reference field is rejected',
         ref('c', 'Customer', {'target': 'Customer', 'cardinality': 'one_to_many'}), 'MODEL.INVALID_VALUE')
    decl('a to-one cardinality on a list field is rejected',
         refs('c', 'Customer', {'target': 'Customer', 'cardinality': 'many_to_one'}), 'MODEL.INVALID_VALUE')
    decl('an unknown on_end rule is rejected',
         ref('c', 'Customer', {'target': 'Customer', 'cardinality': 'many_to_one', 'on_end': 'ignore'}), 'MODEL.INVALID_VALUE')
    decl('a relationship without a cardinality is incomplete',
         ref('c', 'Customer', {'target': 'Customer'}), 'MODEL.DECLARATION_INCOMPLETE')
    S('STD-CLS-040', 'the inverse of a relationship lists the referring objects', R, [
        commit([c1, o1, o2]),
        {'op': 'traverse', 'input': {'branch': 'main', 'object_id': uid(1), 'relation': 'orders'}}])
    S('STD-CLS-040', 'a relationship field is traversed forward to its targets', R, [
        commit([c1, o1]),
        {'op': 'traverse', 'input': {'branch': 'main', 'object_id': uid(2), 'relation': 'customer'}}])
    S('STD-CLS-040', 'an unknown relation is rejected', R, [
        commit([c1]),
        {'op': 'traverse', 'input': {'branch': 'main', 'object_id': uid(1), 'relation': 'invoices'},
         'error': 'MODEL.REFERENCE_INVALID'}])
    a1 = create(uid(10), 'Account', 'a-1', {'number': '1000'}, valid={'from': '2026-01-01'})
    S('STD-CLS-040', 'one_to_one: a second object referring to the same target is rejected', R, [
        commit([a1, create(uid(11), 'Contact', 'k-1', {'account': uri('Account', 'a-1')}, valid={'from': '2026-01-01'})]),
        commit([create(uid(12), 'Contact', 'k-2', {'account': uri('Account', 'a-1')}, valid={'from': '2026-01-01'})],
               error='MODEL.CARDINALITY_VIOLATED')])
    l1 = create(uid(20), 'InvoiceLine', 'l-1', {'text': 'x'}, valid={'from': '2026-01-01'})
    inv = create(uid(21), 'Invoice', 'i-1', {'lines': [uri('InvoiceLine', 'l-1')]}, valid={'from': '2026-01-01'})
    S('STD-CLS-040', 'one_to_many: a target owned by two sources is rejected', R, [
        commit([l1, inv]),
        commit([create(uid(22), 'Invoice', 'i-2', {'lines': [uri('InvoiceLine', 'l-1')]}, valid={'from': '2026-01-01'})],
               error='MODEL.CARDINALITY_VIOLATED')])
    S('STD-CLS-040', 'many_to_one: many sources may refer to one target', R, [
        commit([c1, o1, o2]),
        {'op': 'list_objects', 'input': {'branch': 'main', 'class': 'SalesOrder'}}])
    S('STD-CLS-040', 'restrict: ending a referenced target is rejected', R, [
        commit([c1, o1], label='c1'),
        commit([{'op': 'end', 'object_id': uid(1), 'base': base('c1', uid(1)), 'valid': {'from': '2026-03-01'}}],
               error='MODEL.REFERENCE_RESTRICTED')])
    S('STD-CLS-040', 'restrict: the target may end once no live object refers to it', R, [
        commit([c1, o1], label='c1'),
        commit([{'op': 'end', 'object_id': uid(2), 'base': base('c1', uid(2)), 'valid': {'from': '2026-03-01'}},
                {'op': 'end', 'object_id': uid(1), 'base': base('c1', uid(1)), 'valid': {'from': '2026-03-01'}}]),
        {'op': 'list_objects', 'input': {'branch': 'main', 'class': 'Customer'}}])
    casc = cls('SalesOrder', [ref('customer', 'Customer', dict(order_rule, on_end='cascade'), True)])
    S('STD-CLS-040', 'cascade: ending the target ends the referring objects', [customer, casc], [
        commit([c1, o1, o2], label='c1'),
        commit([{'op': 'end', 'object_id': uid(1), 'base': base('c1', uid(1)), 'valid': {'from': '2026-03-01'}}]),
        {'op': 'list_objects', 'input': {'branch': 'main', 'class': 'SalesOrder'}}])
    t1 = create(uid(30), 'Tag', 't-1', {'label': 'red'}, valid={'from': '2026-01-01'})
    t2 = create(uid(31), 'Tag', 't-2', {'label': 'blue'}, valid={'from': '2026-01-01'})
    it = create(uid(32), 'Item', 'it-1', {'tags': [uri('Tag', 't-1'), uri('Tag', 't-2')]}, valid={'from': '2026-01-01'})
    S('STD-CLS-040', 'detach: ending the target removes it from the referring objects from that date', R, [
        commit([t1, t2, it], label='c1'),
        commit([{'op': 'end', 'object_id': uid(30), 'base': base('c1', uid(30)), 'valid': {'from': '2026-03-01'}}]),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(32), 'asof': '2026-03-01'}},
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(32), 'asof': '2026-02-01'}}])
    S('STD-CLS-040', 'composition: ending the owner ends the owned objects', R, [
        commit([l1, create(uid(23), 'InvoiceLine', 'l-2', {'text': 'y'}, valid={'from': '2026-01-01'}),
                create(uid(21), 'Invoice', 'i-1', {'lines': [uri('InvoiceLine', 'l-1'), uri('InvoiceLine', 'l-2')]},
                       valid={'from': '2026-01-01'})], label='c1'),
        commit([{'op': 'end', 'object_id': uid(21), 'base': base('c1', uid(21)), 'valid': {'from': '2026-03-01'}}]),
        {'op': 'list_objects', 'input': {'branch': 'main', 'class': 'InvoiceLine'}}])
    person = cls('Person', [field('name', 'text', True)])
    member = cls('Membership', [ref('source', 'Team'), ref('target', 'Person'), field('role', 'text')])
    team = cls('Team', [refs('members', 'Membership', {'target': 'Person', 'cardinality': 'many_to_many',
                                                       'attributes_class': 'Membership', 'inverse': 'teams'})])
    T = [person, member, team]
    p1 = create(uid(40), 'Person', 'p-1', {'name': 'Ann'}, valid={'from': '2026-01-01'})
    m1 = create(uid(41), 'Membership', 'm-1', {'source': uri('Team', 'team-a'), 'target': uri('Person', 'p-1'),
                                                'role': 'lead'}, valid={'from': '2026-01-01'})
    tm = create(uid(42), 'Team', 'team-a', {'members': [uri('Membership', 'm-1')]}, valid={'from': '2026-01-01'})
    S('STD-CLS-040', 'a relationship with an attributes class is traversed through its link objects', T, [
        commit([p1, m1, tm]),
        {'op': 'traverse', 'input': {'branch': 'main', 'object_id': uid(42), 'relation': 'members'}},
        {'op': 'traverse', 'input': {'branch': 'main', 'object_id': uid(40), 'relation': 'teams'}}])
    S('STD-CLS-040', 'a link object of another class is rejected', T, [
        commit([p1, tm, dict(p1, object_id=uid(43), slug='m-1', body={'name': 'not a link'})],
               error='MODEL.REFERENCE_INVALID')])
    S('STD-CLS-040', 'a link object that does not reference its source is rejected', T, [
        commit([p1, dict(m1, body=dict(m1['body'], source=uri('Team', 'team-b'))),
                create(uid(44), 'Team', 'team-b', {}, valid={'from': '2026-01-01'}), tm],
               error='MODEL.REFERENCE_INVALID')])

    # ---- STD-CLS-041 referential integrity at valid time ----
    plain = cls('Note', [ref('about', 'Customer'), field('text', 'text')])
    vipc = cls('VipCustomer', [], parent='Customer')
    stock = cls('Delivery', [field('item', 'text', True), field('quantity', 'decimal', True), ref('customer', 'Customer')],
                kind='ledger', ledger={'key': ['item'], 'amounts': ['quantity'], 'double_entry': False})
    I = [customer, vipc, plain, stock]

    def note(n, about, frm='2026-01-01', **kw):
        return create(uid(n), 'Note', 'n-%d' % n, {'about': about}, valid=dict({'from': frm}, **kw))

    S('STD-CLS-041', 'a reference to an existing target valid for the whole period is accepted', I, [
        commit([c1]), commit([note(50, uri('Customer', 'c-1'))]),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(50)}}])
    S('STD-CLS-041', 'a reference to an object created in the same commit resolves', I, [
        commit([c1, note(50, uri('Customer', 'c-1'))]),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(50)}}])
    S('STD-CLS-041', 'a reference that resolves to nothing is rejected', I, [
        commit([note(50, uri('Customer', 'missing'))], error='MODEL.REFERENCE_INVALID')])
    S('STD-CLS-041', 'a reference whose target starts after the period starts is rejected', I, [
        commit([create(uid(1), 'Customer', 'c-1', {'name': 'Acme'}, valid={'from': '2026-02-01'})]),
        commit([note(50, uri('Customer', 'c-1'), '2026-01-01')], error='MODEL.REFERENCE_INVALID')])
    S('STD-CLS-041', 'a reference whose target ends inside an open period is rejected', I, [
        commit([c1], label='c1'),
        commit([{'op': 'end', 'object_id': uid(1), 'base': base('c1', uid(1)), 'valid': {'from': '2026-06-01'}}]),
        commit([note(50, uri('Customer', 'c-1'), '2026-03-01')], error='MODEL.REFERENCE_INVALID')])
    S('STD-CLS-041', 'a new version whose open period outlives the target is rejected', I, [
        commit([c1, note(50, uri('Customer', 'c-1'))], label='c1'),
        commit([{'op': 'end', 'object_id': uid(1), 'base': base('c1', uid(1)), 'valid': {'from': '2026-06-01'}}]),
        commit([{'op': 'update', 'object_id': uid(50), 'base': base('c1', uid(50)), 'body': {'text': 'too long'},
                 'valid': {'kind': 'replace_future', 'from': '2026-03-01'}}], error='MODEL.REFERENCE_INVALID')])
    S('STD-CLS-041', 'a target of another class than target_class is rejected', I + [cls('Supplier', [field('name', 'text')])], [
        commit([create(uid(5), 'Supplier', 's-1', {'name': 'S'}, valid={'from': '2026-01-01'})]),
        commit([note(50, uri('Supplier', 's-1'))], error='MODEL.REFERENCE_INVALID')])
    S('STD-CLS-041', 'a target of a subclass of target_class is accepted', I, [
        commit([create(uid(6), 'VipCustomer', 'v-1', {'name': 'V'}, valid={'from': '2026-01-01'})]),
        commit([note(50, uri('VipCustomer', 'v-1'))]),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(50)}}])
    S('STD-CLS-041', 'a reference with another authority does not resolve', I, [
        commit([c1]), commit([note(50, 'ubos://globex/Customer/c-1')], error='MODEL.REFERENCE_INVALID')])
    S('STD-CLS-041', 'a reference value outside the URI grammar is invalid', I, [
        commit([c1]), commit([note(50, 'Customer/c-1')], error='MODEL.INVALID_VALUE')])
    dl = {'op': 'append', 'class': 'Delivery', 'entry_id': uid(60), 'ledger_key': {'item': 'bolt'},
          'body': {'item': 'bolt', 'quantity': '5', 'customer': uri('Customer', 'c-1')}}
    S('STD-CLS-041', 'a ledger entry reference resolves at the entry valid time', I, [
        commit([c1]), commit([dict(dl, valid_at='2026-03-15')]),
        {'op': 'read_ledger', 'input': {'branch': 'main', 'class': 'Delivery', 'ledger_key': {'item': 'bolt'}}}])
    S('STD-CLS-041', 'a ledger entry dated before the target is valid is rejected', I, [
        commit([create(uid(1), 'Customer', 'c-1', {'name': 'Acme'}, valid={'from': '2026-03-01'})]),
        commit([dict(dl, valid_at='2026-02-15')], error='MODEL.REFERENCE_INVALID')])
    S('STD-CLS-041', 'a later ending of the target does not invalidate committed versions', I, [
        commit([c1], label='c1'), commit([note(50, uri('Customer', 'c-1'))]),
        commit([{'op': 'end', 'object_id': uid(1), 'base': base('c1', uid(1)), 'valid': {'from': '2026-06-01'}}]),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(50)}}])
