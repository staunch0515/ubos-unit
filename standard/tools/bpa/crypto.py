"""Registered algorithms (STD-CONF-011) and signatures (STD-PROOF-010, -011), in plain
Python so that the oracle needs no third-party package: Ed25519 (RFC 8032),
ChaCha20-Poly1305 (RFC 8439) with the XChaCha20 extension (HChaCha20), and AES-256-GCM
(NIST SP 800-38D). They are reference code for vectors, not for production use.

Keys and nonces in adapter inputs are lowercase hexadecimal; public keys and signatures are
base64url without padding (DAT-Signature)."""
import base64
import hashlib
import hmac
import re
import struct

from . import BpaError

ALGORITHMS = {'hash': ('sha256',), 'signature': ('ed25519',), 'aead': ('aes-256-gcm', 'xchacha20-poly1305')}
ROLES = ('author', 'node', 'approver', 'signer', 'notary', 'tagger')
HEX = re.compile(r'\A(?:[0-9a-f]{2})*\Z')
B64URL = re.compile(r'\A[A-Za-z0-9_-]*\Z')


def unhex(v, n=None, what='value'):
    if not isinstance(v, str) or not HEX.match(v) or (n is not None and len(v) != 2 * n):
        raise BpaError('MODEL.INVALID_VALUE', '%s is %slowercase hexadecimal' % (what, '%d bytes of ' % n if n else ''))
    return bytes.fromhex(v)


def b64u(b):
    return base64.urlsafe_b64encode(b).rstrip(b'=').decode('ascii')


def unb64u(v, n, what):
    if not isinstance(v, str) or not B64URL.match(v) or len(v) != (4 * n + 2) // 3:
        raise BpaError('MODEL.INVALID_VALUE', '%s is %d bytes in base64url without padding' % (what, n))
    return base64.urlsafe_b64decode(v + '=' * (-len(v) % 4))


# ---- Ed25519 (RFC 8032, section 5.1) ----

