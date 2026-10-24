"""Integration and addressing of BPA 1.0-rc8 (PH-2 step 1f, DEC-094): connector objects and
`call.connector` (STD-ISA-090), the compensation of pre-commit calls (STD-TXN-051),
`emit.notify` (STD-ISA-071), short forms and VFS paths (STD-ADDR-003, -004) and the REST facade
over the in-process UBTP binding (STD-PROTO-021). Mixed into the reference store; the outside
world is simulated by scripted connector responses."""
import copy
import re

from . import BpaError
from .canon import canonical, content_id
from .values import validate_value, GRAMMARS, LEVELS

CONNECTOR, TEMPLATE = 'Connector', 'NotificationTemplate'
PROTOCOLS = ('https', 'sftp', 'smtp', 'amqp', 'kafka')
REDACTED = '[redacted]'
DEFAULT_ALLOWED = ['public', 'internal']
PRIORITIES = ('low', 'normal', 'high')
PLACEHOLDER = re.compile(r'\{\{\s*this\.([a-z][a-z0-9_]*)\s*\}\}')
SHORT = re.compile(r'\A([A-Z][A-Za-z0-9]*)/([a-z0-9][a-z0-9-]*)\Z')
VFS_ROOTS = {'data': None, 'usr': 'LogicAsset', 'etc': 'ConfigAsset', 'view': 'ViewAsset', 'lib': 'LogicAsset'}
API = '/api/v1/'


def _invalid(what):
    return BpaError('MODEL.INVALID_VALUE', what)


def _incomplete(what):
    return BpaError('MODEL.DECLARATION_INCOMPLETE', what)


def _schema(fields, what):
    if not isinstance(fields, list) or not all(isinstance(f, dict) and isinstance(f.get('name'), str)
                                               and isinstance(f.get('type'), str) for f in fields):
        raise _incomplete('%s is a list of {name, type} fields' % what)
    for f in fields:
        if f.get('classification', 'internal') not in LEVELS:
            raise _invalid('classification of %s' % f['name'])


def check_connector(c):
    """DAT-Connector (FR-LOGIC-101): endpoint, authentication by a credential handle,
    operations with schemas and compensations, resilience and the classifications allowed out."""
    ep = c.get('endpoint')
    if not isinstance(ep, dict) or ep.get('protocol') not in PROTOCOLS or not isinstance(ep.get('host'), str):
        raise _incomplete('endpoint is {protocol, host, path}')
    auth = c.get('auth') or {}
    cred = auth.get('credential')
    if cred is not None and (not isinstance(cred, str) or not cred.startswith('secret://')):
        raise _invalid('a credential is a secret handle secret://..., never the secret')
    ops = c.get('operations')
    if not isinstance(ops, dict) or not ops:
        raise _incomplete('a connector declares its operations')
    for name, op in sorted(ops.items()):
        if not isinstance(op, dict):
            raise _incomplete('operation %s' % name)
        _schema(op.get('request') or [], 'the request of %s' % name)
        _schema(op.get('response') or [], 'the response of %s' % name)
        comp = op.get('compensation')
        if op.get('now'):
            if comp is None:
                raise _incomplete('operation %s is called before commit and declares its compensation' % name)
            if comp == 'none' and not op.get('compensation_reason'):
                raise _incomplete('compensation none states compensation_reason')
        if comp not in (None, 'none') and comp not in ops:
            raise _invalid('the compensation of %s names an operation' % name)
        if not isinstance(op.get('redact') or [], list):
            raise _invalid('redact lists fields')
    allowed = c.get('allowed_classifications', DEFAULT_ALLOWED)
    if not isinstance(allowed, list) or any(a not in LEVELS for a in allowed):
        raise _invalid('allowed_classifications are classification levels')
    for k, members in (('retry', ('max_attempts',)), ('breaker', ('failures',)), ('rate_limit', ('count',))):
        v = c.get(k)
        if v is not None and (not isinstance(v, dict) or any(not isinstance(v.get(m), int) or v.get(m) < 1
                                                             for m in members)):
            raise _invalid('%s members are positive integers' % k)


def check_template(t):
    for k in ('title', 'body'):
        if not isinstance(t.get(k), str):
            raise _incomplete('a template has %s' % k)


def _redact(value, fields):
    if not isinstance(value, dict):
        return value
    return {k: (REDACTED if k in fields else v) for k, v in value.items()}


