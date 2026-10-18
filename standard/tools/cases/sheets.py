"""Vector cases of BPA 1.0-rc2 for governance sheets (PH-1 step 10b, DEC-066): the textual
grammar and its AST (STD-LGS-001), sheet objects (STD-LGS-002), the property registry and the
kernel properties (STD-LGS-010, -011), matching (STD-LGS-020), specificity (STD-LGS-021), the
cascade (STD-LGS-022) and its explanation (STD-LGS-023). Not required of the PH-0 prototype."""
from .common import cls, field, uid, create, base, NOW
from .common import commit as _commit

# GovernanceSheet and PropertyDecl are ConfigAsset classes, written on protected branches only
# by change sets (STD-VER-012, DEC-041): the stateful cases work on a feature branch.
BR = 'feature/rules'
FB = {'op': 'create_branch', 'input': {'name': BR, 'from': 'main'}}


def commit(changes, branch=BR, **kw):
    return _commit(changes, branch=branch, **kw)

ACCOUNT = cls('Account', [field('name', 'text', True), field('kind', 'text')])
PARTY = cls('Party', [field('name', 'text', True), field('country', 'text', default='US'),
                      field('tags', 'list', constraints={'item_type': {'type': 'text'}}),
                      field('state', 'enum', constraints={'allowed_values': ['draft', 'active', 'closed']})])
CUSTOMER = cls('Customer', [field('email', 'text'), field('credit_limit', 'money'),
                            field('account', 'reference', constraints={'target_class': 'Account'})], parent='Party')
TAGGED = {'name': 'Tagged', 'namespace': 'acme.finance', 'trait': True, 'applies_to': ['definition'],
          'label': 'Tagged', 'description': 'Tagged', 'schema_version': 1, 'fields': []}
VIP = cls('VipCustomer', [field('manager', 'text')], parent='Customer', traits=['Tagged'])
SUPPLIER = cls('Supplier', [field('rating', 'integer')], parent='Party')
DOC = cls('ScannedForm', [field('source', 'text', True)], kind='raw_document')
CLASSES = [ACCOUNT, PARTY, CUSTOMER, VIP, SUPPLIER, DOC, TAGGED]

SHEET = uid(900)


def sheet(oid, slug, source, layer='tenant', priority=None, scope=None, name=None, ast=None, **extra):
    body = {'name': name or 'acme.%s' % slug.replace('-', '_'), 'layer': layer, 'source': source,
            'description': slug}
    if priority is not None:
        body['priority'] = priority
    if scope is not None:
        body['scope'] = scope
    if ast is not None:
        body['ast'] = ast
    return create(oid, 'GovernanceSheet', slug, body, **extra)


def prop(oid, name, schema, enforcement='commit', combine='override', **extra):
    body = {'name': name, 'schema': schema, 'enforcement': enforcement, 'combine': combine,
            'owner': name.rsplit('.', 1)[0], 'description': name}
    body.update(extra)
    return create(oid, 'PropertyDecl', name.replace('.', '-'), body)


def op(o, label=None, error=None, **inp):
    s = {'op': o, 'input': inp}
    if label:
        s['label'] = label
    if error:
        s['error'] = error
    return s


def cascade(oid, label=None, error=None, branch=BR, ctx=None, **kw):
    inp = {'branch': branch, 'object_id': oid}
    if ctx is not None:
        inp['ctx'] = ctx
    inp.update(kw)
    return op('cascade', label, error, **inp)


def explain(oid, prop_, label=None, error=None, branch=BR, ctx=None, **kw):
    inp = {'branch': branch, 'object_id': oid, 'property': prop_}
    if ctx is not None:
        inp['ctx'] = ctx
    inp.update(kw)
    return op('explain_cascade', label, error, **inp)


def cases(g):
    V = g.vector

    def P(clause, desc, op_, inp, error=None):
        V(clause, desc, [{'op': op_, 'input': inp, 'error': error}], prototype=False)

    def S(clause, desc, steps, classes=None, now_=NOW, also=()):
        V(clause, desc, setup=[{'op': 'set_now', 'input': {'now': now_}},
                               {'op': 'load_genesis', 'input': {'classes': classes or CLASSES}}, FB],
          steps=steps, prototype=False, also=also)

    grammar(P)
    objects(S, P)
    registry(S, P)
    kernel_properties(S, P)
    matching(S)
    specificity(P)
    cascade_(S)
    explanation(S)


