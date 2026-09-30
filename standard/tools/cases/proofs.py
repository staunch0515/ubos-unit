"""Vector cases of BPA 1.0-rc1 for proofs (PH-1 step 1d, DEC-042): signatures and key
histories (STD-PROOF-010, -011), inclusion and consistency proofs (STD-PROOF-020, -021),
tenant roots (STD-PROOF-030), the portable archive (STD-PROOF-070) and the algorithm
registry (STD-CONF-011). Tampered inputs are built here with the oracle and embedded as
literals, so every vector is self-contained. Not required of the PH-0 prototype."""
import copy
import hashlib

from bpa import crypto, proofs
from bpa.store import Store

from .common import CLASSES, NOW, uid, commit, create, base, stock

S1 = '9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60'   # RFC 8032, test 1
S2 = '4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb'   # RFC 8032, test 2
S3 = '03' * 32
ALICE = 'did:ubos:acme:alice'
CID = 'sha256:' + hashlib.sha256(b'invoice 42').hexdigest()
H = lambda s: 'sha256:' + hashlib.sha256(s.encode()).hexdigest()
ENT = lambda n: [['o/' + uid(i), H('v%d' % i)] for i in range(n)]


def did(id_, *keys):
    out = []
    for key_id, seed, f, t in keys:
        k = {'key_id': key_id, 'public_key': crypto.public_key(seed)}
        if f:
            k['valid_from'] = f
        if t:
            k['valid_to'] = t
        out.append(k)
    return {'id': id_, 'keys': out}


ALICE_DOC = did(ALICE, (ALICE + '#k1', S1, '2026-01-01T00:00:00.000000Z', '2026-06-01T00:00:00.000000Z'),
                (ALICE + '#k2', S2, '2026-06-01T00:00:00.000000Z', None))


def op(o, label=None, error=None, **inp):
    s = {'op': o, 'input': inp}
    if label:
        s['label'] = label
    if error:
        s['error'] = error
    return s


def same(o, target, **inp):
    return {'op': o, 'input': inp, 'same_as': target}


def ref(label, ptr=''):
    return {'$ref': '%s/%s' % (label, ptr)}


def oracle(steps):
    """Run steps on a fresh reference store and return it (for literal inputs)."""
    s = Store()
    s.set_now(NOW)
    s.load_genesis(CLASSES)
    for fn in steps:
        fn(s)
    return s


def sample_store():
    return oracle([
        lambda s: s.commit('main', [create(uid(1)), create(uid(2), slug='c-2', body={'name': 'Beta'})]),
        lambda s: s.commit('main', [stock(uid(10), 'bolt', '5')]),
        lambda s: s.create_branch('feature/a'),
        lambda s: s.commit('feature/a', [create(uid(3), slug='c-3', body={'name': 'Gamma'})]),
        lambda s: s.tag('q1', 'main')])