def _fields(schema):
    return {f['name']: f for f in schema or []}


def _validate(schema, value, what, response=False):
    fs = _fields(schema)
    if not isinstance(value, dict):
        raise _invalid('%s is a map' % what)
    for k, v in value.items():
        if k not in fs:
            raise _invalid('%s has no member %s' % (what, k))
        validate_value(fs[k]['type'], fs[k].get('constraints'), v)
    for k, f in fs.items():
        if f.get('required') and value.get(k) is None:
            raise _invalid('%s misses %s' % (what, k))


def http_statuses():
    """{code: HTTP status} from the error registry (STD-ERR-002)."""
    from .spec import clauses
    block = clauses()['STD-ERR-002']['block']
    return {c: int(h) for c, h in re.findall(
        r'^\s*\|\s*`([A-Z]+\.[A-Z_]+)`\s*\|[^|]*\|[^|]*\|[^|]*\|\s*(\d{3})\s*\|', block, re.M)}


class Integration:

    def _int_init(self):
        self.egress = []           # FR-LOGIC-104: the VAE's egress allow-list of hosts
        self.stubs = {}            # (connector, operation) -> scripted responses
        self.exchanges = []        # recorded exchanges, redacted
        self.breakers = {}         # connector -> {failures, open_until}
        self.connector_calls = {}  # connector -> [instants] for rate limits
        self.principals = {}       # did -> {clearances, roles}
        self._rest_n = 0

    # ---- connectors (STD-ISA-090) ----

    def egress_allow(self, i):
        hosts = i.get('hosts')
        if not isinstance(hosts, list) or not all(isinstance(h, str) and h for h in hosts):
            raise _invalid('hosts is a list of host names')
        self.egress = sorted(set(hosts))
        return {'hosts': self.egress}

    def _check_egress(self, c):
        if c['endpoint']['host'] not in self.egress:
            raise BpaError('LOGIC.EGRESS_NOT_ALLOWED', {'host': c['endpoint']['host']})

    def connector_stub(self, i):
        """Script the responses of a connector operation: `{status, body}` or `{unavailable}`."""
        st = self._state(i.get('branch', 'main'))[0]
        oid, o = self._ref_oid(st, i.get('connector'), CONNECTOR)
        rs = i.get('responses')
        if not isinstance(rs, list) or not all(isinstance(r, dict) for r in rs):
            raise _invalid('responses is a list of {status, body} or {unavailable: true}')
        self.stubs.setdefault((oid, i.get('operation')), []).extend(copy.deepcopy(rs))
        return {'connector': oid, 'operation': i.get('operation'), 'scripted': len(self.stubs[(oid, i['operation'])])}

    def exchange_log(self, i):
        """The recorded exchanges with their redactions (FR-LOGIC-103)."""
        return {'exchanges': copy.deepcopy(self.exchanges)}

    def _exchange(self, oid, c, op, request, kind, process_id, key=None):
        """One exchange through the simulated outside world, with retries and the breaker:
        ('ok', response) or ('error', code, details)."""
        from .workflows import _shift
        b = self.breakers.setdefault(oid, {'failures': 0, 'open_until': None})
        rec = {'connector': oid, 'operation': op, 'kind': kind, 'process_id': process_id,
               'request': _redact(request, c['operations'][op].get('redact') or []), 'at': self.now}
        if key is not None:
            rec['idempotency_key'] = key
        if b['open_until'] is not None and self.now < b['open_until']:
            rec['outcome'] = 'breaker_open'
            self.exchanges.append(rec)
            return ('error', 'LOGIC.CONNECTOR_UNAVAILABLE', {'breaker': 'open'})
        rl = c.get('rate_limit') if kind != 'compensation' else None    # compensations are not limited
        if rl:
            per = {'second': 'PT1S', 'minute': 'PT1M', 'hour': 'PT1H', 'day': 'P1D'}.get(rl.get('per', 'minute'), 'PT1M')
            since = _shift(self.now, per, -1)
            recent = [t for t in self.connector_calls.get(oid, []) if t > since]
            if len(recent) >= rl['count']:
                rec['outcome'] = 'rate_limited'
                self.exchanges.append(rec)
                return ('error', 'LOGIC.CONNECTOR_UNAVAILABLE', {'rate_limited': True})
            self.connector_calls[oid] = recent + [self.now]
        attempts = (c.get('retry') or {}).get('max_attempts', 1)
        q = self.stubs.get((oid, op), [])
        out = None
        for n in range(1, attempts + 1):
            r = q.pop(0) if q else {'unavailable': True}
            rec['attempts'] = n
            if r.get('unavailable'):
                out = ('error', 'LOGIC.CONNECTOR_UNAVAILABLE', {'attempts': n})
                continue
            if r.get('status', 200) >= 400:
                out = ('error', 'LOGIC.CONNECTOR_ERROR', {'status': r['status']})
                break
            body = r.get('body') or {}
            try:
                _validate(c['operations'][op].get('response'), body, 'the response')
            except BpaError:
                out = ('error', 'LOGIC.CONNECTOR_ERROR', {'status': r.get('status', 200), 'invalid_response': True})
                break
            out = ('ok', body)
            break
        if out[0] == 'ok':
            b['failures'] = 0
            rec['outcome'] = 'ok'
            rec['response'] = _redact(out[1], c['operations'][op].get('redact') or [])
        else:
            b['failures'] += 1
            rec['outcome'] = out[1]
            br = c.get('breaker')
            if br and b['failures'] >= br['failures']:
                b['open_until'] = _shift(self.now, br.get('open_for', 'PT1M'))
                b['failures'] = 0
        self.exchanges.append(rec)
        return out

    def _connector(self, st, ref):
        oid, o = self._ref_oid(st, ref, CONNECTOR)
        return oid, self.versions[o['head']]['body']

    def _compensate(self, run, p):
        """STD-TXN-051: the compensations of the pre-commit calls of a failed process, at least
        once: a compensation that cannot be made now is queued in the outbox."""
        calls = getattr(run, 'connector_calls', None) or []
        if not calls:
            return
        rec = self.processes.get(p['process_id'])
        out = []
        for call in reversed(calls):
            comp = call['compensation']
            if comp in (None, 'none'):
                out.append({'connector': call['connector'], 'operation': call['operation'], 'seq': call['seq'],
                            'compensation': 'none'})
                continue
            key = 'comp:%s:%d' % (p['process_id'], call['seq'])
            res = self._exchange(call['connector'], call['body'], comp, call['request'], 'compensation',
                                 p['process_id'], key)
            item = {'connector': call['connector'], 'operation': call['operation'], 'seq': call['seq'],
                    'compensation': comp, 'idempotency_key': key, 'outcome': 'ok' if res[0] == 'ok' else 'queued'}
            if res[0] != 'ok':
                self.outbox.append({'effect_id': '%s/c%d' % (p['process_id'], call['seq']),
                                    'type': 'connector.compensation', 'branch': run.branch,
                                    'cseq': self.commits[self.branches[run.branch]['head']]['cseq'],
                                    'object_id': call['connector'],
                                    'payload': {'operation': comp, 'request': call['request'], 'idempotency_key': key}})
            out.append(item)
        run.connector_calls = []
        p['compensations'] = p.get('compensations', []) + out
        if rec is not None:
            rec['compensations'] = list(p['compensations'])

    # ---- notifications (STD-ISA-071) ----

    def principal(self, i):
        """Register a principal's clearances and roles (the directory of the adapter)."""
        did = i.get('did')
        if not isinstance(did, str):
            raise _incomplete('did')
        cl = i.get('clearances', ['internal'])
        if not isinstance(cl, list) or any(c not in LEVELS for c in cl):
            raise _invalid('clearances are classification levels')
        self.principals[did] = {'clearances': list(cl), 'roles': sorted(i.get('roles') or [])}
        return dict(self.principals[did], did=did)

    def _recipients(self, st, oid, recipients):
        out = []
        for r in recipients:
            if not isinstance(r, str):
                raise _invalid('a recipient is a DID, role:<name> or relation:<name>')
            if r.startswith('did:'):
                found = [r]
            elif r.startswith('role:'):
                found = sorted(d for d, p in self.principals.items() if r[5:] in p['roles'])
            elif r.startswith('relation:'):
                found = []
                for t in self._targets(st, oid, r[9:]):
                    o, body = self._body_at(st, t)
                    pf = self._principal_field(o['class']) if body is not None else None
                    if pf and body.get(pf):
                        found.append(body[pf])
            else:
                raise _invalid('a recipient is a DID, role:<name> or relation:<name>')
            out += [d for d in found if d not in out]
        return out

    def _render(self, text, body):
        def sub(m):
            v = body.get(m.group(1))
            if v is None:
                return ''
            return v if isinstance(v, str) else canonical(v)
        return PLACEHOLDER.sub(sub, text)

    def _notify(self, run, cid):
        """Render each requested notification per recipient on the committed state, with the
        recipient's masking (STD-LGS-052), into the outbox."""
        items = getattr(run, 'notifications', None) or []
        if not items:
            return
        branch = run.branch
        st = self._state(branch)[0]
        cseq = self.commits[cid]['cseq'] if cid is not None else self.commits[self.branches[branch]['head']]['cseq']
        for n in items:
            o, body = self._body_at(st, n['subject'])
            tpl = n['template']
            for k, did in enumerate(self._recipients(st, n['subject'], n['recipients']), 1):
                p = self.principals.get(did) or {'clearances': ['internal'], 'roles': []}
                viewer = {'principal': did, 'clearances': p['clearances'],
                          'ctx': {'principal': did, 'role': p['roles'][0]} if p['roles'] else {'principal': did}}
                seen = self._mask(body or {}, self._mask_modes(st, branch, n['subject'], viewer))
                for ch in n['channels']:
                    payload = {'notification_id': n['id'], 'recipient': did, 'channel': ch, 'priority': n['priority'],
                               'title': self._render(tpl['title'], seen), 'body': self._render(tpl['body'], seen)}
                    if n.get('action'):
                        payload['action'] = n['action']
                    self.outbox.append({'effect_id': '%s/%s/%d/%s' % (n['id'], 'r', k, ch), 'type': 'notification',
                                        'branch': branch, 'cseq': cseq, 'object_id': n['subject'], 'payload': payload})
        run.notifications = []

    # ---- addressing (STD-ADDR-003, -004) ----

    def _vfs(self, st, path):
        """The object a VFS path names, as (uri, object id)."""
        parts = path.split('/')
        if len(parts) < 4 or parts[0] != '' or parts[1] not in VFS_ROOTS or any(not x for x in parts[1:]):
            raise _invalid('a VFS path is /data/<namespace>/<Class>/<slug> or /<root>/<namespace>/<name>')
        root = parts[1]
        if root == 'data':
            if len(parts) != 5:
                raise _invalid('a data path is /data/<namespace>/<Class>/<slug>')
            ns, cls, slug = parts[2:]
            d = self.decls.get(cls)
            if d is None or d.get('namespace') != ns or 'DataObject' not in self._cls(cls)['linearization']:
                raise BpaError('IAM.NOT_FOUND', 'no data class %s in %s' % (cls, ns))
            uri = 'ubos://%s/%s/%s' % (self.TENANT, cls, slug)
            oid = self._resolve(st, uri)
            if oid is None:
                raise BpaError('IAM.NOT_FOUND', 'no object at %s' % path)
            return uri, oid
        if root == 'usr':
            if len(parts) != 5 or parts[2] != 'logic':
                raise _invalid('a logic path is /usr/logic/<namespace>/<name>')
            ns, name = parts[3:]
        else:
            if len(parts) != 4:
                raise _invalid('a path is /%s/<namespace>/<name>' % root)
            ns, name = parts[2:]
        base = VFS_ROOTS[root]
        for oid in sorted(st['objects']):
            o = st['objects'][oid]
            if base not in self._cls(o['class'])['linearization']:
                continue
            v = self.versions[o['head']]
            if not v['tombstone'] and v['body'].get('name') == '%s.%s' % (ns, name):
                return 'ubos://%s/%s/%s' % (self.TENANT, o['class'], o['slug']), oid
        raise BpaError('IAM.NOT_FOUND', 'no object at %s' % path)

    def _canonical_ref(self, st, ref):
        """A short form or VFS path as a canonical URI (STD-ADDR-003); other values unchanged."""
        if not isinstance(ref, str) or ref.startswith('ubos://'):
            return ref
        path, sep, query = ref.partition('?')
        if path.startswith('/'):
            uri = self._vfs(st, path)[0]
        else:
            m = SHORT.match(path)
            if m is None:
                return ref
            uri = 'ubos://%s/%s/%s' % (self.TENANT, m.group(1), m.group(2))
        return uri + sep + query

    def resolve_path(self, i):
        st = self._state(i.get('branch', 'main'), i)[0]
        path = i.get('path')
        if not isinstance(path, str):
            raise _invalid('path is text')
        if path.startswith('/'):
            uri, oid = self._vfs(st, path)
        else:
            uri = self._canonical_ref(st, path)
            if not uri.startswith('ubos://'):
                raise _invalid('not a short form: Class/slug')
            oid = self._resolve(st, uri)
            if oid is None:
                raise BpaError('IAM.NOT_FOUND', 'no object at %s' % path)
        return {'uri': uri, 'object_id': oid}

    # ---- REST facade (STD-PROTO-021) ----

    def _problem(self, code, details=None, status=None):
        st = status or http_statuses().get(code, 500)
        body = {'type': 'https://ubos.dev/errors/%s' % code, 'title': code, 'status': st, 'code': code,
                'explanation': details if details is not None else code}
        return {'status': st, 'headers': {'Content-Type': 'application/problem+json'}, 'body': body}

    def rest(self, i):
        """The REST facade: a request mapped to a UBTP read, query or mutate of the caller's
        session, and the result or RFC 9457 problem."""
        from .ubtp import Session, VERSION
        self._ubtp_init()
        principal = i.get('principal')
        if not isinstance(principal, str):
            raise _incomplete('principal (authentication is the node\'s)')
        key = 'rest:' + principal
        s = self._ubtp_sessions.setdefault(key, Session(key))

        def call(type_, body):
            self._rest_n += 1
            frames = self._message(s, {'v': VERSION, 'id': 'rest-%d' % self._rest_n, 'type': type_, 'body': body})
            f = frames[0]
            return f['type'], f['body']

        if not s.open:
            call('handshake', {'versions': [VERSION], 'credentials': {'kind': 'in_process', 'principal': principal}})
        method, path = i.get('method'), i.get('path') or ''
        query = dict(i.get('query') or {})
        headers = {k.lower(): v for k, v in (i.get('headers') or {}).items()}
        if not path.startswith(API):
            return self._problem('IAM.NOT_FOUND', 'no route %s' % path)
        parts = path[len(API):].split('/')
        if len(parts) < 2 or parts[0] != self.TENANT or not GRAMMARS['class-name'].match(parts[1]) or len(parts) > 4:
            return self._problem('IAM.NOT_FOUND', 'no route %s' % path)
        cls = parts[1]
        branch = query.pop('branch', 'main')
        if method == 'GET' and len(parts) == 3:
            t, b = call('query', {'read': 'ubos://%s/%s/%s' % (self.TENANT, cls, parts[2]), 'branch': branch})
            ok = 200
            body = b['items'][0] if t == 'result' else b
        elif method == 'GET' and len(parts) == 2:
            q = {'from': cls}
            if query.get('q'):
                q['where'] = query['q']
            for k in ('limit', 'cursor'):
                if query.get(k) is not None:
                    q[k] = int(query[k]) if k == 'limit' else query[k]
            t, b = call('query', {'query': q, 'branch': branch})
            ok, body = 200, b
        elif method == 'POST' and len(parts) == 4:
            m = {'port': {'target': 'ubos://%s/%s/%s' % (self.TENANT, cls, parts[2]), 'name': parts[3],
                          'params': i.get('body') or {}}, 'branch': branch}
            if headers.get('idempotency-key'):
                m['idempotency_key'] = headers['idempotency-key']
            t, b = call('mutate', m)
            ok, body = 200, b
        elif method == 'POST' and len(parts) == 2:
            rb = i.get('body') or {}
            ch = {'op': 'create', 'class': cls, 'object_id': rb.get('object_id'), 'slug': rb.get('slug'),
                  'body': rb.get('fields') or {}}
            m = {'commit': {'changes': [ch]}, 'branch': branch}
            if headers.get('idempotency-key'):
                m['idempotency_key'] = headers['idempotency-key']
            t, b = call('mutate', m)
            ok, body = 201, b
        else:
            return self._problem('INT.PROTOCOL_ERROR', 'method %s is not allowed on %s' % (method, path), 405)
        if t == 'error':
            return self._problem(b['code'], b.get('details'))
        return {'status': ok, 'headers': {'Content-Type': 'application/json'}, 'body': body}