def grammar(P):
    def ok(desc, source):
        P('STD-LGS-001', desc, 'parse_sheet', {'source': source})

    def bad(desc, source):
        P('STD-LGS-001', desc, 'parse_sheet', {'source': source}, error='RULE.PARSE_ERROR')

    ok('a block with a type selector and one declaration', 'Customer { audit_level: "strict"; }')
    ok('the universal selector', '* { read_only: true; }')
    ok('an attribute qualifier holds an L1 boolean expression over this',
       'Customer[this.credit_limit > 10000USD] { audit_level: "strict"; }')
    ok('a state qualifier', 'Customer:active { read_only: false; }')
    ok('a tag qualifier', 'Customer:tag(vip) { approvals: 2; }')
    ok('a context qualifier compares a context key with a text',
       'Customer:ctx(branch_kind = "sandbox") { read_only: true; }')
    ok('a context qualifier with in takes a list of texts',
       'Customer:ctx(role in ["auditor", "cfo"]) { mask.email: "partial"; }')
    ok('a context qualifier with a negation', 'Customer:ctx(channel != "api") { require_signature: true; }')
    ok('a relation qualifier evaluates one reference hop',
       'Customer:rel(account)[this.kind == "escrow"] { approvals: 2; }')
    ok('several qualifiers combine on one selector',
       'Customer[this.country == "US"]:active:tag(vip):ctx(vae = "sales") { audit_level: "strict"; }')
    ok('a selector list applies the block to every selector', 'Customer, Supplier { audit_level: "standard"; }')
    ok('several declarations in one block',
       'Customer { audit_level: "strict"; field_required: ["email"]; read_only: false; }')
    ok('a locked declaration', 'Customer { field_required: ["email"] !locked; }')
    ok('a dotted property path names one key of a map property', 'Customer { field_visibility.email: "hidden"; }')
    ok('a map literal value', 'Customer { approvals: {count: 2, roles: ["cfo"], order: "sequence"}; }')
    ok('a list literal value', 'Customer { field_required: ["email", "country"]; }')
    ok('money, date, decimal, integer, boolean and null literals',
       'Customer { field_default: {credit_limit: 500USD, since: 2026-01-01, rate: 0.5, n: 3, ok: true, x: null}; }')
    ok('whitespace and comments are not part of the canonical form',
       '// audit rules\nCustomer   {\n   audit_level :   "strict" ;   // strict\n}\n')
    ok('several blocks', 'Customer { audit_level: "strict"; }\nSupplier { audit_level: "basic"; }')
    ok('escapes in strings', 'Customer { field_default: {memo: "a \\"quoted\\" line\\n"}; }')
    bad('a block without declarations is rejected', 'Customer { }')
    bad('a declaration without its terminating semicolon is rejected', 'Customer { audit_level: "strict" }')
    bad('an unknown context key is rejected', 'Customer:ctx(weather = "rain") { read_only: true; }')
    bad('a context condition with in needs a list of texts', 'Customer:ctx(role in "cfo") { read_only: true; }')
    bad('a lowercase type selector is rejected', 'customer { read_only: true; }')
    bad('an attribute qualifier must be a well-formed expression', 'Customer[this.name ==] { read_only: true; }')
    bad('an unterminated block is rejected', 'Customer { read_only: true;')
    bad('a property path needs a value', 'Customer { read_only; }')
    bad('the locked marker comes after the value', 'Customer { !locked read_only: true; }')
    bad('a map literal with a duplicate member is rejected', 'Customer { approvals: {count: 1, count: 2}; }')
    bad('a sheet is text', None)


def objects(S, P):
    src = 'Customer[this.country == "US"] {\n  audit_level: "strict";\n  field_required: ["email"] !locked;\n}\n'
    ast = {'blocks': [{'selectors': [{'type': 'Customer', 'qualifiers': [{'kind': 'attr', 'expr': 'this.country == "US"'}]}],
                       'declarations': [{'property': 'audit_level', 'value': 'strict'},
                                        {'property': 'field_required', 'value': ['email'], 'locked': True}]}]}
    S('STD-LGS-002', 'a sheet is an object of class GovernanceSheet whose ast is derived from its source', [
        commit([sheet(SHEET, 'audit', src)], label='c'),
        op('read', branch=BR, object_id=SHEET)])
    S('STD-LGS-002', 'source is stored in its canonical form', [
        commit([sheet(SHEET, 'audit', '// strict\nCustomer [ this.country == "US" ]  { audit_level:"strict";'
                      ' field_required: ["email"] !locked; }')]),
        op('read', branch=BR, object_id=SHEET)])
    S('STD-LGS-002', 'a given ast must agree with the source', [
        commit([sheet(SHEET, 'audit', src, ast={'blocks': []})], error='MODEL.INVALID_VALUE')])
    S('STD-LGS-002', 'a given ast that agrees with the source is accepted', [
        commit([sheet(SHEET, 'audit', src, ast=ast)]), op('read', branch=BR, object_id=SHEET)])
    S('STD-LGS-002', 'a sheet with a syntax error is rejected at commit', [
        commit([sheet(SHEET, 'audit', 'Customer { audit_level: "strict" }')], error='RULE.PARSE_ERROR')])
    S('STD-LGS-002', 'the layer is required and enumerated', [
        commit([sheet(SHEET, 'audit', src, layer='galaxy')], error='MODEL.CONSTRAINT_VIOLATED')])
    S('STD-LGS-002', 'an update of the source derives the ast again', [
        commit([sheet(SHEET, 'audit', src)], label='c'),
        commit([{'op': 'update', 'object_id': SHEET, 'base': base('c', SHEET),
                 'body': {'source': 'Customer { audit_level: "basic"; }'}}]),
        op('read', branch=BR, object_id=SHEET)])
    S('STD-LGS-002', 'the ast given by parse_sheet round-trips through print and parse', [
        op('parse_sheet', 'p', source=src),
        {'op': 'parse_sheet', 'input': {'source': {'$ref': 'p/source'}}, 'same_as': 'p'}])


