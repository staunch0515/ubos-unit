"""Proofs of BPA 1.0-rc1 (PH-1 step 1d, DEC-042): signed commit records, inclusion and
consistency proofs (STD-PROOF-020, -021), tenant roots (STD-PROOF-030) and the portable
archive (STD-PROOF-070). The pure functions verify; the mixin produces them from the
reference store."""
import copy
import itertools

from . import BpaError
from . import crypto
from .canon import canonical, content_id
from .tree import is_boundary, node_hash, tree_root

NODE_DID = 'did:ubos:acme:01900000-0000-7000-8000-000000000001'
NODE_SEED = '01' * 32          # the adapter's initial node key (STD-CONF-005, convention 10)
ARCHIVE_FORMAT = 'ubos.archive.v1'


def _fail(msg):
    raise BpaError('PROOF.VERIFICATION_FAILED', msg)


# ---- inclusion proofs (STD-PROOF-020) ----

def inclusion_proof(entries, key):
    """{key, value, nodes}: the canonical nodes of the path from the leaf holding the key to
    the root (STD-PROOF-001)."""
    items = sorted(([k, h] for k, h in entries), key=lambda e: e[0].encode('utf-8'))
    tree_root(items)                                  # rejects duplicate keys
    values = dict((k, h) for k, h in items)
    if key not in values:
        raise BpaError('IAM.NOT_FOUND', 'key not in the tree')
    level, path, target = 0, [], key
    while True:
        nodes, current = [], []
        for k, h in items:
            current.append([k, h])
            if is_boundary(level, k):
                nodes.append(current)
                current = []
        if current:
            nodes.append(current)
        mine = next(n for n in nodes if any(k == target for k, _ in n))
        path.append({'level': level, 'entries': mine})
        if len(nodes) == 1:
            return {'key': key, 'value': values[key], 'nodes': path}
        target = mine[0][0]
        items = [[n[0][0], node_hash(level, n)] for n in nodes]
        level += 1


def _commit_check(commit, dids):
    """The commit identifier and the signatures of a commit record; returns the identifier."""
    if not isinstance(commit, dict) or not isinstance(commit.get('signatures'), list) or not commit['signatures']:
        _fail('a commit record carries signatures')
    cid = content_id('ubos.commit.v1', {k: v for k, v in commit.items() if k != 'signatures'})
    docs = {d.get('id'): d for d in dids or []}
    for s in commit['signatures']:
        doc = docs.get(s.get('signer') if isinstance(s, dict) else None)
        if doc is None:
            raise BpaError('PROOF.SIGNATURE_INVALID', 'no DID document for the signer')
        crypto.verify(s, cid, doc, commit.get('tx_time'))
    return cid


def verify_inclusion(proof, root=None, dids=None):
    if not isinstance(proof, dict) or not isinstance(proof.get('nodes'), list) or not proof['nodes']:
        raise BpaError('MODEL.INVALID_VALUE', 'an inclusion proof is DAT-InclusionProof')
    nodes = proof['nodes']
    if [proof.get('key'), proof.get('value')] not in (nodes[0].get('entries') or []) or nodes[0].get('level') != 0:
        _fail('the leaf does not hold the key and value')
    h = None
    for i, n in enumerate(nodes):
        ents = n.get('entries') or []
        keys = [e[0] for e in ents]
        if not ents or keys != sorted(keys, key=lambda k: k.encode('utf-8')) or len(set(keys)) != len(keys):
            _fail('a node is not canonical')
        if i > 0 and ([nodes[i - 1]['entries'][0][0], h] not in ents or n.get('level') != nodes[i - 1]['level'] + 1):
            _fail('the hash chain is broken')
        h = node_hash(n['level'], ents)
    if 'commit' in proof:
        _commit_check(proof['commit'], dids)
        if proof['commit'].get('root') != h:
            _fail('the root is not the root of the commit')
    elif root is None:
        raise BpaError('MODEL.INVALID_VALUE', 'a root or a signed commit is needed')
    if root is not None and root != h:
        _fail('the proof does not lead to the root')
    return True


# ---- consistency proofs (STD-PROOF-021) ----

