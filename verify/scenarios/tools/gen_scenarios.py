#!/usr/bin/env python3
"""Write the Forge project of the gate-1a scenarios (VER-SCN-1000, EXIT-1-13; PH-1 step 7c,
DEC-060): forge.toml, model/, logic/, seed/ and tests/ with one test file per scenario
SCN-002…008, -010, -011, -018, and SCN-009 of gate 1b (PH-1 step 18d-3b). Run from the repository root; `forge test verify/scenarios`
runs them (`--backend postgres --database …` on PostgreSQL).

The large fixtures (products of SCN-004, the objects of SCN-018) are generated here so that
the YAML stays readable; everything else is written by hand below."""
import json
import os
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
PRINCIPAL = 'did:ubos:acme:01900000-0000-7000-8000-000000000002'
APPROVER = 'did:ubos:acme:01900000-0000-7000-8000-00000000000a'


def uid(n, kind=0):
    return '01890a5d-ac96-7%03x-8000-%012x' % (kind, n)


def w(rel, text):
    p = os.path.join(ROOT, rel)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, 'w', encoding='utf-8') as f:
        f.write(text)


def y(v, indent=0):
    """Compact JSON is valid YAML; one item per line keeps files diffable."""
    return json.dumps(v, ensure_ascii=False)


def field(name, t, required=False, **kw):
    f = {'name': name, 'label': name, 'description': name, 'type': t}
    if required:
        f['required'] = True
    f.update(kw)
    return f


def cls(name, fields, parent='DataObject', kind='definition', **kw):
    c = {'name': name, 'label': name, 'description': name, 'namespace': 'acme.scn', 'parent': parent,
         'kind': kind, 'schema_version': 1, 'fields': fields}
    c.update(kw)
    return c


def port(name, impl, params=None):
    return {'name': name, 'description': name, 'kind': 'command', 'params': params or [], 'permission': name,
            'impl': impl}


# ---- model ----
# Counterparty and BatchJob: the platform genesis Buks hold Party (ontology) and Job (system)
# on every Server (DEC-077), so the scenario model uses other names (DEC-082)
PARTY = cls('Counterparty', [field('name', 'text', True)])
AGREEMENT = cls('Agreement', [
    field('title', 'text', True),
    field('effective_from', 'date'),
    field('parties', 'list', constraints={'item_type': {'type': 'reference', 'constraints': {'target_class': 'Counterparty'}},
                                            'min_items': 2},
          relationship={'cardinality': 'many_to_many', 'target': 'Counterparty'}),
    field('state', 'enum', constraints={'allowed_values': ['active', 'terminated']}, default='active'),
], ports=[port('terminate', 'acme.agreements#terminate')])
SERVICE = cls('ServiceAgreement', [
    field('title', 'text', True, constraints={'max_length': 120}),
    field('service_level', 'enum', constraints={'allowed_values': ['gold', 'silver']}),
    field('in_notice_period', 'boolean'),
], parent='Agreement', ports=[port('terminate', 'acme.agreements#terminate_service')])
PRODUCT = cls('Product', [field('name', 'text', True), field('price', 'decimal'), field('category', 'text')])
CUSTOMER = cls('Customer', [field('name', 'text', True), field('email', 'text'), field('country', 'text')])
PRICELIST = cls('PriceList', [field('price', 'decimal', True)], emit_became_effective=True)
RATE = cls('Rate', [field('value', 'decimal', True)])
CASH = cls('CashMovement', [field('account', 'text', True), field('amount', 'money', True), field('memo', 'text')],
           kind='ledger', ledger={'key': ['account'], 'amounts': ['amount'], 'double_entry': False})
JOB = cls('BatchJob', [field('name', 'text', True), field('note', 'text'), field('counter', 'integer')],
          ports=[port('run', 'acme.runaway#run'), port('probe', 'acme.runaway#probe')])
# SCN-011 starts its process through the port, so that it runs over UBTP too (DEC-082)
FEE = cls('FeeCase', [field('name', 'text', True), field('fee', 'decimal')], ports=[port('calculate', 'acme.fee#calculate')])
# SCN-009 (gate 1b, PH-1 step 18d-3b): a control injected through a governance sheet
MONEY = cls('MoneyMovement', [field('amount', 'decimal', True), field('memo', 'text')],
            ports=[port('issue', 'acme.money#issue')])
PAYMENT = cls('Payment', [field('payee', 'text')], parent='MoneyMovement')
REFUND = cls('Refund', [field('customer', 'text')], parent='MoneyMovement')

w('forge.toml', '[project]\nname = "gate-1a-scenarios"\nversion = "0.1.0"\nvae = "acme"\nabi = "1.0"\nbranch = "main"\n')
w('model/classes.yaml', 'classes:\n' + ''.join('  - %s\n' % y(c) for c in
                                          [PARTY, AGREEMENT, SERVICE, PRODUCT, CUSTOMER, PRICELIST, RATE, CASH, JOB, FEE,
                                           MONEY, PAYMENT, REFUND]))

# ---- logic ----
def asset(name, exports, functions=None, tier='host', source=None, capabilities=None, profile='standard'):
    a = {'name': name, 'abi': '1.0', 'tier': tier, 'profile': profile, 'state': 'committed',
         'capabilities': capabilities or [], 'exports': exports}
    a['source'] = source if tier == 'l2' else {'functions': functions}
    return a