def registry(S, P):
    P('STD-LGS-010', 'a declaration of an unknown property is rejected', 'check_sheet',
      {'source': 'Customer { audit_depth: 3; }', 'classes': CLASSES}, error='RULE.UNKNOWN_PROPERTY')
    P('STD-LGS-010', 'a kernel property is known without any declaration', 'check_sheet',
      {'source': 'Customer { audit_level: "strict"; }', 'classes': CLASSES})
    P('STD-LGS-010', 'a Buk property given as a DAT-PropertyDecl is known', 'check_sheet',
      {'source': 'Customer { acme.crm.tier: "gold"; }', 'classes': CLASSES,
       'properties': [{'name': 'acme.crm.tier', 'schema': {'type': 'text'}, 'enforcement': 'commit',
                       'combine': 'override', 'owner': 'acme.crm', 'description': 'tier'}]})
    P('STD-LGS-010', 'a Buk property value is validated against its schema', 'check_sheet',
      {'source': 'Customer { acme.crm.tier: 3; }', 'classes': CLASSES,
       'properties': [{'name': 'acme.crm.tier', 'schema': {'type': 'text'}, 'enforcement': 'commit',
                       'combine': 'override', 'owner': 'acme.crm', 'description': 'tier'}]},
      error='MODEL.INVALID_VALUE')
    P('STD-LGS-010', 'a Buk property must be namespaced', 'check_sheet',
      {'source': 'Customer { tier: "gold"; }', 'classes': CLASSES,
       'properties': [{'name': 'tier', 'schema': {'type': 'text'}, 'enforcement': 'commit',
                       'combine': 'override', 'owner': 'acme.crm', 'description': 'tier'}]},
      error='MODEL.INVALID_VALUE')
    P('STD-LGS-010', 'a Buk property cannot shadow a kernel property', 'check_sheet',
      {'source': 'Customer { audit_level: "strict"; }', 'classes': CLASSES,
       'properties': [{'name': 'audit_level', 'schema': {'type': 'text'}, 'enforcement': 'commit',
                       'combine': 'override', 'owner': 'acme.crm', 'description': 'x'}]},
      error='MODEL.INVALID_VALUE')
    P('STD-LGS-010', 'the enforcement point and the combination rule are enumerated', 'check_sheet',
      {'source': 'Customer { acme.crm.tier: "gold"; }', 'classes': CLASSES,
       'properties': [{'name': 'acme.crm.tier', 'schema': {'type': 'text'}, 'enforcement': 'sometime',
                       'combine': 'override', 'owner': 'acme.crm', 'description': 'tier'}]},
      error='MODEL.INVALID_VALUE')
    S('STD-LGS-010', 'a PropertyDecl object registers a Buk property for sheets committed after it', [
        commit([prop(uid(901), 'acme.crm.tier', {'type': 'text'})]),
        commit([sheet(SHEET, 'tier', 'Customer { acme.crm.tier: "gold"; }')]),
        commit([create(uid(1), 'Customer', 'c-1', {'name': 'Acme'})]),
        cascade(uid(1))])
    S('STD-LGS-010', 'a sheet declaring an unregistered property is rejected at commit', [
        commit([sheet(SHEET, 'tier', 'Customer { acme.crm.tier: "gold"; }')], error='RULE.UNKNOWN_PROPERTY')])
    S('STD-LGS-010', 'a Buk property combines by its declared rule', [
        commit([prop(uid(901), 'acme.crm.limit', {'type': 'integer'}, combine='max')]),
        commit([sheet(SHEET, 'a', 'Party { acme.crm.limit: 3; }'),
                sheet(uid(902), 'b', 'Customer { acme.crm.limit: 5; }', layer='platform')]),
        commit([create(uid(1), 'Customer', 'c-1', {'name': 'Acme'})]),
        cascade(uid(1))])


