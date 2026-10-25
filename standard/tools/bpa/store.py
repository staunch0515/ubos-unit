"""In-memory reference store for the stateful adapter operations (STD-CONF-005).

Covers objects of Definition, RawDocument and System kind with bitemporal timelines
(STD-TIME-011), ledgers (STD-KIND-020…025), branches and commits (STD-VER-001…013),
base and read-set validation (STD-TXN-020…024), three-way merge with ledger append and
virtual bases (STD-VER-040…056), diff (STD-VER-030) and the state tree (STD-PROOF-001).

Timelines are lists of segments [from, to, version] with to = None for an open end; valid
times are canonical dates or instants, so string order is time order."""
import copy
import datetime
import itertools

from . import BpaError
from .canon import canonical, content_id
import hmac
import hashlib

from .classes import GENESIS, effective_class, is_reference_list, public
from .tree import tree_root
from .workflow import Workflow
from .proofs import Proofs
from .lang import Lang, custom_value
from .exec import Exec
from .sheets import Sheets
from .rules import Rules, DryRun
from .flow import Flow
from .ubtp import Ubtp
from .periods import Periods
from .tables import Tables
from .approvals import Approvals
from .workflows import Workflows
from .lenses import Lenses
from .protection import Protection, is_marker
from .integration import Integration
from .search import Search
from .finance import Finance
from .work import Work
from .ruleops import RuleOps
from . import values as _values
from . import classes as _classes
from .values import GRAMMARS, check_name, validate_value, parse_decimal, fmt_decimal, MINOR_UNITS, REFERENCE

DEFAULT_NOW = '2026-01-01T00:00:00.000000Z'
TENANT = 'acme'                  # authority of object URIs in adapter inputs (STD-CONF-005)
MAX_BODY = 16 * 1024 * 1024      # STD-OBJ-009: bodies up to 16 MiB
PRINCIPAL = 'did:ubos:acme:01900000-0000-7000-8000-000000000002'   # initiator when a process names none (DEC-041)
PROTECTED = ('main', 'release', 'overlay')   # STD-VER-012
# STD-VER-011: protected targets a branch of each kind may be merged into (DEC-041). Merges
# into other kinds (refreshing a working branch) are allowed from any kind but sandbox.
MERGE_TARGETS = {'main': (), 'release': ('main',), 'draft': PROTECTED, 'feature': PROTECTED, 'agent': PROTECTED,
                 'overlay': (), 'loader': PROTECTED, 'evolution': PROTECTED, 'correction': PROTECTED,
                 'sandbox': (), 'device': PROTECTED}
# STD-VER-011: default lifetime in days; draft counts from the last commit (inactivity).
LIFETIME = {'draft': 180, 'feature': 180, 'agent': 30, 'evolution': 90, 'correction': 90, 'sandbox': 14}


def parse_instant(t):
    return datetime.datetime.strptime(t, '%Y-%m-%dT%H:%M:%S.%fZ')


def fmt_instant(d):
    return d.strftime('%Y-%m-%dT%H:%M:%S.%fZ')


def branch_kind(name):
    return 'main' if name == 'main' else name.split('/')[0]


def _lt(a, b):
    """a < b where None is +infinity."""
    if a is None:
        return False
    return b is None or a < b


def _min(a, b):
    return a if not _lt(b, a) else b

# ---- timeline algebra (STD-TIME-011, DEC-032) ----


def tl_remove(tl, a, b):
    out = []
    for f, t, v in tl:
        if _lt(f, a):
            out.append([f, _min(t, a), v])
        if _lt(b, t) and b is not None:
            out.append([b if _lt(f, b) else f, t, v])
    return [s for s in out if _lt(s[0], s[1])]


def tl_apply(tl, kind, v, a, b=None):
    if kind == 'create':
        if tl:
            raise BpaError('TXN.ALREADY_EXISTS')
        return [[a, None, v]]
    if not tl:
        raise BpaError('QRY.NOT_FOUND_AT_TIME', 'no timeline to change')
    if kind == 'supersede_from':
        starts = [f for f, _, _ in tl if _lt(a, f)]
        e = min(starts) if starts else None
        out = tl_remove(tl, a, e) + [[a, e, v]]
    elif kind == 'set_period':
        if not _lt(a, b):
            raise BpaError('MODEL.INVALID_VALUE', 'empty period')
        for f, t, _ in tl:
            if _lt(f, b) and _lt(a, t) and not (not _lt(f, a) and not _lt(b, t)):
                raise BpaError('TIME.OVERLAPPING_VALIDITY')
        out = tl_remove(tl, a, b) + [[a, b, v]]
    elif kind == 'replace_future':
        out = tl_remove(tl, a, None) + [[a, None, v]]
    elif kind == 'end':
        out = tl_remove(tl, a, None)
    else:
        raise BpaError('MODEL.INVALID_VALUE', 'unknown timeline operation')
    return sorted(out, key=lambda s: s[0])


def tl_range(tl, kind, a, b=None):
    """Valid range changed by an operation, as [from, to)."""
    if kind == 'supersede_from':
        starts = [f for f, _, _ in tl if _lt(a, f)]
        return [a, min(starts) if starts else None]
    if kind == 'set_period':
        return [a, b]
    return [a, None]


def tl_at(tl, asof):
    for f, t, v in tl:
        if not _lt(asof, f) and _lt(asof, t):
            return [f, t, v]
    return None


