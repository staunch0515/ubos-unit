"""Vector cases of BPA 1.0-rc1 for execution (PH-1 step 1e-2, DEC-044): the execution
context, gateway, profiles, metering, determinism, journal and replay, logic assets and their
verification (STD-CTX-*), the instructions of ABI 1.0 (STD-ISA-*), the BPU (STD-BPU-*) and
automatic retry (STD-TXN-023). Logic is written as host programs (STD-ISA-007). Not required
of the PH-0 prototype."""
from .common import CLASSES, NOW, cls, field, uid, commit, create, base, stock
from .model import port

SEED = '11' * 32
ACCOUNT = cls('Account', [field('name', 'text', True), field('balance', 'decimal'), field('status', 'text')],
              ports=[port('rename', 'acme.acct#rename', params=[field('name', 'text', True)]),
                     dict(port('close', 'acme.acct#close'), emits=['AccountClosed'])],
              events=[{'name': 'AccountClosed', 'description': 'an account was closed'}])
PREMIUM = cls('PremiumAccount', [field('tier', 'text')], parent='Account',
              ports=[port('rename', 'acme.prem#rename', params=[field('name', 'text', True)])])
TAX = cls('TaxRule', [field('rate', 'decimal', True)], parent='ConfigAsset')
MOVE = cls('CashMove', [field('account', 'reference', constraints={'target_class': 'Account'}),
                        field('amount', 'decimal', True)], kind='ledger',
           ledger={'key': ['account'], 'amounts': ['amount'], 'double_entry': False})
TEAM = cls('Team', [field('name', 'text', True),
                    field('members', 'list', constraints={'item_type': {'type': 'reference',
                                                                        'constraints': {'target_class': 'Account'}}},
                          relationship={'target': 'Account', 'cardinality': 'many_to_many', 'on_end': 'detach',
                                        'inverse': 'teams'})])
PARTY = cls('Party', [field('name', 'text', True)], abstract=True)
EXEC_CLASSES = CLASSES + [ACCOUNT, PREMIUM, TAX, MOVE, TEAM, PARTY]
APP = 'feature/app'


def fn(body, params=(), **extra):
    f = {'params': list(params), 'body': body}
    f.update(extra)
    return f


def asset(n, name, functions, caps=('commit', 'read', 'run', 'emit'), state='committed', abi='1.0', tier='host',
          imports=None, tests=None, exports=None, **extra):
    body = {'name': name, 'tier': tier, 'abi': abi, 'source': {'functions': functions} if tier == 'host' else functions,
            'exports': list(exports if exports is not None else (functions if tier == 'host' else [])),
            'capabilities': list(caps), 'profile': 'standard', 'state': state}
    if imports is not None:
        body['imports'] = imports
    if tests is not None:
        body['tests'] = tests
    body.update(extra)
    return create(uid(n), 'Logic', name.replace('.', '-'), body)


ACCT = asset(80, 'acme.acct', {
    'rename': fn([{'do': 'read.get', 'args': {'target': 'self_id'}},
                  {'do': 'commit.update', 'args': {'target': 'self_id', 'set': {'name': 'params.name'}}},
                  {'return': 'concat("renamed ", this.name, " to ", params.name)'}], ['name']),
    'close': fn([{'do': 'read.get', 'args': {'target': 'self_id'}},
                 {'do': 'commit.update', 'args': {'target': 'self_id', 'set': {'status': '"closed"'}}},
                 {'do': 'emit.event', 'args': {'type': '"AccountClosed"', 'subject': 'self_id', 'payload': {}}},
                 {'return': '"closed"'}])})
PREM = asset(81, 'acme.prem', {
    'rename': fn([{'do': 'run.super', 'args': {}, 'as': 's'}, {'return': 'concat("premium: ", s)'}], ['name'])},
    caps=('commit', 'read', 'run'))