def kernel_properties(S, P):
    def ok(desc, decl):
        P('STD-LGS-011', desc, 'check_sheet', {'source': 'Customer { %s }' % decl, 'classes': CLASSES})

    def bad(desc, decl, error='MODEL.INVALID_VALUE'):
        P('STD-LGS-011', desc, 'check_sheet', {'source': 'Customer { %s }' % decl, 'classes': CLASSES}, error=error)

    ok('audit_level is one of basic, standard, strict', 'audit_level: "strict";')
    bad('audit_level rejects other values', 'audit_level: "loose";', 'MODEL.CONSTRAINT_VIOLATED')
    ok('require_signature is boolean', 'require_signature: true;')
    bad('require_signature rejects a text', 'require_signature: "yes";')
    ok('approvals as an integer count', 'approvals: 2;')
    ok('approvals as a composite with roles and order', 'approvals: {count: 2, roles: ["cfo", "ceo"], order: "sequence"};')
    bad('approvals needs at least one approver', 'approvals: 0;', 'MODEL.CONSTRAINT_VIOLATED')
    bad('approvals rejects an unknown member', 'approvals: {count: 1, who: "x"};')
    ok('maker_checker is boolean', 'maker_checker: true;')
    ok('read_only is boolean', 'read_only: true;')
    ok('field_required is a list of field names', 'field_required: ["email", "country"];')
    bad('field_required rejects a single text', 'field_required: "email";')
    ok('field_default maps fields to values', 'field_default: {country: "CA", credit_limit: 1000USD};')
    ok('field_visibility maps fields to visible, hidden or read_only', 'field_visibility: {email: "hidden", country: "read_only"};')
    bad('field_visibility rejects other flags', 'field_visibility.email: "invisible";', 'MODEL.CONSTRAINT_VIOLATED')
    ok('mask maps fields to hide, partial or hash', 'mask.email: "partial";')
    ok('retention is a composite of min, max and trigger', 'retention: {min: "P7Y", trigger: "closed"};')
    bad('retention durations are ISO 8601 durations', 'retention: {min: "7 years"};')
    ok('classification maps fields to levels', 'classification: {email: "personal"};')
    bad('classification rejects an unknown level', 'classification.email: "top_secret";', 'MODEL.CONSTRAINT_VIOLATED')
    ok('notify is a list of notification rules', 'notify: [{event: "created", to: "sales"}];')
    ok('write_policy is direct or change_set', 'write_policy: "change_set";')
    ok('rate_limit is a composite of per and count', 'rate_limit: {per: "minute", count: 10};')
    bad('rate_limit needs both members', 'rate_limit: {per: "minute"};', 'MODEL.CONSTRAINT_VIOLATED')
    bad('a map key of a non-map property is rejected', 'audit_level.x: "strict";')
    bad('a map key path has one segment', 'mask.a.b: "hide";', 'RULE.UNKNOWN_PROPERTY')
    # combination rules (DEC-066 item 8)
    c1 = create(uid(1), 'Customer', 'c-1', {'name': 'Acme'})
    S('STD-LGS-011', 'field_required combines by union over equal ranks', [
        commit([c1]), commit([sheet(SHEET, 'a', 'Customer { field_required: ["email"]; }'),
                sheet(uid(902), 'b', 'Customer { field_required: ["country"]; }')]), cascade(uid(1))])
    S('STD-LGS-011', 'approvals combines by the maximum count', [
        commit([c1]), commit([sheet(SHEET, 'a', 'Customer { approvals: 1; }'),
                sheet(uid(902), 'b', 'Customer { approvals: {count: 3, roles: ["cfo"]}; }')]), cascade(uid(1))])
    S('STD-LGS-011', 'classification only rises: the higher level wins over a more specific lower one', [
        commit([c1]), commit([sheet(SHEET, 'a', 'Party { classification.email: "personal"; }'),
                sheet(uid(902), 'b', 'Customer { classification.email: "internal"; }')]), cascade(uid(1))])
    S('STD-LGS-011', 'write_policy only rises: change_set wins over direct', [
        commit([c1]), commit([sheet(SHEET, 'a', 'Party { write_policy: "change_set"; }', layer='platform'),
                sheet(uid(902), 'b', 'Customer { write_policy: "direct"; }')]), cascade(uid(1))])
    S('STD-LGS-011', 'notify combines by union', [
        commit([c1]), commit([sheet(SHEET, 'a', 'Customer { notify: [{to: "sales"}]; }'),
                sheet(uid(902), 'b', 'Customer { notify: [{to: "audit"}, {to: "sales"}]; }')]), cascade(uid(1))])
    S('STD-LGS-011', 'override properties take the winning declaration only', [
        commit([c1]), commit([sheet(SHEET, 'a', 'Party { audit_level: "basic"; mask.email: "hide"; }'),
                sheet(uid(902), 'b', 'Customer { audit_level: "strict"; mask.email: "partial"; }')]), cascade(uid(1))])
    S('STD-LGS-011', 'a whole-map declaration and a keyed declaration cascade per key', [
        commit([c1]), commit([sheet(SHEET, 'a', 'Party { field_visibility: {email: "hidden", country: "read_only"}; }'),
                sheet(uid(902), 'b', 'Customer { field_visibility.email: "visible"; }')]), cascade(uid(1))])