AGREEMENTS = asset('acme.agreements', ['terminate', 'terminate_service'], tier='l2', capabilities=['read', 'commit'],
                   source=(
                       'fn terminate() { sys.read.get(#{target: self_id}); sys.commit.update(#{target: self_id, set: #{state: "terminated"}}); "terminated" }\n'
                       'fn terminate_service() {\n'
                       '  if this.in_notice_period == true {\n'
                       '    return #{denied: true, guard: "notice period", rule: "ServiceAgreement.terminate overrides Agreement.terminate"};\n'
                       '  }\n'
                       '  sys.read.get(#{target: self_id}); sys.commit.update(#{target: self_id, set: #{state: "terminated"}}); "terminated"\n'
                       '}\n'))
RUNAWAY = asset('acme.runaway', ['run', 'probe'], tier='l2', capabilities=['read', 'commit'],
                source=('fn run() { sys.read.get(#{target: self_id}); let n = 0; loop { n += 1; sys.commit.update(#{target: self_id, set: #{counter: n}}); } }\n'
                        'fn probe() { http_get("http://169.254.169.254/latest/meta-data") }\n'))
FEECALC = asset('acme.fee', ['calculate'], capabilities=['call'], functions={
    'calculate': {'params': [], 'body': [
        {'do': 'call.connector', 'as': 'r', 'args': {'connector': '"rate"', 'operation': '"get"', 'request': {}, 'when': '"now"'}},
        {'do': 'sys.uuid', 'as': 'u'},
        {'return': '[ctx.now, u, r.usd]'},
    ]}})
MONEYLOGIC = asset('acme.money', ['issue'], functions={'issue': {'params': [], 'body': [{'return': '"issued"'}]}})
w('logic/assets.yaml', 'assets:\n' + ''.join('  - %s\n' % y(a) for a in [AGREEMENTS, RUNAWAY, FEECALC, MONEYLOGIC]))

# ---- seed ----
def create(oid, klass, slug, body, valid=None):
    c = {'op': 'create', 'class': klass, 'object_id': oid, 'slug': slug, 'body': body}
    if valid:
        c['valid'] = valid
    return c


P1, P2, P3 = uid(1, 1), uid(2, 1), uid(3, 1)
PU1, PU2, PU3 = 'ubos://acme/Counterparty/p-1', 'ubos://acme/Counterparty/p-2', 'ubos://acme/Counterparty/p-3'
seed = [create(P1, 'Counterparty', 'p-1', {'name': 'Acme'}), create(P2, 'Counterparty', 'p-2', {'name': 'Beta'}),
        create(P3, 'Counterparty', 'p-3', {'name': 'Gamma'})]
seed += [create(uid(i, 2), 'Product', 'prod-%d' % i, {'name': 'Product %d' % i, 'price': '%d' % (10 + i), 'category': 'a'})
         for i in range(1, 51)]
seed += [create(uid(1, 3), 'Customer', 'acme', {'name': 'Acme Ltd', 'email': 'v1@acme.example', 'country': 'US'})]
seed += [create(uid(1, 4), 'PriceList', 'standard', {'price': '100'}, valid={'from': '2026-01-01'})]
seed += [create(uid(1, 6), 'BatchJob', 'job-1', {'name': 'runaway', 'counter': 0})]
seed += [create(uid(1, 7), 'FeeCase', 'fee-1', {'name': 'fee', 'fee': '0'})]
seed += [create(uid(i, 8), 'Product', 'bulk-%d' % i, {'name': 'Bulk %d' % i, 'price': '1', 'category': 'bulk'}) for i in range(1, 41)]
w('seed/objects.yaml', 'changes:\n' + ''.join('  - %s\n' % y(c) for c in seed))

# ---- tests ----
def test(name, steps, clock=None, **kw):
    t = {'name': name, 'kind': 'scenario'}
    if clock:
        t['clock'] = clock
    t.update(kw)
    t['steps'] = steps
    return t


def step(op, input, expect=None, label=None, **kw):
    s = {'op': op, 'input': input}
    if expect is not None:
        s['expect'] = expect
    if label:
        s['label'] = label
    s.update(kw)
    return s


def subset(result):
    return {'match': 'subset', 'result': result}


def err(code, details=None):
    e = {'error_code': code}
    if details:
        e['details'] = details
    return e


def tests_file(rel, tests, header):
    w(rel, '# %s\ntests:\n' % header + ''.join('  - %s\n' % y(t) for t in tests))


CLASSES_ALL = [PARTY, AGREEMENT, SERVICE, PRODUCT, CUSTOMER, PRICELIST, RATE, CASH, JOB, FEE]
LONG = 'T' * 150
SA = uid(1, 5)
AG = uid(2, 5)

