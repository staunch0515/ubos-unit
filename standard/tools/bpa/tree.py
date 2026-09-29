"""Prolly tree over branch state (STD-PROOF-001, -002).

A key closes its node at level l when the first two bytes of
SHA-256("ubos.tree.boundary.v1" 0x00 decimal(l) 0x00 key), read big-endian, are below 1024.
Level l+1 holds [first key, node hash] of every node of level l; the root is the hash of the
only node of the top level, or of the empty leaf node."""
import hashlib

from . import BpaError
from .canon import content_id

FANOUT_THRESHOLD = 1024


def is_boundary(level, key):
    h = hashlib.sha256(b'ubos.tree.boundary.v1\x00' + str(level).encode('ascii') + b'\x00'
                       + key.encode('utf-8')).digest()
    return int.from_bytes(h[:2], 'big') < FANOUT_THRESHOLD


def node_hash(level, entries):
    return content_id('ubos.tree.leaf.v1' if level == 0 else 'ubos.tree.node.v1',
                      {'level': level, 'entries': entries})


def tree_root(entries):
    items = sorted(([k, h] for k, h in entries), key=lambda e: e[0].encode('utf-8'))
    keys = [k for k, _ in items]
    if len(set(keys)) != len(keys):
        raise BpaError('MODEL.INVALID_VALUE', 'duplicate tree key')
    if not items:
        return node_hash(0, [])
    level = 0
    while True:
        nodes, current = [], []
        for k, h in items:
            current.append([k, h])
            if is_boundary(level, k):
                nodes.append(current)
                current = []
        if current:
            nodes.append(current)
        if len(nodes) == 1:
            return node_hash(level, nodes[0])
        items = [[n[0][0], node_hash(level, n)] for n in nodes]
        level += 1
