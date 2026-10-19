"""Execution of logic in the reference store (PH-1 step 1e-2, DEC-044): the execution context
(STD-CTX-001, -002), the capability gateway (STD-CTX-003), profiles, metering and termination
(STD-CTX-004…006), determinism, journal and replay (STD-CTX-010…012), logic assets and their
verification (STD-CTX-020…024), version pinning (STD-CTX-030), the instructions of ABI 1.0
(STD-ISA-*), automatic retry (STD-TXN-023) and the BPU views (STD-BPU-*).

Logic runs as host programs, the in-process host binding of STD-ISA-007. A program is JSON:
{"functions": {name: {"params": [..], "body": [statement, ..], "retryable": bool}}}, and a
statement is one of
  {"let": name, "value": L1}            {"do": "family.op", "args": template, "as": name}
  {"if": L1, "then": [..], "else": [..]} {"each": name, "in": L1, "body": [..]}
  {"repeat": L1, "body": [..]}          {"try": [..], "catch": [..], "as": name}
  {"return": L1}                        {"fail": code, "message": text}
A template is JSON whose strings are L1 expressions. A port function sees the identifier of its
object as `self_id`."""
import copy
import hashlib
import re

from . import BpaError, expr, isa, addr, crypto
from .flow import guard_ok, eval_value, findings as flow_findings
from .canon import canonical, content_id
from .classes import dispatch, effective_class
from .values import GRAMMARS

TIERS = ('l2', 'l3', 'host')
STATES = ('draft', 'committed', 'verified', 'deprecated', 'retired')
STATEMENTS = ('let', 'do', 'if', 'each', 'repeat', 'try', 'return', 'fail')
ROOTS = ('this', 'ctx', 'params', 'old')
IDENTITY = ('tenant', 'vae', 'principal', 'on_behalf_of')          # STD-CTX-002
RETRY_CODES = ('TXN.HEAD_MOVED', 'TXN.READ_SET_CHANGED')              # STD-TXN-023
MAX_ATTEMPTS = 5
ANALYZER = '1.0.0'
MAX_SOURCE = 64 * 1024
MAX_NESTING = 32          # nesting in functions (STD-CTX-004, L2 expression depth in functions)
TZDB = '2025b'
IMPLEMENTED = ('commit.create', 'commit.update', 'commit.end', 'commit.move', 'commit.append', 'commit.reverse',
               'commit.adjust', 'commit.link', 'commit.unlink', 'read.get', 'read.history', 'read.timeline',
               'read.diff', 'read.effective', 'run.port', 'run.super', 'run.logic', 'run.async', 'emit.event',
               # BPA 1.0-rc2, PH-1 step 10c (DEC-067)
               'read.explain', 'run.validate', 'search.query',
               # BPA 1.0-rc3, PH-1 step 12a (DEC-071)
               'commit.transition', 'read.actions')
SYSTEM = {'sys.uuid': 'pure', 'sys.random_bytes': 'pure', 'adapter.observe': 'journaled'}
READ_ACTIONS = ('read', 'read_history', 'reflect', 'explain')
L2_FORBIDDEN = ('eval', 'print', 'debug', 'import', 'export', 'Fn', 'call', 'curry', 'is_def_fn', 'is_def_var',
                'sleep', 'exit', 'parse_json')
L3_WORLD = ('ubos:isa/v1', 'ubos:types/v1', 'wasi:random')
PROTECTED = ('main', 'release', 'overlay')


def _abi(v):
    m = re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', v) if isinstance(v, str) else None
    if not m:
        raise BpaError('MODEL.INVALID_VALUE', 'an ABI version is MAJOR.MINOR')
    return int(m.group(1)), int(m.group(2))


def code_identity(body):
    return content_id('ubos.chunk.v1', {k: body.get(k) for k in ('tier', 'abi', 'source', 'exports', 'imports',
                                                                  'capabilities')})


class Frame:
    def __init__(self, asset, vh, fname, this=None, cls=None, port=None, impl_class=None, params=None, oid=None):
        self.asset, self.vh, self.fname, self.oid = asset, vh, fname, oid
        self.this, self.cls, self.port, self.impl_class, self.params = this, cls, port, impl_class, params