# SCN-002
sa_widened = dict(SERVICE, fields=[dict(SERVICE['fields'][0], required=False)] + SERVICE['fields'][1:])
tests_file('tests/scn-002.yaml', [
    test('SCN-002 main: the subclass narrows title and a 150-character title is rejected', [
        step('create_branch', {'name': 'feature/model', 'from': 'main'}, subset({})),
        step('commit', {'branch': 'main', 'changes': [create(SA, 'ServiceAgreement', 'sa-1',
             {'title': LONG, 'parties': [PU1, PU2], 'service_level': 'gold'})]}, err('MODEL.CONSTRAINT_VIOLATED')),
        step('commit', {'branch': 'main', 'changes': [create(SA, 'ServiceAgreement', 'sa-1',
             {'title': 'Service agreement one', 'parties': [PU1, PU2], 'service_level': 'gold'})]},
             subset({'cseq': 2}), label='c'),
        step('read', {'branch': 'main', 'object_id': SA},
             subset({'body': {'title': 'Service agreement one', 'parties': [PU1, PU2]}, 'valid': {'from': '2026-03-15', 'to': None}})),
        step('commit_info', {'branch': 'main'}, subset({'cseq': 2})),
        step('commit', {'branch': 'main', 'changes': [create(uid(3, 5), 'Agreement', 'a-bad',
             {'title': 'One party only', 'parties': [PU1]})]}, err('MODEL.CONSTRAINT_VIOLATED')),
    ], clock='2026-03-15T10:00:00.000000Z'),
    test('SCN-002 A1: widening title in the subclass is a narrowing violation', [
        step('effective_class', {'class': 'ServiceAgreement', 'classes': [PARTY, AGREEMENT, sa_widened]},
             err('MODEL.NARROWING_VIOLATION')),
    ]),
    test('SCN-002 post: querying Agreement includes the ServiceAgreement instance', [
        step('commit', {'branch': 'main', 'changes': [create(SA, 'ServiceAgreement', 'sa-1',
             {'title': 'Service agreement one', 'parties': [PU1, PU2], 'service_level': 'gold'}),
             create(AG, 'Agreement', 'a-1', {'title': 'Plain agreement', 'parties': [PU1, PU3]})]}, subset({})),
        step('query', {'branch': 'main', 'from': 'Agreement'}, subset({'objects': [SA, AG]})),
        step('query', {'branch': 'main', 'from': 'Agreement', 'only': True}, subset({'objects': [AG]})),
        step('effective_class', {'class': 'ServiceAgreement', 'classes': CLASSES_ALL},
             subset({'linearization': ['ServiceAgreement', 'Agreement', 'DataObject', 'Object'],
                     'fields': {'title': {'constraints': {'max_length': 120}, 'required': True}}})),
    ], clock='2026-03-15T10:00:00.000000Z'),
], 'SCN-002 — Define a class hierarchy and create instances (FR-MODEL-011, -021, -041, -061, -101, FR-QRY-011)')

# SCN-003
sa_narrow_port = dict(SERVICE, ports=[dict(port('terminate', 'acme.agreements#terminate_service'),
                                           params=[field('days', 'decimal', True, constraints={'max': '500'})])])
ag_port_param = dict(AGREEMENT, ports=[dict(port('terminate', 'acme.agreements#terminate'),
                                            params=[field('days', 'decimal', True, constraints={'max': '1000'})])])
tests_file('tests/scn-003.yaml', [
    test('SCN-003 main: the override guard fails inside the notice period; the base logic terminates a plain agreement', [
        step('commit', {'branch': 'feature/forge', 'changes': [
            create(SA, 'ServiceAgreement', 'sa-1', {'title': 'Service agreement one', 'parties': [PU1, PU2], 'service_level': 'gold', 'in_notice_period': True}),
            create(AG, 'Agreement', 'a-1', {'title': 'Plain agreement', 'parties': [PU1, PU3]})]}, subset({})),
        step('dispatch', {'class': 'ServiceAgreement', 'classes': CLASSES_ALL, 'port': 'terminate'},
             {'match': 'exact', 'result': {'class': 'ServiceAgreement', 'impl': 'acme.agreements#terminate_service'}}),
        step('dispatch', {'class': 'Agreement', 'classes': CLASSES_ALL, 'port': 'terminate'},
             {'match': 'exact', 'result': {'class': 'Agreement', 'impl': 'acme.agreements#terminate'}}),
        step('execute', {'branch': 'feature/forge', 'target': SA, 'port': 'terminate', 'params': {}},
             subset({'result': {'denied': True, 'guard': 'notice period'}}), label='g'),
        step('read', {'branch': 'feature/forge', 'object_id': SA}, subset({'body': {'in_notice_period': True, 'title': 'Service agreement one'}})),
        step('execute', {'branch': 'feature/forge', 'target': AG, 'port': 'terminate', 'params': {}},
             subset({'result': 'terminated'}), label='t'),
        step('read', {'branch': 'feature/forge', 'object_id': AG}, subset({'body': {'state': 'terminated'}})),
    ], clock='2026-03-15T10:00:00.000000Z'),
    test('SCN-003 A1: an override narrowing a parameter type is rejected', [
        step('effective_class', {'class': 'ServiceAgreement', 'classes': [PARTY, ag_port_param, sa_narrow_port]},
             err('MODEL.PORT_SIGNATURE_INCOMPATIBLE')),
    ]),
], 'SCN-003 — Polymorphic action dispatch (step 3, the governance sheet, is gate 1b: rules)')

# SCN-004
def prod(i):
    return uid(i, 2)


draft_edits = [{'op': 'update', 'object_id': prod(i), 'base': {'$ref': 'seedv/%s' % prod(i)}, 'body': {'price': '%d.5' % (10 + i)}}
               for i in range(1, 13)]
