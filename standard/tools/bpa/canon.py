"""Canonical JSON, NFC and content identifiers (STD-FND-001, -002, -003, -010, -011)."""
import hashlib
import unicodedata

from . import BpaError

MAX_SAFE = 2 ** 53 - 1

DOMAINS = (
    'ubos.chunk.v1', 'ubos.version.v1', 'ubos.entry.v1', 'ubos.commit.v1',
    'ubos.tree.leaf.v1', 'ubos.tree.node.v1', 'ubos.tree.boundary.v1',
    'ubos.seal.v1', 'ubos.pack.v1', 'ubos.field-commit.v1',
)

_ESC = {'"': '\\"', '\\': '\\\\', '\b': '\\b', '\f': '\\f', '\n': '\\n', '\r': '\\r', '\t': '\\t'}


def nfc(text):
    """Normalise to NFC; lone surrogates are not text (STD-FND-003)."""
    if any(0xD800 <= ord(c) <= 0xDFFF for c in text):
        raise BpaError('MODEL.INVALID_VALUE', 'lone surrogate')
    return unicodedata.normalize('NFC', text)


def _string(text):
    out = ['"']
    for c in nfc(text):
        if c in _ESC:
            out.append(_ESC[c])
        elif ord(c) < 0x20:
            out.append('\\u%04x' % ord(c))
        else:
            out.append(c)
    out.append('"')
    return ''.join(out)


def _utf16_key(text):
    return nfc(text).encode('utf-16-be')


def canonical(value):
    """RFC 8785 serialisation of canonical data; every string is normalised to NFC first."""
    if value is None:
        return 'null'
    if value is True:
        return 'true'
    if value is False:
        return 'false'
    if isinstance(value, int):
        if abs(value) > MAX_SAFE:
            raise BpaError('MODEL.INVALID_VALUE', 'integer beyond 2^53-1 must be a decimal string')
        return str(value)
    if isinstance(value, float):
        raise BpaError('MODEL.INVALID_VALUE', 'number with a fraction or exponent')
    if isinstance(value, str):
        return _string(value)
    if isinstance(value, list):
        return '[' + ','.join(canonical(v) for v in value) + ']'
    if isinstance(value, dict):
        keys = sorted(value, key=_utf16_key)
        normed = [nfc(k) for k in keys]
        if len(set(normed)) != len(normed):
            raise BpaError('MODEL.INVALID_VALUE', 'duplicate member after NFC')
        return '{' + ','.join(_string(k) + ':' + canonical(value[k]) for k in keys) + '}'
    raise BpaError('MODEL.INVALID_VALUE', 'not JSON')


def digest(domain, data):
    if domain not in DOMAINS:
        raise BpaError('MODEL.INVALID_VALUE', 'unregistered domain tag')
    return 'sha256:' + hashlib.sha256(domain.encode('ascii') + b'\x00' + data).hexdigest()


def content_id(domain, value):
    """Content identifier of a canonical value under a domain tag (STD-FND-010, -011)."""
    return digest(domain, canonical(value).encode('utf-8'))