def matching(S):
    acc = create(uid(10), 'Account', 'escrow-1', {'name': 'Escrow', 'kind': 'escrow'})
    c1 = create(uid(1), 'Customer', 'c-1', {'name': 'Acme', 'country': 'US', 'state': 'active', 'tags': ['vip'],
                                            'credit_limit': {'amount': '20000.00', 'currency': 'USD'},
                                            'account': 'ubos://acme/Account/escrow-1'})
    c2 = create(uid(2), 'Customer', 'c-2', {'name': 'Beta', 'country': 'CA', 'state': 'draft',
                                            'credit_limit': {'amount': '500.00', 'currency': 'USD'}})
    s1 = create(uid(3), 'Supplier', 's-1', {'name': 'Gamma'})

    def M(desc, source, oid, ctx=None, steps=(), **kw):
        S('STD-LGS-020', desc, [commit([acc, c1, c2, s1]), commit([sheet(SHEET, 'm', source, **kw)])]
          + list(steps) + [cascade(oid, ctx=ctx)])

    M('a type selector matches the class itself', 'Customer { audit_level: "strict"; }', uid(1))
    M('a type selector matches a subclass instance through the linearization', 'Party { audit_level: "strict"; }', uid(1))
    M('a type selector does not match a sibling class', 'Customer { audit_level: "strict"; }', uid(3))
    M('the universal selector matches every object', '* { audit_level: "strict"; }', uid(3))
    M('an attribute qualifier that holds matches', 'Customer[this.credit_limit > 10000USD] { approvals: 2; }', uid(1))
    M('an attribute qualifier that fails does not match', 'Customer[this.credit_limit > 10000USD] { approvals: 2; }', uid(2))
    M('an attribute qualifier over a null member does not match', 'Customer[this.email == "x"] { approvals: 2; }', uid(2))
    M('a state qualifier matches the state member', 'Customer:active { read_only: true; }', uid(1))
    M('a state qualifier does not match another state', 'Customer:active { read_only: true; }', uid(2))
    M('a tag qualifier matches a tag of the object', 'Customer:tag(vip) { approvals: 2; }', uid(1))
    M('a tag qualifier does not match an object without the tag', 'Customer:tag(vip) { approvals: 2; }', uid(2))
    M('a context qualifier matches the context given', 'Customer:ctx(role = "auditor") { mask.email: "partial"; }',
      uid(1), ctx={'role': 'auditor'})
    M('a context qualifier does not match another context', 'Customer:ctx(role = "auditor") { mask.email: "partial"; }',
      uid(1), ctx={'role': 'clerk'})
    M('a context qualifier with in matches any listed value', 'Customer:ctx(role in ["auditor", "cfo"]) { mask.email: "partial"; }',
      uid(1), ctx={'role': 'cfo'})
    M('branch_kind is taken from the branch when the context does not give it',
      'Customer:ctx(branch_kind = "feature") { audit_level: "strict"; }', uid(1))
    M('a relation qualifier matches through the referenced object', 'Customer:rel(account)[this.kind == "escrow"] { approvals: 2; }', uid(1))
    M('a relation qualifier over an empty reference does not match', 'Customer:rel(account)[this.kind == "escrow"] { approvals: 2; }', uid(2))
    M('a sheet scoped to other classes does not apply', 'Party { audit_level: "strict"; }', uid(1),
      scope={'classes': ['Supplier']})
    M('a sheet scoped to a class in the linearization applies', 'Party { audit_level: "strict"; }', uid(1),
      scope={'classes': ['Party']})
    M('a sheet scoped to a VAE applies in that VAE only', 'Customer { audit_level: "strict"; }', uid(1),
      ctx={'vae': 'ops'}, scope={'vaes': ['sales']})
    M('a sheet scoped to branch kinds applies on those branches only', 'Customer { audit_level: "strict"; }', uid(1),
      scope={'branch_kinds': ['feature']})
    S('STD-LGS-020', 'a sheet applies only within its valid-time period', [
        commit([acc, c1]),
        commit([sheet(SHEET, 'm', 'Customer { audit_level: "strict"; }', valid={'from': '2026-04-01'})]),
        cascade(uid(1), 'before', asof='2026-03-20'), cascade(uid(1), 'after', asof='2026-04-02')])
    S('STD-LGS-020', 'a sheet applies from the commit that attached it', [
        commit([acc, c1], label='c0'),
        commit([sheet(SHEET, 'm', 'Customer { audit_level: "strict"; }')], label='c1'),
        cascade(uid(1), 'at_c0', time=1), cascade(uid(1), 'at_c1', time=2)])
    S('STD-LGS-020', 'an ended sheet stops applying', [
        commit([acc, c1]), commit([sheet(SHEET, 'm', 'Customer { audit_level: "strict"; }')], label='s'),
        commit([{'op': 'end', 'object_id': SHEET, 'base': base('s', SHEET)}]), cascade(uid(1))])
    S('STD-LGS-020', 'a selector on RawDocument content is untyped access', [
        commit([sheet(SHEET, 'm', 'ScannedForm[this.source == "x"] { read_only: true; }')], error='RULE.UNTYPED_ACCESS')])
    S('STD-LGS-020', 'a qualifier that is not boolean is rejected at commit', [
        commit([sheet(SHEET, 'm', 'Customer[this.name] { read_only: true; }')], error='LOGIC.TYPE_ERROR')])
    S('STD-LGS-020', 'a qualifier over an unknown member is rejected at commit', [
        commit([sheet(SHEET, 'm', 'Customer[this.rating > 3] { read_only: true; }')], error='LOGIC.TYPE_ERROR')])
    S('STD-LGS-020', 'a relation qualifier needs a reference member', [
        commit([sheet(SHEET, 'm', 'Customer:rel(email)[this.kind == "x"] { read_only: true; }')], error='MODEL.INVALID_VALUE')])