draft_adds = [create(uid(i, 2), 'Product', 'prod-%d' % i, {'name': 'Product %d' % i, 'price': '99', 'category': 'new'}) for i in range(51, 54)]
tests_file('tests/scn-004.yaml', [
    test('SCN-004 main: a draft branch, 15 changes, the diff, the approved change set merged in one commit', [
        step('export_state', {'branch': 'main'}, subset({}), label='before'),
        step('commit_info', {'branch': 'main'}, subset({}), label='head0'),
        # the versions of the seeded products, as bases of the draft's updates
        step('query', {'branch': 'main', 'from': 'Product'}, subset({})),
        *[step('read', {'branch': 'main', 'object_id': prod(i)}, subset({}), label='r%d' % i) for i in range(1, 13)],
        step('create_branch', {'name': 'draft/prices', 'from': 'main'}, subset({'head': 1})),
        step('commit', {'branch': 'draft/prices', 'changes':
             [{'op': 'update', 'object_id': prod(i), 'base': {'$ref': 'r%d/version_hash' % i}, 'body': {'price': '%d.5' % (10 + i)}}
              for i in range(1, 13)] + draft_adds}, subset({'cseq': 2})),
        step('read', {'branch': 'main', 'object_id': prod(1)}, subset({'body': {'price': '11'}})),
        step('read', {'branch': 'draft/prices', 'object_id': prod(1)}, subset({'body': {'price': '11.5'}})),
        step('diff', {'from': {'branch': 'main'}, 'to': {'branch': 'draft/prices'}}, subset(
            [{'class': 'Product', 'object_id': prod(i), 'op': 'changed', 'fields': [{'path': '/price', 'old': '%d' % (10 + i), 'new': '%d.5' % (10 + i)}]} for i in range(1, 13)]
            + [{'class': 'Product', 'object_id': uid(i, 2), 'op': 'added'} for i in range(51, 54)]), label='diff'),
        step('merge_preview', {'source': 'draft/prices', 'target': 'main'}, subset({'conflicts': []}), label='preview'),
        step('change_set', {'action': 'open', 'id': uid(1, 9), 'source': 'draft/prices', 'target': 'main', 'title': 'price update'}, subset({'state': 'open'})),
        step('change_set', {'action': 'submit', 'id': uid(1, 9)}, subset({'state': 'submitted'})),
        step('change_set', {'action': 'approve', 'id': uid(1, 9), 'approver': APPROVER}, subset({'state': 'approved'})),
        step('change_set', {'action': 'merge', 'id': uid(1, 9)}, subset({'state': 'merged'}), label='m'),
        step('read', {'branch': 'main', 'object_id': prod(12)}, subset({'body': {'price': '22.5'}})),
        step('read', {'branch': 'main', 'object_id': uid(53, 2)}, subset({'body': {'price': '99'}})),
        step('commit_info', {'branch': 'main'}, subset({}), label='merge_commit'),
        step('read', {'branch': 'main', 'object_id': prod(1), 'time': 1}, subset({'body': {'price': '11'}})),
    ], clock='2026-03-15T10:00:00.000000Z'),
    test('SCN-004 A1: the same field changed on main and the draft is a conflict; nothing merges', [
        step('read', {'branch': 'main', 'object_id': prod(20)}, subset({}), label='r'),
        step('create_branch', {'name': 'draft/conflict', 'from': 'main'}, subset({})),
        step('commit', {'branch': 'draft/conflict', 'changes': [{'op': 'update', 'object_id': prod(20), 'base': {'$ref': 'r/version_hash'}, 'body': {'price': '1'}}]}, subset({})),
        step('commit', {'branch': 'main', 'changes': [{'op': 'update', 'object_id': prod(20), 'base': {'$ref': 'r/version_hash'}, 'body': {'price': '2'}}]}, subset({})),
        step('merge', {'source': 'draft/conflict', 'target': 'main'},
             err('VER.MERGE_CONFLICT', {'conflicts': [{'kind': 'field', 'object_id': prod(20), 'path': '/price'}]})),
        step('read', {'branch': 'main', 'object_id': prod(20)}, subset({'body': {'price': '2'}})),
    ], clock='2026-03-15T10:00:00.000000Z'),
    test('SCN-004 A2: a different field of the same product merges automatically', [
        step('read', {'branch': 'main', 'object_id': prod(21)}, subset({}), label='r'),
        step('create_branch', {'name': 'draft/other', 'from': 'main'}, subset({})),
        step('commit', {'branch': 'draft/other', 'changes': [{'op': 'update', 'object_id': prod(21), 'base': {'$ref': 'r/version_hash'}, 'body': {'price': '3'}}]}, subset({})),
        step('commit', {'branch': 'main', 'changes': [{'op': 'update', 'object_id': prod(21), 'base': {'$ref': 'r/version_hash'}, 'body': {'category': 'b'}}]}, subset({})),
        step('merge', {'source': 'draft/other', 'target': 'main'}, subset({'merged': True})),
        step('read', {'branch': 'main', 'object_id': prod(21)}, subset({'body': {'price': '3', 'category': 'b'}})),
    ], clock='2026-03-15T10:00:00.000000Z'),
], 'SCN-004 — Change on a draft, review the diff, merge (FR-VER-*)')

# SCN-005
CUST = uid(1, 3)
tests_file('tests/scn-005.yaml', [
    test('SCN-005 main: a stale base is rejected with TXN.HEAD_MOVED and the chain stays linear', [
        step('read', {'branch': 'main', 'object_id': CUST}, subset({'body': {'email': 'v1@acme.example'}}), label='v7'),
        step('commit', {'branch': 'main', 'changes': [{'op': 'update', 'object_id': CUST, 'base': {'$ref': 'v7/version_hash'}, 'body': {'email': 'a@acme.example'}}]}, subset({}), label='a'),
        step('commit', {'branch': 'main', 'changes': [{'op': 'update', 'object_id': CUST, 'base': {'$ref': 'v7/version_hash'}, 'body': {'country': 'DE'}}]},
             err('TXN.HEAD_MOVED', {'objects': [CUST]})),
        step('read', {'branch': 'main', 'object_id': CUST}, subset({'body': {'email': 'a@acme.example', 'country': 'US'}}), label='v8'),
        step('commit', {'branch': 'main', 'changes': [{'op': 'update', 'object_id': CUST, 'base': {'$ref': 'v8/version_hash'}, 'body': {'country': 'DE'}}]}, subset({}), label='b'),
        step('read', {'branch': 'main', 'object_id': CUST}, subset({'body': {'email': 'a@acme.example', 'country': 'DE'}})),
        step('history', {'branch': 'main', 'object_id': CUST}, subset({})),
    ], clock='2026-03-15T10:00:00.000000Z'),
    test('SCN-005 A1: 100 concurrent writers on one base version, exactly one succeeds', [
        step('read', {'branch': 'main', 'object_id': CUST}, subset({}), label='v'),
        dict(step('commit', {'branch': 'main', 'changes': [{'op': 'update', 'object_id': CUST, 'base': {'$ref': 'v/version_hash'}, 'body': {'email': 'race@acme.example'}}]},
                  subset({'ok': 1, 'errors': {'TXN.HEAD_MOVED': 99}})), concurrent=100),
        step('read', {'branch': 'main', 'object_id': CUST}, subset({'body': {'email': 'race@acme.example'}})),
    ], clock='2026-03-15T10:00:00.000000Z'),
], 'SCN-005 — Concurrent edits are detected, never lost (FR-TXN-*)')