P = 2 ** 255 - 19
L = 2 ** 252 + 27742317777372353535851937790883648493
D = -121665 * pow(121666, P - 2, P) % P
SQRT_M1 = pow(2, (P - 1) // 4, P)


def _add(a, b):
    x1, y1, z1, t1 = a
    x2, y2, z2, t2 = b
    A = (y1 - x1) * (y2 - x2) % P
    B = (y1 + x1) * (y2 + x2) % P
    C = 2 * t1 * t2 * D % P
    Dd = 2 * z1 * z2 % P
    E, F, G, H = B - A, Dd - C, Dd + C, B + A
    return (E * F % P, G * H % P, F * G % P, E * H % P)


def _mul(s, pt):
    q = (0, 1, 1, 0)
    while s > 0:
        if s & 1:
            q = _add(q, pt)
        pt = _add(pt, pt)
        s >>= 1
    return q


def _eq(a, b):
    return (a[0] * b[2] - b[0] * a[2]) % P == 0 and (a[1] * b[2] - b[1] * a[2]) % P == 0


def _recover_x(y, sign):
    if y >= P:
        return None
    x2 = (y * y - 1) * pow(D * y * y + 1, P - 2, P)
    if x2 == 0:
        return None if sign else 0
    x = pow(x2, (P + 3) // 8, P)
    if (x * x - x2) % P != 0:
        x = x * SQRT_M1 % P
    if (x * x - x2) % P != 0:
        return None
    if (x & 1) != sign:
        x = P - x
    return x


GY = 4 * pow(5, P - 2, P) % P
GX = _recover_x(GY, 0)
G = (GX, GY, 1, GX * GY % P)


def _compress(pt):
    zinv = pow(pt[2], P - 2, P)
    x, y = pt[0] * zinv % P, pt[1] * zinv % P
    return int.to_bytes(y | ((x & 1) << 255), 32, 'little')


def _decompress(b):
    y = int.from_bytes(b, 'little')
    sign = y >> 255
    y &= (1 << 255) - 1
    x = _recover_x(y, sign)
    return None if x is None else (x, y, 1, x * y % P)


def _hint(*parts):
    return int.from_bytes(hashlib.sha512(b''.join(parts)).digest(), 'little')


def _expand(seed):
    h = hashlib.sha512(seed).digest()
    a = int.from_bytes(h[:32], 'little')
    a &= (1 << 254) - 8
    a |= 1 << 254
    return a, h[32:]


def ed25519_public(seed):
    return _compress(_mul(_expand(seed)[0], G))


def ed25519_sign(seed, msg):
    a, prefix = _expand(seed)
    A = _compress(_mul(a, G))
    r = _hint(prefix, msg) % L
    R = _compress(_mul(r, G))
    k = _hint(R, A, msg) % L
    return R + int.to_bytes((r + k * a) % L, 32, 'little')


def ed25519_verify(public, msg, sig):
    A = _decompress(public)
    R = _decompress(sig[:32])
    s = int.from_bytes(sig[32:], 'little')
    if A is None or R is None or s >= L:
        return False
    k = _hint(sig[:32], public, msg) % L
    return _eq(_mul(s, G), _add(R, _mul(k, A)))


def public_key(seed):
    return b64u(ed25519_public(unhex(seed, 32, 'seed')))


def sign(seed, content_id, signer, key_id, role='signer'):
    if role not in ROLES:
        raise BpaError('MODEL.INVALID_VALUE', 'unknown signature role')
    if not isinstance(content_id, str):
        raise BpaError('MODEL.INVALID_VALUE', 'the signed content is a content ID')
    return {'role': role, 'signer': signer, 'key_id': key_id, 'alg': 'ed25519',
            'sig': b64u(ed25519_sign(unhex(seed, 32, 'seed'), content_id.encode('utf-8')))}


def _check_signature(s):
    if not isinstance(s, dict) or any(k not in s for k in ('role', 'signer', 'key_id', 'alg', 'sig')):
        raise BpaError('MODEL.INVALID_VALUE', 'a signature is DAT-Signature')
    if s['role'] not in ROLES:
        raise BpaError('MODEL.INVALID_VALUE', 'unknown signature role')
    if s['alg'] not in ALGORITHMS['signature']:
        raise BpaError('MODEL.INVALID_VALUE', 'unregistered signature algorithm')
    return unb64u(s['sig'], 64, 'sig')


def verify(signature, content_id, did, at=None):
    """STD-PROOF-011: the key named by key_id verifies the signature over the content ID and
    was valid for the signer at `at` according to the signer's key history."""
    sig = _check_signature(signature)
    if not isinstance(did, dict) or not isinstance(did.get('keys'), list):
        raise BpaError('MODEL.INVALID_VALUE', 'a DID document is {id, keys}')
    if signature['signer'] != did.get('id'):
        raise BpaError('PROOF.SIGNATURE_INVALID', 'the signer is not the subject of the DID document')
    keys = [k for k in did['keys'] if k.get('key_id') == signature['key_id']]
    if not keys:
        raise BpaError('PROOF.SIGNATURE_INVALID', 'unknown key')
    k = keys[0]
    if at is not None:
        f, t = k.get('valid_from'), k.get('valid_to')
        if (f is not None and at < f) or (t is not None and at >= t):
            raise BpaError('PROOF.SIGNATURE_INVALID', 'the key was not valid at the signed time')
    if not ed25519_verify(unb64u(k['public_key'], 32, 'public_key'), content_id.encode('utf-8'), sig):
        raise BpaError('PROOF.SIGNATURE_INVALID', 'the signature does not verify')
    return True


# ---- ChaCha20-Poly1305 (RFC 8439) and XChaCha20 ----

def _rotl(v, n):
    return ((v << n) & 0xffffffff) | (v >> (32 - n))


def _rounds(s):
    def qr(a, b, c, d):
        s[a] = (s[a] + s[b]) & 0xffffffff
        s[d] = _rotl(s[d] ^ s[a], 16)
        s[c] = (s[c] + s[d]) & 0xffffffff
        s[b] = _rotl(s[b] ^ s[c], 12)
        s[a] = (s[a] + s[b]) & 0xffffffff
        s[d] = _rotl(s[d] ^ s[a], 8)
        s[c] = (s[c] + s[d]) & 0xffffffff
        s[b] = _rotl(s[b] ^ s[c], 7)
    for _ in range(10):
        qr(0, 4, 8, 12)
        qr(1, 5, 9, 13)
        qr(2, 6, 10, 14)
        qr(3, 7, 11, 15)
        qr(0, 5, 10, 15)
        qr(1, 6, 11, 12)
        qr(2, 7, 8, 13)
        qr(3, 4, 9, 14)


SIGMA = [0x61707865, 0x3320646e, 0x79622d32, 0x6b206574]


def hchacha20(key, nonce16):
    s = SIGMA + list(struct.unpack('<8I', key)) + list(struct.unpack('<4I', nonce16))
    _rounds(s)
    return struct.pack('<8I', *(s[0:4] + s[12:16]))


def _chacha_block(key, counter, nonce12):
    init = SIGMA + list(struct.unpack('<8I', key)) + [counter] + list(struct.unpack('<3I', nonce12))
    s = list(init)
    _rounds(s)
    return struct.pack('<16I', *((a + b) & 0xffffffff for a, b in zip(s, init)))


def _chacha_xor(key, counter, nonce12, data):
    out = bytearray()
    for i in range(0, len(data), 64):
        block = _chacha_block(key, counter + i // 64, nonce12)
        out += bytes(x ^ y for x, y in zip(data[i:i + 64], block))
    return bytes(out)


def _poly1305(key, msg):
    r = int.from_bytes(key[:16], 'little') & 0x0ffffffc0ffffffc0ffffffc0fffffff
    s = int.from_bytes(key[16:], 'little')
    p, acc = (1 << 130) - 5, 0
    for i in range(0, len(msg), 16):
        n = int.from_bytes(msg[i:i + 16] + b'\x01', 'little')
        acc = (acc + n) * r % p
    return int.to_bytes((acc + s) & ((1 << 128) - 1), 16, 'little')


def _pad16(b):
    return b + b'\x00' * (-len(b) % 16)


def _chacha_tag(key, nonce12, aad, ct):
    otk = _chacha_block(key, 0, nonce12)[:32]
    return _poly1305(otk, _pad16(aad) + _pad16(ct) + struct.pack('<QQ', len(aad), len(ct)))


def chacha20poly1305_encrypt(key, nonce12, pt, aad):
    ct = _chacha_xor(key, 1, nonce12, pt)
    return ct + _chacha_tag(key, nonce12, aad, ct)


def chacha20poly1305_decrypt(key, nonce12, data, aad):
    if len(data) < 16:
        return None
    ct, tag = data[:-16], data[-16:]
    if not hmac.compare_digest(tag, _chacha_tag(key, nonce12, aad, ct)):
        return None
    return _chacha_xor(key, 1, nonce12, ct)


# ---- AES-256-GCM (FIPS 197, SP 800-38D) ----

def _sbox():
    sbox, p, q = [0] * 256, 1, 1
    while True:
        p = p ^ ((p << 1) & 0xff) ^ (0x1b if p & 0x80 else 0)
        q ^= q << 1
        q ^= q << 2
        q ^= q << 4
        q &= 0xff
        if q & 0x80:
            q ^= 0x09
        x = q ^ (((q << 1) | (q >> 7)) & 0xff) ^ (((q << 2) | (q >> 6)) & 0xff) ^ \
            (((q << 3) | (q >> 5)) & 0xff) ^ (((q << 4) | (q >> 4)) & 0xff)
        sbox[p] = x ^ 0x63
        if p == 1:
            break
    sbox[0] = 0x63
    return sbox


SBOX = _sbox()


def _xt(a):
    return ((a << 1) ^ 0x1b) & 0xff if a & 0x80 else a << 1


def _expand_key(key):
    w = [list(key[i:i + 4]) for i in range(0, 32, 4)]
    rcon = 1
    for i in range(8, 60):
        t = list(w[i - 1])
        if i % 8 == 0:
            t = [SBOX[b] for b in t[1:] + t[:1]]
            t[0] ^= rcon
            rcon = _xt(rcon)
        elif i % 8 == 4:
            t = [SBOX[b] for b in t]
        w.append([a ^ b for a, b in zip(w[i - 8], t)])
    return [sum(w[4 * r:4 * r + 4], []) for r in range(15)]


def _aes_block(rk, block):
    s = [b ^ k for b, k in zip(block, rk[0])]
    for r in range(1, 15):
        s = [SBOX[b] for b in s]
        s = [s[(i + 4 * (i % 4)) % 16] for i in range(16)]
        if r < 14:
            m = []
            for c in range(4):
                a = s[4 * c:4 * c + 4]
                x = a[0] ^ a[1] ^ a[2] ^ a[3]
                m += [a[i] ^ x ^ _xt(a[i] ^ a[(i + 1) % 4]) for i in range(4)]
            s = m
        s = [b ^ k for b, k in zip(s, rk[r])]
    return bytes(s)


def _gmul(x, y):
    R, z, v = 0xe1 << 120, 0, y
    for i in range(127, -1, -1):
        if (x >> i) & 1:
            z ^= v
        v = (v >> 1) ^ R if v & 1 else v >> 1
    return z


def _ghash(h, aad, ct):
    y = 0
    for chunk in (_pad16(aad), _pad16(ct), struct.pack('>QQ', len(aad) * 8, len(ct) * 8)):
        for i in range(0, len(chunk), 16):
            y = _gmul(y ^ int.from_bytes(chunk[i:i + 16], 'big'), h)
    return y


def _gctr(rk, icb, data):
    out, ctr = bytearray(), int.from_bytes(icb, 'big')
    for i in range(0, len(data), 16):
        ks = _aes_block(rk, int.to_bytes(ctr, 16, 'big'))
        out += bytes(a ^ b for a, b in zip(data[i:i + 16], ks))
        ctr = (ctr & ~0xffffffff) | ((ctr + 1) & 0xffffffff)
    return bytes(out)


def _gcm(key, nonce12, aad, ct):
    rk = _expand_key(key)
    h = int.from_bytes(_aes_block(rk, b'\x00' * 16), 'big')
    j0 = nonce12 + b'\x00\x00\x00\x01'
    tag = int.to_bytes(_ghash(h, aad, ct), 16, 'big')
    return rk, j0, _gctr(rk, j0, tag)


def aesgcm_encrypt(key, nonce12, pt, aad):
    rk = _expand_key(key)
    j0 = nonce12 + b'\x00\x00\x00\x01'
    ct = _gctr(rk, (int.from_bytes(j0, 'big') + 1).to_bytes(16, 'big'), pt)
    return ct + _gcm(key, nonce12, aad, ct)[2]


def aesgcm_decrypt(key, nonce12, data, aad):
    if len(data) < 16:
        return None
    ct, tag = data[:-16], data[-16:]
    rk, j0, expect = _gcm(key, nonce12, aad, ct)
    if not hmac.compare_digest(tag, expect):
        return None
    return _gctr(rk, (int.from_bytes(j0, 'big') + 1).to_bytes(16, 'big'), ct)


def _aead(alg, key, nonce):
    if alg == 'aes-256-gcm':
        return aesgcm_encrypt, aesgcm_decrypt, unhex(key, 32, 'key'), unhex(nonce, 12, 'nonce')
    if alg == 'xchacha20-poly1305':
        n = unhex(nonce, 24, 'nonce')
        sub = hchacha20(unhex(key, 32, 'key'), n[:16])
        return chacha20poly1305_encrypt, chacha20poly1305_decrypt, sub, b'\x00' * 4 + n[16:]
    raise BpaError('MODEL.INVALID_VALUE', 'unregistered AEAD algorithm')


def aead_encrypt(alg, key, nonce, plaintext, aad=''):
    enc, _, k, n = _aead(alg, key, nonce)
    return enc(k, n, unhex(plaintext, what='plaintext'), unhex(aad, what='aad')).hex()


def aead_decrypt(alg, key, nonce, ciphertext, aad=''):
    _, dec, k, n = _aead(alg, key, nonce)
    out = dec(k, n, unhex(ciphertext, what='ciphertext'), unhex(aad, what='aad'))
    if out is None:
        raise BpaError('PROOF.VERIFICATION_FAILED', 'the authentication tag does not verify')
    return out.hex()
