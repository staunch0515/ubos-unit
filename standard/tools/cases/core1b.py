"""Vector cases of BPA 1.0-rc2 for the Core clauses that serve gate 1b (PH-1 step 10c,
DEC-067): port declarations (STD-CLS-003), overlay policy enforcement (STD-CLS-031) and the
textual form of criteria queries (STD-EXPR-010). Not required of the PH-0 prototype."""
from .common import cls, field, uid, commit, create, NOW
from .model import port, ext

BASE = cls('Party', [field('name', 'text', True), field('country', 'text'),
                     field('rating', 'integer', constraints={'min': 0, 'max': 10}),
                     field('memo', 'text')],
           overlay_policy={'country': 'locked', 'rating': 'extend_only', 'memo': 'overridable', 'views': 'locked'})
CUSTOMER = cls('Customer', [field('email', 'text'), field('balance', 'decimal')], parent='Party')
VIP = cls('VipCustomer', [field('tier', 'text')], parent='Customer')
SUPPLIER = cls('Supplier', [field('code', 'text')], parent='Party')
CLASSES = [BASE, CUSTOMER, VIP, SUPPLIER]


def with_ports(*ports):
    return [cls('Account', [field('name', 'text', True)], ports=list(ports))]


def cases(g):
    V = g.vector

    def P(clause, desc, op, inp, error=None):
        g.pure(clause, desc, op, inp, error=error, prototype=False)

    def S(clause, desc, steps, classes=None, now_=NOW, also=()):
        V(clause, desc, setup=[{'op': 'set_now', 'input': {'now': now_}},
                               {'op': 'load_genesis', 'input': {'classes': classes or CLASSES}}],
          steps=steps, prototype=False, also=also)

    ports(P)
    overlay(P, S)
    queries(P, S)


def ports(P):
    def ok(desc, p):
        P('STD-CLS-003', desc, 'effective_class', {'classes': with_ports(p), 'class': 'Account'})

    def bad(desc, p, error='MODEL.INVALID_VALUE'):
        P('STD-CLS-003', desc, 'effective_class', {'classes': with_ports(p), 'class': 'Account'}, error=error)

    full = dict(port('close', 'acme.acct#close', params=[field('reason', 'text', True)], result='text',
                     errors=['FLOW.TRANSITION_NOT_ALLOWED']),
                states=['open'], transition='close', guards=['acme.guards#can_close'], idempotent=False,
                ui={'label': 'Close', 'icon': 'lock', 'confirmation': 'Close the account?'})
    ok('a complete command port declaration', full)
    ok('a minimal port declaration has name, kind, permission and description', port('ping'))
    ok('a query port', port('balance', kind='query', result='decimal'))
    ok('parameters are field declarations', port('rename', params=[field('name', 'text', True), field('note', 'text')]))
    bad('a port needs a name', {'kind': 'command', 'permission': 'x', 'description': 'x'}, 'MODEL.DECLARATION_INCOMPLETE')
    bad('a port needs a permission', {'name': 'x', 'kind': 'command', 'description': 'x'}, 'MODEL.DECLARATION_INCOMPLETE')
    bad('a port needs a description', {'name': 'x', 'kind': 'command', 'permission': 'x'}, 'MODEL.DECLARATION_INCOMPLETE')
    bad('a port name is snake_case', port('CloseAccount'), 'MODEL.INVALID_NAME')
    bad('the kind is query or command', dict(port('x'), kind='event'))
    bad('a parameter is a field declaration', dict(port('x'), params=[{'name': 'n'}]), 'MODEL.DECLARATION_INCOMPLETE')
    bad('the result is a type name', dict(port('x'), result=''))
    bad('errors are registered codes', dict(port('x'), errors=['oops']))
    bad('states are state names', dict(port('x'), states=['Open']), 'MODEL.INVALID_NAME')
    bad('a transition is a transition name', dict(port('x'), transition='Close!'), 'MODEL.INVALID_NAME')
    bad('only a command port triggers a transition', dict(port('x', kind='query'), transition='close'))
    bad('the permission is a policy action name', dict(port('x'), permission=''))
    bad('idempotent is boolean', dict(port('x'), idempotent='yes'))
    bad('a query port cannot be declared non-idempotent', dict(port('x', kind='query'), idempotent=False))
    bad('an unknown port member is rejected', dict(port('x'), colour='red'))
    P('STD-CLS-003', 'two ports of one class need different names', 'effective_class',
      {'classes': with_ports(port('a'), port('a')), 'class': 'Account'}, error='MODEL.DECLARATION_INCOMPLETE')