# SCN-006
PL = uid(1, 4)
tests_file('tests/scn-006.yaml', [
    test('SCN-006 main: a future-dated price is read by asof and becomes effective on its day without a job', [
        step('read', {'branch': 'main', 'object_id': PL}, subset({'body': {'price': '100'}}), label='v'),
        step('commit', {'branch': 'main', 'changes': [{'op': 'update', 'object_id': PL, 'base': {'$ref': 'v/version_hash'}, 'body': {'price': '120'},
             'valid': {'from': '2026-10-01', 'kind': 'supersede_from'}}]}, subset({'retroactive': []})),
        step('read', {'branch': 'main', 'object_id': PL, 'asof': '2026-09-20'}, subset({'body': {'price': '100'}, 'valid': {'from': '2026-01-01', 'to': '2026-10-01'}})),
        step('read', {'branch': 'main', 'object_id': PL, 'asof': '2026-10-05'}, subset({'body': {'price': '120'}, 'valid': {'from': '2026-10-01', 'to': None}})),
        step('read', {'branch': 'main', 'object_id': PL}, subset({'body': {'price': '100'}})),
        {'advance': '16d'},
        step('read', {'branch': 'main', 'object_id': PL}, subset({'body': {'price': '120'}})),
        step('released_effects', {}, subset({'effects': [{'type': 'BecameEffective', 'object_id': PL, 'payload': {'valid_from': '2026-10-01'}}]})),
    ], clock='2026-09-15T10:00:00.000000Z'),
    test('SCN-006 A1: a second future version; every date resolves to one version', [
        step('read', {'branch': 'main', 'object_id': PL}, subset({}), label='v'),
        step('commit', {'branch': 'main', 'changes': [{'op': 'update', 'object_id': PL, 'base': {'$ref': 'v/version_hash'}, 'body': {'price': '120'},
             'valid': {'from': '2026-10-01', 'kind': 'supersede_from'}}]}, subset({}), label='c1'),
        step('commit', {'branch': 'main', 'changes': [{'op': 'update', 'object_id': PL, 'base': {'$ref': 'c1/versions/%s' % PL}, 'body': {'price': '130'},
             'valid': {'from': '2026-11-01', 'kind': 'supersede_from'}}]}, subset({})),
        step('read', {'branch': 'main', 'object_id': PL, 'asof': '2026-10-15'}, subset({'body': {'price': '120'}, 'valid': {'from': '2026-10-01', 'to': '2026-11-01'}})),
        step('read', {'branch': 'main', 'object_id': PL, 'asof': '2026-11-15'}, subset({'body': {'price': '130'}, 'valid': {'from': '2026-11-01', 'to': None}})),
        step('read', {'branch': 'main', 'object_id': PL, 'asof': '2026-09-30'}, subset({'body': {'price': '100'}, 'valid': {'from': '2026-01-01', 'to': '2026-10-01'}})),
        step('history', {'branch': 'main', 'object_id': PL}, subset({})),
    ], clock='2026-09-15T10:00:00.000000Z'),
], 'SCN-006 — Future-dated price becomes effective automatically (STD-TIME-050)')

# SCN-007
RT = uid(1, 10)
tests_file('tests/scn-007.yaml', [
    test('SCN-007 main: what did we believe on March 1 and March 11 about February 15', [
        step('commit', {'branch': 'main', 'changes': [create(RT, 'Rate', 'r-1', {'value': '0.05'}, valid={'from': '2026-01-01'})]}, subset({}), label='c1'),
        step('read', {'branch': 'main', 'object_id': RT}, subset({}), label='v1'),
        {'advance': '9d'},
        step('commit', {'branch': 'main', 'changes': [{'op': 'update', 'object_id': RT, 'base': {'$ref': 'v1/version_hash'}, 'body': {'value': '0.07'},
             'valid': {'from': '2026-02-01', 'kind': 'supersede_from'}}]}, subset({}), label='c2'),
        step('read', {'branch': 'main', 'object_id': RT, 'asof': '2026-02-15', 'tx_time': '2026-03-01T12:00:00.000000Z'}, subset({'body': {'value': '0.05'}})),
        step('read', {'branch': 'main', 'object_id': RT, 'asof': '2026-02-15', 'tx_time': '2026-03-11T12:00:00.000000Z'}, subset({'body': {'value': '0.07'}})),
        step('read', {'branch': 'main', 'object_id': RT, 'asof': '2026-02-15', 'tx_time': '2026-03-01T12:00:00.000000Z'}, subset({'body': {'value': '0.05'}})),
        step('history', {'branch': 'main', 'object_id': RT},
             subset({'commits': [{'ops': [{'from': '2026-01-01', 'kind': 'create'}]}, {'ops': [{'from': '2026-02-01', 'kind': 'supersede_from'}]}]})),
        step('read', {'branch': 'main', 'object_id': RT, 'asof': '2026-02-15', 'tx_time': '2026-03-11T12:00:00.000000Z', 'provenance': True}, subset({'provenance': {'branch': 'main'}})),
    ], clock='2026-03-01T10:00:00.000000Z'),
    test('SCN-007 A1: a time before the creation is not found at that time', [
        step('commit', {'branch': 'main', 'changes': [create(RT, 'Rate', 'r-1', {'value': '0.05'}, valid={'from': '2026-01-01'})]}, subset({})),
        step('read', {'branch': 'main', 'object_id': RT, 'asof': '2025-12-31'}, err('QRY.NOT_FOUND_AT_TIME')),
    ], clock='2026-03-01T10:00:00.000000Z'),
], 'SCN-007 — Bitemporal question from an auditor (STD-TIME-010…015)')

