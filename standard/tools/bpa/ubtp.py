"""The in-process UBTP binding of the reference implementation (BPA 1.0-rc2, STD-PROTO-001,
-002, -010, -011, -012, -013, -020; PH-1 step 10d, DEC-068), with the versioning messages and
introspection queries of BPA 1.0-rc4 (STD-PROTO-014, -015; PH-1 step 18a, DEC-079); since
BPA 1.0-rc5 (PH-1 step 18d, DEC-082) the error of a `mutate` whose process was recorded carries
its `process_id`.

The adapter operation `ubtp {session, messages}` feeds a sequence of `DAT-UbtpEnvelope`
messages to a session and returns the frames the server produced in order: `result`,
`error`, `emit` and `pong`. Sessions persist across calls, so a vector may commit through
other operations between two `ubtp` calls and receive the emits on the next one."""
from . import BpaError
from .canon import content_id

VERSION = '1.0'
ABI = '1.0'
TYPES = ('handshake', 'mutate', 'query', 'subscribe', 'unsubscribe', 'emit', 'sync', 'ack', 'error', 'result',
         'ping', 'pong', 'cancel')
REQUESTS = ('handshake', 'mutate', 'query', 'subscribe', 'unsubscribe', 'ack', 'ping', 'cancel', 'sync')
LIMITS = {'frame_bytes': 16 * 1024 * 1024, 'window': 8, 'retention': 100}
# STD-PROTO-014 (1.0-rc4): the versioning requests of `mutate` and their permissions
VERSIONING = ('create_branch', 'change_set', 'merge', 'revert', 'bulk')
PERMISSIONS = {'create_branch': 'branch.create', 'merge': 'branch.merge', 'revert': 'revert', 'bulk': 'commit.raw'}
CHANGE_SET_PERMISSIONS = {'approve': 'changeset.approve', 'reject': 'changeset.approve', 'merge': 'branch.merge'}
# STD-PROTO-015 (1.0-rc4): the introspection requests of `query`
INTROSPECTION = ('class', 'dispatch', 'commit_info', 'merge_preview', 'ledger', 'process', 'replay')


def _err(code, detail=None):
    raise BpaError(code, detail)


class Session:
    def __init__(self, key):
        self.key = key
        self.open = False
        self.principal = None
        self.kind = 'human'
        self.role = None
        self.grants = None
        self.window = LIMITS['window']
        self.ids = set()
        self.next_id = 0
        self.next_sub = 0
        self.subs = {}
        self.idem = {}
        self.pid = None         # the process of the mutate being handled (1.0-rc5)

    def sid(self):
        self.next_id += 1
        return 's%d' % self.next_id