def specificity(P):
    def sp(desc, selector, error=None):
        P('STD-LGS-021', desc, 'specificity', {'classes': CLASSES, 'selector': selector}, error=error)

    sp('the universal selector has specificity (0, 0, 0, 0)', '*')
    sp('a class one step below DataObject has d = 2', 'Party')
    sp('a subclass has a greater d', 'Customer')
    sp('d grows with each parent step', 'VipCustomer')
    sp('an attribute qualifier counts in a', 'Customer[this.country == "US"]')
    sp('a relation qualifier counts in a', 'Customer:rel(account)[this.kind == "x"]')
    sp('a state qualifier counts in s', 'Customer:active')
    sp('a tag qualifier counts in s', 'Customer:tag(vip)')
    sp('a context qualifier counts in x', 'Customer:ctx(role = "cfo")')
    sp('qualifiers of every kind add up', 'Customer[this.country == "US"]:active:tag(vip):ctx(role = "cfo")')
    sp('the tuple compares lexicographically: a beats any number of s and x', '*[this.name == "x"]')
    sp('an unknown class in the selector is rejected', 'Vendor', error='MODEL.REFERENCE_INVALID')


def cascade_(S):
    c1 = create(uid(1), 'Customer', 'c-1', {'name': 'Acme', 'country': 'US', 'state': 'active', 'tags': ['vip']})

    def C(desc, sheets, oid=uid(1), ctx=None, error=None, extra=()):
        S('STD-LGS-022', desc, [commit([c1]), commit(sheets, error=error)] + list(extra)
          + ([cascade(oid, ctx=ctx)] if not error else []))

    C('a higher layer wins over a lower layer', [
        sheet(SHEET, 'p', 'Customer { audit_level: "basic"; }', layer='platform'),
        sheet(uid(902), 't', 'Customer { audit_level: "strict"; }', layer='tenant')])
    C('the object layer is the highest', [
        sheet(SHEET, 'u', 'Customer { audit_level: "basic"; }', layer='unit'),
        sheet(uid(902), 'o', 'Customer { audit_level: "strict"; }', layer='object')])
    C('within a layer the more specific selector wins', [
        sheet(SHEET, 'a', 'Party { audit_level: "basic"; }'),
        sheet(uid(902), 'b', 'Customer { audit_level: "strict"; }')])
    C('an attribute qualifier beats a deeper type selector', [
        sheet(SHEET, 'a', 'VipCustomer { audit_level: "basic"; }'),
        sheet(uid(902), 'b', 'Party[this.country == "US"] { audit_level: "strict"; }')])
    C('a state qualifier beats a context qualifier', [
        sheet(SHEET, 'a', 'Customer:ctx(role = "cfo") { audit_level: "basic"; }'),
        sheet(uid(902), 'b', 'Customer:active { audit_level: "strict"; }')], ctx={'role': 'cfo'})
    C('at equal specificity the sheet priority wins', [
        sheet(SHEET, 'a', 'Customer { audit_level: "basic"; }', priority=1),
        sheet(uid(902), 'b', 'Customer { audit_level: "strict"; }', priority=5)])
    C('within one sheet the later block wins', [
        sheet(SHEET, 'a', 'Customer { audit_level: "basic"; }\nCustomer { audit_level: "strict"; }')])
    C('within one block the later declaration wins', [
        sheet(SHEET, 'a', 'Customer { audit_level: "basic"; audit_level: "strict"; }')])
    C('a locked lower-layer declaration beats an unlocked higher layer', [
        sheet(SHEET, 'p', 'Party { audit_level: "strict" !locked; }', layer='platform'),
        sheet(uid(902), 't', 'Customer:active[this.country == "US"] { audit_level: "basic"; }', layer='tenant',
              priority=9)], error='RULE.LOCKED_OVERRIDE')
    C('a locked declaration beats a more specific unlocked one in the same layer', [
        sheet(SHEET, 'a', 'Party { audit_level: "strict" !locked; }'),
        sheet(uid(902), 'b', 'Customer:active { audit_level: "basic"; }')])
    C('among locked declarations the lower layer wins', [
        sheet(SHEET, 'p', 'Party { field_required: ["email"] !locked; }', layer='platform'),
        sheet(uid(902), 'v', 'Customer { field_required: ["country"] !locked; }', layer='vae')],
      error='RULE.LOCKED_OVERRIDE')
    C('a higher-layer declaration of a locked property on an overlapping selector is rejected at authoring', [
        sheet(SHEET, 'p', 'Customer { read_only: true !locked; }', layer='platform'),
        sheet(uid(902), 't', 'Customer { read_only: false; }', layer='tenant')], error='RULE.LOCKED_OVERRIDE')
    C('a higher-layer declaration on a disjoint selector is not an override', [
        sheet(SHEET, 'p', 'Supplier { read_only: true !locked; }', layer='platform'),
        sheet(uid(902), 't', 'Customer { read_only: false; }', layer='tenant')])
    C('a higher-layer declaration on a disjoint state is not an override', [
        sheet(SHEET, 'p', 'Customer:closed { read_only: true !locked; }', layer='platform'),
        sheet(uid(902), 't', 'Customer:active { read_only: false; }', layer='tenant')])
    C('a higher-layer declaration on a contradictory context is not an override', [
        sheet(SHEET, 'p', 'Customer:ctx(branch_kind = "main") { read_only: true !locked; }', layer='platform'),
        sheet(uid(902), 't', 'Customer:ctx(branch_kind = "sandbox") { read_only: false; }', layer='tenant')])
    C('two sheets of equal rank with different values are ambiguous', [
        sheet(SHEET, 'a', 'Customer { audit_level: "basic"; }'),
        sheet(uid(902), 'b', 'Customer { audit_level: "strict"; }')], error='RULE.AMBIGUOUS_CASCADE')
    C('equal rank with the same value is not ambiguous', [
        sheet(SHEET, 'a', 'Customer { audit_level: "strict"; }'),
        sheet(uid(902), 'b', 'Customer { audit_level: "strict"; }')])
    C('equal rank on different properties is not ambiguous', [
        sheet(SHEET, 'a', 'Customer { audit_level: "strict"; }'),
        sheet(uid(902), 'b', 'Customer { read_only: true; }')])
    C('equal rank on different keys of a map property is not ambiguous', [
        sheet(SHEET, 'a', 'Customer { mask.email: "hide"; }'),
        sheet(uid(902), 'b', 'Customer { mask.country: "hash"; }')])
    C('equal rank with a combinable property combines', [
        sheet(SHEET, 'a', 'Customer { field_required: ["email"]; }'),
        sheet(uid(902), 'b', 'Customer { field_required: ["country"]; }')])
    C('equal rank on selectors that cannot overlap is not ambiguous', [
        sheet(SHEET, 'a', 'Customer { audit_level: "basic"; }'),
        sheet(uid(902), 'b', 'Supplier { audit_level: "strict"; }')])
    C('attribute qualifiers are assumed to overlap', [
        sheet(SHEET, 'a', 'Customer[this.country == "US"] { audit_level: "basic"; }'),
        sheet(uid(902), 'b', 'Customer[this.country == "CA"] { audit_level: "strict"; }')], error='RULE.AMBIGUOUS_CASCADE')
    C('a trait selector may overlap any class of equal rank', [
        sheet(SHEET, 'a', 'DataObject:active { audit_level: "basic"; }'),
        sheet(uid(902), 'b', 'Tagged:active { audit_level: "strict"; }')], error='RULE.AMBIGUOUS_CASCADE')
    S('STD-LGS-022', 'ambiguity is detected when the second sheet is committed later', [
        commit([c1]), commit([sheet(SHEET, 'a', 'Customer { audit_level: "basic"; }')]),
        commit([sheet(uid(902), 'b', 'Customer { audit_level: "strict"; }')], error='RULE.AMBIGUOUS_CASCADE')])
    S('STD-LGS-022', 'a sheet cascades on its branch and not on a branch forked before it', [
        commit([c1]), op('create_branch', name='feature/other', **{'from': BR}),
        commit([sheet(SHEET, 'a', 'Customer { audit_level: "strict"; }')]),
        cascade(uid(1), 'rules'), cascade(uid(1), 'other', branch='feature/other')])
    S('STD-LGS-022', 'the block specificity is that of its most specific matching selector', [
        commit([c1]),
        commit([sheet(SHEET, 'a', 'Supplier, Customer:active:tag(vip) { audit_level: "strict"; }'),
                sheet(uid(902), 'b', 'Customer[this.country == "US"] { audit_level: "basic"; }')]),
        cascade(uid(1))])
    S('STD-LGS-022', 'an object without matching declarations has no effective properties', [
        commit([c1]), commit([sheet(SHEET, 'a', 'Supplier { audit_level: "strict"; }')]), cascade(uid(1))])
    S('STD-LGS-022', 'the cascade of an unknown object fails', [
        commit([c1]), cascade(uid(7), error='IAM.NOT_FOUND')])
    # since BPA 1.0-rc7 (DEC-084): the sheets a merge brings into the target are checked
    two = [commit([c1]), op('create_branch', name='feature/other', **{'from': BR}),
           commit([sheet(SHEET, 'a', 'Customer { audit_level: "basic"; }')]),
           commit([sheet(uid(902), 'b', 'Customer { audit_level: "strict"; }')], branch='feature/other')]
    S('STD-LGS-022', 'a merge bringing in a sheet ambiguous with a sheet of the target is rejected', two + [
        op('merge', error='RULE.AMBIGUOUS_CASCADE', source='feature/other', target=BR), cascade(uid(1))])
    S('STD-LGS-022', 'the merge preview reports the ambiguity of the sheets it would bring in', two + [
        op('merge_preview', source='feature/other', target=BR)])
    S('STD-LGS-022', 'a merge bringing in a sheet of the same value merges', [
        commit([c1]), op('create_branch', name='feature/other', **{'from': BR}),
        commit([sheet(SHEET, 'a', 'Customer { audit_level: "strict"; }')]),
        commit([sheet(uid(902), 'b', 'Customer { audit_level: "strict"; }')], branch='feature/other'),
        op('merge', source='feature/other', target=BR), cascade(uid(1))])