# SCN-008
E1041, E1187, E1188 = uid(1041, 11), uid(1187, 11), uid(1188, 11)
tests_file('tests/scn-008.yaml', [
    test('SCN-008 main: a wrong posting is reversed and re-posted; history stays', [
        step('commit', {'branch': 'main', 'changes': [{'op': 'append', 'class': 'CashMovement', 'entry_id': E1041, 'ledger_key': {'account': '1010'},
             'body': {'account': '1010', 'amount': {'amount': '10000.00', 'currency': 'USD'}, 'memo': 'wrong'}, 'valid_at': '2026-03-15'}]},
             subset({'entries': {E1041: {'seq': 1}}})),
        step('commit', {'branch': 'main', 'changes': [{'op': 'reverse', 'entry_id': E1041, 'new_entry_id': E1187}]}, subset({'entries': {E1187: {'seq': 2}}})),
        step('commit', {'branch': 'main', 'changes': [{'op': 'adjust', 'entry_id': E1041, 'new_entry_id': E1188,
             'body': {'account': '1010', 'amount': {'amount': '1000.00', 'currency': 'USD'}, 'memo': 'corrected'}}]}, subset({'entries': {E1188: {'seq': 3}}})),
        step('read_ledger', {'branch': 'main', 'class': 'CashMovement', 'ledger_key': {'account': '1010'}},
             subset({'entries': [{'entry_id': E1041, 'seq': 1, 'body': {'amount': {'amount': '10000.00', 'currency': 'USD'}}},
                                 {'entry_id': E1187, 'seq': 2, 'corrects': [E1041], 'correction_type': 'reversal', 'body': {'amount': {'amount': '-10000.00', 'currency': 'USD'}}},
                                 {'entry_id': E1188, 'seq': 3, 'corrects': [E1041], 'correction_type': 'adjustment', 'body': {'amount': {'amount': '1000.00', 'currency': 'USD'}}}]})),
    ], clock='2026-03-15T10:00:00.000000Z'),
    test('SCN-008 A1: a ledger entry cannot be updated', [
        step('commit', {'branch': 'main', 'changes': [{'op': 'append', 'class': 'CashMovement', 'entry_id': E1041, 'ledger_key': {'account': '1010'},
             'body': {'account': '1010', 'amount': {'amount': '10000.00', 'currency': 'USD'}}, 'valid_at': '2026-03-15'}]}, subset({})),
        step('commit', {'branch': 'main', 'changes': [{'op': 'update', 'object_id': E1041, 'base': 'sha256:0000000000000000000000000000000000000000000000000000000000000000', 'body': {'amount': {'amount': '1000.00', 'currency': 'USD'}}}]}, err('LEDG.IMMUTABLE_ENTRY')),
    ], clock='2026-03-15T10:00:00.000000Z'),
    test('SCN-008 A2: a branch with ledger entries merges by appending in branch order', [
        step('commit', {'branch': 'main', 'changes': [{'op': 'append', 'class': 'CashMovement', 'entry_id': uid(1, 11), 'ledger_key': {'account': '2000'},
             'body': {'account': '2000', 'amount': {'amount': '5.00', 'currency': 'USD'}}, 'valid_at': '2026-03-15'}]}, subset({'entries': {uid(1, 11): {'seq': 1}}})),
        step('create_branch', {'name': 'feature/ledger', 'from': 'main'}, subset({})),
        step('commit', {'branch': 'feature/ledger', 'changes': [
            {'op': 'append', 'class': 'CashMovement', 'entry_id': uid(2, 11), 'ledger_key': {'account': '2000'}, 'body': {'account': '2000', 'amount': {'amount': '6.00', 'currency': 'USD'}}, 'valid_at': '2026-03-15'},
            {'op': 'append', 'class': 'CashMovement', 'entry_id': uid(3, 11), 'ledger_key': {'account': '2000'}, 'body': {'account': '2000', 'amount': {'amount': '7.00', 'currency': 'USD'}}, 'valid_at': '2026-03-15'}]},
             subset({'entries': {uid(2, 11): {'seq': 2}, uid(3, 11): {'seq': 3}}})),
        step('commit', {'branch': 'main', 'changes': [{'op': 'append', 'class': 'CashMovement', 'entry_id': uid(4, 11), 'ledger_key': {'account': '2000'},
             'body': {'account': '2000', 'amount': {'amount': '8.00', 'currency': 'USD'}}, 'valid_at': '2026-03-15'}]}, subset({'entries': {uid(4, 11): {'seq': 2}}})),
        step('merge', {'source': 'feature/ledger', 'target': 'main'},
             subset({'merged': True, 'appended': {uid(2, 11): {'seq': 3, 'source_seq': 2}, uid(3, 11): {'seq': 4, 'source_seq': 3}}})),
        step('read_ledger', {'branch': 'main', 'class': 'CashMovement', 'ledger_key': {'account': '2000'}},
             subset({'entries': [{'seq': 1}, {'seq': 2, 'entry_id': uid(4, 11)}, {'seq': 3, 'entry_id': uid(2, 11)}, {'seq': 4, 'entry_id': uid(3, 11)}]})),
    ], clock='2026-03-15T10:00:00.000000Z'),
], 'SCN-008 — Ledger correction by reversal (STD-KIND-020…025)')