def cases(g):
    V = g.vector

    def setup(now=NOW):
        return [{'op': 'set_now', 'input': {'now': now}}, {'op': 'load_genesis', 'input': {'classes': EXEC_CLASSES}}]

    def S(clause, desc, steps, now=NOW, also=()):
        V(clause, desc, setup=setup(now), steps=steps, prototype=False, also=also)

    def P(clause, desc, op, inp, error=None):
        g.pure(clause, desc, op, inp, error=error, prototype=False)

    def deploy(*assets, branch=APP, data=()):
        return [{'op': 'create_branch', 'input': {'name': branch, 'from': 'main'}},
                commit(list(assets) + list(data), branch=branch, label='d')]

    def ex(a, function, args=(), label=None, error=None, branch=APP, pid=1, **kw):
        inp = {'branch': branch, 'asset': a, 'function': function, 'args': list(args),
               'process': {'process_id': uid(500 + pid)}, 'entropy_seed': SEED}
        inp.update(kw)
        s = {'op': 'execute', 'input': inp}
        if label:
            s['label'] = label
        if error:
            s['error'] = error
        return s

    def prog(*body, params=(), **extra):
        return {'main': fn(list(body), params, **extra)}

    def run1(clause, desc, body, caps=('commit', 'read', 'run', 'emit'), error=None, args=(), data=(), extra=(),
             params=(), frozen=None, **kw):
        a = asset(90, 'acme.app', prog(*body, params=params), caps=caps)
        step = ex('acme.app', 'main', args, error=error, **kw)
        if frozen:
            step['frozen'] = frozen
        S(clause, desc, deploy(a, data=data) + [step] + list(extra))

    acct = create(uid(1), 'Account', 'a-1', {'name': 'Acme', 'balance': '100'})
    prem = create(uid(2), 'PremiumAccount', 'p-1', {'name': 'Beta', 'tier': 'gold'})

    # ---- STD-CTX-001 execution context ----
    run1('STD-CTX-001', 'the context carries identity, coordinates, clock, process and profile',
         [{'return': 'ctx'}], ctx={'principal': 'did:ubos:acme:01900000-0000-7000-8000-00000000000a',
                                   'locale': 'de-CH', 'time_zone': 'Europe/Zurich', 'input': {'x': 1}})
    run1('STD-CTX-001', 'the context is fixed for the process', [
        {'let': 'a', 'value': 'ctx.now'}, {'do': 'sys.uuid', 'as': 'u'}, {'return': 'ctx.now == a'}])
    run1('STD-CTX-001', 'logic cannot assign the context', [{'let': 'ctx', 'value': '1'}],
         error='LOGIC.CONTEXT_IMMUTABLE')
    run1('STD-CTX-001', 'an instruction result cannot replace the context', [{'do': 'sys.uuid', 'as': 'ctx'}],
         error='LOGIC.CONTEXT_IMMUTABLE')
    run1('STD-CTX-001', 'the caller cannot set members the implementation owns', [{'return': '1'}],
         ctx={'tenant': 'globex'}, error='LOGIC.CONTEXT_IMMUTABLE')
    S('STD-CTX-001', 'nested port runs share the context of the process', deploy(
        ACCT, asset(90, 'acme.app', prog({'do': 'run.port', 'args': {'target': '"ubos://acme/Account/a-1"',
                                                                     'port': '"rename"',
                                                                     'params': {'name': '"Acme Ltd"'}}, 'as': 'r'},
                                         {'return': 'r'})), data=[acct]) + [
        ex('acme.app', 'main'), {'op': 'read', 'input': {'branch': APP, 'object_id': uid(1)}}])

    # ---- STD-CTX-002 identity comes only from the context ----
    run1('STD-CTX-002', 'an instruction does not take a tenant', [
        {'do': 'commit.create', 'args': {'class': '"Customer"', 'fields': {'name': '"X"'}, 'tenant': '"globex"'}}],
         error='LOGIC.PARAMETER_FORBIDDEN')
    run1('STD-CTX-002', 'an instruction does not take a principal', [
        {'do': 'read.get', 'args': {'target': '"ubos://acme/Account/a-1"', 'principal': '"did:ubos:acme:x"'}}],
         error='LOGIC.PARAMETER_FORBIDDEN', data=[acct])
    run1('STD-CTX-002', 'objects of another VAE are not reached by URI', [
        {'do': 'read.get', 'args': {'target': '"ubos://acme.eu/Account/a-1"'}}], error='IAM.NOT_FOUND', data=[acct])

    # ---- STD-CTX-003 capability gateway ----
    run1('STD-CTX-003', 'an instruction outside the declared capabilities is forbidden', [
        {'do': 'commit.create', 'args': {'class': '"Customer"', 'fields': {'name': '"X"'}}}], caps=('read',),
         error='LOGIC.FORBIDDEN_CAPABILITY')
    run1('STD-CTX-003', 'a capability may name one instruction', [
        {'do': 'read.get', 'args': {'target': '"ubos://acme/Account/a-1"'}, 'as': 'r'}, {'return': 'r.fields.name'}],
         caps=('read.get',), data=[acct])
    run1('STD-CTX-003', 'one declared instruction does not open its family', [
        {'do': 'read.history', 'args': {'target': '"ubos://acme/Account/a-1"'}}], caps=('read.get',),
         error='LOGIC.FORBIDDEN_CAPABILITY', data=[acct])
    run1('STD-CTX-003', 'the profile limits the families: strict has no call family', [
        {'do': 'call.connector', 'args': {'connector': '"bank"', 'operation': '"pay"', 'request': {}}}],
         caps=('call',), ctx={'profile': 'strict'}, error='LOGIC.FORBIDDEN_CAPABILITY')
    run1('STD-CTX-003', 'an instruction outside the ABI is forbidden', [
        {'do': 'commit.destroy', 'args': {'target': '"x"'}}], caps=('commit',), error='LOGIC.FORBIDDEN_CAPABILITY')
    run1('STD-CTX-003', 'a forbidden capability cannot be caught', [
        {'try': [{'do': 'commit.create', 'args': {'class': '"Customer"', 'fields': {'name': '"X"'}}}],
         'catch': [{'return': '"caught"'}]}], caps=('read',), error='LOGIC.FORBIDDEN_CAPABILITY')
    run1('STD-CTX-003', 'an instruction of the ABI not available in this release', [
        {'do': 'search.query', 'args': {'query': {}}}], caps=('search',), error='LOGIC.NOT_AVAILABLE',
         frozen='search.query is available since BPA 1.0-rc2 (STD-ISA-080, DEC-067)')

    # ---- STD-CTX-004 profiles ----
    for p in ('strict', 'standard', 'batch', 'simulation', 'agent'):
        P('STD-CTX-004', 'the default limits of the %s profile' % p, 'profile_limits', {'profile': p})
    loop = [{'repeat': '20', 'body': [{'do': 'sys.uuid'}]}]
    run1('STD-CTX-004', 'a tenant may lower the instruction calls', loop, limits={'instruction_calls': 10},
         error='LOGIC.LIMIT_EXCEEDED')
    run1('STD-CTX-004', 'a lowered limit that is not reached does not stop the process', loop,
         limits={'instruction_calls': 25})
    run1('STD-CTX-004', 'raising a limit needs an entitlement', loop, limits={'instruction_calls': 20000},
         error='IAM.DENIED')
    run1('STD-CTX-004', 'the changes per commit are limited', [
        {'repeat': '3', 'body': [{'do': 'commit.create', 'args': {'class': '"Customer"', 'fields': {'name': '"X"'}}}]}],
         limits={'changes': 2}, error='LOGIC.LIMIT_EXCEEDED')
    run1('STD-CTX-004', 'the operation units are limited', loop, limits={'units': 10}, error='LOGIC.LIMIT_EXCEEDED')
    S('STD-CTX-004', 'the nesting depth is limited', deploy(asset(91, 'acme.lib', {'f': fn([{'return': '1'}])})) + [
        commit([asset(90, 'acme.app', {'main': fn([{'do': 'run.logic', 'args': {'asset': '"acme.lib"', 'function': '"f"',
                                                                              'args': '[]'}, 'as': 'x'},
                                                 {'return': 'x'}])},
                      imports=[{'name': 'acme.lib', 'version': {'$ref': 'd/versions/' + uid(91)}}])], branch=APP),
        ex('acme.app', 'main', limits={'depth': 1}, error='LOGIC.LIMIT_EXCEEDED'),
        ex('acme.app', 'main', limits={'depth': 2}, pid=2)])

    # ---- STD-CTX-005 termination ----
    run1('STD-CTX-005', 'a limit terminates the execution and cannot be caught', [
        {'try': loop, 'catch': [{'return': '"caught"'}]}], limits={'instruction_calls': 5},
         error='LOGIC.LIMIT_EXCEEDED')
    run1('STD-CTX-005', 'the process aborts: nothing is committed', [
        {'do': 'commit.create', 'args': {'class': '"Customer"', 'fields': {'name': '"X"'}}}] + loop,
         limits={'instruction_calls': 5}, error='LOGIC.LIMIT_EXCEEDED',
         extra=[{'op': 'process_info', 'input': {'process_id': uid(501)}},
                {'op': 'commit_info', 'input': {'branch': APP}}])
    run1('STD-CTX-005', 'cancellation terminates at the next metering point', loop, cancel_after_units=10,
         error='LOGIC.CANCELLED', extra=[{'op': 'metering', 'input': {'process_id': uid(501)}}])
    run1('STD-CTX-005', 'the metering record of a terminated execution reports the limit', loop,
         limits={'units': 10}, error='LOGIC.LIMIT_EXCEEDED',
         extra=[{'op': 'metering', 'input': {'process_id': uid(501)}}])

    # ---- STD-CTX-006 metering record ----
    run1('STD-CTX-006', 'the metering record counts units and instructions per family', [
        {'do': 'commit.create', 'args': {'class': '"Customer"', 'fields': {'name': '"X"', 'email': '"x@y.z"'}}, 'as': 'c'},
        {'do': 'read.get', 'args': {'target': 'c.object_id'}}, {'return': '1'}],
         extra=[{'op': 'metering', 'input': {'process_id': uid(501)}}])
    S('STD-CTX-006', 'metering is aggregated per tenant', deploy(asset(90, 'acme.app', prog({'return': '1'}))) + [
        ex('acme.app', 'main'), ex('acme.app', 'main', pid=2), {'op': 'metering_total', 'input': {}}])
    run1('STD-CTX-006', 'a process without an explicit identifier still has a metering record', [{'return': '2'}])

    # ---- STD-CTX-010 non-determinism only from the context and journaled entropy ----
    run1('STD-CTX-010', 'identifiers come from the journaled entropy seed', [
        {'do': 'sys.uuid', 'as': 'a'}, {'do': 'sys.uuid', 'as': 'b'}, {'do': 'sys.random_bytes', 'args': {'n': '8'},
                                                                        'as': 'c'},
        {'return': '[a, b, c]'}])
    S('STD-CTX-010', 'the same seed gives the same values in another process', deploy(
        asset(90, 'acme.app', prog({'do': 'sys.uuid', 'as': 'a'}, {'return': 'a'}))) + [
        ex('acme.app', 'main'), ex('acme.app', 'main', pid=2)])
    S('STD-CTX-010', 'without a seed each process draws from its own journaled entropy', deploy(
        asset(90, 'acme.app', prog({'do': 'sys.uuid', 'as': 'a'}, {'return': 'a'}))) + [
        {'op': 'execute', 'input': {'branch': APP, 'asset': 'acme.app', 'function': 'main', 'args': [],
                                    'process': {'process_id': uid(501)}}},
        {'op': 'execute', 'input': {'branch': APP, 'asset': 'acme.app', 'function': 'main', 'args': [],
                                    'process': {'process_id': uid(502)}}}])
    run1('STD-CTX-010', 'ctx.now does not follow the host clock', [{'return': 'now() == ctx.now'}])
    run1('STD-CTX-010', 'created objects take their identifiers from the entropy', [
        {'do': 'commit.create', 'args': {'class': '"Customer"', 'fields': {'name': '"X"'}}, 'as': 'c'},
        {'return': 'c.object_id'}])

    # ---- STD-CTX-011 journal ----
    obs = [{'do': 'adapter.observe', 'args': {'name': '"rate"'}, 'as': 'r'},
           {'do': 'adapter.observe', 'args': {'name': '"fee"'}, 'as': 'f'}, {'return': 'r + f'}]
    world = {'rate': 3, 'fee': 4}
    run1('STD-CTX-011', 'the journal records the journaled results in order', obs, world=world,
         extra=[{'op': 'journal', 'input': {'process_id': uid(501)}}])
    run1('STD-CTX-011', 'the journal records the entropy seed and the tz database', [{'return': '1'}],
         extra=[{'op': 'journal', 'input': {'process_id': uid(501)}}])
    run1('STD-CTX-011', 'a journaled instruction without an outside value fails', obs, world={'rate': 3},
         error='LOGIC.CONNECTOR_UNAVAILABLE')

    # ---- STD-CTX-012 replay ----
    run1('STD-CTX-012', 'replay with the journal is identical', obs, world=world,
         extra=[{'op': 'replay', 'input': {'process_id': uid(501)}}])
    run1('STD-CTX-012', 'replay serves journaled values and makes no live call', obs, world=world,
         extra=[{'op': 'replay', 'input': {'process_id': uid(501)}}, {'op': 'metering_total', 'input': {}}])
    run1('STD-CTX-012', 'a changed journal record makes the replay diverge', obs, world=world,
         extra=[{'op': 'replay', 'input': {'process_id': uid(501), 'tamper': {'seq': 2, 'result': 5}}}])
    run1('STD-CTX-012', 'a missing journal record fails the replay', obs, world=world,
         extra=[{'op': 'replay', 'input': {'process_id': uid(501), 'drop': 2}, 'error': 'LOGIC.REPLAY_INPUT_MISSING'}])
    run1('STD-CTX-012', 'replay writes nothing', [
        {'do': 'commit.create', 'args': {'class': '"Customer"', 'fields': {'name': '"X"'}}}, {'return': '1'}],
         extra=[{'op': 'branch_info', 'input': {'branch': APP}, 'label': 'c'},
                {'op': 'replay', 'input': {'process_id': uid(501)}},
                {'op': 'branch_info', 'input': {'branch': APP}, 'same_as': 'c'}])
    # since 1.0-rc5 (DEC-082): a replay reads the snapshots the journal recorded
    run1('STD-CTX-012', 'a process that updates what it read replays identically from its snapshot', [
        {'do': 'read.get', 'args': {'target': '"ubos://acme/Account/a-1"'}},
        {'do': 'commit.update', 'args': {'target': '"ubos://acme/Account/a-1"', 'set': {'name': '"Beta"'}}},
        {'return': '1'}], data=[acct], extra=[{'op': 'replay', 'input': {'process_id': uid(501)}}])

    # ---- STD-CTX-020 L2 scripting profile ----
    def l2(n, src, caps=('read',)):
        return asset(n, 'acme.l2-%d' % n, src, tier='l2', caps=caps, exports=['main'])

    def verify(n, branch=APP):
        return {'op': 'verify_asset', 'input': {'branch': branch, 'object_id': uid(n)}}
    for n, (desc, src) in enumerate([
            ('eval is rejected', 'fn main() { eval("1 + 1") }'),
            ('print and debug are rejected', 'fn main() { print("x"); debug(1); }'),
            ('module loading is rejected', 'import "net" as net;\nfn main() { 1 }'),
            ('closures that capture the gateway are rejected', 'fn main() { let f = |x| sys.read.get(x); f.call("a") }'),
            ('a plain script with declared instructions passes', 'fn main(t) { let r = sys.read.get(t); r.fields }'),
            ('an instruction outside the capabilities is reported', 'fn main() { sys.commit.create(#{}) }')]):
        S('STD-CTX-020', 'L2 static analysis: ' + desc, deploy(l2(95 + n, src)) + [verify(95 + n)])
    S('STD-CTX-020', 'an adapter without an L2 engine declares L2 execution unavailable', deploy(
        l2(95, 'fn main() { 1 }')) + [ex('acme.l2-95', 'main', error='LOGIC.NOT_AVAILABLE')])

    # ---- STD-CTX-022 logic asset declaration ----
    S('STD-CTX-022', 'a logic asset is an object of class Logic', deploy(asset(90, 'acme.app', prog({'return': '1'}))) + [
        {'op': 'read', 'input': {'branch': APP, 'object_id': uid(90)}}])
    S('STD-CTX-022', 'a logic asset declares its capabilities', [
        {'op': 'create_branch', 'input': {'name': APP, 'from': 'main'}},
        commit([create(uid(90), 'Logic', 'acme-app', {'name': 'acme.app', 'tier': 'host', 'abi': '1.0',
                                                      'source': {}, 'exports': [], 'profile': 'standard',
                                                      'state': 'draft'})], branch=APP,
               error='MODEL.CONSTRAINT_VIOLATED')])
    S('STD-CTX-022', 'the tier is l2, l3 or host', [
        {'op': 'create_branch', 'input': {'name': APP, 'from': 'main'}},
        commit([asset(90, 'acme.app', prog({'return': '1'}), tier='wasm')], branch=APP,
               error='MODEL.CONSTRAINT_VIOLATED')])
    S('STD-CTX-022', 'only logic assets execute', deploy(asset(90, 'acme.app', prog({'return': '1'})),
                                                         data=[acct]) + [ex(uid(1), 'main', error='MODEL.REFERENCE_INVALID')])
    S('STD-CTX-022', 'an asset of a newer ABI is not executed', deploy(
        asset(90, 'acme.app', prog({'return': '1'}), abi='1.1')) + [ex('acme.app', 'main', error='LOGIC.ABI_UNSUPPORTED')])
    S('STD-CTX-022', 'a function must be exported', deploy(
        asset(90, 'acme.app', prog({'return': '1'}), exports=[])) + [ex('acme.app', 'main', error='LOGIC.FORBIDDEN_CAPABILITY')])
    S('STD-CTX-022', 'an L3 asset declares a publisher signature', deploy(
        asset(90, 'acme.wasm', {'imports': ['ubos:isa/v1']}, tier='l3', exports=['main'])) + [verify(90)])

    # ---- STD-CTX-023 verification record and gate ----
    main_app = asset(90, 'acme.app', prog({'return': '"ok"'}))
    S('STD-CTX-023', 'an unverified asset does not execute on main', [
        commit([main_app], label='d'), ex('acme.app', 'main', branch='main', error='LOGIC.UNVERIFIED_ASSET')])
    S('STD-CTX-023', 'a verified asset executes on main', [
        commit([main_app], label='d'), verify(90, 'main'), ex('acme.app', 'main', branch='main')])
    S('STD-CTX-023', 'setting the state without a verification record does not verify', [
        commit([main_app], label='d'),
        commit([{'op': 'update', 'object_id': uid(90), 'base': base('d', uid(90)), 'body': {'state': 'verified'}}]),
        ex('acme.app', 'main', branch='main', error='LOGIC.UNVERIFIED_ASSET')])
    S('STD-CTX-023', 'changing the code of a verified asset needs a new verification', [
        commit([main_app], label='d'), verify(90, 'main'),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(90)}, 'label': 'r'},
        commit([{'op': 'update', 'object_id': uid(90), 'base': {'$ref': 'r/version_hash'},
                 'body': {'source': {'functions': {'main': fn([{'return': '"changed"'}])}}}}]),
        ex('acme.app', 'main', branch='main', error='LOGIC.UNVERIFIED_ASSET')])
    S('STD-CTX-023', 'a deprecated asset still executes', [
        commit([main_app], label='d'), verify(90, 'main'),
        {'op': 'read', 'input': {'branch': 'main', 'object_id': uid(90)}, 'label': 'r'},
        commit([{'op': 'update', 'object_id': uid(90), 'base': {'$ref': 'r/version_hash'},
                 'body': {'state': 'deprecated'}}]),
        ex('acme.app', 'main', branch='main')])
    S('STD-CTX-023', 'working branches execute unverified assets but not retired ones', deploy(
        asset(90, 'acme.app', prog({'return': '1'})), asset(91, 'acme.old', prog({'return': '2'}), state='retired')) + [
        ex('acme.app', 'main'), ex('acme.old', 'main', pid=2, error='LOGIC.UNVERIFIED_ASSET')])
    S('STD-CTX-023', 'the verification record lists the checks and is signed', deploy(
        asset(90, 'acme.app', prog({'return': '1'}), tests=[{'function': 'main', 'args': [], 'expect': 1}])) + [
        verify(90), {'op': 'node_did', 'input': {}, 'label': 'n'}])
    S('STD-CTX-023', 'failing tests keep the asset unverified', deploy(
        asset(90, 'acme.app', prog({'return': '1'}), tests=[{'function': 'main', 'args': [], 'expect': 2}])) + [
        verify(90), {'op': 'read', 'input': {'branch': APP, 'object_id': uid(90)}}])

    # ---- STD-CTX-024 static analysis ----
    for n, (desc, functions, caps, extra) in enumerate([
            ('a call to an undeclared instruction is reported',
             {'main': fn([{'do': 'commit.create', 'args': {'class': '"Customer"', 'fields': {}}}])}, ('read',), {}),
            ('an instruction outside the ABI is reported',
             {'main': fn([{'do': 'commit.destroy', 'args': {}}])}, ('commit',), {}),
            ('a statement of an unknown kind is a forbidden construct',
             {'main': fn([{'exec': 'rm -rf /'}])}, ('read',), {}),
            ('recursion without a declared bound is reported',
             {'main': fn([{'do': 'run.logic', 'args': {'asset': '"acme.app"', 'function': '"main"', 'args': '[]'}}])},
             ('run',), {}),
            ('recursion with a declared bound is accepted',
             {'main': fn([{'do': 'run.logic', 'args': {'asset': '"acme.app"', 'function': '"main"', 'args': '[]'}}],
                         max_depth=3)}, ('run',), {}),
            ('an expression that does not type-check is reported',
             {'main': fn([{'return': '1 + "a"'}])}, ('read',), {}),
            ('nesting beyond the limit is reported',
             {'main': fn(nested(40))}, ('read',), {})]):
        S('STD-CTX-024', desc, deploy(asset(90, 'acme.app', functions, caps=caps, **extra)) + [verify(90)])
    S('STD-CTX-024', 'L3 imports outside the world are reported', deploy(
        asset(90, 'acme.wasm', {'imports': ['ubos:isa/v1', 'wasi:filesystem']}, tier='l3', exports=['main'],
              publisher_signature={'role': 'signer'})) + [verify(90)])

    # ---- STD-CTX-030 version pinning per process ----
    def port_run(target, port_, params, pid=1, **kw):
        s_ = {'op': 'execute', 'input': {'branch': APP, 'target': target, 'port': port_, 'params': params,
                                         'process': {'process_id': uid(500 + pid)}, 'entropy_seed': SEED}}
        s_.update(kw)
        return s_
    v2 = {'source': {'functions': {'rename': fn([{'return': '"v2"'}], ['name']), 'close': fn([{'return': '"v2"'}])}}}
    S('STD-CTX-030', 'a new asset version affects only processes that start after it', deploy(ACCT, data=[acct]) + [
        port_run('ubos://acme/Account/a-1', 'rename', {'name': 'First'}),
        {'op': 'read', 'input': {'branch': APP, 'object_id': uid(80)}, 'label': 'r'},
        commit([{'op': 'update', 'object_id': uid(80), 'base': {'$ref': 'r/version_hash'}, 'body': v2}], branch=APP),
        port_run('ubos://acme/Account/a-1', 'rename', {'name': 'Second'}, pid=2)])
    upgrade = asset(90, 'acme.ops', prog(
        {'do': 'read.get', 'args': {'target': '"ubos://acme/Logic/acme-acct"'}, 'as': 'l'},
        {'do': 'commit.update', 'args': {'target': 'l.object_id', 'set': {'source': {'functions': {
            'rename': {'params': '["name"]', 'body': '[]'}, 'close': {'params': '[]', 'body': '[]'}}}}}},
        {'do': 'run.port', 'args': {'target': '"ubos://acme/Account/a-1"', 'port': '"rename"',
                                    'params': {'name': '"Pinned"'}}, 'as': 'x'},
        {'return': 'x'}))
    S('STD-CTX-030', 'a process uses the version it resolved, even after changing the asset', deploy(
        ACCT, upgrade, data=[acct]) + [ex('acme.ops', 'main'),
                                       port_run('ubos://acme/Account/a-1', 'rename', {'name': 'Later'}, pid=2)])

    # ---- STD-ISA-001 instruction descriptors ----
    # The released vector of 1.0-rc1 asserts the catalogue of its release: superseded by 1.0-rc3,
    # which adds read.actions and error codes (STD-CONF-030, DEC-071); the next vector asserts the current one.
    V('STD-ISA-001', 'the descriptors of ABI 1.0 are normative', prototype=False,
      steps=[{'op': 'instructions', 'input': {}, 'frozen': 'the catalogue of ABI 1.0 grows in BPA 1.0-rc3 (STD-ISA-036, DEC-071)'}])
    run1('STD-ISA-001', 'logic sees no instruction outside the descriptors', [
        {'do': 'read.everything', 'args': {}}], caps=('read',), error='LOGIC.FORBIDDEN_CAPABILITY')
    # VER-CONF-3002 asserts the catalogue of 1.0-rc3…rc7: superseded by 1.0-rc8, which declares
    # the parameters of search.aggregate, search.traverse and search.text (DEC-095).
    V('STD-ISA-001', 'the descriptors of ABI 1.0 are normative (since 1.0-rc3)', prototype=False,
      steps=[{'op': 'instructions', 'input': {}, 'frozen': 'the search instructions declare their parameters in '
                                                            'BPA 1.0-rc8 (STD-ISA-081…083, DEC-095)'}])
    # VER-CONF-3003 asserts the catalogue of 1.0-rc8: superseded by 1.0-rc9, which adds
    # read.projection (STD-ISA-037, DEC-098).
    V('STD-ISA-001', 'the descriptors of ABI 1.0 are normative (since 1.0-rc8)', prototype=False,
      steps=[{'op': 'instructions', 'input': {}, 'frozen': 'read.projection joins the catalogue in BPA 1.0-rc9 '
                                                            '(STD-ISA-037, DEC-098)'}])
    # VER-CONF-3004 asserts the catalogue of 1.0-rc9…rc11: superseded by 1.0-rc12, which adds
    # ask.input (STD-ISA-101, DEC-119).
    V('STD-ISA-001', 'the descriptors of ABI 1.0 are normative (since 1.0-rc9)', prototype=False,
      steps=[{'op': 'instructions', 'input': {}, 'frozen': 'ask.input joins the catalogue in BPA 1.0-rc12 '
                                                            '(STD-ISA-101, DEC-119)'}])
    P('STD-ISA-001', 'the descriptors of ABI 1.0 are normative (since 1.0-rc12)', 'instructions', {})

    # ---- STD-ISA-002 ABI versioning ----
    for abi, err in (('1.0', None), ('1.1', 'LOGIC.ABI_UNSUPPORTED'), ('2.0', 'LOGIC.ABI_UNSUPPORTED'),
                     ('0.9', 'LOGIC.ABI_UNSUPPORTED'), ('one', 'MODEL.INVALID_VALUE')):
        S('STD-ISA-002', 'an asset declaring ABI %s' % abi, deploy(asset(90, 'acme.app', prog({'return': '1'}),
                                                                         abi=abi)) +
          [ex('acme.app', 'main', error=err)])

    # ---- STD-ISA-004 metering units ----
    run1('STD-ISA-004', 'commit.create charges 50 units plus one per field', [
        {'do': 'commit.create', 'args': {'class': '"Customer"', 'fields': {'name': '"X"', 'email': '"a@b.c"',
                                                                           'country': '"US"'}}}],
         extra=[{'op': 'metering', 'input': {'process_id': uid(501)}}])
    run1('STD-ISA-004', 'read.get charges 10 units plus one per KiB', [
        {'do': 'read.get', 'args': {'target': '"ubos://acme/Account/a-1"'}}], data=[acct],
         extra=[{'op': 'metering', 'input': {'process_id': uid(501)}}])
    run1('STD-ISA-004', 'instruction costs count against the budget', [
        {'do': 'commit.create', 'args': {'class': '"Customer"', 'fields': {'name': '"X"'}}}],
         limits={'units': 40}, error='LOGIC.LIMIT_EXCEEDED')

    # ---- STD-ISA-005 authorization ----
    mk = [{'do': 'commit.create', 'args': {'class': '"Customer"', 'fields': {'name': '"X"'}}}, {'return': '1'}]
    run1('STD-ISA-005', 'an instruction without a grant is denied', mk, grants=['read'], error='IAM.DENIED')
    run1('STD-ISA-005', 'a grant for the class allows the instruction', mk, grants=['create:Customer'])
    run1('STD-ISA-005', 'a grant for another class does not', mk, grants=['create:Account'], error='IAM.DENIED')
    run1('STD-ISA-005', 'an object that cannot be read is not found', [
        {'do': 'read.get', 'args': {'target': '"ubos://acme/Account/a-1"'}}], grants=['create'], data=[acct],
         error='IAM.NOT_FOUND')
    run1('STD-ISA-005', 'a caught denial is not a success', [
        {'try': [{'do': 'commit.create', 'args': {'class': '"Customer"', 'fields': {'name': '"X"'}}, 'as': 'c'}],
         'catch': [{'return': 'e.code'}], 'as': 'e'}], grants=[], extra=[{'op': 'commit_info', 'input': {'branch': APP}}])
    S('STD-ISA-005', 'running a port needs its permission', deploy(ACCT, data=[acct]) + [
        {'op': 'execute', 'input': {'branch': APP, 'target': 'ubos://acme/Account/a-1', 'port': 'rename',
                                    'params': {'name': 'X'}, 'grants': ['read', 'update'],
                                    'process': {'process_id': uid(501)}, 'entropy_seed': SEED},
         'error': 'IAM.DENIED'}])

    # ---- STD-ISA-006 errors ----
    run1('STD-ISA-006', 'business errors can be caught', [
        {'try': [{'do': 'read.get', 'args': {'target': '"ubos://acme/Account/none"'}}],
         'catch': [{'return': 'e.code'}], 'as': 'e'}])
    run1('STD-ISA-006', 'logic raises registered business errors', [
        {'try': [{'fail': 'RULE.INVARIANT_VIOLATED', 'message': 'limit'}], 'catch': [{'return': 'e.code'}],
         'as': 'e'}])
    run1('STD-ISA-006', 'an uncaught business error fails the process', [{'fail': 'RULE.INVARIANT_VIOLATED'}],
         error='RULE.INVARIANT_VIOLATED')
    run1('STD-ISA-006', 'unregistered codes cannot be raised', [{'fail': 'ACME.OOPS'}], error='MODEL.INVALID_VALUE')
    run1('STD-ISA-006', 'non-catchable errors terminate', [
        {'try': [{'let': 'ctx', 'value': '1'}], 'catch': [{'return': '"caught"'}]}],
         error='LOGIC.CONTEXT_IMMUTABLE')

    # ---- STD-ISA-007 tier neutrality ----
    S('STD-ISA-007', 'an instruction fails with the same code as the host adapter operation', deploy(
        asset(90, 'acme.app', prog({'do': 'commit.create', 'args': {'class': '"Party"', 'fields': {'name': '"X"'}}}))) + [
        ex('acme.app', 'main', error='MODEL.ABSTRACT_INSTANTIATION'),
        commit([create(uid(3), 'Party', 'x', {'name': 'X'})], branch=APP, error='MODEL.ABSTRACT_INSTANTIATION')])
    S('STD-ISA-007', 'an update gives the same body as the host adapter operation', deploy(
        asset(90, 'acme.app', prog({'do': 'read.get', 'args': {'target': '"ubos://acme/Account/a-1"'}},
                                   {'do': 'commit.update', 'args': {'target': '"ubos://acme/Account/a-1"',
                                                                    'set': {'balance': '"250"'}}})),
        data=[acct]) + [ex('acme.app', 'main'), {'op': 'read', 'input': {'branch': APP, 'object_id': uid(1)}}])

    # ---- STD-ISA-010 commit.create ----
    run1('STD-ISA-010', 'create returns the new object identifier', [
        {'do': 'commit.create', 'args': {'class': '"Customer"', 'slug': '"c-9"', 'fields': {'name': '"Nine"'}}, 'as': 'c'},
        {'return': 'c'}], extra=[{'op': 'list_objects', 'input': {'branch': APP, 'class': 'Customer'}}])
    run1('STD-ISA-010', 'the slug defaults to the object identifier', [
        {'do': 'commit.create', 'args': {'class': '"Customer"', 'fields': {'name': '"X"'}}, 'as': 'c'},
        {'do': 'read.get', 'args': {'target': 'c.object_id'}, 'as': 'r'},
        {'return': 'r.fields'}])
    run1('STD-ISA-010', 'validation is complete before commit', [
        {'do': 'commit.create', 'args': {'class': '"Customer"', 'fields': {'email': '"x@y.z"'}}}],
         error='MODEL.CONSTRAINT_VIOLATED')
    run1('STD-ISA-010', 'the object identifier is given by import processes only', [
        {'do': 'commit.create', 'args': {'class': '"Customer"', 'fields': {'name': '"X"'},
                                         'object_id': '"%s"' % uid(9)}}], error='MODEL.INVALID_VALUE')
    S('STD-ISA-010', 'change_set classes are not created directly on main', [
        commit([asset(90, 'acme.app', prog({'do': 'commit.create', 'args': {'class': '"TaxRule"',
                                                                            'fields': {'rate': '0.2'}}}))], label='d'),
        {'op': 'verify_asset', 'input': {'branch': 'main', 'object_id': uid(90)}},
        ex('acme.app', 'main', branch='main', error='VER.CHANGE_SET_REQUIRED')])
    run1('STD-ISA-010', 'values written by logic carry a derivation record', [
        {'do': 'read.get', 'args': {'target': '"ubos://acme/Account/a-1"'}},
        {'do': 'commit.create', 'args': {'class': '"Customer"', 'slug': '"c-9"', 'fields': {'name': '"Nine"'}}}],
         data=[acct], extra=[{'op': 'list_objects', 'input': {'branch': APP, 'class': 'Customer'}, 'label': 'l'},
                             {'op': 'derivation', 'input': {'branch': APP, 'object_id': {'$ref': 'l/objects/0'}},
                              'label': 'dv', 'check': False},
                             {'op': 'read', 'input': {'branch': APP, 'object_id': uid(90)}, 'label': 'a'},
                             {'op': 'normalize_text', 'input': {'text': {'$ref': 'dv/logic/0'}},
                              'same_as': 'a/version_hash'},
                             {'op': 'normalize_text', 'input': {'text': {'$ref': 'dv/inputs/0/object'}}}])

    # ---- STD-ISA-011…016 commit.update, end, move, append, reverse, adjust, link ----
    rd = {'do': 'read.get', 'args': {'target': '"ubos://acme/Account/a-1"'}, 'as': 'r'}
    run1('STD-ISA-011', 'update sets fields and null clears one', [rd, {'do': 'commit.update', 'args': {
        'target': 'r.object_id', 'set': {'balance': '"120"', 'status': 'null'}}}], data=[acct],
         extra=[{'op': 'read', 'input': {'branch': APP, 'object_id': uid(1)}}])
    run1('STD-ISA-011', 'the base defaults to the version read in the process', [rd, {'do': 'commit.update', 'args': {
        'target': '"ubos://acme/Account/a-1"', 'set': {'balance': '"1"'}}}], data=[acct])
    run1('STD-ISA-011', 'an object that was not read needs a base', [{'do': 'commit.update', 'args': {
        'target': '"ubos://acme/Account/a-1"', 'set': {'balance': '"1"'}}}], data=[acct],
         error='MODEL.DECLARATION_INCOMPLETE')
    run1('STD-ISA-011', 'a timeline operation dates the change', [rd, {'do': 'commit.update', 'args': {
        'target': 'r.object_id', 'set': {'balance': '"500"'},
        'timeline': {'op': '"supersede_from"', 'from': '"2026-06-01"'}}}], data=[acct],
         extra=[{'op': 'read_range', 'input': {'branch': APP, 'object_id': uid(1), 'from': '2026-01-01'}}])
    run1('STD-ISA-011', 'updates of an object created in the process are merged into the creation', [
        {'do': 'commit.create', 'args': {'class': '"Customer"', 'slug': '"c-9"', 'fields': {'name': '"X"'}}, 'as': 'c'},
        {'do': 'commit.update', 'args': {'target': 'c.object_id', 'set': {'email': '"x@y.z"'}}},
        {'do': 'read.get', 'args': {'target': '"ubos://acme/Customer/c-9"'}, 'as': 'r'}, {'return': 'r.fields'}])
    run1('STD-ISA-012', 'end closes the object from a date', [rd, {'do': 'commit.end', 'args': {
        'target': 'r.object_id', 'from': '"2026-04-01"'}}], data=[acct],
         extra=[{'op': 'read', 'input': {'branch': APP, 'object_id': uid(1), 'asof': '2026-04-01'},
                 'error': 'QRY.NOT_FOUND_AT_TIME'}])
    run1('STD-ISA-013', 'move changes the slug', [rd, {'do': 'commit.move', 'args': {'target': 'r.object_id',
                                                                                    'slug': '"acme"'}}],
         data=[acct], extra=[{'op': 'resolve', 'input': {'uri': 'ubos://acme/Account/acme?branch=feature/app'}}])
    run1('STD-ISA-014', 'append adds an entry with source references', [
        {'do': 'commit.append', 'args': {'class': '"CashMove"', 'ledger_key': {'account': '"ubos://acme/Account/a-1"'},
                                         'fields': {'account': '"ubos://acme/Account/a-1"', 'amount': '"25"'},
                                         'source_refs': '["ubos://acme/Account/a-1"]'}, 'as': 'e'},
        {'return': 'e'}], data=[acct],
         extra=[{'op': 'read_ledger', 'input': {'branch': APP, 'class': 'CashMove',
                                                'ledger_key': {'account': 'ubos://acme/Account/a-1'}}}])
    run1('STD-ISA-015', 'reverse and adjust append corrections', [
        {'do': 'commit.reverse', 'args': {'entry': '"%s"' % uid(20), 'reason': '"wrong"'}, 'as': 'r'},
        {'do': 'commit.adjust', 'args': {'corrects': '["%s"]' % uid(21), 'reason': '"fix"',
                                         'fields': {'item': '"nut"', 'quantity': '"2"'}}, 'as': 'a'},
        {'return': '[r, a]'}], data=[stock(uid(20), 'bolt', '5'), stock(uid(21), 'nut', '1')],
         extra=[{'op': 'read_ledger', 'input': {'branch': APP, 'class': 'StockMove', 'ledger_key': {'item': 'bolt'}}}])
    run1('STD-ISA-015', 'an adjustment corrects one entry', [
        {'do': 'commit.adjust', 'args': {'corrects': '["%s", "%s"]' % (uid(20), uid(21)), 'reason': '"fix"',
                                         'fields': {'item': '"nut"', 'quantity': '"2"'}}}],
         data=[stock(uid(20), 'bolt', '5'), stock(uid(21), 'nut', '1')], error='MODEL.INVALID_VALUE')
    team = create(uid(5), 'Team', 't-1', {'name': 'Ops'})
    run1('STD-ISA-016', 'link and unlink change a relationship', [
        {'do': 'commit.link', 'args': {'from': '"ubos://acme/Team/t-1"', 'relationship': '"members"',
                                       'to': '"ubos://acme/Account/a-1"'}},
        {'do': 'commit.link', 'args': {'from': '"ubos://acme/Team/t-1"', 'relationship': '"members"',
                                       'to': '"ubos://acme/PremiumAccount/p-1"'}},
        {'do': 'commit.unlink', 'args': {'from': '"ubos://acme/Team/t-1"', 'relationship': '"members"',
                                         'to': '"ubos://acme/Account/a-1"'}}], data=[acct, prem, team],
         extra=[{'op': 'read', 'input': {'branch': APP, 'object_id': uid(5)}}])
    run1('STD-ISA-016', 'link needs a relationship field', [
        {'do': 'commit.link', 'args': {'from': '"ubos://acme/Team/t-1"', 'relationship': '"name"',
                                       'to': '"ubos://acme/Account/a-1"'}}], data=[acct, team],
         error='MODEL.REFERENCE_INVALID')

    # ---- STD-ISA-030…034 read.* ----
    run1('STD-ISA-030', 'read.get returns fields, validity and where it was resolved', [
        {'do': 'read.get', 'args': {'target': '"ubos://acme/Account/a-1"', 'fields': '["name"]'}, 'as': 'r'},
        {'return': '[r.object_id, r.class, r.fields, r.valid, r.resolved_from.branch]'}], data=[acct])
    S('STD-ISA-030', 'read.get at explicit coordinates', deploy(
        asset(90, 'acme.app', prog({'do': 'read.get', 'args': {'target': '"ubos://acme/Account/a-1?tag=v1"'},
                                    'as': 'r'}, {'return': 'r.fields.name'})), data=[acct]) + [
        {'op': 'tag', 'input': {'name': 'v1', 'branch': APP}},
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('d', uid(1)), 'body': {'name': 'Acme 2'}}],
               branch=APP),
        ex('acme.app', 'main')])
    run1('STD-ISA-030', 'an unknown object is not found', [
        {'do': 'read.get', 'args': {'target': '"ubos://acme/Account/none"'}}], error='IAM.NOT_FOUND')
    run1('STD-ISA-030', 'an object outside the valid time of the process is not found at that time', [
        {'do': 'read.get', 'args': {'target': '"ubos://acme/Account/a-1"'}}], data=[acct], ctx={'asof': '2026-01-01'},
         error='QRY.NOT_FOUND_AT_TIME')
    run1('STD-ISA-031', 'read.history lists the commits that changed the object', [
        {'do': 'read.history', 'args': {'target': '"ubos://acme/Account/a-1"'}, 'as': 'h'},
        {'return': 'map(h.commits, c -> c.ops)'}], data=[acct])
    run1('STD-ISA-032', 'read.timeline returns the segments', [
        {'do': 'read.timeline', 'args': {'target': '"ubos://acme/Account/a-1"'}, 'as': 't'}, {'return': 't'}],
         data=[acct])
    run1('STD-ISA-033', 'read.diff compares two coordinates of an object', [
        {'do': 'read.diff', 'args': {'scope': '"ubos://acme/Account/a-1"', 'a': {'time': '0'}, 'b': {'time': '1'}},
         'as': 'd'}, {'return': 'd'}], data=[acct])
    run1('STD-ISA-034', 'read.effective returns the effective class', [
        {'do': 'read.effective', 'args': {'what': '"class"', 'target': '"PremiumAccount"'}, 'as': 'c'},
        {'return': 'c.linearization'}])
    run1('STD-ISA-034', 'effective declarations come with the Governance profile', [
        {'do': 'read.effective', 'args': {'what': '"declarations"', 'target': '"Account"'}}],
         error='LOGIC.NOT_AVAILABLE', frozen='effective declarations are available since BPA 1.0-rc2 (DEC-067)')

    # ---- STD-ISA-050…055 run.* ----
    S('STD-ISA-050', 'run.port executes the implementation with this bound to the target', deploy(
        ACCT, data=[acct]) + [
        {'op': 'execute', 'input': {'branch': APP, 'target': 'ubos://acme/Account/a-1', 'port': 'rename',
                                    'params': {'name': 'Acme Ltd'},
                                    'process': {'process_id': uid(501)}, 'entropy_seed': SEED}},
        {'op': 'read', 'input': {'branch': APP, 'object_id': uid(1)}}])
    S('STD-ISA-050', 'a port without an implementation is not implemented', deploy(ACCT, data=[acct]) + [
        {'op': 'execute', 'input': {'branch': APP, 'target': 'ubos://acme/Account/a-1', 'port': 'freeze',
                                    'params': {}, 'process': {'process_id': uid(501)}},
         'error': 'MODEL.PORT_NOT_IMPLEMENTED'}])
    S('STD-ISA-050', 'ports run in the unit of work of the caller', deploy(
        ACCT, asset(90, 'acme.app', prog({'do': 'run.port', 'args': {'target': '"ubos://acme/Account/a-1"',
                                                                     'port': '"close"', 'params': {}}},
                                         {'fail': 'RULE.INVARIANT_VIOLATED'})), data=[acct]) + [
        ex('acme.app', 'main', error='RULE.INVARIANT_VIOLATED'),
        {'op': 'read', 'input': {'branch': APP, 'object_id': uid(1)}}, {'op': 'outbox', 'input': {}}])
    S('STD-ISA-050', 'events emitted by a port are released after commit', deploy(ACCT, data=[acct]) + [
        {'op': 'execute', 'input': {'branch': APP, 'target': 'ubos://acme/Account/a-1', 'port': 'close',
                                    'params': {}, 'process': {'process_id': uid(501)}, 'entropy_seed': SEED}},
        {'op': 'outbox', 'input': {}}])
    S('STD-ISA-051', 'run.super runs the next implementation in the linearization', deploy(
        ACCT, PREM, data=[prem]) + [
        {'op': 'execute', 'input': {'branch': APP, 'target': 'ubos://acme/PremiumAccount/p-1', 'port': 'rename',
                                    'params': {'name': 'Gamma'}, 'process': {'process_id': uid(501)},
                                    'entropy_seed': SEED}},
        {'op': 'read', 'input': {'branch': APP, 'object_id': uid(2)}}])
    S('STD-ISA-051', 'run.super outside a port has no next implementation', deploy(
        asset(90, 'acme.app', prog({'do': 'run.super', 'args': {}}))) + [
        ex('acme.app', 'main', error='MODEL.PORT_NOT_IMPLEMENTED')])
    lib = asset(91, 'acme.lib', {'double': fn([{'return': 'params.x * 2'}], ['x'])})
    S('STD-ISA-052', 'run.logic calls the pinned version of an imported library', deploy(lib) + [
        commit([asset(90, 'acme.app', prog({'do': 'run.logic', 'args': {'asset': '"acme.lib"', 'function': '"double"',
                                                                         'args': '[21]'}, 'as': 'x'},
                                           {'return': 'x'}),
                      imports=[{'name': 'acme.lib', 'version': {'$ref': 'd/versions/' + uid(91)}}])], branch=APP),
        ex('acme.app', 'main')])
    S('STD-ISA-052', 'run.logic needs the library among the imports', deploy(lib) + [
        commit([asset(90, 'acme.app', prog({'do': 'run.logic', 'args': {'asset': '"acme.lib"', 'function': '"double"',
                                                                         'args': '[21]'}}))], branch=APP),
        ex('acme.app', 'main', error='LOGIC.FORBIDDEN_CAPABILITY')])
    run1('STD-ISA-055', 'run.async requests a job through the outbox', [
        {'do': 'run.async', 'args': {'target': '"ubos://acme/Account/a-1"', 'port': '"close"', 'params': {}},
         'as': 'j'}, {'return': 'j'}], data=[acct], extra=[{'op': 'outbox', 'input': {}}])

    # ---- STD-BPU-001 the seven elements ----
    S('STD-BPU-001', 'the state of a BPU instance includes the entries that reference it', deploy(
        data=[acct, {'op': 'append', 'class': 'CashMove', 'entry_id': uid(30),
                     'ledger_key': {'account': 'ubos://acme/Account/a-1'},
                     'body': {'account': 'ubos://acme/Account/a-1', 'amount': '10'},
                     'source_refs': ['ubos://acme/Account/a-1']}]) + [
        {'op': 'bpu_state', 'input': {'branch': APP, 'object_id': uid(1)}}])
    S('STD-BPU-001', 'a ledger class is not a BPU', [
        {'op': 'bpu_descriptor', 'input': {'class': 'CashMove'}, 'error': 'MODEL.INVALID_VALUE'}])
    S('STD-BPU-001', 'an abstract class is not a BPU', [
        {'op': 'bpu_descriptor', 'input': {'class': 'DataObject'}, 'error': 'MODEL.REFERENCE_INVALID'}])
    S('STD-BPU-001', 'the elements of a BPU class', [{'op': 'bpu_descriptor', 'input': {'class': 'Account'}}])

    # ---- STD-BPU-002 descriptor ----
    S('STD-BPU-002', 'the descriptor of a subclass shows origins, ports and ledgers', [
        {'op': 'bpu_descriptor', 'input': {'class': 'PremiumAccount'}}])
    S('STD-BPU-002', 'the descriptor is a pure function of the effective definitions', [
        {'op': 'bpu_descriptor', 'input': {'class': 'Account'}, 'label': 'a'},
        {'op': 'create_branch', 'input': {'name': 'feature/x', 'from': 'main'}},
        {'op': 'bpu_descriptor', 'input': {'branch': 'feature/x', 'class': 'Account'}, 'same_as': 'a'}])
    S('STD-BPU-002', 'logic reads the same descriptor', deploy(asset(90, 'acme.app', prog(
        {'do': 'read.effective', 'args': {'what': '"descriptor"', 'target': '"Account"'}, 'as': 'd'},
        {'return': 'd'}))) + [
        ex('acme.app', 'main', label='x'),
        {'op': 'bpu_descriptor', 'input': {'branch': APP, 'class': 'Account'}, 'same_as': 'x/result'}])
    S('STD-BPU-002', 'extensions on a branch change its descriptor', [
        {'op': 'create_branch', 'input': {'name': 'feature/x', 'from': 'main'}},
        commit([create(uid(60), 'ClassExtension', 'x-account', {
            'target': 'Account', 'namespace': 'acme.crm', 'order': 1,
            'fields': [field('segment', 'text')]})], branch='feature/x'),
        {'op': 'bpu_descriptor', 'input': {'branch': 'feature/x', 'class': 'Account'}}])

    # ---- STD-BPU-004 commit cycle ----
    S('STD-BPU-004', 'a failed process changes nothing', deploy(asset(90, 'acme.app', prog(
        {'do': 'read.get', 'args': {'target': '"ubos://acme/Account/a-1"'}},
        {'do': 'commit.update', 'args': {'target': '"ubos://acme/Account/a-1"', 'set': {'balance': '"0"'}}},
        {'fail': 'RULE.INVARIANT_VIOLATED'})), data=[acct]) + [
        ex('acme.app', 'main', error='RULE.INVARIANT_VIOLATED'),
        {'op': 'read', 'input': {'branch': APP, 'object_id': uid(1)}}])
    S('STD-BPU-004', 'state is constant between two commits', deploy(data=[acct]) + [
        {'op': 'set_now', 'input': {'now': '2026-03-15T11:00:00.000000Z'}},
        commit([{'op': 'update', 'object_id': uid(1), 'base': base('d', uid(1)), 'body': {'balance': '1'}}],
               branch=APP),
        {'op': 'read', 'input': {'branch': APP, 'object_id': uid(1), 'tx_time': '2026-03-15T10:30:00.000000Z'},
         'label': 'a'},
        {'op': 'read', 'input': {'branch': APP, 'object_id': uid(1), 'tx_time': '2026-03-15T10:59:59.999999Z'},
         'same_as': 'a'}])
    S('STD-BPU-004', 'all changes of a process become visible in one commit', deploy(asset(90, 'acme.app', prog(
        {'do': 'commit.create', 'args': {'class': '"Customer"', 'slug': '"c-8"', 'fields': {'name': '"A"'}}},
        {'do': 'commit.create', 'args': {'class': '"Customer"', 'slug': '"c-9"', 'fields': {'name': '"B"'}}}))) + [
        ex('acme.app', 'main'), {'op': 'list_objects', 'input': {'branch': APP, 'class': 'Customer', 'time': 1}},
        {'op': 'list_objects', 'input': {'branch': APP, 'class': 'Customer', 'time': 2}}])

    # ---- STD-BPU-010 hardware-ization properties ----
    S('STD-BPU-010', 'determinism: equal inputs and versions give equal outputs', deploy(asset(90, 'acme.app', prog(
        {'do': 'sys.uuid', 'as': 'u'}, {'return': '[u, ctx.asof]'}))) + [
        ex('acme.app', 'main'), ex('acme.app', 'main', pid=2)])
    run1('STD-BPU-010', 'fixed instruction set: logic affects state only through instructions', [
        {'do': 'commit.write_raw', 'args': {}}], caps=('commit',), error='LOGIC.FORBIDDEN_CAPABILITY')
    run1('STD-BPU-010', 'power and timing budgets: every execution is metered', [{'return': '1'}],
         extra=[{'op': 'metering', 'input': {'process_id': uid(501)}}])

    # ---- STD-BPU-011 BPU class verification ----
    S('STD-BPU-011', 'unverified port implementations fail the class verification', deploy(ACCT) + [
        {'op': 'verify_bpu', 'input': {'branch': APP, 'class': 'Account'}}])
    S('STD-BPU-011', 'verified implementations and emitted events pass', [
        commit([ACCT], label='d'), {'op': 'verify_asset', 'input': {'branch': 'main', 'object_id': uid(80)}},
        {'op': 'verify_bpu', 'input': {'branch': 'main', 'class': 'Account'}}])
    S('STD-BPU-011', 'a declared event that no port emits fails', [
        commit([ACCT, PREM], label='d'),
        {'op': 'verify_asset', 'input': {'branch': 'main', 'object_id': uid(80)}},
        {'op': 'verify_asset', 'input': {'branch': 'main', 'object_id': uid(81)}},
        {'op': 'verify_bpu', 'input': {'branch': 'main', 'class': 'PremiumAccount'}}])
    V('STD-BPU-011', 'undocumented members fail the class verification', prototype=False, setup=[
        {'op': 'set_now', 'input': {'now': NOW}},
        {'op': 'load_genesis', 'input': {'classes': CLASSES + [cls('Memo', [dict(field('text', 'text'),
                                                                                 description='')],
                                                                   description='')]}}],
      steps=[{'op': 'verify_bpu', 'input': {'class': 'Memo'}}])

    # ---- STD-TXN-023 automatic retry ----
    retry_body = [{'do': 'read.get', 'args': {'target': '"ubos://acme/Account/a-1"'}, 'as': 'r'},
                  {'do': 'sys.uuid', 'as': 'u'},
                  {'do': 'commit.update', 'args': {'target': 'r.object_id', 'set': {'balance': '"0"'}}},
                  {'return': '[u, r.fields.balance]'}]
    interfere = [{'attempt': 1, 'changes': [{'op': 'update', 'object_id': uid(1), 'base': base('d', uid(1)),
                                             'body': {'balance': '7'}}]}]
    S('STD-TXN-023', 'a retryable process is re-executed after its base moved', deploy(
        asset(90, 'acme.app', {'main': fn(retry_body, retryable=True)}), data=[acct]) + [
        ex('acme.app', 'main', interfere=interfere), {'op': 'journal', 'input': {'process_id': uid(501)}}])
    S('STD-TXN-023', 'a process that is not retryable fails', deploy(
        asset(90, 'acme.app', {'main': fn(retry_body)}), data=[acct]) + [
        ex('acme.app', 'main', interfere=interfere, error='TXN.HEAD_MOVED')])
    S('STD-TXN-023', 'retries stop at the configured maximum', deploy(
        asset(90, 'acme.app', {'main': fn(retry_body, retryable=True)}), data=[acct]) + [
        ex('acme.app', 'main', max_attempts=1, interfere=interfere, error='TXN.HEAD_MOVED')])
    S('STD-TXN-023', 'a serializable process is retried after its read set changed', deploy(
        asset(90, 'acme.app', {'main': fn([{'do': 'read.get', 'args': {'target': '"ubos://acme/Account/a-1"'}},
                                           {'do': 'commit.create', 'args': {'class': '"Customer"',
                                                                            'fields': {'name': '"X"'}}},
                                           {'return': '1'}], retryable=True)}), data=[acct]) + [
        ex('acme.app', 'main', serializable=True, interfere=interfere)])


def nested(n):
    body = [{'return': '1'}]
    for _ in range(n):
        body = [{'if': 'true', 'then': body}]
    return body