class Store(Workflow, Proofs, Lang, Exec, Sheets, Rules, Ubtp, Flow, Periods, Tables, Approvals, Workflows, Lenses, Protection, Integration,
            Search, Finance, Work, RuleOps):
    def __init__(self):
        self.reset()

    # ---- adapter lifecycle (STD-CONF-002) ----

    def reset(self):
        self.max_violations = Rules.MAX_VIOLATIONS
        self._dry = False
        self.now = DEFAULT_NOW
        self.max_body = MAX_BODY
        self._exts = None
        self._eff_cache = {}
        self.decls = {}
        self.classes = {}
        self.value_types = {}
        _classes.CUSTOM_TYPES = {}
        self.versions = {}                     # version hash -> {object_id, class, body, valid, tombstone}
        self.commits = {}
        self.branches = {}
        self.ids = itertools.count(1)
        self.tags = {}                         # STD-VER-060: name -> {commit, branch, signer, deleted}
        self.processes = {}                    # STD-TXN-001, -031
        self.idempotency = {}                  # STD-TXN-040: (initiator, key) -> {hash, result}
        self.outbox = []                       # STD-TXN-050: released effects in commit order
        self.acked = set()
        self.schedule = []                     # STD-TIME-050: pending became-effective events
        self.derivations = {}                  # STD-TXN-030: record id -> DAT-Derivation
        self.derivation_inputs = {}
        self.max_inputs = 10000
        self.change_sets = {}                  # STD-VER-020: object id -> {target, merge commit}
        self.bulks = {}                        # STD-TXN-070
        self.pids = itertools.count(1)
        self._cseq = 0
        self._exec_init()
        self._node_init()
        self._orch_init()
        self._lens_init()
        self._prot_init()
        self._int_init()
        self._search_init()
        self._fin_init()
        self.work_branches = []                # STD-LGS-063 (1.0-rc10)
        self.detach_schedule = []              # STD-LGS-070 (1.0-rc11)
        root = self._new_commit('main', [], {'objects': {}, 'ledger': {}}, 0, tx=False)
        self.branches['main'] = {'head': root, 'kind': 'main', 'created_at': self.now, 'active_at': self.now,
                                 'parent_line': None, 'created_from': root}
        return True

    def set_now(self, now):
        self.now = now
        self._tick()
        self._wf_timers()       # STD-TXN-091 (1.0-rc8)
        self._task_escalations()  # STD-LGS-063 (1.0-rc10)
        return True

    def set_limits(self, max_body_bytes=None, max_derivation_inputs=None, max_violations=None):
        for v in (max_body_bytes, max_derivation_inputs, max_violations):
            if v is not None and (not isinstance(v, int) or isinstance(v, bool) or v < 1):
                raise BpaError('MODEL.INVALID_VALUE', 'a limit is a positive integer')
        if max_body_bytes is not None:
            self.max_body = max_body_bytes
        if max_derivation_inputs is not None:
            self.max_inputs = max_derivation_inputs
        if max_violations is not None:
            self.max_violations = max_violations
        return True

    # ---- commit records, transaction time (STD-VER-001, STD-TIME-001) ----

    def _hlc(self, prev):
        """Hybrid logical clock: the adapter's physical clock reads `now` (DEC-041)."""
        t = self.now
        if prev is not None and t <= prev:
            t = fmt_instant(parse_instant(prev) + datetime.timedelta(microseconds=1))
        return t

    def _new_commit(self, branch, parents, state, cseq, tx=True, **meta):
        prev = self.commits[parents[0]]['tx_time'] if parents else None
        tx_time = self._hlc(prev) if tx else None
        rec = {'vae': TENANT, 'branch': branch, 'parents': [self.commits[p]['commit_id'] for p in parents],
               'cseq': cseq, 'tx_time': tx_time, 'root': tree_root(self.state_entries(state))}
        cid = next(self.ids) if self.commits else 0
        commit_id = content_id('ubos.commit.v1', rec)
        rec['signatures'] = [self._node_sign(commit_id)]
        c = {'parents': list(parents), 'state': state, 'cseq': cseq, 'branch': branch, 'tx_time': tx_time,
             'commit_id': commit_id, 'ops': {}, 'hidden': [], 'record': rec}
        c.update(meta)
        self.commits[cid] = c
        return cid

    def _advance(self, branch, cid):
        br = self.branches[branch]
        br['head'] = cid
        br['active_at'] = self.now

    def _process_id(self):
        return '00000000-0000-4000-8000-%012x' % next(self.pids)

    def load_genesis(self, classes, value_types=None):
        if value_types is not None:
            self._load_value_types(value_types)
        decls = dict(self.decls)
        for d in classes:
            decls[d['name']] = d
        eff = {}
        for n in list(decls) + ['ChangeSet', 'Release', 'Logic', 'GovernanceSheet', 'PropertyDecl', 'Lifecycle', 'Policy',
                                'CloseArtefact', 'MatchRecord', 'DecisionTable', 'SimulationRun',
                                'WorkflowDefinition', 'WorkflowInstance', 'Saga', 'Quarantine', 'QuarantineRelease',
                                'Lens', 'Connector', 'NotificationTemplate', 'View',
                                'Currency', 'FxRate', 'LedgerAccount', 'Projection',
                                'Delegation', 'Task', 'WorkQueue', 'RuleTest']:
            eff[n] = effective_class(list(decls.values()), n)
        for n, e in eff.items():
            old = self.classes.get(n)
            if old and old['kind'] != e['kind'] and self._instances(n):
                raise BpaError('MODEL.KIND_IMMUTABLE')
        self.decls, self.classes = decls, eff
        return True

    def _instances(self, name):
        subclasses = {n for n, e in self.classes.items() if name in e['linearization']}
        for c in self.commits.values():
            st = c['state']
            if any(o['class'] in subclasses for o in st['objects'].values()):
                return True
            if any(k[0] in subclasses for k in st['ledger']):
                return True
        return False

    # ---- helpers ----

    def _branch(self, name):
        if name not in self.branches:
            raise BpaError('IAM.NOT_FOUND', 'unknown branch')
        return self.branches[name]

    def _commit_at(self, branch, time=None):
        cid = self._branch(branch)['head']
        if time is None:
            return cid
        while self.commits[cid]['cseq'] > time:
            cid = self.commits[cid]['parents'][0]
        if self.commits[cid]['cseq'] != time:
            raise BpaError('IAM.NOT_FOUND', 'no commit with this cseq on the branch')
        return cid

    def _first_parents(self, cid):
        while True:
            yield cid
            ps = self.commits[cid]['parents']
            if not ps:
                return
            cid = ps[0]

    def _at_tx(self, branch, t):
        """Latest commit on the first-parent history with tx_time <= t (STD-VER-090)."""
        for c in self._first_parents(self._branch(branch)['head']):
            tx = self.commits[c]['tx_time']
            if tx is None or tx <= t:
                return c
        raise BpaError('IAM.NOT_FOUND', 'no commit at that time')

    def _by_id(self, branch, commit_id):
        for c in self._first_parents(self._branch(branch)['head']):
            if self.commits[c]['commit_id'] == commit_id:
                return c
        raise BpaError('IAM.NOT_FOUND', 'commit not on the history of the branch')

    def _select(self, branch, i):
        """Snapshot of a branch (STD-VER-090): (commit, snapshot time); the time is None for
        the head, which resolves live parent lines at their heads (STD-VER-080)."""
        sel = [k for k in ('time', 'commit', 'tag', 'tx_time') if i.get(k) is not None]
        if len(sel) > 1:
            raise BpaError('QRY.INVALID_QUERY', 'at most one of time, commit, tag and tx_time')
        pins = i.get('evaluated_at') or {}
        if pins and sel:
            raise BpaError('QRY.INVALID_QUERY', 'evaluated_at replaces the selectors')
        if branch in pins:
            cid = self._by_id(branch, pins[branch])
            return cid, self.commits[cid]['tx_time']
        if not sel:
            return self._branch(branch)['head'], None
        k = sel[0]
        if k == 'time':
            cid = self._commit_at(branch, i['time'])
        elif k == 'commit':
            cid = self._by_id(branch, i['commit'])
        elif k == 'tag':
            tag = self.tags.get(i['tag'])
            if tag is None or tag['deleted']:
                raise BpaError('IAM.NOT_FOUND', 'unknown tag')
            cid = self._by_id(branch, self.commits[tag['commit']]['commit_id'])
        else:
            validate_value('instant', None, i['tx_time'])
            cid = self._at_tx(branch, i['tx_time'])
            return cid, i['tx_time']
        return cid, self.commits[cid]['tx_time']

    # ---- branch inheritance (STD-VER-080, -081) ----

    def _parent_view(self, branch, cid, tau, pins, depth=0):
        """(state, provenance, evaluated_at) of the parent line of a branch, or None."""
        pl = self.branches[branch].get('parent_line')
        if not pl:
            return None
        if depth > 16:
            raise BpaError('VER.INHERITANCE_CYCLE')
        p = pl['branch']
        if p in pins:
            pcid = self._by_id(p, pins[p])
        elif pl['mode'] == 'pinned':
            pcid = pl['pin']
        elif tau is None:
            pcid = self.branches[p]['head']
        else:
            pcid = self._at_tx(p, tau)
        ptau = None if (pl['mode'] == 'live' and tau is None and p not in pins) else self.commits[pcid]['tx_time']
        return self._view(p, pcid, ptau, pins, depth + 1)

    def _view(self, branch, cid, tau=None, pins=None, depth=0):
        """Objects visible on a branch at a snapshot: its own versions and hide markers over
        its parent line (STD-VER-080), with the branch and commit each was resolved from."""
        c = self.commits[cid]
        ev = {branch: c['commit_id']}
        parent = self._parent_view(branch, cid, tau, pins or {}, depth)
        if parent is None:
            prov = {oid: (branch, cid) for oid in c['state']['objects']}
            return c['state'], prov, ev
        pst, pprov, pev = parent
        objects, prov = dict(pst['objects']), dict(pprov)
        for oid in c['hidden']:
            objects.pop(oid, None)
            prov.pop(oid, None)
        for oid, o in c['state']['objects'].items():
            objects[oid] = o
            prov[oid] = (branch, cid)
        ev.update(pev)
        return {'objects': objects, 'ledger': c['state']['ledger']}, prov, ev

    def _state(self, branch, i=None):
        """(state, provenance, evaluated_at, commit) of a read."""
        i = i or {}
        cid, tau = self._select(branch, i)
        st, prov, ev = self._view(branch, cid, tau, i.get('evaluated_at') or {})
        return st, prov, ev, cid

    def _decl(self, name):
        return self.decls.get(name) or GENESIS[name]

    def _cls(self, name):
        exts = getattr(self, '_exts', None)
        if exts or name == 'ClassExtension':
            if name != 'ClassExtension' and name not in self.classes:
                raise BpaError('MODEL.REFERENCE_INVALID', 'unknown class %s' % name)
            key = (name, id(exts))
            if key not in self._eff_cache:
                self._eff_cache[key] = effective_class(list(self.decls.values()), name, exts or ())
            return self._eff_cache[key]
        if name not in self.classes:
            raise BpaError('MODEL.REFERENCE_INVALID', 'unknown class %s' % name)
        return self.classes[name]

    def _today(self):
        return self.now[:10]

    def _default_valid(self, cls):
        return self._today() if self._decl(cls).get('valid_time', 'date') == 'date' else self.now

    def _check_body(self, cls, body, partial=False):
        eff = self._cls(cls)
        if not isinstance(body, dict):
            raise BpaError('MODEL.INVALID_VALUE', 'body is an object')
        out = {}
        for k, v in body.items():
            # STD-OBJ-011 (1.0-rc8): an erased field is kept as it is stored
            if is_marker(v) and v['commitment'] in self.erased_store:
                out[k] = v
                continue
            if k not in eff['fields']:
                # STD-CLS-060 (1.0-rc8): values a lens archived by drop_with_archive
                if k == '_archived' and isinstance(v, dict):
                    out[k] = v
                    canonical(v)
                    continue
                if eff['kind'] == 'raw_document':
                    out[k] = v
                    canonical(v)
                    continue
                raise BpaError('MODEL.INVALID_VALUE', 'undeclared field %s' % k)
            f = eff['fields'][k]
            if f['type'] in self.value_types:
                out[k] = None if v is None else custom_value(self.value_types[f['type']], v, f.get('constraints'))
            else:
                out[k] = validate_value(f['type'], f.get('constraints'), v)
        if not partial:
            for name, f in eff['fields'].items():
                if f.get('required') and out.get(name) is None:
                    raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'required field %s' % name)
            if len(canonical(out).encode('utf-8')) > self.max_body:
                raise BpaError('MODEL.OBJECT_TOO_LARGE', 'body larger than the configured limit')
        return out

    def _version(self, oid, cls, slug, body, valid, predecessor=None, tombstone=False, provenance=None, state=None):
        stored = self._protect(oid, cls, body)
        env = {'object_id': oid, 'class': cls, 'class_version': content_id('ubos.chunk.v1', self._decl(cls)),
               'schema_version': self._decl(cls).get('schema_version', 1), 'kind': self._cls(cls)['kind'],
               'slug': slug, 'valid': valid,
               # STD-OBJ-010 (1.0-rc8): hashes cover the stored form of protected fields
               'body_ref': content_id('ubos.chunk.v1', stored)}
        if state is not None:
            env['state'] = state
        if predecessor:
            env['predecessor'] = predecessor
        if tombstone:
            env['tombstone'] = True
        if provenance:
            env['provenance_ref'] = provenance
        vh = content_id('ubos.version.v1', env)
        old = self.versions.get(vh)
        self.versions[vh] = {'object_id': oid, 'class': cls, 'slug': slug, 'body': body, 'valid': valid,
                             'tombstone': tombstone, 'cseq': old['cseq'] if old else self._cseq, 'envelope': env}
        if stored is not body:
            self.versions[vh]['stored'] = stored
            if old is not None:
                self.versions[vh]['body'] = old['body']     # an erased version stays erased
        if provenance:
            self.versions[vh]['provenance_ref'] = provenance
        if state is not None:
            self.versions[vh]['state'] = state
        return vh

    # ---- branches (STD-VER-010, -013) ----

    def create_branch(self, name, source='main', time=None, parent_line=None):
        check_name('branch-name', name)
        if name in self.branches:
            raise BpaError('TXN.ALREADY_EXISTS', 'branch exists')
        if parent_line is not None:
            pl = self._parent_line(name, parent_line)
            self._cseq = 0
            cid = self._new_commit(name, [], {'objects': {}, 'ledger': {}}, 0, tx=False)
            created_from = self.branches[pl['branch']]['head'] if pl['mode'] == 'live' else pl['pin']
        else:
            pl = None
            cid = created_from = self._commit_at(source, time)
        self.branches[name] = {'head': cid, 'kind': branch_kind(name), 'created_at': self.now,
                               'active_at': self.now, 'parent_line': pl, 'created_from': created_from}
        return {'head': self.commits[cid]['cseq']}

    def _parent_line(self, name, pl):
        if not isinstance(pl, dict) or pl.get('mode', 'live') not in ('live', 'pinned'):
            raise BpaError('MODEL.INVALID_VALUE', 'parent_line is {branch, mode, pin}')
        p = pl.get('branch')
        self._branch(p)
        out = {'branch': p, 'mode': pl.get('mode', 'live')}
        if out['mode'] == 'pinned':
            out['pin'] = self._commit_at(p, pl.get('pin'))
        seen, b = {name}, p
        while b is not None:
            if b in seen:
                raise BpaError('VER.INHERITANCE_CYCLE')
            seen.add(b)
            nxt = self.branches[b].get('parent_line')
            b = nxt['branch'] if nxt else None
        return out

    def set_parent_line(self, branch, parent_line):
        br = self._branch(branch)
        br['parent_line'] = None if parent_line is None else self._parent_line(branch, parent_line)
        return True

    def _expires(self, name):
        br = self.branches[name]
        days = LIFETIME.get(br['kind'])
        if days is None:
            return None
        start = br['active_at'] if br['kind'] == 'draft' else br['created_at']
        return fmt_instant(parse_instant(start) + datetime.timedelta(days=days))

    def _branch_state(self, name):
        exp = self._expires(name)
        return 'archived' if exp is not None and self.now >= exp else 'active'

    def _check_active(self, name):
        self._branch(name)
        if self._branch_state(name) != 'active':
            raise BpaError('VER.BRANCH_ARCHIVED')

    def branch_info(self, branch):
        br = self._branch(branch)
        out = {'name': branch, 'kind': br['kind'], 'state': self._branch_state(branch),
               'created_from': self.commits[br['created_from']]['commit_id'],
               'head': self.commits[br['head']]['commit_id'], 'cseq': self.commits[br['head']]['cseq']}
        exp = self._expires(branch)
        if exp is not None:
            out['expires_at'] = exp
        if br['parent_line']:
            pl = br['parent_line']
            out['parent_line'] = {'branch': pl['branch'], 'mode': pl['mode']}
            if pl['mode'] == 'pinned':
                out['parent_line']['pin'] = self.commits[pl['pin']]['cseq']
        return out

    # ---- processes (STD-TXN-001, -031, -040) ----

    PROCESS_KINDS = ('action', 'schedule', 'job', 'handler', 'merge', 'import', 'agent_run', 'sync', 'kernel')
    TRIGGERS = ('user', 'event', 'schedule', 'job', 'merge', 'external')

    def _process(self, process, kind='action'):
        p = dict(process or {})
        explicit = p.get('process_id') is not None
        if explicit and (not isinstance(p['process_id'], str) or not GRAMMARS['uuid'].match(p['process_id'])):
            raise BpaError('MODEL.INVALID_VALUE', 'process_id is a UUID')
        p.setdefault('kind', kind)
        if p['kind'] not in self.PROCESS_KINDS:
            raise BpaError('MODEL.INVALID_VALUE', 'process kind')
        trig = p.setdefault('trigger', {'type': 'user'})
        if not isinstance(trig, dict) or trig.get('type') not in self.TRIGGERS:
            raise BpaError('MODEL.INVALID_VALUE', 'trigger type')
        parent = p.get('parent_process')
        if parent is None and trig['type'] in ('event', 'job'):
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a process triggered by an event or job names its parent')
        if parent is not None and parent not in self.processes:
            raise BpaError('IAM.NOT_FOUND', 'unknown parent process')
        p.setdefault('initiator', PRINCIPAL)
        for e in p.get('effects') or []:
            if not isinstance(e, dict) or not isinstance(e.get('type'), str):
                raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'an effect has a type')
        if not explicit:
            p['process_id'] = self._process_id()
        p['explicit'] = explicit
        return p

    def _record(self, p, state, commit=None):
        rec = self.processes.get(p['process_id'])
        if rec is None:
            rec = {'process_id': p['process_id'], 'kind': p['kind'], 'initiator': p['initiator'],
                   'trigger': p['trigger'], 'state': state, 'commits': [], 'effects': 0,
                   'explicit': p['explicit']}
            for k in ('parent_process', 'bulk', 'idempotency_key', 'workflow', 'saga', 'saga_step', 'compensations'):
                if p.get(k) is not None:
                    rec[k] = p[k]
            self.processes[p['process_id']] = rec
        rec['state'] = state
        if commit is not None:
            rec['commits'].append(commit)
        return rec

    def process_info(self, process_id):
        rec = self.processes.get(process_id)
        if rec is None:
            raise BpaError('IAM.NOT_FOUND', 'unknown process')
        out = {k: v for k, v in rec.items() if k not in ('commits', 'effects', 'explicit')}
        out['commits'] = [{'branch': b, 'cseq': self.commits[c]['cseq']} for b, c in rec['commits']]
        kids = sorted(k for k, r in self.processes.items() if r.get('bulk') == process_id)
        if kids:
            out['children'] = len(kids)
        return out

    def lineage(self, process_id, direction='ancestors'):
        if process_id not in self.processes:
            raise BpaError('IAM.NOT_FOUND', 'unknown process')
        if direction == 'ancestors':
            out, p = [], self.processes[process_id].get('parent_process')
            while p is not None:
                out.append(p)
                p = self.processes[p].get('parent_process')
            return {'processes': out}
        if direction != 'descendants':
            raise BpaError('MODEL.INVALID_VALUE', 'direction is ancestors or descendants')
        return {'processes': sorted(self._descendants({process_id}) - {process_id})}

    def _descendants(self, roots):
        out, todo = set(roots), list(roots)
        while todo:
            x = todo.pop()
            for k, r in self.processes.items():
                if r.get('parent_process') == x and k not in out:
                    out.add(k)
                    todo.append(k)
        return out

    # ---- commit (STD-TXN-001…050, STD-KIND-020…025) ----

    def commit(self, branch, changes, process=None, tx_time=None):
        if tx_time is not None or any('tx_time' in ch for ch in changes):
            raise BpaError('TIME.RESERVED_FIELD')
        p = self._process(process)
        key = None
        if p.get('idempotency_key') is not None:
            key = (TENANT, p['initiator'], p['idempotency_key'])
            h = content_id('ubos.chunk.v1', {'branch': branch, 'changes': changes})
            known = self.idempotency.get(key)
            if known is not None:
                if known['hash'] != h:
                    raise BpaError('TXN.IDEMPOTENCY_MISMATCH')
                return copy.deepcopy(known['result'])
        self._check_active(branch)
        if p.get('abort'):
            self._record(p, 'aborted')
            return {'state': 'aborted'}
        try:
            result, cid = self._do_commit(branch, changes, p)
        except BpaError:
            self._record(p, 'failed')
            raise
        self._record(p, 'committed', (branch, cid))
        self._effects(p, branch, cid)
        if key is not None:
            self.idempotency[key] = {'hash': h, 'result': copy.deepcopy(result), 'at': self.now}
        return result

    def _do_commit(self, branch, changes, p, via_change_set=False, flow=None):
        """`flow` carries what a process's transitions produced (STD-ISA-017, 1.0-rc3): the new
        state per object and the guard evaluations to record in the rule binding."""
        self._flow = flow or {}
        br = self._branch(branch)
        head = self.commits[br['head']]
        view, prov, _ = self._view(branch, br['head'])
        st = copy.deepcopy(view)
        inh = {oid: (o['head'], o['timeline']) for oid, o in view['objects'].items() if prov[oid][0] != branch}
        parent = self._parent_view(branch, br['head'], None, {})
        self._inherit = {'inh': inh, 'hidden': set(head['hidden']),
                         'parent': set(parent[0]['objects']) if parent else None,
                         'pstate': parent[0] if parent else None}
        cseq = head['cseq'] + 1
        self._cseq = cseq
        result = {'cseq': cseq, 'versions': {}, 'entries': {}, 'retroactive': []}
        if p.get('serializable'):
            for oid, vh in p.get('reads', {}).items():
                if st['objects'].get(oid, {}).get('head') != vh:
                    raise BpaError('TXN.READ_SET_CHANGED')
        self._begin(st, p)
        self._cur_branch, self._cur_p = branch, p
        try:
            appended = self._commit(st, cseq, result, changes)
            warnings = self._enforce(branch, st, view, result, p, via_change_set)
            if warnings:
                result['warnings'] = warnings
            self._write_policy(branch, st, result, via_change_set)
            binding = self._binding(st, result)
            if self._dry:
                raise DryRun(result)
        finally:
            self._exts = None
            self._eff_cache = {}
            self._cur_branch = self._cur_p = self._cur_st = None
            _values.ADDED.clear()
        own = {oid: o for oid, o in st['objects'].items() if oid not in inh or (o['head'], o['timeline']) != inh[oid]}
        self._note_overrides(branch, own, inh)          # STD-VER-082 (1.0-rc8)
        state = {'objects': own, 'ledger': st['ledger']}
        cid = self._new_commit(branch, [br['head']], state, cseq, ops=self._ops, hidden=sorted(self._inherit['hidden']),
                               process_id=p['process_id'] if p['explicit'] else None, rule_binding=binding,
                               derived=self._derived, retroactive=result['retroactive'], appended=appended)
        self._advance(branch, cid)
        self._schedule(branch, cid, st, result)
        result['commit_id'] = self.commits[cid]['commit_id']
        result['tx_time'] = self.commits[cid]['tx_time']
        return result, cid

    def _begin(self, st, p):
        self._cur_st = st                               # STD-OBJ-010 (1.0-rc8): subjects by reference
        self._use_currencies(st)                        # STD-FND-028 (1.0-rc9): codes added on the branch
        self._exts = self._extensions(st)
        self._eff_cache = {}
        self._written, self._ended, self._ops, self._derived = [], [], {}, []
        self._sheets_bound = set()
        self._pid = p['process_id']

    def _op(self, oid, kind, a=None, b=None):
        op = {'kind': kind}
        if a is not None:
            op['from'] = a
        if b is not None:
            op['to'] = b
        self._ops.setdefault(oid, []).append(op)

    def _commit(self, st, cseq, result, changes):
        appended = []
        stale = []
        for ch in changes:
            op = ch.get('op')
            if op in ('update', 'end', 'hide') and ch.get('base') is not None:
                o = st['objects'].get(ch.get('object_id'))
                if o is not None and o['head'] != ch['base']:
                    stale.append(ch['object_id'])
        if stale:
            raise BpaError('TXN.HEAD_MOVED', {'objects': sorted(stale)})
        for ch in changes:
            op = ch.get('op')
            if op in ('create', 'update') and 'schema_version' in ch:
                ch = self._lens_change(st, ch)          # STD-CLS-061 (1.0-rc8)
            if op == 'create':
                self._create(st, ch, result)
            elif op in ('update', 'end'):
                self._update(st, ch, result, op)
            elif op in ('append', 'reverse', 'adjust'):
                appended.append(self._append(st, ch, result, op, cseq))
            elif op in ('hide', 'unhide'):
                self._hide(st, ch, op)
            else:
                raise BpaError('MODEL.INVALID_VALUE', 'unknown change')
        self._check_double_entry(appended)
        self._check_finance(st, appended)               # 1.0-rc9 (DEC-098)
        self._relationships(st, result)
        self._integrity(st, appended)
        self._check_extensions(st)
        self._check_sheets(st)
        self._check_lifecycle_objects(st)
        return [e['entry_id'] for e in appended]

    def _hide(self, st, ch, op):
        """Hide markers of inherited objects (STD-VER-080)."""
        inh = self._inherit
        oid = ch.get('object_id')
        if inh['parent'] is None:
            raise BpaError('MODEL.INVALID_VALUE', 'hide and unhide need a parent line')
        if op == 'hide':
            if oid not in st['objects']:
                raise BpaError('IAM.NOT_FOUND', 'unknown object')
            if oid not in inh['parent']:
                raise BpaError('MODEL.INVALID_VALUE', 'only objects of the parent line can be hidden')
            if ch.get('base') is None:
                raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'hide names its base (STD-TXN-020)')
            del st['objects'][oid]
            inh['hidden'].add(oid)
        else:
            if oid not in inh['hidden']:
                raise BpaError('IAM.NOT_FOUND', 'object is not hidden')
            inh['hidden'].discard(oid)
            st['objects'][oid] = copy.deepcopy(inh['pstate']['objects'][oid])
            inh['inh'][oid] = (st['objects'][oid]['head'], st['objects'][oid]['timeline'])
        self._op(oid, op)

    def _write_policy(self, branch, st, result, via_change_set):
        """STD-VER-012: on protected branches, change_set classes are written only by the
        merge of an approved change set."""
        if via_change_set or self.branches[branch]['kind'] not in PROTECTED:
            return
        classes = [st['objects'][oid]['class'] for oid in result['versions']]
        for q in st['ledger'].values():
            classes += [e['class'] for e in q if e['entry_id'] in result['entries']]
        if any(self._cls(c)['write_policy'] == 'change_set' for c in classes):
            raise BpaError('VER.CHANGE_SET_REQUIRED')

    def _binding(self, st, result, logic=()):
        """DAT-RuleBinding of a commit (STD-TIME-040): the class versions evaluated for its
        changes, the extensions applied to them and the logic that produced derived values."""
        names = {st['objects'][oid]['class'] for oid in result['versions'] if oid in st['objects']}
        for q in st['ledger'].values():
            names |= {e['class'] for e in q if e['entry_id'] in result['entries']}
        lin = set()
        for n in names:
            lin |= set(self._cls(n)['linearization'])
        items = [{'class': c, 'class_version': content_id('ubos.chunk.v1', self._decl(c))} for c in sorted(lin)]
        for oid, vh, body in self._extension_objects(st):
            if body.get('target') in lin:
                items.append({'class': 'ClassExtension', 'object_id': oid, 'class_version': vh})
        # STD-CLS-004, STD-TIME-040 (1.0-rc3): the lifecycle versions of the changed classes
        for n in sorted(names):
            lc = self._lifecycle(st, n)
            if lc and not any(i.get('object_id') == lc[1] for i in items):
                items.append({'class': 'Lifecycle', 'object_id': lc[1], 'class_version': lc[2]})
        out = {'classes': items}
        lg = sorted(set(logic) | {x for d in self._derived for x in self.derivations[d[3]]['logic']})
        if lg:
            out['logic'] = lg
        # STD-TIME-040, STD-ISA-053 (1.0-rc8): the decision table versions a process evaluated
        tables = (getattr(self, '_flow', None) or {}).get('tables') or []
        if tables:
            out['tables'] = sorted(tables)
        guards = (getattr(self, '_flow', None) or {}).get('guards') or []
        if guards:
            out['guards'] = list(guards)
        # STD-TIME-040 (1.0-rc7): the governance sheets evaluated by the commit enforcement
        bound = getattr(self, '_sheets_bound', None) or set()
        if bound:
            out['sheets'] = [vh for _, (vh, _b) in sorted(self._sheet_objects(st).items()) if vh in bound]
        return out
    def _entry_owner(self, st, entry_id):
        for k, q in st['ledger'].items():
            for e in q:
                if e['entry_id'] == entry_id:
                    return k, e
        return None, None

    def _create(self, st, ch, result):
        oid, cls = ch['object_id'], ch['class']
        eff = self._cls(cls)
        if not GRAMMARS['uuid'].match(oid):
            raise BpaError('MODEL.INVALID_VALUE', 'object_id is a UUID')
        if self.decls.get(cls, {}).get('abstract'):
            raise BpaError('MODEL.ABSTRACT_INSTANTIATION')
        # STD-KIND-026 (1.0-rc8): close artefacts are written by close_period only
        if (eff['kind'] == 'system' or eff['kind'] == 'ledger') and not (
                cls == 'CloseArtefact' and getattr(self, '_period_op', None) == ('closing', 'closed')) and not (
                cls == 'SimulationRun' and getattr(self, '_sim_op', False)) and cls not in self._sys_ok:
            raise BpaError('LOGIC.FORBIDDEN_TARGET')
        if oid in st['objects'] or self._entry_owner(st, oid)[1]:
            raise BpaError('TXN.ALREADY_EXISTS')
        check_name('slug', ch['slug'])
        body = ch.get('body', {})
        if cls == 'GovernanceSheet':
            body = self._sheet_body(body)
        # STD-CLS-004 (1.0-rc3): the lifecycle state is set by the kernel, never by a change
        lc = self._lifecycle(st, cls)
        if 'state' in ch or (lc and isinstance(body, dict) and 'state' in body):
            raise BpaError('FLOW.STATE_DIRECT_WRITE', 'the lifecycle state changes by transitions only')
        state = (self._flow.get('transitions') or {}).get(oid, {}).get('to', lc[0]['initial']) if lc else None
        self._kernel_fields(cls, body)
        body = self._apply_defaults(st, cls, oid, body)
        body = self._check_body(cls, body)
        self._check_special(cls, body)
        valid = ch.get('valid') or {}
        a = valid.get('from') or self._default_valid(cls)
        rid = self._derivation(ch)
        vh = self._version(oid, cls, ch['slug'], body, {'from': a, 'to': None}, provenance=rid, state=state)
        st['objects'][oid] = {'class': cls, 'kind': eff['kind'], 'slug': ch['slug'], 'head': vh,
                              'timeline': tl_apply([], 'create', vh, a)}
        result['versions'][oid] = vh
        self._written.append((oid, vh))
        self._op(oid, 'create', a)
        if rid:
            self._derived.append(('object', oid, vh, rid))

    def _update(self, st, ch, result, op):
        oid = ch.get('object_id')
        o = st['objects'].get(oid)
        if o is None:
            if self._entry_owner(st, oid)[1]:
                raise BpaError('LEDG.IMMUTABLE_ENTRY')
            raise BpaError('IAM.NOT_FOUND', 'unknown object')
        if o['kind'] == 'system' and o['class'] not in self._sys_ok:
            raise BpaError('LOGIC.FORBIDDEN_TARGET')
        self._kernel_fields(o['class'], ch.get('body'))
        if ch.get('base') is None:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'updates name their base (STD-TXN-020)')
        base = self.versions[o['head']]
        valid = ch.get('valid') or {}
        # STD-CLS-004 (1.0-rc3): the state moves by transitions only; field rules of the state
        lc = self._lifecycle(st, o['class'])
        if 'state' in ch or (lc and isinstance(ch.get('body'), dict) and 'state' in ch['body']):
            raise BpaError('FLOW.STATE_DIRECT_WRITE', 'the lifecycle state changes by transitions only')
        tr = (self._flow.get('transitions') or {}).get(oid)
        state = tr['to'] if tr else base.get('state')
        if lc and op != 'end' and base.get('state') is not None:
            rules = (lc[0].get('field_rules') or {}).get(base['state']) or {}
            effect_fields = set(tr.get('fields') or ()) if tr else set()
            for f in rules.get('read_only') or []:
                if f in (ch.get('body') or {}) and f not in effect_fields:
                    raise BpaError('FLOW.FIELD_RULE_VIOLATED', {'state': base['state'], 'field': f, 'rule': 'read_only'})
        if op == 'end':
            d = valid.get('from') or self._default_valid(o['class'])
            vh = self._version(oid, o['class'], o['slug'], base['body'], {'from': d, 'to': None},
                               predecessor=o['head'], tombstone=True, state=state)
            o['timeline'] = tl_apply(o['timeline'], 'end', vh, d)
            self._ended.append((oid, d))
            self._op(oid, 'end', d)
            if d < self._default_valid(o['class']):
                result['retroactive'].append({'object_id': oid, 'from': d, 'to': None})
        else:
            body = dict(self._current_body(base))       # STD-CLS-061 (1.0-rc8)
            body.update(self._check_body(o['class'], ch.get('body', {}), partial=True))
            if o['class'] == 'GovernanceSheet':
                if 'source' in ch.get('body', {}) and 'ast' not in ch['body']:
                    body.pop('ast', None)
                body = self._sheet_body(body)
            self._check_body(o['class'], body)
            self._check_special(o['class'], body)
            # STD-KIND-025 (1.0-rc8): period transitions
            self._check_period_transition(o['class'], base['body'].get('state'), body.get('state'))
            slug = ch.get('slug', o['slug'])
            check_name('slug', slug)
            kind = valid.get('kind', 'supersede_from')
            a = valid.get('from') or self._default_valid(o['class'])
            b = valid.get('to')
            to = b if kind == 'set_period' else (None if kind == 'replace_future' else tl_range(o['timeline'], kind, a)[1])
            rid = self._derivation(ch)
            vh = self._version(oid, o['class'], slug, body, {'from': a, 'to': to}, predecessor=o['head'],
                               provenance=rid, state=state)
            rng = tl_range(o['timeline'], kind, a, b)
            o['timeline'] = tl_apply(o['timeline'], kind, vh, a, b)
            o['slug'] = slug
            self._written.append((oid, vh))
            self._op(oid, kind, a, b if kind == 'set_period' else None)
            if rid:
                self._derived.append(('object', oid, vh, rid))
            if rng[0] < self._default_valid(o['class']):
                result['retroactive'].append({'object_id': oid, 'from': rng[0], 'to': rng[1]})
        o['head'] = vh
        result['versions'][oid] = vh

    def _amounts(self, cls):
        return self._decl(cls).get('ledger', {}).get('amounts', [])

    def _append(self, st, ch, result, op, cseq):
        if op == 'append':
            cls = ch['class']
            eff = self._cls(cls)
            if eff['kind'] != 'ledger':
                raise BpaError('LOGIC.FORBIDDEN_TARGET')
            # STD-KIND-028 (1.0-rc8): match records are appended by match and unmatch only
            if cls == 'MatchRecord' and not getattr(self, '_match_op', False):
                raise BpaError('LOGIC.FORBIDDEN_TARGET')
            spec = self._decl(cls)['ledger']
            lk = ch['ledger_key']
            if sorted(lk) != sorted(spec['key']):
                raise BpaError('MODEL.INVALID_VALUE', 'ledger key fields')
            for k in spec['key']:
                f = eff['fields'][k]
                lk[k] = validate_value(f['type'], f.get('constraints'), lk[k])
            body = self._check_body(cls, ch.get('body', {}))
            entry = {'entry_id': ch['entry_id'], 'class': cls, 'ledger_key': lk,
                     'valid_at': ch.get('valid_at') or self._default_valid(cls), 'body': body}
            if ch.get('posting_group'):
                entry['posting_group'] = ch['posting_group']
            if ch.get('source_refs') is not None:
                refs = ch['source_refs']
                if not isinstance(refs, list) or not all(isinstance(r, str) and REFERENCE.match(r) for r in refs):
                    raise BpaError('MODEL.INVALID_VALUE', 'source_refs are canonical URIs')
                entry['source_refs'] = list(refs)
        else:
            key, target = self._entry_owner(st, ch['entry_id'])
            if target is None:
                raise BpaError('IAM.NOT_FOUND', 'unknown entry')
            cls = target['class']
            q = st['ledger'][key]
            if op == 'reverse':
                if target.get('correction_type') == 'reversal':
                    raise BpaError('LEDG.REVERSAL_NOT_REVERSIBLE')
                if any(e.get('correction_type') == 'reversal' and target['entry_id'] in e.get('corrects', [])
                       for e in q):
                    raise BpaError('LEDG.ALREADY_REVERSED')
                body = dict(target['body'])
                for a in self._amounts(cls):
                    body[a] = self._negate(body[a])
            else:
                body = self._check_body(cls, ch.get('body', {}))
            entry = {'entry_id': ch['new_entry_id'], 'class': cls, 'ledger_key': target['ledger_key'],
                     'valid_at': ch.get('valid_at') or target['valid_at'], 'body': body,
                     'corrects': [target['entry_id']],
                     'correction_type': 'reversal' if op == 'reverse' else 'adjustment'}
            if target.get('posting_group') and op == 'reverse':
                entry['posting_group'] = target['posting_group'] + '/reversal'
        if not GRAMMARS['uuid'].match(entry['entry_id']):
            raise BpaError('MODEL.INVALID_VALUE', 'entry_id is a UUID')
        if self._entry_owner(st, entry['entry_id'])[1] or entry['entry_id'] in st['objects']:
            raise BpaError('TXN.ALREADY_EXISTS')
        self._check_period(st, cls)
        key = (cls, canonical(entry['ledger_key']))
        q = st['ledger'].setdefault(key, [])
        entry['seq'] = len(q) + 1
        entry['cseq'] = cseq
        rid = self._derivation(ch)
        if rid:
            entry['provenance_ref'] = rid
        entry['class_version'] = content_id('ubos.chunk.v1', self._decl(entry['class']))
        entry['entry_hash'] = self.entry_hash(entry)
        if rid:
            self._derived.append(('entry', entry['entry_id'], entry['entry_hash'], rid))
        q.append(entry)
        result['entries'][entry['entry_id']] = {'seq': entry['seq']}
        return entry

    @staticmethod
    def _negate(v):
        if isinstance(v, dict):
            return {'amount': fmt_decimal(-parse_decimal(v['amount']), MINOR_UNITS[v['currency']]),
                    'currency': v['currency']}
        return fmt_decimal(-parse_decimal(v))

    def entry_hash(self, e):
        rec = {k: e[k] for k in ('entry_id', 'class', 'ledger_key', 'valid_at', 'corrects', 'correction_type',
                                 'posting_group', 'provenance_ref', 'source_refs') if k in e}
        rec['class_version'] = e.get('class_version') or content_id('ubos.chunk.v1', self._decl(e['class']))
        rec['body_ref'] = content_id('ubos.chunk.v1', e['body'])
        return content_id('ubos.entry.v1', rec)

    def _check_period(self, st, cls):
        scope = self._decl(cls).get('ledger', {}).get('period_scope')
        if not scope:
            return
        day = self._today()
        for o in st['objects'].values():
            if o['class'] != scope:
                continue
            seg = tl_at(o['timeline'], day)
            body = self.versions[o['head']]['body'] if seg is None else self.versions[seg[2]]['body']
            if body.get('state') == 'closed' and body['start'] <= day < body['end']:
                raise BpaError('LEDG.PERIOD_CLOSED')

    def _check_double_entry(self, appended):
        groups = {}
        for e in appended:
            if not self._decl(e['class']).get('ledger', {}).get('double_entry'):
                continue
            g = groups.setdefault((e['class'], e.get('posting_group', e['entry_id'])), {})
            for a in self._amounts(e['class']):
                v = e['body'][a]
                cur = v['currency'] if isinstance(v, dict) else ''
                amt = parse_decimal(v['amount'] if isinstance(v, dict) else v)
                g[cur] = g.get(cur, 0) + amt
        for totals in groups.values():
            if any(t != 0 for t in totals.values()):
                raise BpaError('LEDG.UNBALANCED')

    # ---- extensions (STD-CLS-030) ----

    def _extension_objects(self, st):
        out = []
        for oid in sorted(st['objects']):
            o = st['objects'][oid]
            if o['class'] == 'ClassExtension' and not self.versions[o['head']]['tombstone']:
                out.append((oid, o['head'], self.versions[o['head']]['body']))
        return out

    def _extensions(self, st):
        return tuple(body for _, _, body in self._extension_objects(st))

    def _check_extensions(self, st):
        """Every extension written in this commit must apply to its target."""
        exts = self._extensions(st)
        for oid, vh in self._written:
            if st['objects'][oid]['class'] != 'ClassExtension':
                continue
            target = self.versions[vh]['body']['target']
            if target not in self.classes:
                raise BpaError('MODEL.REFERENCE_INVALID', 'unknown target class %s' % target)
            for name, e in self.classes.items():
                if target in e['linearization']:
                    effective_class(list(self.decls.values()), name, exts)

    # ---- references and relationships (STD-CLS-040, -041) ----

    def _uri(self, o):
        return 'ubos://%s/%s/%s' % (TENANT, o['class'], o['slug'])

    def _resolve(self, st, uri):
        """Object ID of the object a canonical URI names on this state, or None."""
        if not REFERENCE.match(uri):
            return None
        authority, cls, slug = uri[len('ubos://'):].split('/')
        if authority != TENANT:
            return None
        for oid, o in st['objects'].items():
            if o['class'] == cls and o['slug'] == slug:
                return oid
        return None

    def _live(self, st, oid):
        o = st['objects'].get(oid)
        return o is not None and not self.versions[o['head']]['tombstone']

    def _refs(self, eff, body):
        """[(field, uri)] of the reference values of a body."""
        out = []
        for name, f in eff['fields'].items():
            v = body.get(name)
            if v is None:
                continue
            if f['type'] == 'reference':
                out.append((name, v))
            elif is_reference_list(f):
                out += [(name, x) for x in v]
        return out

    def _covered(self, tl, f, t):
        """True when the segments of a timeline cover [f, t) without a gap."""
        at = f
        for sf, stt, _ in sorted(tl, key=lambda x: x[0]):
            if _lt(at, sf):
                break
            if _lt(at, stt) or stt is None:
                at = stt
                if at is None or (t is not None and not _lt(at, t)):
                    return True
        return at is None or (t is not None and not _lt(at, t))

    def _target_ok(self, st, uri, field, eff, period=None, instant=None):
        oid = self._resolve(st, uri)
        if oid is None:
            raise BpaError('MODEL.REFERENCE_INVALID', 'reference %s does not resolve' % uri)
        o = st['objects'][oid]
        tc = (eff['fields'][field].get('constraints') or {}).get('target_class')
        item = (eff['fields'][field].get('constraints') or {}).get('item_type') or {}
        tc = tc or (item.get('constraints') or {}).get('target_class')
        if tc and tc not in self._cls(o['class'])['linearization']:
            raise BpaError('MODEL.REFERENCE_INVALID', 'target is not a %s' % tc)
        if period is not None and not self._covered(o['timeline'], period[0], period[1]):
            raise BpaError('MODEL.REFERENCE_INVALID', 'target not valid for the whole period')
        if instant is not None and tl_at(o['timeline'], instant) is None:
            raise BpaError('MODEL.REFERENCE_INVALID', 'target not valid at the entry time')
        return oid

    def _integrity(self, st, appended):
        for oid, vh in self._written:
            v = self.versions[vh]
            if v['tombstone'] or not self._live(st, oid):
                continue
            eff = self._cls(v['class'])
            for field, uri in self._refs(eff, v['body']):
                self._target_ok(st, uri, field, eff, period=(v['valid']['from'], v['valid']['to']))
            self._links(st, oid, eff, v['body'])
        for e in appended:
            eff = self._cls(e['class'])
            for field, uri in self._refs(eff, e['body']):
                self._target_ok(st, uri, field, eff, instant=e['valid_at'])

    def _links(self, st, oid, eff, body):
        """Link objects of relationships with an attributes class (STD-CLS-040)."""
        me = self._uri(st['objects'][oid])
        for name, f in eff['fields'].items():
            rel = f.get('relationship') or {}
            if not rel.get('attributes_class') or body.get(name) is None:
                continue
            for uri in (body[name] if isinstance(body[name], list) else [body[name]]):
                lid = self._resolve(st, uri)
                link = st['objects'].get(lid) if lid else None
                if link is None or rel['attributes_class'] not in self._cls(link['class'])['linearization']:
                    raise BpaError('MODEL.REFERENCE_INVALID', 'link is not a %s' % rel['attributes_class'])
                if self.versions[link['head']]['body'].get('source') != me:
                    raise BpaError('MODEL.REFERENCE_INVALID', 'link does not reference its source')

    def _relations(self, st):
        """[(source oid, field, relationship, target oid)] over live sources."""
        out = []
        for oid in sorted(st['objects']):
            if not self._live(st, oid):
                continue
            o = st['objects'][oid]
            eff = self._cls(o['class'])
            body = self.versions[o['head']]['body']
            for field, uri in self._refs(eff, body):
                rel = eff['fields'][field].get('relationship')
                tid = self._resolve(st, uri)
                if rel and tid:
                    out.append((oid, field, rel, tid))
        return out

    def _end_object(self, st, oid, d, result):
        o = st['objects'][oid]
        base = self.versions[o['head']]
        vh = self._version(oid, o['class'], o['slug'], base['body'], {'from': d, 'to': None},
                           predecessor=o['head'], tombstone=True, state=base.get('state'))
        o['timeline'] = tl_apply(o['timeline'], 'end', vh, d)
        o['head'] = vh
        result['versions'][oid] = vh
        self._ended.append((oid, d))
        self._op(oid, 'end', d)
        if d < self._default_valid(o['class']):
            result['retroactive'].append({'object_id': oid, 'from': d, 'to': None})

    def _relationships(self, st, result):
        """on_end rules, composition and cardinality (STD-CLS-040)."""
        done = set()
        while True:
            todo = [(x, d) for x, d in self._ended if x not in done]
            if not todo:
                break
            for x, d in todo:
                done.add(x)
                o = st['objects'][x]
                eff = self._cls(o['class'])
                for field, uri in self._refs(eff, self.versions[o['head']]['body']):
                    rel = eff['fields'][field].get('relationship') or {}
                    tid = self._resolve(st, uri)
                    if rel.get('ownership') == 'composition' and tid and self._live(st, tid):
                        self._end_object(st, tid, d, result)
                for src, field, rel, tid in self._relations(st):
                    if tid != x or src == x:
                        continue
                    rule = rel.get('on_end', 'restrict')
                    if rule == 'restrict':
                        raise BpaError('MODEL.REFERENCE_RESTRICTED', 'referenced by %s' % src)
                    if rule == 'cascade':
                        if self._live(st, src):
                            self._end_object(st, src, d, result)
                        continue
                    o = st['objects'][src]
                    body = dict(self.versions[o['head']]['body'])
                    uri = self._uri(st['objects'][x])
                    body[field] = [u for u in body[field] if u != uri] if isinstance(body[field], list) else None
                    if body[field] is None:
                        del body[field]
                    self._check_body(o['class'], body)
                    rng = tl_range(o['timeline'], 'supersede_from', d)
                    vh = self._version(src, o['class'], o['slug'], body, {'from': rng[0], 'to': rng[1]},
                                       predecessor=o['head'], state=self.versions[o['head']].get('state'))
                    o['timeline'] = tl_apply(o['timeline'], 'supersede_from', vh, d)
                    o['head'] = vh
                    result['versions'][src] = vh
                    self._op(src, 'supersede_from', d)
                    if d < self._default_valid(o['class']):
                        result['retroactive'].append({'object_id': src, 'from': rng[0], 'to': rng[1]})
        self._cardinality(st)

    def _cardinality(self, st):
        counts = {}
        for src, field, rel, tid in self._relations(st):
            if rel['cardinality'] in ('one_to_one', 'one_to_many'):
                counts.setdefault((field, tid), set()).add(src)
        if any(len(v) > 1 for v in counts.values()):
            raise BpaError('MODEL.CARDINALITY_VIOLATED')

    # ---- queries of the class system (STD-CLS-022, -030, -040) ----

    def list_objects(self, branch, cls, only=False, sel=None):
        st = self._state(branch, sel)[0]
        if cls not in self.classes and cls not in GENESIS:
            raise BpaError('MODEL.REFERENCE_INVALID', 'unknown class %s' % cls)
        out = []
        for oid, o in st['objects'].items():
            if not self._live(st, oid):
                continue
            lin = self._cls(o['class'])['linearization']
            if (o['class'] == cls) if only else (cls in lin):
                out.append(oid)
        return {'objects': sorted(out)}

    def traverse(self, branch, object_id, relation, sel=None):
        st = self._state(branch, sel)[0]
        o = st['objects'].get(object_id)
        if o is None:
            raise BpaError('IAM.NOT_FOUND', 'unknown object')
        eff = self._cls(o['class'])
        f = eff['fields'].get(relation)
        out = set()
        if f is not None and f.get('relationship'):
            rel = f['relationship']
            for fld, uri in self._refs(eff, self.versions[o['head']]['body']):
                if fld != relation:
                    continue
                tid = self._resolve(st, uri)
                if tid and rel.get('attributes_class'):
                    tid = self._resolve(st, self.versions[st['objects'][tid]['head']]['body'].get('target', ''))
                if tid and self._live(st, tid):
                    out.add(tid)
            return {'objects': sorted(out)}
        known = False
        for src, field, rel, tid in self._relations(st):
            if rel.get('inverse') != relation:
                continue
            known = True
            if rel.get('attributes_class'):
                link = self.versions[st['objects'][tid]['head']]['body']
                if self._resolve(st, link.get('target', '')) == object_id:
                    out.add(src)
            elif tid == object_id:
                out.add(src)
        if not known and not any((fd.get('relationship') or {}).get('inverse') == relation
                                 for c in self.classes.values() for fd in c['fields'].values()):
            raise BpaError('MODEL.REFERENCE_INVALID', 'unknown relation %s' % relation)
        return {'objects': sorted(out)}

    def class_at(self, branch, cls, sel=None):
        st = self._state(branch, sel)[0]
        if cls not in self.classes and cls not in GENESIS:
            raise BpaError('MODEL.REFERENCE_INVALID', 'unknown class %s' % cls)
        return public(effective_class(list(self.decls.values()), cls, self._extensions(st)))

    # ---- reads (STD-TIME-012, -020) ----

    def read(self, branch, object_id, asof=None, sel=None):
        sel = sel or {}
        st, prov, ev, _ = self._state(branch, sel)
        o = st['objects'].get(object_id)
        if o is None:
            raise BpaError('IAM.NOT_FOUND', 'unknown object')
        a = asof or self._default_valid(o['class'])
        seg = tl_at(o['timeline'], a)
        if seg is None:
            raise BpaError('QRY.NOT_FOUND_AT_TIME')
        self._check_keys(self.versions[seg[2]])         # DSN-DVM-834 (1.0-rc8)
        out = self._lens_read(self._segment(seg), self.versions[seg[2]], sel)
        out = self._view_read(st, branch, object_id, out, sel)     # STD-LGS-051, -052 (1.0-rc8)
        out['evaluated_at'] = ev
        if sel.get('provenance'):
            b, c = prov[object_id]
            out['provenance'] = {'branch': b, 'cseq': self.commits[c]['cseq'], 'commit_id': self.commits[c]['commit_id']}
        if sel.get('follow') is not None:
            out['targets'] = self._follow(st, o, seg, sel['follow'], a)
        return out

    def _segment(self, seg):
        v = self.versions[seg[2]]
        out = {'version_hash': seg[2], 'body': v['body'], 'slug': v['slug'], 'valid': {'from': seg[0], 'to': seg[1]}}
        if v.get('provenance_ref'):
            out['provenance_ref'] = v['provenance_ref']
        if v.get('state') is not None:
            out['state'] = v['state']
        return out

    def _follow(self, st, o, seg, field, a):
        """Targets of a reference field at the coordinates of the source (STD-TIME-013)."""
        eff = self._cls(o['class'])
        if field not in eff['fields']:
            raise BpaError('MODEL.INVALID_VALUE', 'unknown field %s' % field)
        out = []
        for f, uri in self._refs(eff, self.versions[seg[2]]['body']):
            if f != field:
                continue
            tid = self._resolve(st, uri)
            tseg = tl_at(st['objects'][tid]['timeline'], a) if tid else None
            if tseg is not None:
                out.append(dict(self._segment(tseg), object_id=tid))
        return out

    def read_ledger(self, branch, cls, ledger_key, asof=None, sel=None):
        st, _, ev, _ = self._state(branch, sel)
        q = st['ledger'].get((cls, canonical(ledger_key)), [])
        out = []
        for e in q:
            if asof is not None and e['valid_at'] > asof:
                continue
            rec = {k: e[k] for k in ('entry_id', 'seq', 'valid_at', 'body', 'corrects', 'correction_type') if k in e}
            rec['entry_hash'] = e['entry_hash']
            if e.get('provenance_ref'):
                rec['provenance_ref'] = e['provenance_ref']
            out.append(rec)
        return {'entries': out, 'evaluated_at': ev}

    # ---- state tree (STD-PROOF-001) ----

    def state_entries(self, st):
        ent = [['o/' + oid, o['head']] for oid, o in st['objects'].items() if o['kind'] != 'system']
        for (cls, key), q in st['ledger'].items():
            ch = content_id('ubos.chunk.v1', cls).split(':')[1]
            kh = content_id('ubos.chunk.v1', q[0]['ledger_key']).split(':')[1] if q else ''
            for e in q:
                ent.append(['l/%s/%s/%020d' % (ch, kh, e['seq']), e['entry_hash']])
        return sorted(ent, key=lambda x: x[0].encode('utf-8'))

    def export_state(self, branch, sel=None):
        st = self.commits[self._select(branch, sel or {})[0]]['state']
        ent = self.state_entries(st)
        return {'root': tree_root(ent), 'entries': ent}

    # ---- merge (STD-VER-040…056, DEC-032) ----

    def _anc(self, cid):
        seen, stack = set(), [cid]
        while stack:
            c = stack.pop()
            if c not in seen:
                seen.add(c)
                stack.extend(self.commits[c]['parents'])
        return seen

    def _best(self, xs, ys):
        ax = set().union(*(self._anc(x) for x in xs))
        ay = set().union(*(self._anc(y) for y in ys))
        common = ax & ay
        return sorted((c for c in common if not any(c != d and c in self._anc(d) for d in common)),
                      key=lambda c: self.commits[c]['commit_id'])

    def _base_state(self, xs, ys):
        """Merge base of two sets of commits; a virtual base for several best ancestors."""
        best = self._best(xs, ys)
        acc, tips = self.commits[best[0]]['state'], {best[0]}
        for c in best[1:]:
            b = self._base_state(tips, {c})
            acc, _ = self._merge_states(b, acc, self.commits[c]['state'], virtual=True)
            tips.add(c)
        return acc

    def _merge_states(self, B, S, T, virtual=False, cseq=None, branch=None, keep=False):
        conflicts, objects = [], {}
        for oid in sorted(set(B['objects']) | set(S['objects']) | set(T['objects'])):
            o, c = self._merge_object(oid, B['objects'].get(oid), S['objects'].get(oid), T['objects'].get(oid), virtual)
            if c and keep and not virtual:
                o = copy.deepcopy(T['objects'].get(oid))
            conflicts += c
            if o is not None:
                objects[oid] = o
        ledger = copy.deepcopy(T['ledger'])
        for key, q in S['ledger'].items():
            tq = ledger.setdefault(key, [])
            for e in q:
                if any(x['entry_id'] == e['entry_id'] for x in tq):
                    continue
                for tgt in e.get('corrects', []):
                    if not any(x['entry_id'] == tgt for x in tq) and not virtual:
                        raise BpaError('LEDG.CORRECTION_TARGET_MISSING')
                ne = dict(e, seq=len(tq) + 1, source_seq={'branch': branch, 'seq': e['seq']})
                if cseq is not None:
                    ne['cseq'] = cseq
                tq.append(ne)
        return {'objects': objects, 'ledger': ledger}, conflicts

    @staticmethod
    def _same(a, b):
        return (a is None and b is None) or (a is not None and b is not None and
                                             a['timeline'] == b['timeline'] and a['head'] == b['head'])

    def _merge_object(self, oid, bo, so, to_, virtual):
        if self._same(so, to_) or self._same(so, bo):
            return copy.deepcopy(to_), []
        if self._same(to_, bo):
            return copy.deepcopy(so), []
        meta = to_ or so
        if meta['kind'] == 'system':
            return copy.deepcopy(to_), []
        tls = [x['timeline'] if x else [] for x in (bo, so, to_)]
        cuts = sorted({s[0] for tl in tls for s in tl} | {s[1] for tl in tls for s in tl if s[1] is not None})
        bounds = cuts + [None]
        pieces, conflicts = [], []
        for i in range(len(cuts)):
            a, b = bounds[i], bounds[i + 1]
            vb, vs, vt = (tl_at(tl, a) for tl in tls)
            vb, vs, vt = (x[2] if x else None for x in (vb, vs, vt))
            if vs == vt or vs == vb:
                r = vt
            elif vt == vb:
                r = vs
            elif vs is None or vt is None or meta['kind'] == 'raw_document':
                kind = 'raw_document' if vs is not None and vt is not None else 'ended_vs_changed'
                res = None if virtual else self._resolution(oid, None, vb, vs, vt)
                if res is None:
                    conflicts.append({'object_id': oid, 'kind': kind})
                    r = vb
                else:
                    r = res[0]
            else:
                r, fc = self._merge_body(oid, vb, vs, vt, virtual)
                conflicts += fc
            pieces.append([a, b, r])
        if conflicts and not virtual:
            return None, conflicts
        timeline, created, head = [], [], None
        for a, b, r in pieces:
            if r is None:
                continue
            if timeline and timeline[-1][1] == a and timeline[-1][2] == r:
                timeline[-1][1] = b
            else:
                timeline.append([a, b, r])
        final = []
        for a, b, r in timeline:
            if isinstance(r, tuple):                      # a merged body: a new version
                body = dict(r)
                pred = tl_at(to_['timeline'], a) if to_ else None
                slug = self.versions[pred[2]]['slug'] if pred else meta['slug']
                vh = self._version(oid, meta['class'], slug, body, {'from': a, 'to': b},
                                   predecessor=pred[2] if pred else None,
                                   state=self.versions[pred[2]].get('state') if pred else None)
                created.append(vh)
                final.append([a, b, vh])
            else:
                final.append([a, b, r])
        if created:
            head = created[-1]
        elif so and final == so['timeline']:
            head = so['head']
        else:
            head = to_['head'] if to_ else so['head']
        out = copy.deepcopy(meta)
        out['timeline'], out['head'] = final, head
        out['slug'] = self.versions[head]['slug']
        return out, conflicts

    def _merge_body(self, oid, vb, vs, vt, virtual):
        # STD-CLS-061 (1.0-rc8): base, source and target in the current schema version
        B = self._current_body(self.versions[vb]) if vb else {}
        S, T = self._current_body(self.versions[vs]), self._current_body(self.versions[vt])
        absent = object()
        out, conflicts = {}, []
        for k in sorted(set(B) | set(S) | set(T)):
            b, s, t = B.get(k, absent), S.get(k, absent), T.get(k, absent)
            if s == t or s == b:
                r = t
            elif t == b:
                r = s
            else:
                res = None if virtual else self._resolution(oid, '/' + k, b, s, t, absent)
                if res is None:
                    conflicts.append({'object_id': oid, 'kind': 'field', 'path': '/' + k})
                    r = b
                else:
                    r = res[0]
            if r is not absent:
                out[k] = r
        return tuple(sorted(out.items(), key=lambda kv: kv[0])), conflicts

    RES_CHOICES = ('source', 'target', 'base', 'custom')

    def _resolutions(self, items):
        """Recorded resolutions supplied with a merge (STD-VER-058)."""
        out = {}
        for r in items or []:
            if not isinstance(r, dict) or not r.get('object_id') or not r.get('resolver') or 'choice' not in r:
                raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a resolution names object_id, choice and resolver')
            if r['choice'] not in self.RES_CHOICES or (r['choice'] == 'custom') != ('value' in r):
                raise BpaError('MODEL.INVALID_VALUE', 'choice is source, target, base, or custom with a value')
            out[(r['object_id'], r.get('path'))] = r
        return out

    def _resolution(self, oid, path, b, s, t, absent=None):
        """The resolved value of a conflict as a 1-tuple, or None when unresolved."""
        r = getattr(self, '_res', {}).get((oid, path))
        if r is None:
            return None
        if r['choice'] == 'custom':
            if path is None:
                raise BpaError('MODEL.INVALID_VALUE', 'an object-level conflict has no custom value')
            v = r['value']
        else:
            v = {'source': s, 'target': t, 'base': b}[r['choice']]
        if path is None:
            chosen = None if v is None else self.versions[v]['body']
        else:
            chosen = None if v is absent else v
        rec = {'object_id': oid, 'choice': r['choice'], 'value_hash': content_id('ubos.chunk.v1', chosen),
               'resolver': r['resolver']}
        if path is not None:
            rec['path'] = path
        if r.get('reason') is not None:
            rec['reason'] = r['reason']
        self._used[(oid, path)] = rec
        return (v,)

    def _merge_allowed(self, source, target, via_change_set=False):
        """STD-VER-011 merge targets per branch kind (DEC-041)."""
        sk, tk = self.branches[source]['kind'], self.branches[target]['kind']
        if sk == 'sandbox' or (tk in PROTECTED and tk not in MERGE_TARGETS[sk]):
            raise BpaError('VER.MERGE_NOT_ALLOWED', '%s branches do not merge into %s' % (sk, tk))
        if tk in PROTECTED and sk == 'agent' and not via_change_set:
            raise BpaError('VER.CHANGE_SET_REQUIRED', 'agent branches reach protected targets by change set only')

    def _changed(self, T, st):
        """Objects whose state differs from T, and entries absent from T."""
        objs = [oid for oid, o in st['objects'].items()
                if oid not in T['objects'] or not self._same(T['objects'][oid], o)]
        seen = {e['entry_id'] for q in T['ledger'].values() for e in q}
        entries = [e for q in st['ledger'].values() for e in q if e['entry_id'] not in seen]
        return objs, entries

    def _validate(self, T, st, entries):
        """STD-VER-055: constraints, references, cardinality, double entry and periods of a
        merged (or reverted) state, relative to the target state T."""
        objs, _ = self._changed(T, st)
        self._exts = self._extensions(st)
        self._eff_cache = {}
        try:
            self._written = []
            for oid in objs:
                o = st['objects'][oid]
                v = self.versions[o['head']]
                if not v['tombstone']:
                    self._check_body(o['class'], v['body'])
                    self._written.append((oid, o['head']))
            for src in sorted(st['objects']):
                if not self._live(st, src) or (src, st['objects'][src]['head']) in self._written:
                    continue
                o = st['objects'][src]
                refs = self._refs(self._cls(o['class']), self.versions[o['head']]['body'])
                if any(self._resolve(st, uri) in objs for _, uri in refs):
                    self._written.append((src, o['head']))
            self._integrity(st, entries)
            self._cardinality(st)
            self._check_double_entry(entries)
            for cls in sorted({e['class'] for e in entries}):
                self._check_period(st, cls)
            self._check_extensions(st)
        finally:
            self._exts = None
            self._eff_cache = {}

    def _merge_result(self, s, t, source, cseq, keep=False):
        B = self._base_state({t}, {s})
        return self._merge_states(B, self.commits[s]['state'], self.commits[t]['state'],
                                  cseq=cseq, branch=source, keep=keep)

    @staticmethod
    def _unique(items):
        out = []
        for c in items:
            if c not in out:
                out.append(c)
        return out

    def merge(self, source, target, resolutions=None, process=None, _change_set=None):
        self._branch(source)
        self._check_active(target)
        self._merge_allowed(source, target, _change_set is not None)
        s, t = self._branch(source)['head'], self._branch(target)['head']
        if s in self._anc(t):
            return {'cseq': self.commits[t]['cseq'], 'merged': False}
        p = self._process(process, kind='merge')
        self._res, self._used = self._resolutions(resolutions), {}
        cseq = self.commits[t]['cseq'] + 1
        self._cseq = cseq
        try:
            st, conflicts = self._merge_result(s, t, source, cseq)
            if conflicts:
                raise BpaError('VER.MERGE_CONFLICT', {'conflicts': self._unique(conflicts)})
            unused = set(self._res) - set(self._used)
            if unused:
                raise BpaError('MODEL.INVALID_VALUE', 'a resolution matches no conflict')
            used = [self._used[k] for k in sorted(self._used, key=lambda k: (k[0], k[1] or ''))]
        finally:
            self._res, self._used = {}, {}
        T = self.commits[t]['state']
        objs, entries = self._changed(T, st)
        self._check_merged_sheets(T, st)
        self._begin(st, p)
        result = {'versions': {oid: st['objects'][oid]['head'] for oid in objs},
                  'entries': {e['entry_id']: {} for e in entries}}
        try:
            self._write_policy(target, st, result, _change_set is not None)
            binding = self._binding(st, result)
        finally:
            self._exts = None
            self._eff_cache = {}
        self._validate(T, st, entries)
        ops = {oid: [{'kind': 'merge'}] for oid in objs}
        cid = self._new_commit(target, [t, s], st, cseq, ops=ops, rule_binding=binding, resolutions=used,
                               change_set=_change_set, process_id=p['process_id'] if p['explicit'] else None,
                               appended=[e['entry_id'] for e in entries])
        self._advance(target, cid)
        self._record(p, 'committed', (target, cid))
        self._effects(p, target, cid)
        self._schedule(target, cid, st, {'versions': result['versions']})
        appended = {}
        for q in st['ledger'].values():
            for e in q:
                if e.get('cseq') == cseq and 'source_seq' in e:
                    appended[e['entry_id']] = {'seq': e['seq'], 'source_seq': e['source_seq']['seq']}
        out = {'cseq': cseq, 'merged': True, 'appended': appended, 'commit_id': self.commits[cid]['commit_id'],
               'tx_time': self.commits[cid]['tx_time']}
        if used:
            out['resolutions'] = used
        return out

    # ---- diff (STD-VER-030) ----

    def _coord(self, c):
        return self._state(c['branch'], c)[0]

    def diff(self, a, b):
        return self._diff_states(self._coord(a), self._coord(b))

    def _diff_states(self, A, B):
        out = []
        for oid in sorted(set(A['objects']) | set(B['objects'])):
            x, y = A['objects'].get(oid), B['objects'].get(oid)
            if x and y and x['head'] == y['head'] and x['timeline'] == y['timeline']:
                continue
            ent = {'object_id': oid, 'class': (y or x)['class'],
                   'versions': {'a': x['head'] if x else None, 'b': y['head'] if y else None}}
            if x is None:
                ent['op'] = 'added'
            elif y is None or self.versions[y['head']]['tombstone'] and not self.versions[x['head']]['tombstone']:
                ent['op'] = 'ended'
            else:
                vx, vy = self.versions[x['head']], self.versions[y['head']]
                fields = [{'path': '/' + k, 'old': vx['body'].get(k), 'new': vy['body'].get(k)}
                          for k in sorted(set(vx['body']) | set(vy['body'])) if vx['body'].get(k, ()) != vy['body'].get(k, ())]
                ent['op'] = 'moved' if vx['slug'] != vy['slug'] and not fields else 'changed'
                if fields:
                    ent['fields'] = fields
            out.append(ent)
        seen = {e['entry_id'] for q in A['ledger'].values() for e in q}
        for q in B['ledger'].values():
            for e in q:
                if e['entry_id'] not in seen:
                    out.append({'object_id': e['entry_id'], 'class': e['class'], 'op': 'appended',
                                'versions': {'a': None, 'b': e['entry_hash']}})
        return out