# SCN-010
JOB1 = uid(1, 6)
tests_file('tests/scn-010.yaml', [
    test('SCN-010 main: runaway L2 logic is stopped, nothing is committed or released, the run is metered', [
        step('execute', {'branch': 'feature/forge', 'target': JOB1, 'port': 'run', 'params': {}, 'process': {'process_id': uid(1, 12)}},
             err('LOGIC.LIMIT_EXCEEDED')),
        step('read', {'branch': 'feature/forge', 'object_id': JOB1}, subset({'body': {'counter': 0}})),
        step('released_effects', {}, {'match': 'exact', 'result': {'effects': []}}),
        step('metering', {'process_id': uid(1, 12)}, subset({'outcome': 'limit', 'process_id': uid(1, 12)})),
    ], clock='2026-03-15T10:00:00.000000Z'),
    test('SCN-010 A1: a script that opens a network connection is refused', [
        step('execute', {'branch': 'feature/forge', 'target': JOB1, 'port': 'probe', 'params': {}}, err('LOGIC.FORBIDDEN_CAPABILITY')),
    ], clock='2026-03-15T10:00:00.000000Z'),
], 'SCN-010 — Runaway logic is stopped and rolled back (STD-CTX-004, -005, -020)')

# SCN-011
PID = uid(1, 13)
tests_file('tests/scn-011.yaml', [
    test('SCN-011 main: a process with the clock, entropy and a connector replays identically', [
        step('execute', {'branch': 'feature/forge', 'target': uid(1, 7), 'port': 'calculate', 'params': {}, 'process': {'process_id': PID}},
             subset({'metering': {'connector_calls': 1, 'outcome': 'ok'}}), label='p'),
        step('journal', {'process_id': PID}, subset({'records': [{'instruction': 'decision', 'seq': 1}, {'instruction': 'call.connector', 'seq': 2}]})),
        step('replay', {'process_id': PID}, subset({'outcome': 'identical'})),
        step('export_state', {'branch': 'feature/forge'}, subset({}), label='after'),
    ], clock='2026-03-15T10:00:00.000000Z', connectors={'rate': {'get': {'usd': '1.10'}}}),
    test('SCN-011 A1: a missing journal record stops the replay without calling the connector', [
        step('execute', {'branch': 'feature/forge', 'target': uid(1, 7), 'port': 'calculate', 'params': {}, 'process': {'process_id': PID}}, subset({})),
        step('replay', {'process_id': PID, 'drop': 1}, err('LOGIC.REPLAY_INPUT_MISSING')),
    ], clock='2026-03-15T10:00:00.000000Z', connectors={'rate': {'get': {'usd': '1.10'}}}),
], 'SCN-011 — Deterministic replay of a past process (STD-CTX-011, -012)')

# SCN-018
BULK = [uid(i, 8) for i in range(1, 41)]
P_BULK = uid(1, 14)
reads = [step('read', {'branch': 'main', 'object_id': o}, subset({}), label='b%d' % i) for i, o in enumerate(BULK, 1)]
updates = [{'op': 'update', 'object_id': o, 'base': {'$ref': 'b%d/version_hash' % i}, 'body': {'price': '2'}} for i, o in enumerate(BULK, 1)]
appends = [{'op': 'append', 'class': 'CashMovement', 'entry_id': uid(i, 15), 'ledger_key': {'account': '3000'},
            'body': {'account': '3000', 'amount': {'amount': '1.00', 'currency': 'USD'}}, 'valid_at': '2026-03-15'} for i in range(1, 13)]
later = [step('read', {'branch': 'main', 'object_id': BULK[i]}, subset({}), label='l%d' % i) for i in range(5)]
later_updates = [step('commit', {'branch': 'main', 'changes': [{'op': 'update', 'object_id': BULK[i], 'base': {'$ref': 'l%d/version_hash' % i}, 'body': {'price': '3'}}]}, subset({})) for i in range(5)]
tests_file('tests/scn-018.yaml', [
    test('SCN-018 main: a bulk process is reverted; the 5 objects changed since are conflicts, resolved to keep the later change', [
        *reads,
        step('bulk', {'branch': 'main', 'changes': updates + appends, 'max_changes': 100, 'process_id': P_BULK}, subset({'state': 'committed'})),
        step('read', {'branch': 'main', 'object_id': BULK[10]}, subset({'body': {'price': '2'}})),
        *later, *later_updates,
        step('revert', {'process_id': P_BULK, 'reversal_ids': {uid(i, 15): uid(i, 16) for i in range(1, 13)}},
             err('VER.MERGE_CONFLICT', {'conflicts': [{'kind': 'field', 'object_id': BULK[i], 'path': '/price'} for i in range(5)]})),
        step('revert', {'process_id': P_BULK, 'reversal_ids': {uid(i, 15): uid(i, 16) for i in range(1, 13)},
             'resolutions': [{'object_id': BULK[i], 'path': '/price', 'choice': 'target', 'resolver': PRINCIPAL} for i in range(5)]},
             subset({'reverted': [{'branch': 'main'}], 'dependents': []}), label='rev'),
        step('read', {'branch': 'main', 'object_id': BULK[10]}, subset({'body': {'price': '1'}})),
        step('read', {'branch': 'main', 'object_id': BULK[0]}, subset({'body': {'price': '3'}})),
        step('read_ledger', {'branch': 'main', 'class': 'CashMovement', 'ledger_key': {'account': '3000'}},
             subset({'entries': [{'seq': i, 'entry_id': uid(i, 15)} for i in range(1, 13)]
                     + [{'seq': 12 + i, 'correction_type': 'reversal', 'corrects': [uid(i, 15)], 'body': {'amount': {'amount': '-1.00', 'currency': 'USD'}}} for i in range(1, 13)]})),
        step('history', {'branch': 'main', 'object_id': BULK[10]}, subset({})),
        step('process_info', {'process_id': P_BULK}, subset({'state': 'committed', 'process_id': P_BULK})),
    ], clock='2026-03-15T10:00:00.000000Z'),
], 'SCN-018 — Revert a whole process (STD-VER-070, -071; A1, dependent processes, needs the event triggers of gate 1b)')
# SCN-009 (gate 1b): the control is a governance sheet merged into the logic branch, where the
# ports run; the rejected commit names the declaration (`audit_level: strict` needs a message
# and a reason), the cascade explanation names the sheet, selector and specificity, the
# signature is a step required of the command port (`require_signature`, STD-LGS-031), and the
# commit's rule binding lists the sheet (STD-TIME-040, BPA 1.0-rc7)
SHEET_SRC = 'MoneyMovement[this.amount > 10000] { audit_level: "strict"; require_signature: true; }'
SHEET_URI = 'ubos://acme/GovernanceSheet/control'
RF, PM = uid(1, 17), uid(2, 17)