def cases(g):
    V = g.vector

    def P(clause, desc, o, inp, error=None):
        g.pure(clause, desc, o, inp, error=error, prototype=False)

    def S(clause, desc, steps, setup=True):
        V(clause, desc, setup=[{'op': 'set_now', 'input': {'now': NOW}},
                               {'op': 'load_genesis', 'input': {'classes': CLASSES}}] if setup else [],
          steps=steps, prototype=False)

    sig1 = crypto.sign(S1, CID, ALICE, ALICE + '#k1')

    # ---- STD-PROOF-010 signature format ----
    P('STD-PROOF-010', 'the public key of the RFC 8032 test key', 'public_key', {'seed': S1})
    P('STD-PROOF-010', 'an Ed25519 signature over the UTF-8 bytes of a content ID', 'sign',
      {'seed': S1, 'content_id': CID, 'signer': ALICE, 'key_id': ALICE + '#k1'})
    P('STD-PROOF-010', 'a signature verifies under the key of its signer', 'verify_signature',
      {'signature': sig1, 'content_id': CID, 'did': ALICE_DOC})
    P('STD-PROOF-010', 'a signature does not verify for another content ID', 'verify_signature',
      {'signature': sig1, 'content_id': H('invoice 43'), 'did': ALICE_DOC}, error='PROOF.SIGNATURE_INVALID')
    P('STD-PROOF-010', 'the signed bytes are the content ID string, not its digest', 'verify_signature',
      {'signature': dict(sig1, sig=crypto.b64u(crypto.ed25519_sign(bytes.fromhex(S1), bytes.fromhex(CID[7:])))),
       'content_id': CID, 'did': ALICE_DOC}, error='PROOF.SIGNATURE_INVALID')
    P('STD-PROOF-010', 'an unregistered signature algorithm is rejected', 'verify_signature',
      {'signature': dict(sig1, alg='rsa-pss'), 'content_id': CID, 'did': ALICE_DOC}, error='MODEL.INVALID_VALUE')
    P('STD-PROOF-010', 'a signature is 64 bytes in base64url without padding', 'verify_signature',
      {'signature': dict(sig1, sig=sig1['sig'] + '=='), 'content_id': CID, 'did': ALICE_DOC},
      error='MODEL.INVALID_VALUE')
    P('STD-PROOF-010', 'a signature role is one of the registered roles', 'sign',
      {'seed': S1, 'content_id': CID, 'signer': ALICE, 'key_id': ALICE + '#k1', 'role': 'witness'},
      error='MODEL.INVALID_VALUE')
    P('STD-PROOF-010', 'the signer must be the subject of the DID document', 'verify_signature',
      {'signature': dict(sig1, signer='did:ubos:acme:bob'), 'content_id': CID, 'did': ALICE_DOC},
      error='PROOF.SIGNATURE_INVALID')
    S('STD-PROOF-010', 'commit records are signed by the node key', [
        commit([create(uid(1))]), op('commit_record', 'r', branch='main'), op('node_did', 'n'),
        op('verify_signature', signature=ref('r', 'signatures/0'), content_id=ref('r', 'commit_id'),
           did=ref('n'))])
    V('STD-PROOF-010', 'a content ID computed and then signed verifies', prototype=False, steps=[
        op('content_id', 'c', domain='ubos.chunk.v1', value={'invoice': 42}),
        op('sign', 's', seed=S2, content_id=ref('c'), signer=ALICE, key_id=ALICE + '#k2'),
        op('verify_signature', signature=ref('s'), content_id=ref('c'), did=ALICE_DOC)])

    # ---- STD-PROOF-011 verification with key history ----
    P('STD-PROOF-011', 'a key verifies inside its validity period', 'verify_signature',
      {'signature': sig1, 'content_id': CID, 'did': ALICE_DOC, 'at': '2026-03-15T10:00:00.000000Z'})
    P('STD-PROOF-011', 'a key does not verify before its validity period', 'verify_signature',
      {'signature': sig1, 'content_id': CID, 'did': ALICE_DOC, 'at': '2025-12-31T23:59:59.999999Z'},
      error='PROOF.SIGNATURE_INVALID')
    P('STD-PROOF-011', 'a retired key does not verify items signed after it was retired', 'verify_signature',
      {'signature': sig1, 'content_id': CID, 'did': ALICE_DOC, 'at': '2026-06-01T00:00:00.000000Z'},
      error='PROOF.SIGNATURE_INVALID')
    P('STD-PROOF-011', 'the successor key verifies items signed after the rotation', 'verify_signature',
      {'signature': crypto.sign(S2, CID, ALICE, ALICE + '#k2'), 'content_id': CID, 'did': ALICE_DOC,
       'at': '2026-07-01T00:00:00.000000Z'})
    P('STD-PROOF-011', 'a key the DID document does not list does not verify', 'verify_signature',
      {'signature': dict(sig1, key_id=ALICE + '#k9'), 'content_id': CID, 'did': ALICE_DOC},
      error='PROOF.SIGNATURE_INVALID')
    P('STD-PROOF-011', 'a signature by a key of the history under another key id does not verify',
      'verify_signature', {'signature': dict(sig1, key_id=ALICE + '#k2'), 'content_id': CID, 'did': ALICE_DOC,
                           'at': '2026-07-01T00:00:00.000000Z'}, error='PROOF.SIGNATURE_INVALID')
    S('STD-PROOF-011', 'commits before a node key rotation verify under the retired key at their time', [
        commit([create(uid(1))]), op('commit_record', 'r1', branch='main'),
        op('rotate_node_key', 'n', seed=S3), commit([create(uid(2), slug='c-2')]),
        op('commit_record', 'r2', branch='main'),
        op('verify_signature', signature=ref('r1', 'signatures/0'), content_id=ref('r1', 'commit_id'),
           did=ref('n'), at=ref('r1', 'tx_time')),
        op('verify_signature', signature=ref('r2', 'signatures/0'), content_id=ref('r2', 'commit_id'),
           did=ref('n'), at=ref('r2', 'tx_time'))])
    S('STD-PROOF-011', 'a commit signature does not verify at a time after its key was retired', [
        commit([create(uid(1))]), op('commit_record', 'r1', branch='main'),
        op('rotate_node_key', 'n', seed=S3), commit([create(uid(2), slug='c-2')]),
        op('commit_record', 'r2', branch='main'),
        op('verify_signature', error='PROOF.SIGNATURE_INVALID', signature=ref('r1', 'signatures/0'),
           content_id=ref('r1', 'commit_id'), did=ref('n'), at=ref('r2', 'tx_time'))])
    S('STD-PROOF-011', 'the node DID document records the key history', [
        commit([create(uid(1))]), op('set_now', now='2026-04-01T00:00:00.000000Z'),
        op('rotate_node_key', seed=S3), op('node_did')])

    # ---- STD-PROOF-020 inclusion proof ----
    small = ENT(10)
    big = ENT(500)
    p_small = proofs.inclusion_proof(small, 'o/' + uid(3))
    p_big = proofs.inclusion_proof(big, 'o/' + uid(250))
    P('STD-PROOF-020', 'an inclusion proof of a one-node tree is the leaf', 'inclusion_proof',
      {'entries': small, 'key': 'o/' + uid(3)})
    P('STD-PROOF-020', 'an inclusion proof lists the nodes from the leaf to the root', 'inclusion_proof',
      {'entries': big, 'key': 'o/' + uid(250)})
    V('STD-PROOF-020', 'an inclusion proof verifies against the tree root', prototype=False, steps=[
        op('tree_root', 'r', entries=big), op('inclusion_proof', 'p', entries=big, key='o/' + uid(250)),
        op('verify_inclusion', proof=ref('p'), root=ref('r'))])
    P('STD-PROOF-020', 'a key that is not in the tree has no inclusion proof', 'inclusion_proof',
      {'entries': small, 'key': 'o/' + uid(99)}, error='IAM.NOT_FOUND')
    P('STD-PROOF-020', 'a proof for another value does not verify', 'verify_inclusion',
      {'proof': dict(p_small, value=H('forged')), 'root': proofs.tree_root(small)}, error='PROOF.VERIFICATION_FAILED')
    P('STD-PROOF-020', 'a proof does not verify against another root', 'verify_inclusion',
      {'proof': p_small, 'root': proofs.tree_root(ENT(11))}, error='PROOF.VERIFICATION_FAILED')
    forged = copy.deepcopy(p_big)
    forged['nodes'][0]['entries'][-1][1] = H('forged')
    P('STD-PROOF-020', 'a changed sibling breaks the hash chain', 'verify_inclusion',
      {'proof': forged, 'root': proofs.tree_root(big)}, error='PROOF.VERIFICATION_FAILED')
    P('STD-PROOF-020', 'a proof is checked against a root or a signed commit', 'verify_inclusion',
      {'proof': p_small}, error='MODEL.INVALID_VALUE')
    S('STD-PROOF-020', 'a proof of an object verifies against the exported state root', [
        commit([create(uid(1)), create(uid(2), slug='c-2')]), op('prove', 'p', branch='main', object_id=uid(2)),
        op('export_state', 'e', branch='main'), op('node_did', 'n'),
        op('verify_inclusion', proof=ref('p', 'proof'), root=ref('e', 'root'), dids=[ref('n')])])
    S('STD-PROOF-020', 'a proof of an entry verifies against the signed commit', [
        commit([create(uid(1)), stock(uid(10), 'bolt', '5')]), op('node_did', 'n'),
        op('prove', 'p', branch='main', entry_id=uid(10)),
        op('verify_inclusion', proof=ref('p', 'proof'), dids=[ref('n')])])
    S('STD-PROOF-020', 'a proof at an earlier commit verifies against that commit', [
        commit([create(uid(1))], label='a'), commit([{'op': 'update', 'object_id': uid(1), 'base': base('a', uid(1)),
                                                      'body': {'email': 'a@x.com'}}]),
        op('node_did', 'n'), op('prove', 'p', branch='main', object_id=uid(1), time=1),
        op('export_state', 'e', branch='main', time=1),
        op('verify_inclusion', proof=ref('p', 'proof'), root=ref('e', 'root'), dids=[ref('n')])])
    S('STD-PROOF-020', 'a proof whose commit is signed by an unknown key does not verify', [
        commit([create(uid(1))]), op('prove', 'p', branch='main', object_id=uid(1)),
        op('verify_inclusion', error='PROOF.SIGNATURE_INVALID', proof=ref('p', 'proof'),
           dids=[did(proofs.NODE_DID, (proofs.NODE_DID + '#k1', S3, None, None))])])
    S('STD-PROOF-020', 'an object absent from the branch has no proof', [
        commit([create(uid(1))]), op('prove', error='IAM.NOT_FOUND', branch='main', object_id=uid(2))])

    # ---- STD-PROOF-021 consistency proof ----
    three = [commit([create(uid(1))]), commit([create(uid(2), slug='c-2')]), commit([create(uid(3), slug='c-3')])]
    S('STD-PROOF-021', 'a consistency proof lists the commit records back to the earlier commit', three + [
        op('node_did', 'n'), op('consistency_proof', 'p', branch='main', **{'from': 1, 'to': 3}),
        op('verify_consistency', commits=ref('p', 'records'), dids=[ref('n')],
           **{'from': ref('p', 'from_commit'), 'to': ref('p', 'to_commit')})])
    S('STD-PROOF-021', 'a proof from a commit to itself has one record', three + [
        op('node_did', 'n'), op('consistency_proof', 'p', branch='main', **{'from': 2, 'to': 2}),
        op('verify_consistency', commits=ref('p', 'records'), dids=[ref('n')])])
    S('STD-PROOF-021', 'the earlier commit comes first', three + [
        op('consistency_proof', error='IAM.NOT_FOUND', branch='main', **{'from': 3, 'to': 1})])
    S('STD-PROOF-021', 'a consistency proof follows first parents across a merge', [
        commit([create(uid(1))]), op('create_branch', name='feature/a', **{'from': 'main'}),
        commit([create(uid(2), slug='c-2')], branch='feature/a'), commit([create(uid(3), slug='c-3')]),
        op('merge', source='feature/a', target='main'), op('node_did', 'n'),
        op('consistency_proof', 'p', branch='main', **{'from': 1}),
        op('verify_consistency', commits=ref('p', 'records'), dids=[ref('n')])])
    st = oracle([lambda s: s.commit('main', [create(uid(1))]),
                 lambda s: s.commit('main', [create(uid(2), slug='c-2')]),
                 lambda s: s.commit('main', [create(uid(3), slug='c-3')])])
    recs = st.consistency_proof('main', 1, 3)['records']
    node = st.node_did()
    P('STD-PROOF-021', 'records of another oracle verify when their links are intact', 'verify_consistency',
      {'commits': recs, 'dids': [node]})
    P('STD-PROOF-021', 'records out of order break the parent links', 'verify_consistency',
      {'commits': list(reversed(recs)), 'dids': [node]}, error='PROOF.VERIFICATION_FAILED')
    P('STD-PROOF-021', 'a missing record breaks the parent links', 'verify_consistency',
      {'commits': [recs[0], recs[2]], 'dids': [node]}, error='PROOF.VERIFICATION_FAILED')
    changed = copy.deepcopy(recs)
    changed[1]['root'] = H('forged')
    P('STD-PROOF-021', 'a changed record no longer matches its signature', 'verify_consistency',
      {'commits': changed, 'dids': [node]}, error='PROOF.SIGNATURE_INVALID')
    P('STD-PROOF-021', 'the proof must end at the stated earlier commit', 'verify_consistency',
      {'commits': recs[:2], 'dids': [node], 'from': st.commits[st._commit_at('main', 1)]['commit_id']},
      error='PROOF.VERIFICATION_FAILED')

    # ---- STD-PROOF-030 tenant root ----
    lines = [op('create_branch', name='release/v1', **{'from': 'main'}),
             op('create_branch', name='overlay/acme', **{'from': 'main'}),
             op('create_branch', name='feature/a', **{'from': 'main'}), commit([create(uid(1))])]
    S('STD-PROOF-030', 'the tenant root covers the main, release and overlay branches', lines + [
        op('tenant_root', 't'), same('tree_root', 't/root', entries=ref('t', 'heads'))])
    S('STD-PROOF-030', 'the tenant root is signed by the node key', lines + [
        op('tenant_root', 't'), op('node_did', 'n'),
        op('verify_signature', signature=ref('t', 'signature'), content_id=ref('t', 'root'), did=ref('n'))])
    S('STD-PROOF-030', 'commits on other branch kinds do not change the tenant root', lines + [
        op('tenant_root', 't'), commit([create(uid(2), slug='c-2')], branch='feature/a'), same('tenant_root', 't')])
    S('STD-PROOF-030', 'the tenant root names each branch by VAE and branch name', lines + [
        op('tenant_root', 't'), op('commit_record', 'r', branch='release/v1'),
        same('normalize_text', 't/heads/2/1', text=ref('r', 'commit_id')),
        same('normalize_text', 't/heads/2/0', text='acme/release/v1')])

    # ---- STD-PROOF-070 portable archive ----
    build = [commit([create(uid(1)), create(uid(2), slug='c-2', body={'name': 'Beta'})]),
             commit([stock(uid(10), 'bolt', '5')]),
             op('create_branch', name='feature/a', **{'from': 'main'}),
             commit([create(uid(3), slug='c-3', body={'name': 'Gamma'})], branch='feature/a'),
             op('tag', name='q1', branch='main')]
    S('STD-PROOF-070', 'importing an export reproduces the head roots', build + [
        op('export_state', 'm', branch='main'), op('export_state', 'f', branch='feature/a'),
        op('export_archive', 'a'), op('reset'), op('set_now', now=NOW), op('import_archive', archive=ref('a', 'archive')),
        same('export_state', 'm', branch='main'), same('export_state', 'f', branch='feature/a')])
    S('STD-PROOF-070', 'objects, entries and tags are readable after import', build + [
        op('export_archive', 'a'), op('reset'), op('set_now', now=NOW), op('import_archive', archive=ref('a', 'archive')),
        op('read', branch='feature/a', object_id=uid(3)),
        op('read_ledger', branch='main', ledger_key={'item': 'bolt'}, **{'class': 'StockMove'}),
        op('tag_info', name='q1')])
    S('STD-PROOF-070', 'the export lists its counts', build + [op('export_archive')])
    S('STD-PROOF-070', 'history survives the import', build + [
        op('export_archive', 'a'), op('reset'), op('set_now', now=NOW), op('import_archive', archive=ref('a', 'archive')),
        op('read', branch='main', object_id=uid(1), time=1), op('export_state', branch='main', time=0)])
    S('STD-PROOF-070', 'an archive is imported into an empty store only', build + [
        op('export_archive', 'a'), op('import_archive', error='MODEL.INVALID_VALUE', archive=ref('a', 'archive'))])
    arc = sample_store().export_archive()['archive']
    V('STD-PROOF-070', 'an archive written by another implementation imports with its head roots',
      prototype=False, steps=[op('set_now', now=NOW), op('import_archive', archive=arc), op('export_state', branch='main'),
                              op('read', branch='feature/a', object_id=uid(3))])
    bad = copy.deepcopy(arc)
    body = next(k for k, v in bad['chunks'].items() if v == {'name': 'Beta'})
    bad['chunks'][body] = {'name': 'Mallory'}
    V('STD-PROOF-070', 'a changed body chunk is detected', prototype=False,
      steps=[op('import_archive', error='PROOF.VERIFICATION_FAILED', archive=bad)])
    bad = copy.deepcopy(arc)
    bad['manifest']['counts']['tags'] = 0
    V('STD-PROOF-070', 'a changed manifest no longer matches its signature', prototype=False,
      steps=[op('import_archive', error='PROOF.SIGNATURE_INVALID', archive=bad)])
    bad = copy.deepcopy(arc)
    vh = next(iter(sorted(bad['versions'])))
    bad['versions'][vh]['slug'] = 'forged'
    V('STD-PROOF-070', 'a changed version envelope is detected', prototype=False,
      steps=[op('import_archive', error='PROOF.VERIFICATION_FAILED', archive=bad)])
    bad = copy.deepcopy(arc)
    eh = next(iter(bad['entries']))
    bad['entries'][eh]['valid_at'] = '2020-01-01'
    V('STD-PROOF-070', 'a changed entry record is detected', prototype=False,
      steps=[op('import_archive', error='PROOF.VERIFICATION_FAILED', archive=bad)])
    bad = copy.deepcopy(arc)
    del bad['commits'][next(iter(sorted(bad['commits'])))]
    V('STD-PROOF-070', 'a missing commit is detected', prototype=False,
      steps=[op('import_archive', error='PROOF.VERIFICATION_FAILED', archive=bad)])

    # ---- STD-CONF-011 algorithm registry ----
    K = 'feffe9928665731c6d6a8f9467308308feffe9928665731c6d6a8f9467308308'
    N = 'cafebabefacedbaddecaf888'
    PT = ('d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a72'
          '1c3c0c95956809532fcf0e2449a6b525b16aedf5aa0de657ba637b39')
    AAD = 'feedfacedeadbeeffeedfacedeadbeefabaddad2'
    CT = crypto.aead_encrypt('aes-256-gcm', K, N, PT, AAD)
    XK = ''.join('%02x' % i for i in range(0x80, 0xa0))
    XN = ''.join('%02x' % i for i in range(0x40, 0x58))
    XPT = b"Ladies and Gentlemen of the class of '99".hex()
    XAAD = '50515253c0c1c2c3c4c5c6c7'
    XCT = crypto.aead_encrypt('xchacha20-poly1305', XK, XN, XPT, XAAD)
    P('STD-CONF-011', 'AES-256-GCM content decrypts (SP 800-38D test case 16)', 'aead_decrypt',
      {'alg': 'aes-256-gcm', 'key': K, 'nonce': N, 'ciphertext': CT, 'aad': AAD})
    P('STD-CONF-011', 'XChaCha20-Poly1305 content decrypts', 'aead_decrypt',
      {'alg': 'xchacha20-poly1305', 'key': XK, 'nonce': XN, 'ciphertext': XCT, 'aad': XAAD})
    P('STD-CONF-011', 'a changed AES-256-GCM tag fails', 'aead_decrypt',
      {'alg': 'aes-256-gcm', 'key': K, 'nonce': N, 'ciphertext': CT[:-2] + ('00' if CT[-2:] != '00' else '01'),
       'aad': AAD}, error='PROOF.VERIFICATION_FAILED')
    P('STD-CONF-011', 'changed associated data fails XChaCha20-Poly1305', 'aead_decrypt',
      {'alg': 'xchacha20-poly1305', 'key': XK, 'nonce': XN, 'ciphertext': XCT, 'aad': ''},
      error='PROOF.VERIFICATION_FAILED')
    P('STD-CONF-011', 'XChaCha20-Poly1305 uses a 24-byte nonce', 'aead_decrypt',
      {'alg': 'xchacha20-poly1305', 'key': XK, 'nonce': N, 'ciphertext': XCT, 'aad': XAAD},
      error='MODEL.INVALID_VALUE')
    P('STD-CONF-011', 'an unregistered AEAD algorithm is rejected', 'aead_decrypt',
      {'alg': 'aes-128-gcm', 'key': K, 'nonce': N, 'ciphertext': CT}, error='MODEL.INVALID_VALUE')
    P('STD-CONF-011', 'empty content with associated data decrypts', 'aead_decrypt',
      {'alg': 'aes-256-gcm', 'key': K, 'nonce': N, 'aad': AAD,
       'ciphertext': crypto.aead_encrypt('aes-256-gcm', K, N, '', AAD)})
    P('STD-CONF-011', 'Ed25519 signatures of the RFC 8032 test key verify', 'verify_signature',
      {'signature': crypto.sign(S2, CID, ALICE, ALICE + '#k2'), 'content_id': CID, 'did': ALICE_DOC})
    P('STD-CONF-011', 'content identifiers use sha256', 'content_id', {'domain': 'ubos.chunk.v1', 'value': 'abc'})