def explanation(S):
    c1 = create(uid(1), 'Customer', 'c-1', {'name': 'Acme', 'country': 'US', 'state': 'active'})
    three = [sheet(SHEET, 'p', 'Party { audit_level: "basic"; }', layer='platform'),
             sheet(uid(902), 't', 'Customer { audit_level: "standard"; }', layer='tenant'),
             sheet(uid(903), 'u', 'Customer:active { audit_level: "strict"; }', layer='tenant')]
    S('STD-LGS-023', 'the explanation lists every matching candidate with its rank and why it lost', [
        commit([c1]), commit(three), explain(uid(1), 'audit_level')])
    S('STD-LGS-023', 'a locked winner explains the losers as locked elsewhere', [
        commit([c1]), commit([sheet(SHEET, 'p', 'Party { audit_level: "basic" !locked; }', layer='platform'),
                              sheet(uid(902), 't', 'Customer:active { audit_level: "strict"; }', layer='platform')]),
        explain(uid(1), 'audit_level')])
    S('STD-LGS-023', 'combined candidates are all winners', [
        commit([c1]), commit([sheet(SHEET, 'a', 'Customer { field_required: ["email"]; }'),
                              sheet(uid(902), 'b', 'Customer { field_required: ["country"]; }')]),
        explain(uid(1), 'field_required')])
    S('STD-LGS-023', 'a map key is explained as its own property path', [
        commit([c1]), commit([sheet(SHEET, 'a', 'Party { mask: {email: "hide", country: "hash"}; }'),
                              sheet(uid(902), 'b', 'Customer { mask.email: "partial"; }')]),
        explain(uid(1), 'mask.email')])
    S('STD-LGS-023', 'a property without candidates has no effective value', [
        commit([c1]), commit([sheet(SHEET, 'a', 'Supplier { audit_level: "strict"; }')]),
        explain(uid(1), 'audit_level')])
    S('STD-LGS-023', 'an unknown property cannot be explained', [
        commit([c1]), explain(uid(1), 'audit_depth', error='RULE.UNKNOWN_PROPERTY')])
    S('STD-LGS-023', 'losers in one sheet are explained by position', [
        commit([c1]), commit([sheet(SHEET, 'a', 'Customer { audit_level: "basic"; }\nCustomer { audit_level: "strict"; }')]),
        explain(uid(1), 'audit_level')])