class Run:
    """One attempt of one process."""

    def __init__(self, store, i, p, seed, replay=None):
        self.s, self.i, self.p = store, i, p
        self.branch = i.get('branch', 'main')
        profile = (i.get('ctx') or {}).get('profile', 'standard')
        if profile not in isa.PROFILE_LIMITS:
            raise BpaError('MODEL.INVALID_VALUE', 'unknown profile')
        self.profile = profile
        self.limits = dict(isa.PROFILE_LIMITS[profile])
        for k, v in (i.get('limits') or {}).items():
            if k not in self.limits or not isinstance(v, int) or v < 0:
                raise BpaError('MODEL.INVALID_VALUE', 'unknown limit %s' % k)
            if v > self.limits[k]:
                raise BpaError('IAM.DENIED', 'raising a limit needs an entitlement')
            self.limits[k] = v
        self.grants = i.get('grants')
        self.world = i.get('world') or {}
        self.seed = seed
        self.replay = replay
        self.units = self.calls = self.entropy_n = 0
        self.families = {}
        self.uow, self.pending, self.reads, self.triples = [], {}, {}, []
        self.effects, self.records, self.pins, self.pinned, self.logic = [], [], {}, {}, []
        if replay is not None:
            # STD-CTX-012: a replay reads the snapshots the journal recorded (since 1.0-rc4)
            self.pins = dict(replay['journal'].get('snapshots') or {})
        self.frames = []
        # STD-ISA-017 (1.0-rc3): the transitions of this process by object and the guards passed
        self.transitions, self.guards = {}, []
        self.cancel_after = i.get('cancel_after_units')
        self.context = self._context()

    # ---- context (STD-CTX-001) ----

    def _context(self):
        c = self.i.get('ctx') or {}
        self.s._branch(self.branch)
        ctx = {'principal': c.get('principal', self.p['initiator']), 'principal_kind': c.get('principal_kind', 'human'),
               'auth_level': c.get('auth_level', 'aal1'), 'tenant': self.s.TENANT, 'vae': self.s.TENANT,
               'branch': self.branch, 'asof': c.get('asof') or self.s._today(), 'time': 'head',
               'now': self.s.now, 'locale': c.get('locale', 'en-US'), 'time_zone': c.get('time_zone', 'UTC'),
               'process_id': self.p['process_id'], 'trigger': self.p['trigger'], 'channel': c.get('channel', 'api'),
               'profile': self.profile,
               'budget_remaining': {'units': self.limits['units'], 'instruction_calls': self.limits['instruction_calls']},
               'replay': self.replay is not None}
        if c.get('on_behalf_of'):
            ctx['on_behalf_of'] = c['on_behalf_of']
        if c.get('input') is not None:
            ctx['input'] = c['input']
        if c.get('role') is not None:
            ctx['role'] = c['role']
        for k in c:
            if k not in ('principal', 'principal_kind', 'auth_level', 'asof', 'locale', 'time_zone', 'channel',
                         'profile', 'on_behalf_of', 'input', 'role'):
                raise BpaError('LOGIC.CONTEXT_IMMUTABLE', 'the implementation sets %s' % k)
        return ctx

    def decision_ctx(self):
        """The context the decisions of STD-LGS-031 see (DEC-067)."""
        c = self.context
        out = {'principal_kind': c['principal_kind'], 'channel': c['channel'], 'vae': c['vae'],
               'branch_kind': self.s.branches[self.branch]['kind']}
        if c.get('role') is not None:
            out['role'] = c['role']
        return out

    def ctx_value(self):
        out = dict(self.context)
        out['now'] = expr.to_instant(out['now'])
        return out

    # ---- metering (STD-CTX-004…006) ----

    def charge(self, n):
        self.units += n
        if self.cancel_after is not None and self.units > self.cancel_after:
            raise BpaError('LOGIC.CANCELLED', 'cancelled')
        if self.units > self.limits['units']:
            raise BpaError('LOGIC.LIMIT_EXCEEDED', 'operation units')

    def metering(self, outcome):
        return {'process_id': self.p['process_id'], 'asset': self.logic[0] if self.logic else None,
                'units': self.units, 'cpu_us': 0, 'peak_memory': 0, 'wall_us': 0,
                'instructions': dict(sorted(self.families.items())), 'connector_calls': 0, 'outcome': outcome}

    # ---- entropy (STD-CTX-010) ----

    def random_bytes(self, n):
        out = b''
        while len(out) < n:
            out += hashlib.sha256(self.seed + self.entropy_n.to_bytes(8, 'big')).digest()
            self.entropy_n += 1
        return out[:n]

    def uuid(self):
        b = bytearray(self.random_bytes(16))
        b[6] = (b[6] & 0x0f) | 0x40
        b[8] = (b[8] & 0x3f) | 0x80
        h = b.hex()
        return '%s-%s-%s-%s-%s' % (h[:8], h[8:12], h[12:16], h[16:20], h[20:])

    # ---- snapshot (STD-TXN-002) ----

    def state(self):
        if self.branch not in self.pins:
            self.pins[self.branch] = self.s.commits[self.s.branches[self.branch]['head']]['commit_id']
        return self.s._state(self.branch, {'evaluated_at': self.pins})[0]

    # ---- L1 inside programs ----

    def eval(self, text, scope):
        if not isinstance(text, str):
            return text
        f = self.frames[-1]
        classes = list(self.s.decls.values())
        this_t = expr._class_type(classes, f.cls, self.s.value_types) if f.cls else None
        env = expr.Env(this=this_t, params=('obj', {k: 'any' for k in (f.params or {})}))
        for k in scope:
            env.vars[k] = 'any'
        ast = expr.parse(text)
        expr.check(ast, env)
        ev = expr.Evaluator(env, ctx=self.ctx_value(), max_units=self.limits['units'] - self.units,
                            this=expr.from_json(this_t, f.this) if this_t and f.this is not None else None,
                            params=f.params, typed_ref=self.typed_ref)
        ev.vars = dict(scope)
        try:
            v = ev.ev(ast)
        finally:
            self.charge(ev.units)
        return v

    def template(self, t, scope):
        if isinstance(t, dict):
            return {k: self.template(v, scope) for k, v in t.items()}
        if isinstance(t, list):
            return [self.template(v, scope) for v in t]
        if isinstance(t, str):
            return expr.to_json(self.eval(t, scope))
        return t

    def typed_ref(self, uri):
        oid = self.target_id(uri, missing_ok=True)
        if oid is None:
            return None
        if oid in self.pending:
            body = self.pending[oid]['body']
            cls = self.pending[oid]['class']
        else:
            seg = self.segment(oid)
            if seg is None:
                return None
            body, cls = self.s.versions[seg[2]]['body'], self.state()['objects'][oid]['class']
        return expr.from_json(expr._class_type(list(self.s.decls.values()), cls, self.s.value_types), body)

    # ---- targets ----

    def target_id(self, t, missing_ok=False):
        if isinstance(t, str) and GRAMMARS['uuid'].match(t):
            if t in self.pending or t in self.state()['objects']:
                return t
            if missing_ok:
                return None
            raise BpaError('IAM.NOT_FOUND', 'unknown object')
        if not isinstance(t, str) or not t.startswith('ubos://'):
            raise BpaError('MODEL.INVALID_VALUE', 'a target is a URI or an object id')
        p = addr.parse_uri(t)
        if p['tenant'] != self.s.TENANT or 'vae' in p:
            raise BpaError('IAM.NOT_FOUND', 'objects of other VAEs are reached only as published objects')
        for oid, o in self.pending.items():
            if o['class'] == p['class'] and o['slug'] == p['slug']:
                return oid
        st = self.state()
        for oid in sorted(st['objects']):
            o = st['objects'][oid]
            if o['class'] == p['class'] and o['slug'] == p['slug']:
                return oid
        if missing_ok:
            return None
        raise BpaError('IAM.NOT_FOUND', 'no object at %s' % p['canonical'])

    def segment(self, oid, asof=None):
        from .store import tl_at
        o = self.state()['objects'].get(oid)
        if o is None:
            return None
        return tl_at(o['timeline'], asof or self.context['asof'])

    def class_of(self, oid):
        if oid in self.pending:
            return self.pending[oid]['class']
        return self.state()['objects'][oid]['class']

    # ---- authorization (STD-ISA-005) ----

    def authorize(self, action, cls):
        if self.grants is None or action == 'none':
            return
        if action in self.grants or '%s:%s' % (action, cls) in self.grants:
            return
        if action in READ_ACTIONS:
            raise BpaError('IAM.NOT_FOUND', 'not visible')
        raise BpaError('IAM.DENIED', '%s on %s' % (action, cls))

    # ---- unit of work ----

    def add(self, change):
        if len(self.uow) + 1 > self.limits['changes']:
            raise BpaError('LOGIC.LIMIT_EXCEEDED', 'changes per commit')
        f = self.frames[-1]
        if change['op'] in ('create', 'update', 'append'):
            lm = getattr(self.s, '_logic_mode', None)
            change['derivation'] = {'logic': [f.vh], 'inputs': [dict(t) for t in self.triples],
                                    'mode': lm['mode'] if lm else 'as_effective'}
        self.uow.append(change)

    # ---- program execution ----

    def run_function(self, frame, args):
        if len(self.frames) + 1 > self.limits['depth']:
            raise BpaError('LOGIC.LIMIT_EXCEEDED', 'nesting depth')
        src = frame.asset.get('source') or {}
        fn = (src.get('functions') or {}).get(frame.fname)
        exported = [e.get('name') if isinstance(e, dict) else e for e in frame.asset.get('exports') or []]
        if fn is None or frame.fname not in exported:
            raise BpaError('LOGIC.FORBIDDEN_CAPABILITY', 'function %s is not exported' % frame.fname)
        names = fn.get('params') or []
        if isinstance(args, list):
            if len(args) > len(names):
                raise BpaError('MODEL.INVALID_VALUE', 'too many arguments')
            args = dict(zip(names, args))
        frame.params = {n: (args or {}).get(n) for n in names}
        if frame.port is not None and frame.params is not None:
            for k, v in (args or {}).items():
                frame.params.setdefault(k, v)
        frame.args = dict(frame.params)
        frame.params = {k: expr.from_json('any', v) for k, v in frame.params.items()}
        self.frames.append(frame)
        try:
            out = self.block(fn.get('body') or [], {'self_id': frame.oid} if frame.oid else {})
        finally:
            self.frames.pop()
        return out[1] if out else None

    def block(self, stmts, scope):
        for st in stmts:
            self.charge(1)
            kinds = [k for k in STATEMENTS if k in st] if isinstance(st, dict) else []
            if len(kinds) != 1:
                raise BpaError('MODEL.INVALID_VALUE', 'a statement has one kind')
            k = kinds[0]
            if k == 'let':
                name = st['let']
                if name in ROOTS:
                    raise BpaError('LOGIC.CONTEXT_IMMUTABLE', '%s cannot be assigned' % name)
                scope[name] = self.eval(st.get('value'), scope)
            elif k == 'do':
                res = self.instruction(st['do'], self.template(st.get('args') or {}, scope))
                if st.get('as'):
                    if st['as'] in ROOTS:
                        raise BpaError('LOGIC.CONTEXT_IMMUTABLE', '%s cannot be assigned' % st['as'])
                    scope[st['as']] = expr.from_json('any', res)
            elif k == 'if':
                c = self.eval(st['if'], scope)
                out = self.block(st.get('then') or [] if c is True else st.get('else') or [], scope)
                if out:
                    return out
            elif k == 'each':
                items = self.eval(st['in'], scope)
                if not isinstance(items, list):
                    raise BpaError('LOGIC.TYPE_ERROR', 'each needs a list')
                for x in items:
                    scope[st['each']] = x
                    out = self.block(st.get('body') or [], scope)
                    if out:
                        return out
            elif k == 'repeat':
                n = self.eval(st['repeat'], scope)
                for _ in range(n if isinstance(n, int) else 0):
                    out = self.block(st.get('body') or [], scope)
                    if out:
                        return out
            elif k == 'try':
                try:
                    out = self.block(st['try'], scope)
                except BpaError as e:
                    if not isa.catchable().get(e.code, False):
                        raise
                    if st.get('as'):
                        scope[st['as']] = {'code': e.code}
                    out = self.block(st.get('catch') or [], scope)
                if out:
                    return out
            elif k == 'return':
                return ('return', expr.to_json(self.eval(st['return'], scope)))
            elif k == 'fail':
                if st['fail'] not in isa.catchable():
                    raise BpaError('MODEL.INVALID_VALUE', 'unregistered error code')
                raise BpaError(st['fail'], st.get('message'))
        return None

    # ---- the gateway (STD-CTX-003) and instructions ----

    def instruction(self, name, args):
        self.calls += 1
        if self.calls > self.limits['instruction_calls']:
            raise BpaError('LOGIC.LIMIT_EXCEEDED', 'instruction calls')
        f = self.frames[-1]
        if name in SYSTEM:
            return self.system(name, args)
        d = isa.by_name().get(name)
        fam = name.split('.')[0]
        caps = f.asset.get('capabilities') or []
        if d is None or fam not in isa.PROFILE_FAMILIES[self.profile] or not (fam in caps or name in caps):
            raise BpaError('LOGIC.FORBIDDEN_CAPABILITY', name)
        if _abi(d['since']) > _abi(f.asset.get('abi', '1.0')):
            raise BpaError('LOGIC.FORBIDDEN_CAPABILITY', '%s is newer than the asset ABI' % name)
        if not isinstance(args, dict):
            raise BpaError('MODEL.INVALID_VALUE', 'arguments are a map')
        bad = [k for k in args if k in IDENTITY]
        if bad:
            raise BpaError('LOGIC.PARAMETER_FORBIDDEN', 'identity comes from the context (%s)' % bad[0])
        known = {p['name']: p for p in d['params']}
        if any(k not in known for k in args):
            raise BpaError('MODEL.INVALID_VALUE', 'unknown parameter of %s' % name)
        missing = [p['name'] for p in d['params'] if not p['optional'] and args.get(p['name']) is None
                   and not (name == 'commit.update' and p['name'] == 'set' and 'timeline' in args)]
        if missing and name not in ('run.async', 'run.validate'):     # run.validate: changes or target
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'missing parameter %s of %s' % (missing[0], name))
        self.charge(d['cost']['base'])
        if name not in IMPLEMENTED:
            raise BpaError('LOGIC.NOT_AVAILABLE', '%s is not available in BPA 1.0-rc1' % name)
        self.families[fam] = self.families.get(fam, 0) + 1
        res = getattr(self, 'i_' + name.replace('.', '_'))(args, d)
        return res

    def per_item(self, d, n):
        self.charge(d['cost']['per_item'] * n)

    def system(self, name, args):
        if name == 'sys.uuid':
            return self.uuid()
        if name == 'sys.random_bytes':
            n = args.get('n')
            if not isinstance(n, int) or not 0 < n <= 1024:
                raise BpaError('MODEL.INVALID_VALUE', 'n is 1 to 1024')
            return self.random_bytes(n).hex()
        # adapter.observe: a journaled value of the outside world (test instruction of the adapter)
        req = {'instruction': name, 'args_hash': content_id('ubos.chunk.v1', args)}
        seq = len(self.records) + 1
        if self.replay is not None:
            recs = self.replay['records']
            if seq > len(recs):
                raise BpaError('LOGIC.REPLAY_INPUT_MISSING', 'no journal record %d' % seq)
            r = recs[seq - 1]
            if (r['instruction'], r['args_hash']) != (req['instruction'], req['args_hash']):
                self.replay['divergence'] = self.replay.get('divergence') or {
                    'seq': seq, 'expected': r['instruction'], 'actual': name}
            val = r['result']
        else:
            if args.get('name') not in self.world:
                raise BpaError('LOGIC.CONNECTOR_UNAVAILABLE', 'nothing observed as %s' % args.get('name'))
            val = self.world[args['name']]
        self.records.append(dict(req, seq=seq, result_ref=content_id('ubos.chunk.v1', val), result=val))
        return val

    # commit.*

    def i_commit_create(self, a, d):
        cls = a['class'].split('/')[-1] if isinstance(a['class'], str) else a['class']
        self.authorize(d['authorization'], cls)
        fields = a['fields'] or {}
        self.per_item(d, len(fields))
        oid = a.get('object_id')
        if oid is not None and self.p['kind'] != 'import':
            raise BpaError('MODEL.INVALID_VALUE', 'object_id is given by import processes only')
        oid = oid or self.uuid()
        slug = a.get('slug') or oid
        ch = {'op': 'create', 'object_id': oid, 'class': cls, 'slug': slug, 'body': dict(fields)}
        if a.get('valid'):
            ch['valid'] = {'from': a['valid'].get('from')}
        self.add(ch)
        self.pending[oid] = {'class': cls, 'slug': slug, 'body': dict(fields), 'change': ch}
        return {'object_id': oid, 'version': None}

    def base_of(self, oid, given):
        if given is not None:
            return given
        if oid in self.reads:
            return self.reads[oid]
        raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'the base is required when the object was not read')

    def i_commit_update(self, a, d, slug=None):
        oid = self.target_id(a['target'])
        self.authorize(d['authorization'], self.class_of(oid))
        sets = a.get('set') or {}
        self.per_item(d, len(sets))
        return self._stage_update(oid, sets, d, slug, a.get('base'), a.get('timeline'))

    def _stage_update(self, oid, sets, d, slug=None, base=None, tl=None):
        """Merge field sets into the pending change of an object, or stage an update."""
        if oid in self.pending:
            p = self.pending[oid]
            p['body'].update(sets)
            if 'change' in p:
                p['change']['body'].update(sets)
                if slug:
                    p['change']['slug'] = p['slug'] = slug
                return {'version': None}
            p['update']['body'].update(sets)
            if slug:
                p['update']['slug'] = p['slug'] = slug
            return {'version': None}
        ch = {'op': 'update', 'object_id': oid, 'base': self.base_of(oid, base), 'body': dict(sets)}
        if slug:
            ch['slug'] = slug
        if tl:
            ch['valid'] = {'kind': tl.get('op'), 'from': tl.get('from')}
            if tl.get('to') is not None:
                ch['valid']['to'] = tl['to']
        self.add(ch)
        o = self.state()['objects'][oid]
        body = dict(self.s.versions[o['head']]['body'])
        body.update(sets)
        self.pending[oid] = {'class': o['class'], 'slug': slug or o['slug'], 'body': body, 'update': ch}
        return {'version': None}

    def i_commit_end(self, a, d):
        oid = self.target_id(a['target'])
        self.authorize(d['authorization'], self.class_of(oid))
        ch = {'op': 'end', 'object_id': oid, 'base': self.base_of(oid, a.get('base'))}
        if a.get('from'):
            ch['valid'] = {'from': a['from']}
        self.add(ch)
        return {'version': None}

    def i_commit_move(self, a, d):
        return self.i_commit_update({'target': a['target'], 'set': {}, 'base': a.get('base')}, d, slug=a['slug'])

    def i_commit_append(self, a, d):
        cls = a['class'].split('/')[-1]
        self.authorize(d['authorization'], cls)
        self.per_item(d, len(a['fields'] or {}))
        eid = self.uuid()
        ch = {'op': 'append', 'class': cls, 'entry_id': eid, 'ledger_key': a['ledger_key'], 'body': a['fields']}
        for k in ('valid_at', 'posting_group', 'source_refs'):
            if a.get(k) is not None:
                ch[k] = a[k]
        self.add(ch)
        return {'entry_id': eid}

    def _entry_class(self, eid):
        _, e = self.s._entry_owner(self.state(), eid)
        if e is None:
            raise BpaError('IAM.NOT_FOUND', 'unknown entry')
        return e['class']

    def i_commit_reverse(self, a, d):
        self.authorize(d['authorization'], self._entry_class(a['entry']))
        nid = self.uuid()
        ch = {'op': 'reverse', 'entry_id': a['entry'], 'new_entry_id': nid}
        if a.get('valid_at'):
            ch['valid_at'] = a['valid_at']
        self.add(ch)
        return {'entry_id': nid}

    def i_commit_adjust(self, a, d):
        if not isinstance(a['corrects'], list) or len(a['corrects']) != 1:
            raise BpaError('MODEL.INVALID_VALUE', 'an adjustment corrects one entry in BPA 1.0-rc1')
        self.authorize(d['authorization'], self._entry_class(a['corrects'][0]))
        nid = self.uuid()
        ch = {'op': 'adjust', 'entry_id': a['corrects'][0], 'new_entry_id': nid, 'body': a['fields']}
        if a.get('valid_at'):
            ch['valid_at'] = a['valid_at']
        self.add(ch)
        return {'entry_id': nid}

    def _link(self, a, d, on):
        oid = self.target_id(a['from'])
        cls = self.class_of(oid)
        self.authorize(d['authorization'], cls)
        f = self.s._cls(cls)['fields'].get(a['relationship'])
        if f is None or not f.get('relationship'):
            raise BpaError('MODEL.REFERENCE_INVALID', '%s is not a relationship of %s' % (a['relationship'], cls))
        to = a['to']
        tid = self.target_id(to)
        uri = 'ubos://%s/%s/%s' % (self.s.TENANT, self.class_of(tid),
                                    self.pending[tid]['slug'] if tid in self.pending else self.state()['objects'][tid]['slug'])
        body = self.pending[oid]['body'] if oid in self.pending else self.s.versions[self.state()['objects'][oid]['head']]['body']
        cur = body.get(a['relationship'])
        if f['type'] == 'list':
            vals = list(cur or [])
            vals = vals + [uri] if on and uri not in vals else [x for x in vals if x != uri or on]
            new = vals
        else:
            new = uri if on else None
        if oid not in self.pending and oid not in self.reads:
            self.reads[oid] = self.state()['objects'][oid]['head']
        self.i_commit_update({'target': oid, 'set': {a['relationship']: new}}, isa.by_name()['commit.update'])
        return {'link': None}

    def i_commit_link(self, a, d):
        return self._link(a, d, True)

    def i_commit_unlink(self, a, d):
        return self._link(a, d, False)

    # read.*

    def i_read_get(self, a, d):
        t = a['target']
        explicit = isinstance(t, str) and '?' in t
        if explicit:
            r = self.s.resolve(t)
            self.authorize(d['authorization'], self.s.versions[r['version_hash']]['class'])
            out = {'object_id': r['object_id'], 'class': self.s.versions[r['version_hash']]['class'],
                   'version': r['version_hash'], 'fields': r['body'], 'valid': r['valid'],
                   'resolved_from': {'branch': r['branch'], 'commit': r['evaluated_at'][r['branch']]}}
        else:
            oid = self.target_id(t)
            cls = self.class_of(oid)
            self.authorize(d['authorization'], cls)
            if oid in self.pending:
                out = {'object_id': oid, 'class': cls, 'version': None, 'fields': dict(self.pending[oid]['body']),
                       'valid': None, 'resolved_from': {'branch': self.branch, 'commit': None}}
            else:
                seg = self.segment(oid)
                if seg is None:
                    raise BpaError('QRY.NOT_FOUND_AT_TIME')
                o = self.state()['objects'][oid]
                self.reads.setdefault(oid, o['head'])
                trip = {'object': oid, 'version': seg[2], 'asof': self.context['asof']}
                if trip not in self.triples:
                    self.triples.append(trip)
                out = {'object_id': oid, 'class': cls, 'version': seg[2],
                       'fields': dict(self.s.versions[seg[2]]['body']), 'valid': {'from': seg[0], 'to': seg[1]},
                       'resolved_from': {'branch': self.branch, 'commit': self.pins[self.branch]}}
                if self.s.versions[seg[2]].get('state') is not None:
                    out['state'] = self.s.versions[seg[2]]['state']
        if a.get('fields') is not None:
            out['fields'] = {k: v for k, v in out['fields'].items() if k in a['fields']}
        self.per_item(d, -(-len(canonical(out['fields']).encode('utf-8')) // 1024))
        return out

    def i_read_history(self, a, d):
        oid = self.target_id(a['target'])
        self.authorize(d['authorization'], self.class_of(oid))
        h = self.s.history(self.branch, oid, a.get('from_time'), a.get('to_time'))
        self.per_item(d, len(h['commits']))
        return {'commits': [{'cseq': c['cseq'], 'tx_time': c['tx_time'], 'ops': c['ops']} for c in h['commits']]}

    def i_read_timeline(self, a, d):
        oid = self.target_id(a['target'])
        self.authorize(d['authorization'], self.class_of(oid))
        o = self.state()['objects'][oid]
        lo = a.get('valid_from') or min(s[0] for s in o['timeline']) if o['timeline'] else a.get('valid_from')
        segs = self.s.read_range(self.branch, oid, lo, a.get('valid_to'), {'evaluated_at': self.pins})['segments'] \
            if lo else []
        self.per_item(d, len(segs))
        return {'segments': [{'from': x['valid']['from'], 'to': x['valid']['to'], 'fields': x['body']} for x in segs]}

    def i_read_diff(self, a, d):
        oid = self.target_id(a['scope'])
        self.authorize(d['authorization'], self.class_of(oid))
        ca, cb = dict(a['a'], branch=a['a'].get('branch', self.branch)), dict(a['b'], branch=a['b'].get('branch', self.branch))
        entries = [e for e in self.s.diff(ca, cb) if e['object_id'] == oid]
        self.per_item(d, len(entries))
        return {'entries': [{k: v for k, v in e.items() if k != 'versions'} for e in entries]}

    def i_read_effective(self, a, d):
        cls = a['target'].split('/')[-1]
        self.authorize(d['authorization'], cls)
        if a['what'] == 'class':
            return self.s.class_at(self.branch, cls, {'evaluated_at': self.pins} if self.pins else None)
        if a['what'] == 'descriptor':
            return self.s.bpu_descriptor(self.branch, cls)
        if a['what'] == 'declarations':
            # since 1.0-rc2 (DEC-067): the effective sheet declarations of an object
            oid = self.target_id(a['target'])
            r = self.s.cascade(self.branch, oid, self.decision_ctx(), {'evaluated_at': self.pins} if self.pins else None)
            return {'properties': r['properties'], 'bindings': r['bindings']}
        raise BpaError('MODEL.INVALID_VALUE', 'what is class, declarations or descriptor')

    # run.*

    def i_run_port(self, a, d):
        oid = self.target_id(a['target'])
        cls = self.class_of(oid)
        eff = self.s._cls(cls)
        port = (eff.get('_ports') or {}).get(a['port'])
        if port is None:
            raise BpaError('MODEL.PORT_NOT_IMPLEMENTED', 'no port %s' % a['port'])
        # STD-ISA-050 (1.0-rc3): the port is available in the object's lifecycle state
        state = self.object_state(oid)
        lc = self.s._lifecycle(self.state(), cls)
        if port.get('states') and state not in port['states']:
            raise BpaError('FLOW.PORT_UNAVAILABLE', {'port': a['port'], 'state': state})
        if port.get('transition') and lc:
            tr = next((t for t in lc[0].get('transitions') or [] if t['name'] == port['transition']), None)
            if tr and state not in tr['from']:
                raise BpaError('FLOW.PORT_UNAVAILABLE', {'port': a['port'], 'state': state})
        self.authorize(port.get('permission', 'none'), cls)
        if oid not in self.pending:
            # STD-LGS-031: the decisions of a command port run before it (DSN-DVM-720)
            self.s._check_decision(self.state(), self.branch, oid, a['port'], a.get('params') or {},
                                   self.decision_ctx())
        found = dispatch(list(self.s.decls.values()), cls, a['port'], None, self.s._extensions(self.state()))
        body = self.pending[oid]['body'] if oid in self.pending else self.s.versions[self.state()['objects'][oid]['head']]['body']
        # STD-ISA-050 (1.0-rc3): the port's guards, L1 booleans over this, params and ctx
        ptypes = {q['name']: q for q in port.get('params') or []}
        failed = [g for g in port.get('guards') or []
                  if not guard_ok(list(self.s.decls.values()), cls, g, body, ptypes, a.get('params') or {}, self.decision_ctx())]
        if failed:
            raise BpaError('FLOW.GUARD_FAILED', {'port': a['port'], 'failed': [{'guard': g} for g in failed]})
        for i, g in enumerate(port.get('guards') or []):
            self.guards.append({'guard': '%s/%d' % (a['port'], i), 'result': True})
        return self.call_impl(found, oid, cls, body, a['port'], a.get('params') or {})

    def object_state(self, oid):
        """The lifecycle state of an object as this process sees it (STD-CLS-004)."""
        if oid in self.transitions:
            return self.transitions[oid]['to']
        if oid in self.pending and 'change' in self.pending[oid]:
            lc = self.s._lifecycle(self.state(), self.pending[oid]['class'])
            return lc[0]['initial'] if lc else None
        o = self.state()['objects'][oid]
        return self.s.versions[o['head']].get('state')

    def i_commit_transition(self, a, d):
        """STD-ISA-017: a lifecycle transition as the effect of the current port."""
        oid = self.target_id(a['target'])
        cls = self.class_of(oid)
        name = a['transition']
        frame = self.frames[-1] if self.frames else None
        decl = (self.s._cls(cls).get('_ports') or {}).get(frame.port) if frame and frame.port else None
        if decl is None or decl.get('transition') != name:
            raise BpaError('FLOW.TRANSITION_NOT_ALLOWED', {'transition': name, 'message': 'the port does not name the transition'})
        lc = self.s._lifecycle(self.state(), cls)
        if lc is None:
            raise BpaError('FLOW.TRANSITION_NOT_ALLOWED', {'transition': name, 'message': 'the class has no lifecycle'})
        tr = next((t for t in lc[0].get('transitions') or [] if t['name'] == name), None)
        current = self.object_state(oid)
        if tr is None or current not in tr['from']:
            raise BpaError('FLOW.TRANSITION_NOT_ALLOWED', {'transition': name, 'from': current})
        effects = lc[0].get('effects') or {}
        self.per_item(d, len(tr.get('guards') or []) + sum(len(v) for st_ in (current, tr['to'])
                                                           for v in (effects.get(st_) or {}).values()))
        body = dict(self.pending[oid]['body']) if oid in self.pending else dict(self.s.versions[self.state()['objects'][oid]['head']]['body'])
        classes = list(self.s.decls.values())
        ptypes = {q['name']: q for q in decl.get('params') or []}
        params, ctx = frame.args or {}, self.decision_ctx()
        failed = [{'name': g['name'], 'message': g.get('message', g['name'])} for g in tr.get('guards') or []
                  if not guard_ok(classes, cls, g['rule'], body, ptypes, params, ctx)]
        if failed:
            raise BpaError('FLOW.GUARD_FAILED', {'transition': name, 'failed': failed})
        sets = {}
        for state, kind in ((current, 'exit'), (tr['to'], 'entry')):
            for ef in (effects.get(state) or {}).get(kind) or []:
                try:
                    if 'set' in ef:
                        for f, text in ef['set'].items():
                            if f not in self.s._cls(cls)['fields']:
                                raise BpaError('MODEL.INVALID_VALUE', 'undeclared field %s' % f)
                            body[f] = sets[f] = eval_value(classes, cls, text, body, ptypes, params, ctx)
                    else:
                        asset_name, _, fname = ef['logic'].partition('#')
                        asset, vh = self.asset(asset_name)
                        self.run_function(Frame(asset, vh, fname, this=body, cls=cls, port=frame.port, oid=oid), params)
                        if oid in self.pending:
                            body = dict(self.pending[oid]['body'])
                except BpaError as e:
                    raise BpaError('FLOW.EFFECT_FAILED', {'state': state, 'effect': kind, 'code': e.code})
        for f in ((lc[0].get('field_rules') or {}).get(tr['to']) or {}).get('required') or []:
            if body.get(f) is None:
                raise BpaError('FLOW.FIELD_RULE_VIOLATED', {'state': tr['to'], 'field': f, 'rule': 'required'})
        self._stage_update(oid, sets, d)
        self.transitions[oid] = {'from': current, 'to': tr['to'],
                                 'fields': sorted(set(self.transitions.get(oid, {}).get('fields', ())) | set(sets))}
        for g in tr.get('guards') or []:
            self.guards.append({'guard': '%s/%s' % (name, g['name']), 'result': True})
        return {'from': current, 'to': tr['to']}

    def i_read_actions(self, a, d):
        """STD-ISA-036: the ports of an object with their availability for this principal."""
        oid = self.target_id(a['target'])
        cls = self.class_of(oid)
        self.authorize(d['authorization'], cls)

        def permit(action, c):
            try:
                self.authorize(action, c)
                return True
            except BpaError:
                return False
        out = self.s._actions_in(self.state(), self.branch, oid, self.decision_ctx(), permit)
        self.per_item(d, len(out['actions']))
        return out

    def call_impl(self, found, oid, cls, body, port, params):
        impl = found.get('impl') or ''
        name, _, fname = impl.partition('#')
        asset, vh = self.asset(name)
        return self.run_function(Frame(asset, vh, fname, this=body, cls=cls, port=port, impl_class=found['class'],
                                       oid=oid), params)

    def i_run_validate(self, a, d):
        """STD-ISA-054: the validation preview of proposed changes or of the unit of work on
        a target."""
        if a.get('changes') is not None:
            changes = a['changes']
        else:
            if a.get('target') is None:
                raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'run.validate takes changes or a target')
            oid = self.target_id(a['target'])
            changes = [{k: v for k, v in c.items() if k != 'derivation'} for c in self.uow if c.get('object_id') == oid]
        for c in changes:
            if c.get('object_id'):
                self.authorize('read', self.class_of(c['object_id']))
        return self.s.validate(self.branch, changes, {'channel': self.context['channel'],
                                                      'principal_kind': self.context['principal_kind']})

    def i_read_explain(self, a, d):
        """STD-ISA-035: explanations of a cascade, a validation outcome or a decision."""
        oid = self.target_id(a['target'])
        cls = self.class_of(oid)
        self.authorize(d['authorization'], cls)
        kind = a['kind']
        if kind == 'cascade':
            if not a.get('property'):
                raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a cascade explanation names the property')
            r = self.s.explain_cascade(self.branch, oid, a['property'], self.decision_ctx(),
                                       {'evaluated_at': self.pins} if self.pins else None)
            return {k: v for k, v in r.items() if k != 'evaluated_at'}
        if kind == 'validation':
            return self.i_run_validate({'target': a['target']}, d)
        if kind == 'decision':
            if not a.get('property'):
                raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a decision explanation names the port')
            return self.s.decide(self.branch, oid, a['property'], {}, self.decision_ctx(),
                                 {'evaluated_at': self.pins} if self.pins else None)
        raise BpaError('LOGIC.NOT_AVAILABLE', 'lineage explanations come with PH-2')

    def i_search_query(self, a, d):
        """STD-ISA-080: a criteria query at the process snapshot."""
        q = a['query']
        if isinstance(q, str):
            q = {'text': q}
        if not isinstance(q, dict):
            raise BpaError('QRY.INVALID_QUERY', 'query is a DAT-Query or its text')
        self.state()
        # `limit` and `cursor` are members of the query object (DAT-Query): the ABI 1.0
        # catalogue stays as released
        i = dict(q, branch=self.branch, commit=self.pins[self.branch])
        if a.get('coordinates'):
            raise BpaError('LOGIC.NOT_AVAILABLE', 'explicit coordinates come with PH-2')
        self.authorize(d['authorization'], i.get('from') if isinstance(i.get('from'), str) else '')
        r = self.s.query(i)
        self.per_item(d, len(r['objects']))
        out = {'items': r['objects'], 'evaluated_at': r['evaluated_at']}
        if r.get('cursor'):
            out['cursor'] = r['cursor']
        return out

    def i_run_super(self, a, d):
        f = self.frames[-1]
        if f.port is None:
            raise BpaError('MODEL.PORT_NOT_IMPLEMENTED', 'run.super outside a port')
        found = dispatch(list(self.s.decls.values()), f.cls, f.port, f.impl_class, self.s._extensions(self.state()))
        params = {k: expr.to_json(v) for k, v in (f.params or {}).items()}
        return self.call_impl(found, f.oid, f.cls, f.this, f.port, params)

    def i_run_logic(self, a, d):
        f = self.frames[-1]
        pins = {x.get('name'): x.get('version') for x in f.asset.get('imports') or []}
        if a['asset'] not in pins:
            raise BpaError('LOGIC.FORBIDDEN_CAPABILITY', 'the asset is not among the imports')
        vh = pins[a['asset']]
        v = self.s.versions.get(vh)
        if v is None or v['body'].get('name') != a['asset']:
            raise BpaError('MODEL.REFERENCE_INVALID', 'the pinned version of %s is unknown' % a['asset'])
        self.authorize(d['authorization'], v['class'])
        self.check_asset(v['body'])
        if vh not in self.logic:
            self.logic.append(vh)
        return self.run_function(Frame(v['body'], vh, a['function']), a.get('args') or [])

    def i_run_async(self, a, d):
        job = self.uuid()
        self.effects.append({'type': 'Job', 'object_id': self.target_id(a['target']),
                             'payload': {'job_id': job, 'port': a.get('port'), 'params': a.get('params') or {},
                                         'not_before': a.get('not_before'), 'parent_process': self.p['process_id']}})
        return {'job_id': job}

    def i_emit_event(self, a, d):
        oid = self.target_id(a['subject'])
        cls = self.class_of(oid)
        self.authorize(d['authorization'], cls)
        declared = [e.get('name') for c in self.s._cls(cls)['linearization']
                    for e in (self.s._decl(c).get('events') or [])]
        if a['type'] not in declared:
            raise BpaError('EVT.UNKNOWN_EVENT_TYPE', a['type'])
        self.effects.append({'type': a['type'], 'object_id': oid, 'payload': a.get('payload') or {}})
        n = self.s.processes.get(self.p['process_id'], {}).get('effects', 0) + len(self.effects)
        return {'event_id': '%s/%d' % (self.p['process_id'], n)}

    # ---- assets (STD-CTX-022, -023, -030) ----

    def asset(self, key):
        """(body, version hash) of a logic asset by object id or name, pinned for the process."""
        st = self.state()
        cands = [oid for oid in sorted(st['objects']) if oid == key or (
            'LogicAsset' in self.s._cls(st['objects'][oid]['class'])['linearization']
            and self.s.versions[st['objects'][oid]['head']]['body'].get('name') == key)]
        cands = [c for c in cands if not self.s.versions[st['objects'][c]['head']]['tombstone']]
        if not cands:
            raise BpaError('IAM.NOT_FOUND', 'no logic asset %s' % key)
        oid = cands[0]
        if 'LogicAsset' not in self.s._cls(st['objects'][oid]['class'])['linearization']:
            raise BpaError('MODEL.REFERENCE_INVALID', 'not a logic asset')
        vh = self.pinned.setdefault(oid, self._logic_version(st['objects'][oid]))
        body = self.s.versions[vh]['body']
        self.check_asset(body)
        if vh not in self.logic:
            self.logic.append(vh)
        return body, vh

    def _logic_version(self, o):
        """The version of a logic asset a process runs (STD-TIME-041, 1.0-rc8): the head, or
        under a restatement the version of its mode — the version whose timeline covers the
        recomputed date (`as_effective`), the version the original process ran
        (`as_originally`) or the head (`current`)."""
        lm = getattr(self.s, '_logic_mode', None)
        if not lm or lm['mode'] == 'current':
            return o['head']
        if lm['mode'] == 'as_originally':
            for vh in lm['original']:
                if any(s[2] == vh for s in o['timeline']) or vh == o['head']:
                    return vh
            return o['head']
        from .store import tl_at
        seg = tl_at(o['timeline'], lm['date'])
        return seg[2] if seg else o['head']

    def check_asset(self, body):
        major, minor = _abi(body.get('abi'))
        if major != 1 or minor > 0:
            raise BpaError('LOGIC.ABI_UNSUPPORTED', 'this implementation supports ABI 1.0')
        if body.get('tier') != 'host':
            raise BpaError('LOGIC.NOT_AVAILABLE', 'the adapter executes host programs only')
        if self.s.branches[self.branch]['kind'] in PROTECTED:
            ok = body.get('state') in ('verified', 'deprecated') and self.s.verified(body)
            if not ok:
                raise BpaError('LOGIC.UNVERIFIED_ASSET')
        elif body.get('state') == 'retired':
            raise BpaError('LOGIC.UNVERIFIED_ASSET', 'retired')


class Exec:

    def _exec_init(self):
        self.journals, self.meterings, self.invocations, self.verifications = {}, {}, {}, {}

    def verified(self, body):
        rec = self.verifications.get(body.get('verification'))
        return bool(rec) and all(c['result'] == 'pass' for c in rec['record']['checks']) and \
            rec['code'] == code_identity(body)

    def _seed(self, i, pid):
        s = i.get('entropy_seed')
        if s is not None:
            return crypto.unhex(s, 32, 'entropy_seed')
        return hashlib.sha256(b'ubos.entropy.v1\x00' + pid.encode()).digest()

    def execute(self, i, replay=None):
        """Run a logic function or a port as one process (STD-TXN-001) and commit its unit of work."""
        branch = i.get('branch', 'main')
        self._check_active(branch)
        p = self._process(i.get('process'))
        seed0 = self._seed(i, p['process_id'])
        interfere = {x.get('attempt'): x.get('changes') for x in i.get('interfere') or []}
        attempt, max_attempts = 0, i.get('max_attempts', MAX_ATTEMPTS)
        while True:
            attempt += 1
            seed = seed0 if attempt == 1 else hashlib.sha256(seed0 + attempt.to_bytes(4, 'big')).digest()
            run = Run(self, i, p, seed, replay)
            retryable = False
            try:
                result, retryable = self._start(run, i)
                if replay is not None:
                    return self._compare(run, result, replay)
                if attempt in interfere:
                    self.commit(branch, interfere[attempt])
                cid = None
                if run.uow:
                    p['serializable'] = i.get('serializable', False)
                    p['reads'] = dict(run.reads)
                    _, cid = self._do_commit(branch, run.uow, p, flow={'transitions': run.transitions, 'guards': run.guards})
            except BpaError as e:
                if replay is not None:
                    raise
                if e.code in RETRY_CODES and retryable and attempt < max_attempts:
                    continue
                outcome = {'LOGIC.LIMIT_EXCEEDED': 'limit', 'LOGIC.CANCELLED': 'cancelled'}.get(e.code, 'error')
                self._finish(run, i, p, None, None, outcome, attempt)
                raise
            return self._finish(run, i, p, result, cid, 'ok', attempt)

    def _start(self, run, i):
        if i.get('port') is not None:
            run.frames.append(Frame({'capabilities': ['run'], 'abi': '1.0'}, None, None))
            d = isa.by_name()['run.port']
            run.frames[-1].params = {}
            res = run.i_run_port({'target': i['target'], 'port': i['port'], 'params': i.get('params') or {}}, d)
            run.frames.pop()
            found = dispatch(list(self.decls.values()), run.class_of(run.target_id(i['target'])), i['port'])
            asset, _ = run.asset(found['impl'].partition('#')[0])
        else:
            asset, vh = run.asset(i['asset'])
            res = run.run_function(Frame(asset, vh, i['function']), i.get('args') or [])
        fn = ((asset.get('source') or {}).get('functions') or {}).get(i.get('function') or found['impl'].partition('#')[2])
        return res, bool(fn and fn.get('retryable'))

    def _finish(self, run, i, p, result, cid, outcome, attempt):
        pid = p['process_id']
        state = 'committed' if outcome == 'ok' else 'failed'
        if outcome == 'ok':
            p['effects'] = run.effects
            self._record(p, state, (run.branch, cid) if cid is not None else None)
            if run.effects:
                self._effects(p, run.branch, cid if cid is not None else self.branches[run.branch]['head'])
        else:
            self._record(p, state)
        m = run.metering(outcome)
        self.meterings[pid] = m
        changes_hash = content_id('ubos.chunk.v1', [{k: v for k, v in c.items() if k != 'derivation'} for c in run.uow])
        self.journals[pid] = {'context': run.context, 'snapshots': dict(run.pins), 'logic': list(run.logic),
                              'entropy_seed': crypto.b64u(run.seed), 'tzdb': TZDB,
                              'records': [dict(r) for r in run.records],
                              'outputs': {'commit_ids': [self.commits[cid]['commit_id']] if cid is not None else [],
                                          'result_hash': content_id('ubos.chunk.v1', result),
                                          'changes_hash': changes_hash,
                                          'effects': [e['type'] for e in run.effects]}}
        self.invocations[pid] = {k: v for k, v in i.items() if k not in ('interfere', 'process', 'world')}
        if outcome != 'ok':
            return None
        out = {'result': result, 'attempts': attempt, 'metering': m}
        if cid is not None:
            out['cseq'] = self.commits[cid]['cseq']
        if p['explicit']:
            out['process_id'] = pid
        return out

    def journal(self, process_id):
        j = self.journals.get(process_id)
        if j is None:
            raise BpaError('IAM.NOT_FOUND', 'no journal for the process')
        out = copy.deepcopy(j)
        for r in out['records']:
            r.pop('result', None)
        out['logic_versions'] = out.pop('logic')       # version hashes (STD-CONF-005, rule 6)
        return out

    def metering_record(self, process_id):
        m = self.meterings.get(process_id)
        if m is None:
            raise BpaError('IAM.NOT_FOUND', 'no metering record for the process')
        return dict(m)

    def metering_total(self):
        units = sum(m['units'] for m in self.meterings.values())
        return {'tenants': {self.TENANT: {'units': units, 'processes': len(self.meterings)}}}

    def replay(self, process_id, tamper=None, drop=None):
        """STD-CTX-012: re-execute with the journaled context, snapshots, logic and records."""
        j = self.journals.get(process_id)
        if j is None:
            raise BpaError('IAM.NOT_FOUND', 'no journal for the process')
        records = [dict(r) for r in j['records']]
        if tamper is not None:
            records[tamper['seq'] - 1]['result'] = tamper['result']
        if drop is not None:
            records = records[:drop - 1]
        inv = dict(self.invocations[process_id])
        ctx = {k: j['context'][k] for k in ('principal', 'principal_kind', 'auth_level', 'asof', 'locale', 'time_zone',
                                            'channel', 'profile') if k in j['context']}
        inv['ctx'] = ctx
        inv['entropy_seed'] = crypto.unb64u(j['entropy_seed'], 32, 'seed').hex()
        saved_now, self.now = self.now, j['context']['now']
        saved = (copy.deepcopy(self.processes), dict(self.branches))
        try:
            return self.execute(dict(inv, process={'process_id': process_id}),
                                replay={'records': records, 'journal': j})
        finally:
            self.now = saved_now
            self.processes, self.branches = saved[0], saved[1]

    def _compare(self, run, result, replay):
        j = replay['journal']
        if replay.get('divergence'):
            return {'outcome': 'diverged', 'divergence': replay['divergence']}
        got = {'result_hash': content_id('ubos.chunk.v1', result),
               'changes_hash': content_id('ubos.chunk.v1', [{k: v for k, v in c.items() if k != 'derivation'}
                                                            for c in run.uow]),
               'effects': [e['type'] for e in run.effects]}
        for k in ('result_hash', 'changes_hash', 'effects'):
            if got[k] != j['outputs'][k]:
                return {'outcome': 'diverged', 'divergence': {'seq': len(j['records']) + 1, 'output': k,
                                                              'expected': j['outputs'][k], 'actual': got[k]}}
        return {'outcome': 'identical'}

    # ---- verification (STD-CTX-023, -024) ----

    def _analyse(self, body):
        """Static analysis findings of an asset (STD-CTX-024)."""
        f = []
        tier = body.get('tier')
        caps = body.get('capabilities') or []
        src = body.get('source')
        size = len(canonical(src).encode('utf-8')) if tier != 'l2' else len((src or '').encode('utf-8'))
        if size > MAX_SOURCE:
            f.append({'rule': 'source_size'})
        if tier == 'host':
            funcs = (src or {}).get('functions') or {}

            def walk(stmts, depth, fname):
                if depth > MAX_NESTING:
                    f.append({'rule': 'nesting_depth', 'function': fname})
                    return
                for st in stmts or []:
                    kinds = [k for k in STATEMENTS if isinstance(st, dict) and k in st]
                    if len(kinds) != 1:
                        f.append({'rule': 'forbidden_construct', 'function': fname})
                        continue
                    k = kinds[0]
                    if k == 'do':
                        name = st['do']
                        fam = name.split('.')[0]
                        if name not in SYSTEM and not (fam in caps or name in caps):
                            f.append({'rule': 'undeclared_instruction', 'function': fname, 'instruction': name})
                        if name not in SYSTEM and name not in isa.by_name():
                            f.append({'rule': 'unknown_instruction', 'function': fname, 'instruction': name})
                        if name == 'run.logic' and (st.get('args') or {}).get('asset') == '"%s"' % body.get('name') \
                                and not (funcs.get(fname) or {}).get('max_depth'):
                            f.append({'rule': 'unbounded_recursion', 'function': fname})
                    for key in ('then', 'else', 'body', 'try', 'catch'):
                        if key in st:
                            walk(st[key], depth + 1, fname)
                    for key in ('let', 'if', 'in', 'repeat', 'return'):
                        if isinstance(st.get(key if key != 'let' else 'value'), str):
                            try:
                                env = expr.Env(this='any', params='any')
                                env.vars = {v: 'any' for v in _names(funcs.get(fname)) | {'self_id'}}
                                expr.check(expr.parse(st[key if key != 'let' else 'value']), env)
                            except BpaError as e:
                                if e.code == 'LOGIC.TYPE_ERROR':
                                    f.append({'rule': 'type_error', 'function': fname})
            for fname, fn in sorted(funcs.items()):
                walk(fn.get('body'), 1, fname)
        elif tier == 'l2':
            text = src or ''
            for word in L2_FORBIDDEN:
                if re.search(r'\b%s\s*\(' % re.escape(word), text) or re.search(r'\bimport\b', text) and word == 'import':
                    f.append({'rule': 'forbidden_construct', 'construct': word})
            for body_ in re.findall(r'\|[^|]*\|\s*(\{[^}]*\}|[^;\n]*)', text):
                if re.search(r'\bsys\b', body_):
                    f.append({'rule': 'closure_captures_gateway'})
            for fam, op in re.findall(r'\bsys\.([a-z]+)\.([a-z_]+)\s*\(', text):
                if fam not in caps and '%s.%s' % (fam, op) not in caps:
                    f.append({'rule': 'undeclared_instruction', 'instruction': '%s.%s' % (fam, op)})
            for fname in re.findall(r'\bfn\s+([a-z_][a-z0-9_]*)\s*\(', text):
                bodies = re.split(r'\bfn\s+', text)
                own = next((b for b in bodies if re.match(r'%s\s*\(' % fname, b)), '')
                if re.search(r'\b%s\s*\(' % fname, own[len(fname):]) and '@max_depth' not in own:
                    f.append({'rule': 'unbounded_recursion', 'function': fname})
            depth = mx = 0
            for ch in text:
                depth += ch in '({['
                depth -= ch in ')}]'
                mx = max(mx, depth)
            if mx > MAX_NESTING:
                f.append({'rule': 'nesting_depth'})
        elif tier == 'l3':
            for imp in (src or {}).get('imports') or []:
                if imp not in L3_WORLD:
                    f.append({'rule': 'import_outside_world', 'import': imp})
        return f

    def verify_asset(self, branch, object_id, verifier=None):
        st = self._state(branch)[0]
        o = st['objects'].get(object_id)
        if o is None:
            raise BpaError('IAM.NOT_FOUND', 'unknown object')
        if 'LogicAsset' not in self._cls(o['class'])['linearization']:
            raise BpaError('MODEL.REFERENCE_INVALID', 'not a logic asset')
        body = self.versions[o['head']]['body']
        findings = self._analyse(body)
        static = [x for x in findings if x['rule'] not in ('undeclared_instruction', 'type_error')]
        caps = [x for x in findings if x['rule'] == 'undeclared_instruction']
        types = [x for x in findings if x['rule'] == 'type_error']
        sig_ok = body.get('tier') != 'l3' or bool(body.get('publisher_signature'))
        tests = self._run_tests(branch, object_id, body)
        checks = [{'name': 'static_analysis', 'result': 'fail' if static else 'pass', 'findings': static},
                  {'name': 'type_check', 'result': 'fail' if types else 'pass', 'findings': types},
                  {'name': 'capabilities', 'result': 'fail' if caps else 'pass', 'findings': caps},
                  {'name': 'tests', 'result': 'fail' if tests else 'pass', 'findings': tests},
                  {'name': 'signature', 'result': 'pass' if sig_ok else 'fail',
                   'findings': [] if sig_ok else [{'rule': 'publisher_signature_missing'}]}]
        from .proofs import NODE_DID
        rec = {'asset': o['head'], 'checks': checks, 'analyzer_version': ANALYZER,
               'verified_by': verifier or NODE_DID, 'at': self.now}
        rid = content_id('ubos.chunk.v1', rec)
        rec['signature'] = self._node_sign(rid, 'signer')
        passed = all(c['result'] == 'pass' for c in checks)
        out = {'verified': passed, 'record': rec}
        if passed:
            self.verifications[rid] = {'record': rec, 'code': code_identity(body)}
            self.commit(branch, [{'op': 'update', 'object_id': object_id, 'base': o['head'],
                                  'body': {'state': 'verified', 'verification': rid}}])
            out['record_id'] = rid
        return out

    def _run_tests(self, branch, oid, body):
        fails = []
        for t in body.get('tests') or []:
            if not isinstance(t, dict):
                continue
            saved = copy.deepcopy(self.__dict__)
            try:
                run = Run(self, {'branch': branch, 'ctx': {'profile': 'simulation'}},
                          self._process(None), self._seed({}, 'test'))
                run.pinned[oid] = self._state(branch)[0]['objects'][oid]['head']
                got = run.run_function(Frame(body, run.pinned[oid], t.get('function')), t.get('args') or [])
                if got != t.get('expect'):
                    fails.append({'rule': 'test_failed', 'function': t.get('function')})
            except BpaError as e:
                if e.code != t.get('error'):
                    fails.append({'rule': 'test_failed', 'function': t.get('function'), 'error': e.code})
            finally:
                self.__dict__.update(saved)
        return fails

    # ---- BPU (STD-BPU-001, -002, -011) ----

    def _bpu_class(self, cls):
        if cls not in self.classes:
            raise BpaError('MODEL.REFERENCE_INVALID', 'unknown class %s' % cls)
        eff = self.classes[cls]
        if eff['kind'] != 'definition' or self._decl(cls).get('abstract') or self._decl(cls).get('trait'):
            raise BpaError('MODEL.INVALID_VALUE', 'a BPU class is a concrete Definition class')
        return eff

    def bpu_descriptor(self, branch, cls):
        """DAT-BpuDescriptor: a pure function of the effective definitions (STD-BPU-002)."""
        self._branch(branch)
        self._bpu_class(cls)
        st = self._state(branch)[0]
        eff = effective_class(list(self.decls.values()), cls, self._extensions(st))
        lin = eff['linearization']
        fields = []
        for name, f in sorted(eff['fields'].items()):
            origin = next((c for c in reversed(lin) if any(x['name'] == name for x in self._decl(c).get('fields') or [])),
                          name.rsplit('.', 1)[0] if '.' in name else cls)
            fields.append({'name': name, 'type': f['type'], 'origin': origin,
                           'classification': f.get('classification', 'internal')})
        ledgers = []
        for n, e in sorted(self.classes.items()):
            if e['kind'] != 'ledger':
                continue
            refs = [k for k, f in e['fields'].items() if f['type'] == 'reference'
                    and (f.get('constraints') or {}).get('target_class') in lin]
            if refs:
                ledgers.append({'class': n, 'key': self._decl(n)['ledger']['key'], 'fields': refs})
        ports = [{'name': k, 'kind': p.get('kind'), 'params': p.get('params', []), 'result': p.get('result'),
                  'permission': p.get('permission'), 'implemented': bool(p.get('impl_by'))}
                 for k, p in sorted((eff.get('_ports') or {}).items())]
        events = [e for c in reversed(lin) for e in (self._decl(c).get('events') or [])]
        schema = {'type': 'object', 'properties': {f['name']: {'type': f['type']} for f in fields},
                  'required': sorted(k for k, f in eff['fields'].items() if f.get('required'))}
        return {'class': {'name': cls, 'class_version': content_id('ubos.chunk.v1', self._decl(cls))},
                'linearization': lin, 'fields': fields, 'ledgers': ledgers, 'ports': ports, 'events': events,
                'governance': [], 'knowledge': [], 'proof': {'signed_commits': True, 'merkle': True, 'anchoring': False},
                'json_schema': content_id('ubos.chunk.v1', schema)}

    def bpu_state(self, branch, object_id, sel=None):
        """STD-BPU-001, element 2: the object's versions and the entries that name it in source_refs."""
        st = self._state(branch, sel)[0]
        o = st['objects'].get(object_id)
        if o is None:
            raise BpaError('IAM.NOT_FOUND', 'unknown object')
        self._bpu_class(o['class'])
        uri = self._uri(o)
        entries = sorted(e['entry_id'] for q in st['ledger'].values() for e in q if uri in (e.get('source_refs') or []))
        return {'timeline': [{'from': a, 'to': b} for a, b, _ in o['timeline']], 'entries': entries}

    def verify_bpu(self, branch, cls):
        """STD-BPU-011: the checks before a BPU class version becomes effective on a protected branch."""
        eff = self._bpu_class(cls)
        st = self._state(branch)[0]
        ports = eff.get('_ports') or {}
        unverified = []
        for name, p in sorted(ports.items()):
            impl = p.get('impl') or ''
            asset = impl.partition('#')[0]
            ok = False
            for oid, o in st['objects'].items():
                v = self.versions[o['head']]
                if 'LogicAsset' in self._cls(o['class'])['linearization'] and v['body'].get('name') == asset:
                    ok = v['body'].get('state') in ('verified', 'deprecated') and self.verified(v['body'])
            if not ok:
                unverified.append({'port': name})
        emitted = {e for p in ports.values() for e in p.get('emits') or []}
        lin = eff['linearization']
        silent = [{'event': e['name']} for c in reversed(lin) for e in (self._decl(c).get('events') or [])
                  if e['name'] not in emitted]
        undocumented = []
        if not self._decl(cls).get('description'):
            undocumented.append({'item': cls})
        undocumented += [{'item': k} for k, f in sorted(eff['fields'].items()) if not f.get('description')]
        undocumented += [{'item': k} for k, p in sorted(ports.items()) if not p.get('description')]
        # STD-BPU-011 item 2 (1.0-rc3): unreachable states and dead ends of the lifecycle
        lc = self._lifecycle(st, cls)
        lf = flow_findings(lc[0]) if lc else None
        lifecycle_check = ({'name': 'lifecycle', 'result': 'fail' if lf else 'pass', 'findings': lf} if lc
                           else {'name': 'lifecycle', 'result': 'not_applicable', 'findings': []})
        checks = [{'name': 'ports_verified', 'result': 'fail' if unverified else 'pass', 'findings': unverified},
                  lifecycle_check,
                  {'name': 'events_emitted', 'result': 'fail' if silent else 'pass', 'findings': silent},
                  {'name': 'documented', 'result': 'fail' if undocumented else 'pass', 'findings': undocumented},
                  {'name': 'tests', 'result': 'not_applicable', 'findings': []}]
        return {'passed': all(c['result'] != 'fail' for c in checks), 'checks': checks}


def _names(fn):
    """Names a function can see: its parameters and every name it binds."""
    out = set()

    def walk(stmts):
        for st in stmts or []:
            if not isinstance(st, dict):
                continue
            for k in ('let', 'as', 'each'):
                if isinstance(st.get(k), str):
                    out.add(st[k])
            for k in ('then', 'else', 'body', 'try', 'catch'):
                walk(st.get(k))
    walk((fn or {}).get('body'))
    return out