def sheet(oid, slug, source):
    return create(oid, 'GovernanceSheet', slug, {'name': 'acme.' + slug.replace('-', '_'), 'layer': 'tenant',
                                                 'source': source, 'description': slug})


CONTROL = sheet(uid(1, 18), 'control', SHEET_SRC)
FF = 'feature/forge'
tests_file('tests/scn-009.yaml', [
    test('SCN-009 main: a control merged as a sheet rejects a large refund without audit data and requires a signature', [
        step('create_branch', {'name': 'draft/control', 'from': FF}, subset({})),
        step('commit', {'branch': 'draft/control', 'changes': [CONTROL]}, subset({}), label='sc'),
        step('merge', {'source': 'draft/control', 'target': FF}, subset({'merged': True}), label='m'),
        step('commit', {'branch': FF, 'changes': [create(RF, 'Refund', 'rf-1', {'amount': '12000', 'customer': 'Acme'})]},
             err('RULE.DECLARATION_UNSATISFIED', {'property': 'audit_level', 'object_id': RF})),
        step('commit', {'branch': FF, 'changes': [create(RF, 'Refund', 'rf-1', {'amount': '12000', 'customer': 'Acme'})],
                        'process': {'message': 'refund of order 1001', 'reason': 'customer-return'}}, subset({}), label='r'),
        step('commit_info', {'branch': FF}, subset({'rule_binding': {'sheets': [{'$ref': 'sc/versions/%s' % uid(1, 18)}]}})),
        step('execute', {'branch': FF, 'target': RF, 'port': 'issue', 'params': {}}, err('RULE.DECISION_REQUIRES')),
        step('commit', {'branch': FF, 'changes': [create(PM, 'Payment', 'pm-1', {'amount': '9000', 'payee': 'Beta'})]},
             subset({}), label='p'),
        step('execute', {'branch': FF, 'target': PM, 'port': 'issue', 'params': {}}, subset({'result': 'issued'})),
        step('explain_cascade', {'branch': FF, 'object_id': RF, 'property': 'audit_level'},
             subset({'effective': 'strict', 'property': 'audit_level', 'object': 'ubos://acme/Refund/rf-1',
                     'candidates': [{'selector': 'MoneyMovement[this.amount > 10000]', 'sheet': SHEET_URI,
                                     'value': 'strict', 'won': True, 'reason': 'won'}]})),
        step('explain_cascade', {'branch': FF, 'object_id': RF, 'property': 'require_signature'},
             subset({'effective': True, 'candidates': [{'selector': 'MoneyMovement[this.amount > 10000]', 'sheet': SHEET_URI,
                                                        'won': True}]})),
        step('explain_cascade', {'branch': FF, 'object_id': PM, 'property': 'audit_level'},
             subset({'candidates': []})),
    ], clock='2026-03-15T10:00:00.000000Z'),
    test('SCN-009 A1: a second sheet setting audit_level with equal specificity and priority is rejected at its merge', [
        step('create_branch', {'name': 'draft/a', 'from': FF}, subset({})),
        step('create_branch', {'name': 'draft/b', 'from': FF}, subset({})),
        step('commit', {'branch': 'draft/a', 'changes': [CONTROL]}, subset({})),
        step('commit', {'branch': 'draft/b', 'changes': [sheet(uid(2, 18), 'control-b',
             'MoneyMovement[this.amount > 10000] { audit_level: "standard"; }')]}, subset({})),
        step('merge', {'source': 'draft/a', 'target': FF}, subset({'merged': True})),
        step('merge_preview', {'source': 'draft/b', 'target': FF}, subset({'validation': [{'code': 'RULE.AMBIGUOUS_CASCADE'}]})),
        step('merge', {'source': 'draft/b', 'target': FF}, err('RULE.AMBIGUOUS_CASCADE')),
    ], clock='2026-03-15T10:00:00.000000Z'),
], 'SCN-009 — Compliance officer injects a control through a governance sheet (gate 1b; FR-RULE-011, -021, -051, FR-TIME-061)')
print('scenarios written')
