"""Addressing (STD-ADDR-001, -002, -010; PH-1 step 1e-1, DEC-043): object URIs with
selectors and version suffixes, and the `did:ubos` method."""
import re

from . import BpaError
from .crypto import b64u, unb64u
from .values import GRAMMARS, SLUG, check_name, validate_value

URI = re.compile(r'\Aubos://(?P<authority>[^/?@]+)/(?P<cls>[^/?@]+)/(?P<slug>[^/?@]+)'
                 r'(?:\?(?P<sel>[^@]*))?(?:@v(?P<v>[0-9]+))?\Z')
SELECTORS = ('branch', 'commit', 'tag', 'time', 'asof', 'key', 'view')
EXCLUSIVE = ('commit', 'tag', 'time')
B58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
ED25519_PUB = b'\xed\x01'           # multicodec prefix of an Ed25519 public key


def _bad(msg):
    raise BpaError('MODEL.INVALID_VALUE', msg)


def parse_uri(uri):
    """STD-ADDR-001: the parts of an object URI; STD-ADDR-002: selector precedence."""
    m = URI.match(uri) if isinstance(uri, str) else None
    if not m:
        _bad('not a ubos URI')
    auth = m.group('authority').split('.')
    if len(auth) > 2 or not all(GRAMMARS['slug'].match(a) for a in auth):
        _bad('the authority is tenant[.vae]')
    if not GRAMMARS['class-name'].match(m.group('cls')):
        _bad('the class is a class name')
    if not GRAMMARS['slug'].match(m.group('slug')):
        _bad('the object is named by its slug')
    sel = {}
    if m.group('sel') is not None:
        for part in m.group('sel').split('&'):
            k, eq, v = part.partition('=')
            if not eq or k not in SELECTORS:
                _bad('unknown selector %s' % k)
            if k in sel:
                raise BpaError('ADDR.CONFLICTING_SELECTORS', 'selector %s given twice' % k)
            if k == 'branch':
                check_name('branch-name', v)
            elif k == 'commit':
                validate_value('text', {'pattern': r'sha256:[0-9a-f]{64}'}, v)
            elif k in ('tag', 'key', 'view'):
                if not GRAMMARS['slug'].match(v):
                    _bad('%s is a slug' % k)
            elif k == 'time':
                validate_value('instant', None, v)
            elif k == 'asof':
                try:
                    validate_value('date', None, v)
                except BpaError:
                    validate_value('instant', None, v)
            sel[k] = v
    if len([k for k in EXCLUSIVE if k in sel]) > 1:
        raise BpaError('ADDR.CONFLICTING_SELECTORS', 'at most one of commit, tag and time')
    out = {'tenant': auth[0], 'class': m.group('cls'), 'slug': m.group('slug'), 'selectors': sel,
           'canonical': 'ubos://%s/%s/%s' % (m.group('authority'), m.group('cls'), m.group('slug'))}
    if len(auth) == 2:
        out['vae'] = auth[1]
    if m.group('v') is not None:
        n = int(m.group('v'))
        if n < 1 or m.group('v') != str(n):
            _bad('versions are numbered from 1')
        out['version'] = n
    return out


# ---- did:ubos (STD-ADDR-010) ----

def b58encode(b):
    n = int.from_bytes(b, 'big')
    out = ''
    while n:
        n, r = divmod(n, 58)
        out = B58[r] + out
    return '1' * (len(b) - len(b.lstrip(b'\x00'))) + out


def b58decode(s):
    n = 0
    for ch in s:
        if ch not in B58:
            _bad('not base58btc')
        n = n * 58 + B58.index(ch)
    body = n.to_bytes((n.bit_length() + 7) // 8, 'big') if n else b''
    return b'\x00' * (len(s) - len(s.lstrip('1'))) + body


def key_did(public_key):
    """The key-form DID of an Ed25519 public key given in base64url."""
    return 'did:ubos:k:z' + b58encode(ED25519_PUB + unb64u(public_key, 32, 'public_key'))


def parse_did(did):
    if not isinstance(did, str) or not did.startswith('did:ubos:'):
        _bad('not a did:ubos identifier')
    rest = did[len('did:ubos:'):]
    if rest.startswith('k:'):
        mb = rest[2:]
        if not mb.startswith('z') or len(mb) < 2:
            _bad('the key form is multibase base58btc (z…)')
        raw = b58decode(mb[1:])
        if raw[:2] != ED25519_PUB or len(raw) != 34:
            _bad('the key form holds an Ed25519 public key')
        return {'form': 'key', 'public_key': b64u(raw[2:])}
    authority, sep, uid = rest.rpartition(':')
    parts = authority.split('.')
    if not sep or len(parts) > 2 or not all(re.fullmatch(SLUG, p) for p in parts) or \
            not GRAMMARS['uuid'].match(uid):
        _bad('the registered form is did:ubos:<tenant[.vae]>:<uuid>')
    out = {'form': 'registered', 'tenant': parts[0], 'uuid': uid}
    if len(parts) == 2:
        out['vae'] = parts[1]
    return out


def key_document(did):
    """The DID document derived from a key-form DID."""
    p = parse_did(did)
    if p['form'] != 'key':
        _bad('only key-form documents are derived from the identifier')
    return {'id': did, 'keys': [{'key_id': did + '#' + did[len('did:ubos:k:'):], 'public_key': p['public_key']}]}