def overlay(P, S):
    def E(desc, extension, error=None, klass='Customer'):
        P('STD-CLS-031', desc, 'effective_class', {'classes': CLASSES, 'class': klass, 'extensions': [extension]},
          error=error)

    E('an overridable member accepts a refinement', ext('Party', refinements=[{'name': 'memo', 'label': 'note'}]))
    E('a locked member cannot be refined by an extension', ext('Party', refinements=[{'name': 'country', 'default': 'CA'}]),
      'TEN.OVERRIDE_LOCKED')
    E('an extend_only member accepts a narrowing of its constraints',
      ext('Party', refinements=[{'name': 'rating', 'constraints': {'max': 5}}]))
    E('an extend_only member accepts additional validations',
      ext('Party', refinements=[{'name': 'rating', 'validations': [{'name': 'even', 'rule': 'this.rating % 2 == 0',
                                                                      'severity': 'warning', 'message': 'even'}]}]))
    E('an extend_only member cannot change its default', ext('Party', refinements=[{'name': 'rating', 'default': 1}]),
      'TEN.OVERRIDE_LOCKED')
    E('an extend_only member cannot be made optional or required', ext('Party', refinements=[{'name': 'rating', 'required': True}]),
      'TEN.OVERRIDE_LOCKED')
    E('a locked views member rejects overriding views', ext('Party', views=[{'mode': 'edit', 'view': 'ubos://acme/View/x'}]),
      'TEN.OVERRIDE_LOCKED')
    P('STD-CLS-031', 'an overlay policy value is one of the three', 'effective_class',
      {'classes': [cls('Party', [field('name', 'text', True)], overlay_policy={'name': 'frozen'})], 'class': 'Party'},
      error='MODEL.INVALID_VALUE')
    S('STD-CLS-031', 'an extension object that overrides a locked member is rejected at commit', [
        {'op': 'create_branch', 'input': {'name': 'feature/crm', 'from': 'main'}},
        commit([create(uid(90), 'ClassExtension', 'crm', {'target': 'Party', 'namespace': 'acme.crm', 'order': 1,
                                                          'refinements': [{'name': 'country', 'default': 'CA'}]})],
               branch='feature/crm', error='TEN.OVERRIDE_LOCKED')])
    S('STD-CLS-031', 'an extension object with an additive refinement of an extend_only member commits', [
        {'op': 'create_branch', 'input': {'name': 'feature/crm', 'from': 'main'}},
        commit([create(uid(90), 'ClassExtension', 'crm', {'target': 'Party', 'namespace': 'acme.crm', 'order': 1,
                                                          'refinements': [{'name': 'rating', 'constraints': {'max': 5}}]})],
               branch='feature/crm'),
        {'op': 'class_at', 'input': {'branch': 'feature/crm', 'class': 'Customer'}}])


def queries(P, S):
    def ok(desc, text):
        P('STD-EXPR-010', desc, 'parse_query', {'text': text})

    def bad(desc, text):
        P('STD-EXPR-010', desc, 'parse_query', {'text': text}, error='QRY.INVALID_QUERY')

    ok('the textual form names the class', 'from Customer')
    ok('only excludes subclasses', 'from Customer only')
    ok('where carries an L1 boolean expression over this', 'from Customer where this.balance > 100 and this.country == "US"')
    ok('order by takes paths with asc or desc, ascending by default', 'from Customer order by this.balance desc, this.name')
    ok('limit sets the page size', 'from Customer limit 10')
    ok('every clause together', 'from Customer only where this.balance > 0 order by this.name asc limit 5')
    ok('whitespace is free', '  from   Customer\n  where this.balance > 0\n  limit 1 ')
    bad('a query starts with from', 'select * from Customer')
    bad('a class name is capitalised', 'from customer')
    bad('where needs an expression', 'from Customer where')
    bad('order by takes asc or desc only', 'from Customer order by this.name up')
    bad('a limit is a number at the end', 'from Customer limit ten')
    bad('text between the clauses is rejected', 'from Customer having this.x')
    c1 = create(uid(1), 'Customer', 'c-1', {'name': 'Acme', 'balance': '150', 'country': 'US'})
    c2 = create(uid(2), 'Customer', 'c-2', {'name': 'Beta', 'balance': '20', 'country': 'CA'})
    v1 = create(uid(3), 'VipCustomer', 'v-1', {'name': 'Gamma', 'balance': '900', 'country': 'US', 'tier': 'gold'})
    S('STD-EXPR-010', 'the textual and the JSON form evaluate identically', [
        commit([c1, c2, v1]),
        {'op': 'query', 'label': 'j', 'input': {'branch': 'main', 'from': 'Customer', 'where': 'this.balance > 100',
                                                'order_by': [{'path': 'this.balance', 'dir': 'desc'}]}},
        {'op': 'query', 'input': {'branch': 'main', 'text': 'from Customer where this.balance > 100 order by this.balance desc'},
         'same_as': 'j'}])
    S('STD-EXPR-010', 'only in the textual form excludes subclasses as in the JSON form', [
        commit([c1, c2, v1]),
        {'op': 'query', 'label': 'j', 'input': {'branch': 'main', 'from': 'Customer', 'only': True}},
        {'op': 'query', 'input': {'branch': 'main', 'text': 'from Customer only'}, 'same_as': 'j'}])
    S('STD-EXPR-010', 'a limit in the textual form pages like the JSON form', [
        commit([c1, c2, v1]),
        {'op': 'query', 'label': 'j', 'input': {'branch': 'main', 'from': 'Customer', 'limit': 2,
                                                'order_by': [{'path': 'this.name', 'dir': 'asc'}]}},
        {'op': 'query', 'input': {'branch': 'main', 'text': 'from Customer order by this.name limit 2'}, 'same_as': 'j'}])
    S('STD-EXPR-010', 'a textual query with a type error in where fails like the JSON form', [
        commit([c1]), {'op': 'query', 'input': {'branch': 'main', 'text': 'from Customer where this.name + 1'},
                       'error': 'LOGIC.TYPE_ERROR'}])