class Ubtp:
    """Store mixin."""

    def _ubtp_init(self):
        if not hasattr(self, '_ubtp_sessions'):
            self._ubtp_sessions = {}

    # -- frames --

    def _frame(self, s, type_, body, reply_to=None, trace=None):
        f = {'v': VERSION, 'id': s.sid(), 'type': type_, 'body': body}
        if reply_to is not None:
            f['reply_to'] = reply_to
        if trace is not None:
            f['trace'] = trace
        return f

    def _error_frame(self, s, reply_to, e, trace=None, process_id=None):
        body = {'code': e.code, 'message': e.code}
        if e.detail is not None:
            body['details'] = e.detail
        if process_id is not None:
            body['process_id'] = process_id
        return self._frame(s, 'error', body, reply_to, trace)

    def _ubtp_pid(self, s):
        """A process identifier for the mutate being handled, remembered for its error (1.0-rc5)."""
        s.pid = self._process_id()
        return s.pid

    # -- entry --

    def ubtp(self, messages, session='s1'):
        """STD-PROTO-020 in-process binding: the frames produced for the messages."""
        self._ubtp_init()
        if not isinstance(messages, list):
            raise BpaError('MODEL.INVALID_VALUE', 'messages is a list')
        s = self._ubtp_sessions.setdefault(session, Session(session))
        out = []
        if s.open:
            out += self._catch_up(s)
        for m in messages:
            out += self._message(s, m)
        return {'responses': out}

    def _message(self, s, m):
        try:
            self._envelope(s, m)
        except BpaError as e:
            rid = m.get('id') if isinstance(m, dict) and isinstance(m.get('id'), str) else None
            return [self._error_frame(s, rid, e)]
        rid, trace = m['id'], m.get('trace')
        t = m['type']
        s.pid = None
        try:
            if t == 'handshake':
                body = self._handshake(s, m['body'])
            elif t == 'ping':
                return [self._frame(s, 'pong', {}, rid, trace)]
            elif t == 'cancel':
                body = {'cancelled': False}
            elif t == 'mutate':
                body = self._mutate(s, m['body'])
            elif t == 'query':
                body = self._query(s, m['body'])
            elif t == 'subscribe':
                body = self._subscribe(s, m['body'])
            elif t == 'unsubscribe':
                body = self._unsubscribe(s, m['body'])
            elif t == 'ack':
                body = self._ack(s, m['body'])
            else:
                _err('LOGIC.NOT_AVAILABLE', 'sync comes with PH-3')
        except BpaError as e:
            # STD-PROTO-010 (1.0-rc5): a mutate whose process was recorded names it
            pid = s.pid if t == 'mutate' and s.pid in self.processes else None
            return [self._error_frame(s, rid, e, trace, pid)]
        frames = [self._frame(s, 'result', body, rid, trace)]
        if t in ('mutate', 'ack'):
            frames += self._catch_up(s)
        return frames

    def _envelope(self, s, m):
        if not isinstance(m, dict):
            _err('INT.PROTOCOL_ERROR', 'a message is a DAT-UbtpEnvelope')
        for k in ('v', 'id', 'type', 'body'):
            if k not in m:
                _err('INT.PROTOCOL_ERROR', 'envelope member %s' % k)
        for k in m:
            if k not in ('v', 'id', 'reply_to', 'type', 'body', 'trace'):
                _err('INT.PROTOCOL_ERROR', 'unknown envelope member %s' % k)
        if m['v'] != VERSION:
            _err('INT.VERSION_UNSUPPORTED', {'supported': {'versions': [VERSION]}})
        if not isinstance(m['id'], str) or not m['id']:
            _err('INT.PROTOCOL_ERROR', 'id is a client-unique text')
        if m['type'] not in TYPES:
            _err('INT.PROTOCOL_ERROR', 'unknown message type %s' % m['type'])
        if m['type'] not in REQUESTS:
            _err('INT.PROTOCOL_ERROR', '%s is a server message' % m['type'])
        if 'reply_to' in m:
            _err('INT.PROTOCOL_ERROR', 'a request carries no reply_to')
        if not isinstance(m['body'], dict):
            _err('INT.PROTOCOL_ERROR', 'body is a map')
        if 'trace' in m and not isinstance(m['trace'], str):
            _err('INT.PROTOCOL_ERROR', 'trace is text')
        if m['id'] in s.ids:
            _err('INT.PROTOCOL_ERROR', 'duplicate id %s' % m['id'])
        s.ids.add(m['id'])
        if m['type'] != 'handshake' and not s.open:
            _err('INT.PROTOCOL_ERROR', 'the first message is the handshake')
        if m['type'] == 'handshake' and s.open:
            _err('INT.PROTOCOL_ERROR', 'the session is already open')

    # -- handshake (STD-PROTO-002) --

    def _handshake(self, s, b):
        from .store import PRINCIPAL, TENANT
        versions = b.get('versions')
        if not isinstance(versions, list) or VERSION not in versions:
            _err('INT.VERSION_UNSUPPORTED', {'supported': {'versions': [VERSION], 'abi': {'min': ABI, 'max': ABI}}})
        abi = b.get('abi') or {'min': ABI, 'max': ABI}
        if not isinstance(abi, dict) or not (str(abi.get('min', ABI)) <= ABI <= str(abi.get('max', ABI))):
            _err('INT.VERSION_UNSUPPORTED', {'supported': {'versions': [VERSION], 'abi': {'min': ABI, 'max': ABI}}})
        enc = b.get('encodings') or ['json']
        if 'json' not in enc:
            _err('INT.VERSION_UNSUPPORTED', {'supported': {'encodings': ['json']}})
        cred = b.get('credentials') or {'kind': 'in_process'}
        if not isinstance(cred, dict) or cred.get('kind') != 'in_process':
            _err('IAM.DENIED', 'the in-process binding takes in_process credentials')
        s.principal = cred.get('principal') or PRINCIPAL
        s.kind = cred.get('principal_kind') or 'human'
        s.role = cred.get('role')
        s.grants = list(cred['grants']) if cred.get('grants') is not None else None   # None: unrestricted
        s.open = True
        return {'version': VERSION, 'abi': ABI, 'session_id': s.key, 'encoding': 'json',
                'principal': {'did': s.principal, 'kind': s.kind, 'authority': TENANT},
                'limits': dict(LIMITS, window=s.window), 'features': []}

    def _ubtp_process(self, s, extra=None):
        p = {'initiator': s.principal, 'channel': 'api', 'principal_kind': s.kind}
        if s.role is not None:
            p['role'] = s.role
        p.update(extra or {})
        return p

    def _ctx(self, s):
        c = {'principal': s.principal, 'principal_kind': s.kind, 'channel': 'api'}
        if s.role is not None:
            c['role'] = s.role
        return c

    # -- mutate (STD-PROTO-010) --

    def _mutate(self, s, b):
        branch = b.get('branch', 'main')
        kinds = [k for k in ('port', 'commit') + VERSIONING if k in b]
        if any(k in b for k in VERSIONING):
            if len(kinds) != 1:
                _err('MODEL.INVALID_VALUE', 'mutate carries exactly one of port, commit, ' + ', '.join(VERSIONING))
        elif ('port' in b) == ('commit' in b):
            _err('MODEL.INVALID_VALUE', 'mutate carries port or commit')
        key = b.get('idempotency_key')
        payload = {k: v for k, v in b.items() if k != 'idempotency_key'}
        h = content_id('ubos.chunk.v1', payload)
        if key is not None:
            known = s.idem.get(key)
            if known is not None:
                if known[0] != h:
                    _err('TXN.IDEMPOTENCY_MISMATCH')
                return known[1]
        if b.get('base') is not None:
            head = self.commits[self._branch(branch)['head']]['cseq']
            if b['base'] != head:
                _err('TXN.HEAD_MOVED', {'head': head})
        extra = {'message': b['message']} if b.get('message') else {}
        if b.get('reason'):
            extra['reason'] = b['reason']
        if kinds[0] in VERSIONING:
            body = self._versioning(s, b, kinds[0], branch, extra)
        elif 'commit' in b:
            c = b['commit']
            if not isinstance(c, dict) or not isinstance(c.get('changes'), list):
                _err('MODEL.INVALID_VALUE', 'commit is {changes}')
            if s.grants is not None and 'commit.raw' not in s.grants:
                _err('IAM.DENIED', 'raw commits need the commit.raw permission')
            if b.get('validate_only'):
                r = self.validate(branch, c['changes'], self._ubtp_process(s, extra))
                body = {'validated': True, 'violations': r['violations'], 'warnings': r['warnings']}
            else:
                pid = self._ubtp_pid(s)
                r = self.commit(branch, c['changes'], self._ubtp_process(s, dict(extra, process_id=pid)))
                body = {'process_id': pid, 'commits': [r['commit_id']], 'versions': r['versions'],
                        'warnings': r.get('warnings', []),
                        'result': {'cseq': r['cseq'], 'entries': r['entries'], 'retroactive': r['retroactive']}}
        else:
            p = b['port']
            if not isinstance(p, dict) or not isinstance(p.get('target'), str) or not isinstance(p.get('name'), str):
                _err('MODEL.INVALID_VALUE', 'port is {target, name, params}')
            if b.get('validate_only'):
                _err('LOGIC.NOT_AVAILABLE', 'the preview of a port call comes with the simulation of PH-2')
            pid = self._ubtp_pid(s)
            i = {'branch': branch, 'target': p['target'], 'port': p['name'], 'params': p.get('params') or {},
                 'process': self._ubtp_process(s, dict(extra, process_id=pid)), 'ctx': self._ctx(s)}
            if s.grants is not None:
                i['grants'] = s.grants
            try:
                r = self.execute(i)
            except BpaError as e:
                if e.code == 'RULE.DECISION_REQUIRES':
                    # the stored pending change and its approval come with PH-2 (FR-FLOW-042)
                    body = {'process_id': pid, 'pending': {'pending_change_id': pid, 'steps': e.detail.get('steps', [])},
                            'commits': [], 'versions': {}, 'warnings': [], 'result': None}
                    if key is not None:
                        s.idem[key] = (h, body)
                    return body
                raise
            commits, versions = [], {}
            if r.get('cseq'):
                c = self.commits[self._commit_at(branch, r['cseq'])]
                commits = [c['commit_id']]
                versions = {oid: c['state']['objects'][oid]['head'] for oid in sorted(c.get('ops') or {})
                            if oid in c['state']['objects']}
            body = {'process_id': pid, 'commits': commits, 'versions': versions, 'warnings': [], 'result': r['result']}
        if key is not None:
            s.idem[key] = (h, body)
        return body

    # -- versioning requests (STD-PROTO-014, since 1.0-rc4) --

    def _versioning(self, s, b, kind, branch, extra):
        v = b[kind]
        if not isinstance(v, dict):
            _err('MODEL.INVALID_VALUE', '%s is a map' % kind)
        if b.get('validate_only'):
            _err('MODEL.INVALID_VALUE', 'validate_only applies to port and commit')
        perm = CHANGE_SET_PERMISSIONS.get(v.get('action')) if kind == 'change_set' else PERMISSIONS[kind]
        if s.grants is not None and perm is not None and perm not in s.grants:
            _err('IAM.DENIED', '%s needs the %s permission' % (kind, perm))
        before = set(self.commits)
        pid = None
        if kind == 'create_branch':
            r = self.create_branch(v.get('name'), v.get('from', branch), v.get('time'))
            r = {'branch': v['name'], 'head': r['head']}
        elif kind == 'change_set':
            i = {k: v[k] for k in ('action', 'id', 'source', 'target', 'title', 'description', 'resolutions') if k in v}
            if i.get('action') == 'open':
                i.setdefault('target', branch)
            if i.get('action') in ('approve', 'reject'):
                i['approver'] = s.principal
            r = self.change_set(i)
        elif kind == 'merge':
            pid = self._ubtp_pid(s)
            r = self.merge(v.get('source'), branch, v.get('resolutions'),
                           self._ubtp_process(s, dict(extra, process_id=pid)))
        elif kind == 'revert':
            i = {k: v[k] for k in ('process_id', 'change_set', 'time', 'commit', 'reversal_ids', 'resolutions', 'dry_run') if k in v}
            i['branch'] = branch
            if not v.get('dry_run'):
                pid = self._ubtp_pid(s)
                i['process'] = self._ubtp_process(s, dict(extra, process_id=pid))
            r = self.revert(i)
        else:
            i = {k: v[k] for k in ('changes', 'max_changes', 'mode', 'staging') if k in v}
            if v.get('resume') is not None:
                pid = s.pid = v['resume']
                i['resume'] = True
            else:
                pid = self._ubtp_pid(s)
            i.update(process_id=pid, branch=branch, process=self._ubtp_process(s, extra))
            r = self.bulk(i)
        commits, versions = [], {}
        for cid in [c for c in self.commits if c not in before]:
            c = self.commits[cid]
            commits.append(c['commit_id'])
            for oid in sorted(c.get('ops') or {}):
                if oid in c['state']['objects']:
                    versions[oid] = c['state']['objects'][oid]['head']
        body = {'commits': commits, 'versions': versions, 'warnings': [], 'result': r}
        if pid is not None:
            body['process_id'] = pid
        return body

    # -- query (STD-PROTO-011) --

    def _coords(self, b):
        sel = {}
        for k in ('time', 'commit', 'asof'):
            if b.get(k) is not None:
                sel[k] = b[k]
        return b.get('branch', 'main'), sel

    def _item(self, branch, oid, sel):
        r = self.read(branch, oid, sel.get('asof'), {k: v for k, v in sel.items() if k != 'asof'})
        o = self._state(branch, {k: v for k, v in sel.items() if k != 'asof'})[0]['objects'][oid]
        return dict(r, object_id=oid, **{'class': o['class']})

    def _query(self, s, b):
        kinds = [k for k in ('read', 'query', 'view', 'explain', 'history', 'timeline', 'diff') + INTROSPECTION
                 if k in b]
        if any(k in b for k in INTROSPECTION):
            if len(kinds) != 1:
                _err('MODEL.INVALID_VALUE', 'query carries exactly one request kind')
            return self._introspect(s, b, kinds[0])
        if len(kinds) != 1:
            _err('MODEL.INVALID_VALUE', 'query carries exactly one of read, query, view, explain, history, timeline, diff')
        k = kinds[0]
        branch, sel = self._coords(b)
        if k == 'read':
            uri = b['read']
            # the message's coordinates complete the URI's selectors (branch, asof, time)
            for name, value in [('branch', branch)] + [(n, sel[n]) for n in ('asof', 'time') if n in sel]:
                if ('?%s=' % name) not in uri and ('&%s=' % name) not in uri:
                    uri += ('&' if '?' in uri else '?') + '%s=%s' % (name, value)
            r = self.resolve(uri)
            item = dict(r, **{'class': self.versions[r['version_hash']]['class']})
            ev = item.pop('evaluated_at')
            return {'items': [item], 'evaluated_at': ev}
        if k == 'query':
            q = b['query'] if isinstance(b['query'], dict) else {'text': b['query']}
            r = self.query(dict(q, branch=branch, **sel))
            out = {'items': [self._item(branch, oid, dict(sel, commit=r['evaluated_at'][branch]))
                             for oid in r['objects']], 'evaluated_at': r['evaluated_at']}
            if r.get('cursor'):
                out['cursor'] = r['cursor']
            return out
        if k == 'view':
            _err('LOGIC.NOT_AVAILABLE', 'view payloads come with the views of PH-1 step 13')
        if k == 'explain':
            e = b['explain']
            oid = self._object_of(branch, e['target'], sel)
            ctx = {'principal_kind': s.kind, 'channel': 'api'}
            if s.role is not None:
                ctx['role'] = s.role
            if e.get('kind', 'cascade') == 'cascade':
                r = self.explain_cascade(branch, oid, e['property'], ctx, sel)
            elif e['kind'] == 'decision':
                r = self.decide(branch, oid, e['port'], e.get('params'), ctx, sel)
                r = dict(r, evaluated_at=self._state(branch, sel)[2])
            else:
                _err('MODEL.INVALID_VALUE', 'explain kind is cascade or decision')
            ev = r.pop('evaluated_at')
            return {'items': [r], 'evaluated_at': ev}
        if k == 'history':
            hh = b['history']
            oid = self._object_of(branch, hh['target'], sel)
            r = self.history(branch, oid, hh.get('from'), hh.get('to'))
            return {'items': r['commits'], 'evaluated_at': r['evaluated_at']}
        if k == 'timeline':
            tl = b['timeline']
            oid = self._object_of(branch, tl['target'], sel)
            r = self.read_range(branch, oid, tl.get('from', '0000-01-01'), tl.get('to'), sel)
            return {'items': r['segments'], 'evaluated_at': r['evaluated_at']}
        d = b['diff']
        items = self.diff(d['from'], d['to'])
        return {'items': items, 'evaluated_at': {}}

    # -- introspection queries (STD-PROTO-015, since 1.0-rc4) --

    def _introspect(self, s, b, kind):
        from .classes import dispatch
        branch, sel = self._coords(b)
        snap = {k: v for k, v in sel.items() if k != 'asof'}
        v = b[kind]
        if not isinstance(v, dict):
            _err('MODEL.INVALID_VALUE', '%s is a map' % kind)
        if kind in ('process', 'replay'):
            pid = v.get('process_id')
            if kind == 'replay':
                return {'items': [self.replay(pid)], 'evaluated_at': {}}
            item = self.process_info(pid)
            if v.get('journal'):
                item['journal'] = self.journal(pid)
            if v.get('metering'):
                item['metering'] = self.metering_record(pid)
            return {'items': [item], 'evaluated_at': {}}
        st, _, ev, _ = self._state(branch, snap)
        if kind == 'class':
            items = [self.class_at(branch, v.get('name'), snap)]
        elif kind == 'dispatch':
            items = [dispatch(list(self.decls.values()), v.get('class'), v.get('port'), v.get('after'),
                              self._extensions(st))]
        elif kind == 'commit_info':
            item = self.commit_info(branch, snap)
            item['root'] = self.commit_record(branch, snap)['root']
            items = [item]
        elif kind == 'merge_preview':
            items = [self.merge_preview(v.get('source'), branch)]
        else:
            items = self.read_ledger(branch, v.get('class'), v.get('ledger_key'), sel.get('asof'), snap)['entries']
        return {'items': items, 'evaluated_at': ev}

    def _object_of(self, branch, target, sel):
        if isinstance(target, str) and target.startswith('ubos://'):
            st = self._state(branch, sel)[0]
            oid = self._resolve(st, target)
            if oid is None:
                _err('IAM.NOT_FOUND', 'unknown object')
            return oid
        return target

    # -- subscriptions (STD-PROTO-012, -013) --

    def _cursor(self, branch, cseq, n=0):
        return '%s:%d.%d' % (branch, cseq, n) if n else '%s:%d' % (branch, cseq)

    def _parse_cursor(self, c):
        """`(branch, cseq, n)` of a cursor `<branch>:<cseq>` or `<branch>:<cseq>.<n>` (DEC-083)."""
        if not isinstance(c, str) or ':' not in c:
            _err('MODEL.INVALID_VALUE', 'a cursor is <branch>:<cseq>')
        branch, _, pos = c.rpartition(':')
        cseq, dot, n = pos.partition('.')
        if not cseq.isdigit() or (dot and not n.isdigit()):
            _err('MODEL.INVALID_VALUE', 'a cursor is <branch>:<cseq>')
        return branch, int(cseq), int(n) if dot else 0

    def _timer_effects(self, branch):
        """The effects released on a branch without a commit, in outbox order (STD-TIME-050)."""
        return [e for e in self.outbox if e['branch'] == branch and 'cseq' not in e]

    def _head_cseq(self, branch):
        return self.commits[self._branch(branch)['head']]['cseq']

    def _subscribe(self, s, b):
        kinds = [k for k in ('target', 'query', 'events') if k in b]
        if len(kinds) != 1:
            _err('MODEL.INVALID_VALUE', 'subscribe carries exactly one of target, query, events')
        branch = b.get('branch', 'main')
        head = self._head_cseq(branch)
        released = len(self._timer_effects(branch)) if kinds[0] == 'events' else 0
        if b.get('from_cursor') is not None:
            cb, cseq, n = self._parse_cursor(b['from_cursor'])
            if cb != branch:
                _err('MODEL.INVALID_VALUE', 'the cursor names another branch')
            if cseq > head or n > released and kinds[0] == 'events':
                _err('MODEL.INVALID_VALUE', 'the cursor is ahead of the branch')
            if head - cseq > LIMITS['retention']:
                _err('INT.CURSOR_EXPIRED', {'retention': LIMITS['retention'], 'head': self._cursor(branch, head)})
            n = n if kinds[0] == 'events' else 0
        else:
            cseq, n = head, released
        sub = {'kind': kinds[0], 'spec': b[kinds[0]], 'branch': branch, 'cseq': cseq, 'n': n, 'unacked': [],
               'queued': []}
        if kinds[0] == 'target':
            st = self._state(branch, {})[0]
            oid = self._resolve(st, sub['spec']) if isinstance(sub['spec'], str) else None
            if oid is None:
                _err('IAM.NOT_FOUND', 'unknown object')
            sub['object_id'] = oid
        elif kinds[0] == 'query':
            q = sub['spec']
            if not isinstance(q, dict) or not isinstance(q.get('from'), str):
                _err('QRY.INVALID_QUERY', 'a subscription query is a DAT-Query')
            self.query(dict(q, branch=branch, limit=1))
        s.next_sub += 1
        sid = 'sub-%d' % s.next_sub
        s.subs[sid] = sub
        return {'subscription_id': sid, 'cursor': self._cursor(branch, cseq, n)}

    def _unsubscribe(self, s, b):
        sid = b.get('subscription_id')
        if sid not in s.subs:
            _err('IAM.NOT_FOUND', 'unknown subscription')
        del s.subs[sid]
        return {'subscription_id': sid}

    def _ack(self, s, b):
        sid = b.get('subscription_id')
        if sid not in s.subs:
            _err('IAM.NOT_FOUND', 'unknown subscription')
        _, cseq, n = self._parse_cursor(b.get('cursor'))
        sub = s.subs[sid]
        sub['unacked'] = [e for e in sub['unacked'] if e['_pos'] > (cseq, n)]
        return {'subscription_id': sid, 'cursor': b['cursor']}

    def _changes_of(self, sub, cid):
        c = self.commits[cid]
        branch, cseq = c['branch'], c['cseq']
        if sub['kind'] == 'events':
            types = (sub['spec'] or {}).get('types')
            evs = [e for e in self.outbox if e['branch'] == branch and e['cseq'] == cseq
                   and (not types or e['type'] in types)]
            return [{'event_id': e['effect_id'], 'type': e['type'], 'object_id': e.get('object_id'),
                     'payload': e.get('payload', {})} for e in evs] if evs else None
        changed = sorted(c.get('ops') or {})
        if sub['kind'] == 'target':
            changed = [o for o in changed if o == sub['object_id']]
        elif sub['kind'] == 'query':
            q = sub['spec']
            matching = set(self.query(dict(q, branch=branch, commit=c['commit_id'], limit=1000))['objects'])
            cls = q['from']
            changed = [o for o in changed if o in matching or
                       (o in c['state']['objects'] and cls in self._cls(c['state']['objects'][o]['class'])['linearization'])]
        items = []
        for oid in changed:
            o = c['state']['objects'].get(oid)
            if o is None:
                continue
            ops = c['ops'][oid]
            kind = ops[-1]['kind'] if ops else 'update'
            op = 'added' if kind == 'create' else 'ended' if kind == 'end' else 'changed'
            items.append({'object_id': oid, 'class': o['class'], 'op': op, 'version_hash': o['head'], 'cseq': cseq})
        return items or None

    def _catch_up(self, s):
        out = []
        for sid in sorted(s.subs, key=lambda x: int(x.split('-')[1])):
            sub = s.subs[sid]
            head = self._head_cseq(sub['branch'])
            while sub['cseq'] < head:
                sub['cseq'] += 1
                cid = self._commit_at(sub['branch'], sub['cseq'])
                items = self._changes_of(sub, cid)
                if items is None:
                    continue
                body = {'subscription_id': sid, 'cursor': self._cursor(sub['branch'], sub['cseq'], sub['n'])}
                body['events' if sub['kind'] == 'events' else 'changes'] = items
                sub['queued'].append({'_pos': (sub['cseq'], sub['n']), 'body': body})
            if sub['kind'] == 'events':
                # effects released without a commit follow the commits, one frame each (DEC-083)
                types = (sub['spec'] or {}).get('types')
                for e in self._timer_effects(sub['branch'])[sub['n']:]:
                    sub['n'] += 1
                    if types and e['type'] not in types:
                        continue
                    body = {'subscription_id': sid, 'cursor': self._cursor(sub['branch'], sub['cseq'], sub['n']),
                            'events': [{'event_id': e['effect_id'], 'type': e['type'], 'object_id': e.get('object_id'),
                                        'payload': e.get('payload', {})}]}
                    sub['queued'].append({'_pos': (sub['cseq'], sub['n']), 'body': body})
            while sub['queued'] and len(sub['unacked']) < s.window:
                e = sub['queued'].pop(0)
                sub['unacked'].append(e)
                out.append(self._frame(s, 'emit', e['body']))
        return out