def verify_consistency(commits, frm=None, to=None, dids=None):
    """commits: records from the later commit B back to A along first parents."""
    if not isinstance(commits, list) or not commits:
        raise BpaError('MODEL.INVALID_VALUE', 'a consistency proof is a list of commit records')
    ids = [_commit_check(c, dids) for c in commits]
    for i in range(len(commits) - 1):
        ps = commits[i].get('parents') or []
        if not ps or ps[0] != ids[i + 1]:
            _fail('a first-parent link is broken')
        a, b = commits[i].get('tx_time'), commits[i + 1].get('tx_time')
        if a is None or (b is not None and not b < a):
            _fail('transaction times do not increase')
    if to is not None and ids[0] != to:
        _fail('the proof does not start at the later commit')
    if frm is not None and ids[-1] != frm:
        _fail('the proof does not end at the earlier commit')
    return True


class Proofs:
    TENANT = 'acme'

    # ---- node key and signed commit records (STD-PROOF-010, -011) ----

    def _node_init(self):
        self.node = {'seed': NODE_SEED, 'n': 1,
                     'keys': [{'key_id': NODE_DID + '#k1', 'public_key': crypto.public_key(NODE_SEED)}]}

    def _node_sign(self, cid, role='node'):
        return crypto.sign(self.node['seed'], cid, NODE_DID, self.node['keys'][-1]['key_id'], role)

    def node_did(self):
        return {'id': NODE_DID, 'keys': copy.deepcopy(self.node['keys'])}

    def rotate_node_key(self, seed):
        crypto.unhex(seed, 32, 'seed')
        txs = [c['tx_time'] for c in self.commits.values() if c['tx_time']]
        at = self._hlc(max(txs) if txs else None)
        self.node['keys'][-1]['valid_to'] = at
        self.node['n'] += 1
        self.node['seed'] = seed
        self.node['keys'].append({'key_id': '%s#k%d' % (NODE_DID, self.node['n']),
                                  'public_key': crypto.public_key(seed), 'valid_from': at})
        return self.node_did()

    def commit_record(self, branch, sel=None):
        cid, _ = self._select(branch, sel or {})
        return dict(copy.deepcopy(self.commits[cid]['record']), commit_id=self.commits[cid]['commit_id'])

    # ---- proofs from the store (STD-PROOF-020, -021, -030) ----

    def prove(self, branch, object_id=None, entry_id=None, sel=None):
        cid, _ = self._select(branch, sel or {})
        st = self.commits[cid]['state']
        if object_id is not None:
            key = 'o/' + object_id
        else:
            k, e = self._entry_owner(st, entry_id)
            if e is None:
                raise BpaError('IAM.NOT_FOUND', 'unknown entry')
            key = next(x for x, h in self.state_entries(st) if h == e['entry_hash'] and x.startswith('l/'))
        ents = self.state_entries(st)
        if key not in dict(ents):
            raise BpaError('IAM.NOT_FOUND', 'not in the state tree of the branch')
        proof = inclusion_proof(ents, key)
        proof['commit'] = copy.deepcopy(self.commits[cid]['record'])
        return {'key': key, 'proof': proof}

    def consistency_proof(self, branch, frm, to=None):
        b = self._commit_at(branch, to)
        a = self._commit_at(branch, frm)
        if self.commits[a]['cseq'] > self.commits[b]['cseq']:
            raise BpaError('IAM.NOT_FOUND', 'the earlier commit is after the later one')
        records = []
        for c in self._first_parents(b):
            records.append(copy.deepcopy(self.commits[c]['record']))
            if c == a:
                break
        return {'length': len(records), 'records': records, 'from_commit': self.commits[a]['commit_id'],
                'to_commit': self.commits[b]['commit_id']}

    def tenant_root(self):
        names = sorted(n for n, b in self.branches.items()
                       if b['kind'] in ('main', 'release', 'overlay') and self._branch_state(n) == 'active')
        heads = [['%s/%s' % (self.TENANT, n), self.commits[self.branches[n]['head']]['commit_id']] for n in names]
        root = tree_root(heads)
        return {'branches': names, 'heads': heads, 'root': root, 'signature': self._node_sign(root)}

    # ---- portable archive (STD-PROOF-070) ----

    def _entry_record(self, e):
        rec = {k: e[k] for k in ('entry_id', 'class', 'ledger_key', 'valid_at', 'corrects', 'correction_type',
                                 'posting_group', 'provenance_ref', 'source_refs') if k in e}
        rec['class_version'] = e['class_version']
        rec['body_ref'] = content_id('ubos.chunk.v1', e['body'])
        return rec

    def export_archive(self):
        chunks, versions, entries, commits = {}, {}, {}, {}

        def chunk(v):
            cid = content_id('ubos.chunk.v1', v)
            chunks[cid] = v
            return cid
        classes = sorted(chunk(d) for d in self.decls.values())
        for vh, v in self.versions.items():
            if any(vh == o['head'] or any(s[2] == vh for s in o['timeline'])
                   for c in self.commits.values() for o in c['state']['objects'].values()):
                chunk(v['body'])
                versions[vh] = v['envelope']
        for c in self.commits.values():
            ledger = []
            for (cls, _), q in sorted(c['state']['ledger'].items()):
                items = []
                for e in q:
                    chunk(e['body'])
                    entries[e['entry_hash']] = self._entry_record(e)
                    item = {'entry_hash': e['entry_hash'], 'seq': e['seq'], 'cseq': e['cseq']}
                    if 'source_seq' in e:
                        item['source_seq'] = e['source_seq']
                    items.append(item)
                if q:
                    ledger.append({'class': cls, 'ledger_key': q[0]['ledger_key'], 'entries': items})
            objects = {oid: {k: o[k] for k in ('class', 'kind', 'slug', 'head', 'timeline')}
                       for oid, o in c['state']['objects'].items()}
            commits[c['commit_id']] = {'record': c['record'], 'objects': objects, 'ledger': ledger,
                                       'hidden': c['hidden'], 'ops': c['ops']}
        cid_of = lambda i: self.commits[i]['commit_id']
        refs = {}
        for n, b in self.branches.items():
            r = {'head': cid_of(b['head']), 'kind': b['kind'], 'created_at': b['created_at'],
                 'active_at': b['active_at'], 'created_from': cid_of(b['created_from'])}
            if b['parent_line']:
                pl = dict(b['parent_line'])
                if 'pin' in pl:
                    pl['pin'] = cid_of(pl['pin'])
                r['parent_line'] = pl
            refs[n] = r
        tags = {n: dict(t, commit=cid_of(t['commit'])) for n, t in self.tags.items()}
        heads = {n: {'commit': cid_of(b['head']), 'root': self.commits[b['head']]['record']['root']}
                 for n, b in self.branches.items()}
        counts = {'chunks': len(chunks), 'versions': len(versions), 'entries': len(entries),
                  'commits': len(commits), 'refs': len(refs), 'tags': len(tags)}
        manifest = {'format': ARCHIVE_FORMAT, 'vae': self.TENANT, 'counts': counts, 'heads': heads,
                    'classes': classes}
        manifest['signature'] = self._node_sign(content_id('ubos.chunk.v1', manifest), 'signer')
        archive = {'manifest': manifest, 'chunks': chunks, 'versions': versions, 'entries': entries,
                   'commits': commits, 'refs': refs, 'tags': tags, 'dids': [self.node_did()]}
        # the chunk count depends on how an implementation stores content, so it is left out
        summary = {'version_records': counts['versions'], 'entries': counts['entries'], 'commits': counts['commits'],
                   'refs': counts['refs'], 'tags': counts['tags']}
        return {'counts': summary, 'archive': copy.deepcopy(archive)}

    def import_archive(self, archive):
        """Verify an archive and load it into an empty store; head roots are recomputed."""
        if len(self.commits) > 1 or self.decls:
            raise BpaError('MODEL.INVALID_VALUE', 'an archive is imported into an empty store')
        a = archive if isinstance(archive, dict) else {}
        m = a.get('manifest')
        if not isinstance(m, dict) or m.get('format') != ARCHIVE_FORMAT:
            raise BpaError('MODEL.INVALID_VALUE', 'not a %s archive' % ARCHIVE_FORMAT)
        dids = {d['id']: d for d in a.get('dids') or []}
        sig = m.get('signature')
        if not isinstance(sig, dict) or sig.get('signer') not in dids:
            raise BpaError('PROOF.SIGNATURE_INVALID', 'the manifest is not signed by a known node')
        crypto.verify(sig, content_id('ubos.chunk.v1', {k: v for k, v in m.items() if k != 'signature'}),
                      dids[sig['signer']])
        chunks = a.get('chunks') or {}
        for cid, v in chunks.items():
            if content_id('ubos.chunk.v1', v) != cid:
                _fail('chunk %s does not match its content ID' % cid)
        versions = a.get('versions') or {}
        for vh, env in versions.items():
            if content_id('ubos.version.v1', env) != vh or env.get('body_ref') not in chunks:
                _fail('version %s does not match its envelope' % vh)
        entries = a.get('entries') or {}
        for eh, rec in entries.items():
            if content_id('ubos.entry.v1', rec) != eh or rec.get('body_ref') not in chunks:
                _fail('entry %s does not match its record' % eh)
        commits = a.get('commits') or {}
        for cid, c in commits.items():
            if _commit_check(c['record'], list(dids.values())) != cid:
                _fail('commit %s does not match its record' % cid)
            if any(p not in commits for p in c['record'].get('parents') or []):
                _fail('a parent of commit %s is missing' % cid)
        for r in (a.get('refs') or {}).values():
            if r.get('head') not in commits or r.get('created_from') not in commits:
                _fail('a branch names a missing commit')
        counts = {'chunks': len(chunks), 'versions': len(versions), 'entries': len(entries),
                  'commits': len(commits), 'refs': len(a.get('refs') or {}), 'tags': len(a.get('tags') or {})}
        if counts != m.get('counts'):
            _fail('the counts differ from the manifest')
        # load
        now = self.now
        self.reset()
        self.now = now
        self.load_genesis([chunks[c] for c in m.get('classes') or []])
        self.commits, self.branches = {}, {}
        for vh, env in versions.items():
            self.versions[vh] = {'object_id': env['object_id'], 'class': env['class'], 'slug': env['slug'],
                                 'body': chunks[env['body_ref']], 'valid': env['valid'],
                                 'tombstone': bool(env.get('tombstone')), 'cseq': 0, 'envelope': env}
            if env.get('provenance_ref'):
                self.versions[vh]['provenance_ref'] = env['provenance_ref']
        ids = {}
        for n, cid in enumerate(sorted(commits, key=lambda x: (commits[x]['record']['cseq'], x))):
            ids[cid] = n
        for cid, c in commits.items():
            rec = c['record']
            ledger = {}
            for grp in c['ledger']:
                q = []
                for it in grp['entries']:
                    er = entries[it['entry_hash']]
                    e = {k: v for k, v in er.items() if k != 'body_ref'}
                    e.update(body=chunks[er['body_ref']], seq=it['seq'], cseq=it['cseq'], entry_hash=it['entry_hash'])
                    if 'source_seq' in it:
                        e['source_seq'] = it['source_seq']
                    q.append(e)
                ledger[(grp['class'], canonical(grp['ledger_key']))] = q
            objects = {}
            for oid, o in c['objects'].items():
                if o['head'] not in versions or any(s[2] not in versions for s in o['timeline']):
                    _fail('an object names a version that is not in the archive')
                objects[oid] = copy.deepcopy(o)
            state = {'objects': objects, 'ledger': ledger}
            if tree_root(self.state_entries(state)) != rec['root']:
                _fail('the state of commit %s does not give its root' % cid)
            self.commits[ids[cid]] = {'parents': [ids[p] for p in rec['parents']], 'state': state,
                                      'cseq': rec['cseq'], 'branch': rec['branch'], 'tx_time': rec['tx_time'],
                                      'commit_id': cid, 'ops': c.get('ops') or {}, 'hidden': c.get('hidden') or [],
                                      'record': copy.deepcopy(rec)}
        for n, r in (a.get('refs') or {}).items():
            pl = None
            if r.get('parent_line'):
                pl = dict(r['parent_line'])
                if 'pin' in pl:
                    pl['pin'] = ids[pl['pin']]
            self.branches[n] = {'head': ids[r['head']], 'kind': r['kind'], 'created_at': r['created_at'],
                                'active_at': r['active_at'], 'parent_line': pl, 'created_from': ids[r['created_from']]}
            if self.commits[ids[r['head']]]['record']['root'] != m['heads'][n]['root']:
                _fail('the head root of %s differs from the manifest' % n)
        for n, t in (a.get('tags') or {}).items():
            self.tags[n] = dict(t, commit=ids[t['commit']])
        self.ids = itertools.count(len(ids))
        return {'branches': {n: self.commits[b['head']]['cseq'] for n, b in sorted(self.branches.items())}}
