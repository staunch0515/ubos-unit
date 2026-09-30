"""Vector cases for the state tree (STD-PROOF-001…003)."""
import hashlib

from .common import setup, commit, create, uid, stock

H = lambda s: 'sha256:' + hashlib.sha256(s.encode()).hexdigest()
ENT = lambda n, p='o/': [[p + uid(i), H('v%d' % i)] for i in range(n)]


def cases(g):
    V, P = g.vector, g.pure
    # STD-PROOF-001 prolly tree
    P('STD-PROOF-001', 'the root of the empty state is the hash of the empty leaf node', 'tree_root', {'entries': []})
    P('STD-PROOF-001', 'a single entry', 'tree_root', {'entries': ENT(1)})
    P('STD-PROOF-001', 'ten entries', 'tree_root', {'entries': ENT(10)})
    P('STD-PROOF-001', 'a hundred entries span several leaf nodes', 'tree_root', {'entries': ENT(100)})
    P('STD-PROOF-001', 'five hundred entries build more than one level', 'tree_root', {'entries': ENT(500)})
    P('STD-PROOF-001', 'object and ledger keys together', 'tree_root',
      {'entries': ENT(5) + [['l/%s/%s/%020d' % ('a' * 64, 'b' * 64, i), H('e%d' % i)] for i in range(1, 6)]})
    V('STD-PROOF-001', 'the input order of entries does not matter', steps=[
        {'op': 'tree_root', 'input': {'entries': ENT(50)}, 'label': 'a'},
        {'op': 'tree_root', 'input': {'entries': list(reversed(ENT(50)))}, 'same_as': 'a/'}])
    V('STD-PROOF-001', 'changing one value changes the root', steps=[
        {'op': 'tree_root', 'input': {'entries': ENT(50)}, 'label': 'a'},
        {'op': 'tree_root', 'input': {'entries': ENT(49) + [['o/' + uid(49), H('changed')]]}}])
    P('STD-PROOF-001', 'duplicate keys are rejected', 'tree_root', {'entries': ENT(2) + ENT(1)}, error='MODEL.INVALID_VALUE')
    V('STD-PROOF-001', 'the exported state root is the tree root of the exported entries', setup=setup(), steps=[
        commit([create(uid(1)), create(uid(2), slug='b'), stock(uid(20), 'bolt', '5')]),
        {'op': 'export_state', 'input': {'branch': 'main'}, 'label': 'e'},
        {'op': 'tree_root', 'input': {'entries': {'$ref': 'e/entries'}}, 'same_as': 'e/root'}])
    V('STD-PROOF-001', 'the state tree has one key per object and per entry', setup=setup(), steps=[
        commit([create(uid(1)), create(uid(2), slug='b'), stock(uid(20), 'bolt', '5'), stock(uid(21), 'bolt', '1')]),
        {'op': 'export_state', 'input': {'branch': 'main'}, 'label': 'e'}], prototype=True)
    # STD-PROOF-002 history independence
    V('STD-PROOF-002', 'the same objects created in a different order give the same root', setup=setup(), steps=[
        commit([create(uid(1)), create(uid(2), slug='b')]),
        {'op': 'create_branch', 'input': {'name': 'feature/x', 'from': 'main', 'time': 0}},
        commit([create(uid(2), slug='b')], branch='feature/x'), commit([create(uid(1))], branch='feature/x'),
        {'op': 'export_state', 'input': {'branch': 'main'}, 'label': 'a'},
        {'op': 'export_state', 'input': {'branch': 'feature/x'}, 'same_as': 'a/'}])
    V('STD-PROOF-002', 'the same entries appended in separate commits give the same root', setup=setup(), steps=[
        commit([stock(uid(20), 'bolt', '5'), stock(uid(21), 'bolt', '1')]),
        {'op': 'create_branch', 'input': {'name': 'feature/x', 'from': 'main', 'time': 0}},
        commit([stock(uid(20), 'bolt', '5')], branch='feature/x'), commit([stock(uid(21), 'bolt', '1')], branch='feature/x'),
        {'op': 'export_state', 'input': {'branch': 'main'}, 'label': 'a'},
        {'op': 'export_state', 'input': {'branch': 'feature/x'}, 'same_as': 'a/'}])
    V('STD-PROOF-002', 'equal key-value sets give equal tree roots', steps=[
        {'op': 'tree_root', 'input': {'entries': ENT(30)[::2] + ENT(30)[1::2]}, 'label': 'a'},
        {'op': 'tree_root', 'input': {'entries': ENT(30)}, 'same_as': 'a/'}])
    # STD-PROOF-003 root in commit
    V('STD-PROOF-003', 'the root after a commit covers the changes of that commit', setup=setup(), steps=[
        commit([create(uid(1))]), {'op': 'export_state', 'input': {'branch': 'main'}, 'label': 'a'},
        commit([create(uid(2), slug='b')]),
        {'op': 'export_state', 'input': {'branch': 'main', 'time': 1}, 'same_as': 'a/'}])
    V('STD-PROOF-003', 'the root at a past time is the root of that commit', setup=setup(), steps=[
        commit([create(uid(1))]), {'op': 'export_state', 'input': {'branch': 'main'}, 'label': 'a'},
        commit([stock(uid(20), 'bolt', '5')]), {'op': 'export_state', 'input': {'branch': 'main', 'time': 1}, 'same_as': 'a/'}])
