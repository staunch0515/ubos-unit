"""Protection of BPA 1.0-rc8 (PH-2 step 1e, DEC-093): encrypted fields under subject keys and
their erasure (STD-OBJ-010, -011), authorization relations (STD-LGS-050), relation-based access
rules (STD-LGS-051) and field masking (STD-LGS-052). Mixed into the reference store;
`verify_disclosure` is also a pure operation."""
import copy
import hashlib
import hmac
import re

from . import BpaError
from . import crypto
from .canon import canonical
from .values import LEVELS, classification_covers

PROTECTED = ('personal', 'sensitive_personal')
ALG = 'aes-256-gcm'
MAX_DEPTH = 4
MODES = ('hide', 'partial', 'hash')
ACCESS = re.compile(r'\A\s*principal\s+in\s+this((?:\.[a-z][a-z0-9_]*)*)\s*\Z')
DEFAULT_CLEARANCES = ['internal']


def commitment(salt, value):
    """STD-OBJ-010: H(ubos.field-commit.v1 || salt || canonical(plaintext))."""
    h = hashlib.sha256(b'ubos.field-commit.v1' + salt + canonical(value).encode('utf-8'))
    return 'sha256:' + h.hexdigest()


def verify_disclosure(i):
    """A disclosed value with its salt matches a field commitment."""
    salt = crypto.unb64u(i.get('salt'), 16, 'salt')
    return commitment(salt, i.get('value')) == i.get('commitment')


def is_marker(v):
    return isinstance(v, dict) and v.get('erased') is True and set(v) == {'erased', 'commitment'}


def parse_access(text):
    """STD-LGS-051: `principal in this(.relation)*` with at most four relations."""
    m = ACCESS.match(text) if isinstance(text, str) else None
    if m is None:
        raise BpaError('RULE.PARSE_ERROR', 'an access rule is principal in this(.relation)*')
    path = [p for p in m.group(1).split('.') if p]
    if len(path) > MAX_DEPTH:
        raise BpaError('RULE.PARSE_ERROR', 'a relation path has at most %d relations' % MAX_DEPTH)
    return path


def check_rules(value):
    for r in value or []:
        if not isinstance(r, dict) or not isinstance(r.get('action'), str) or 'allow_if' not in r:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'an access rule is {action, allow_if}')
        parse_access(r['allow_if'])


def partial(v):
    keep = v[-4:] if len(v) > 4 else ''
    return '*' * (len(v) - len(keep)) + keep


class Protection:

    def _prot_init(self):
        self.subject_keys = {}     # subject id -> {versions: {n: key}, current, destroyed, unavailable}
        self.erased_store = {}     # commitment -> the stored entry of an erased field
        self.lookup = None         # the tenant lookup key (STD-OBJ-012), hexadecimal

    # ---- subject keys (STD-OBJ-010, -011) ----

    def _subject_decl(self, cls):
        """(subject_key, protected field names) of a class, or (None, [])."""
        try:
            eff = self._cls(cls)
        except BpaError:
            return None, []
        sk = None
        for n in eff['linearization']:
            d = self.decls.get(n) or {}
            if d.get('subject_key'):
                sk = d['subject_key']
                break
        if sk is None:
            return None, []
        return sk, sorted(f for f, d in eff['fields'].items() if d.get('classification') in PROTECTED)

    def _new_key(self, subject, n):
        h = hashlib.sha256(b'ubos.subject-key.v1\x00' + self.TENANT.encode() + b'\x00' + subject.encode() +
                           b'\x00' + str(n).encode())
        return h.hexdigest()

    def _key(self, subject, version=None):
        rec = self.subject_keys.get(subject)
        if rec is None:
            rec = self.subject_keys[subject] = {'versions': {1: self._new_key(subject, 1)}, 'current': 1,
                                                'destroyed': False, 'unavailable': False}
        if rec['destroyed']:
            raise BpaError('PROOF.VALUE_ERASED', {'subject_id': subject})
        if rec['unavailable']:
            raise BpaError('IAM.KEY_UNAVAILABLE', {'subject_id': subject})
        n = rec['current'] if version is None else version
        return n, rec['versions'][n]

    def subject_key(self, i):
        """Create or rotate the key of a subject; keys stay outside the business data."""
        subject = i.get('subject_id')
        if not isinstance(subject, str) or not subject:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'subject_id')
        n, _ = self._key(subject)
        if i.get('rotate'):
            rec = self.subject_keys[subject]
            n = rec['current'] + 1
            rec['versions'][n] = self._new_key(subject, n)
            rec['current'] = n
        return {'subject_id': subject, 'key_version': n}

    def key_availability(self, i):
        """Simulate a key the key service cannot provide for now (DSN-DVM-834)."""
        rec = self.subject_keys.get(i.get('subject_id'))
        if rec is None:
            raise BpaError('IAM.NOT_FOUND', 'unknown subject key')
        rec['unavailable'] = not i.get('available', True)
        return {'subject_id': i['subject_id'], 'available': not rec['unavailable']}

    def _subject_of(self, oid, sk, body):
        if sk == 'self':
            return oid
        v = body.get(sk)
        if not isinstance(v, str) or not v:
            raise BpaError('MODEL.CONSTRAINT_VIOLATED', 'the subject key field %s has no value' % sk)
        st = getattr(self, '_cur_st', None)
        if v.startswith('ubos://') and st is not None:
            return self._resolve(st, v) or v            # a referenced subject is its object ID
        return v

    def _protect(self, oid, cls, body):
        """The stored form of a body: protected fields as DAT-EncryptedField items. The
        reference model derives salt and nonce from the key and the value; implementations
        draw them at random."""
        sk, fields = self._subject_decl(cls)
        if not fields or not isinstance(body, dict) or not any(body.get(f) is not None for f in fields):
            return body
        stored = dict(body)
        for f in fields:
            v = body.get(f)
            if v is None:
                continue
            if is_marker(v):
                stored[f] = self.erased_store[v['commitment']]
                continue
            subject = self._subject_of(oid, sk, body)
            n, key = self._key(subject)
            mac = lambda tag: hmac.new(bytes.fromhex(key), tag + b'\x00' + oid.encode() + b'\x00' + f.encode() +  # noqa: E731
                                       b'\x00' + canonical(v).encode('utf-8'), hashlib.sha256).digest()
            salt, nonce = mac(b'salt')[:16], mac(b'nonce')[:12]
            pt = canonical({'salt': crypto.b64u(salt), 'value': v}).encode('utf-8')
            ct = crypto.aead_encrypt(ALG, key, nonce.hex(), pt.hex())
            stored[f] = {'enc': ALG, 'key': {'subject_id': subject, 'key_version': n}, 'nonce': crypto.b64u(nonce),
                         'ct': crypto.b64u(bytes.fromhex(ct)), 'commitment': commitment(salt, v)}
        return stored

    def _open(self, entry):
        """{salt, value} of a stored field."""
        n, key = self._key(entry['key']['subject_id'], entry['key']['key_version'])
        nonce = crypto.unb64u(entry['nonce'], 12, 'nonce')
        import base64
        ct = base64.urlsafe_b64decode(entry['ct'] + '=' * (-len(entry['ct']) % 4))
        pt = crypto.aead_decrypt(ALG, key, nonce.hex(), ct.hex())
        import json
        return json.loads(bytes.fromhex(pt).decode('utf-8'))

    def _unprotect(self, stored):
        """The body of a stored form: decrypted fields, or erased markers."""
        if not isinstance(stored, dict) or not any(isinstance(e, dict) and e.get('enc') and 'ct' in e
                                                   for e in stored.values()):
            return stored
        out = dict(stored)
        for f, e in stored.items():
            if isinstance(e, dict) and e.get('enc') and 'ct' in e:
                rec = self.subject_keys.get(e['key']['subject_id'])
                if rec is None:
                    out[f] = e                  # no key on this node: reads fail (DSN-DVM-834)
                elif rec['destroyed']:
                    self.erased_store[e['commitment']] = e
                    out[f] = {'erased': True, 'commitment': e['commitment']}
                else:
                    out[f] = self._open(e)['value']
        return out

    def _check_keys(self, v):
        """Reads of a version whose subject key is unavailable fail (DSN-DVM-834)."""
        for e in (v.get('stored') or {}).values():
            if isinstance(e, dict) and e.get('enc'):
                rec = self.subject_keys.get(e['key']['subject_id'])
                if rec is None or (rec['unavailable'] and not rec['destroyed']):
                    raise BpaError('IAM.KEY_UNAVAILABLE', {'subject_id': e['key']['subject_id']})

    def erase_subject(self, i):
        """STD-OBJ-011: destroy a subject key; the subject's fields read as erased markers in
        every version, and hashes and proofs stay valid."""
        subject = i.get('subject_id')
        rec = self.subject_keys.get(subject)
        if rec is None:
            raise BpaError('IAM.NOT_FOUND', 'unknown subject key')
        if not isinstance(i.get('reason'), str) or not i['reason']:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'an erasure states its reason')
        rec['destroyed'] = True
        rec['versions'] = {}
        fields = versions = 0
        for vh in sorted(self.versions):
            v = self.versions[vh]
            hit = False
            for f, e in sorted((v.get('stored') or {}).items()):
                if isinstance(e, dict) and e.get('enc') and e['key']['subject_id'] == subject \
                        and not is_marker(v['body'].get(f)):
                    self.erased_store[e['commitment']] = e
                    v['body'] = dict(v['body'], **{f: {'erased': True, 'commitment': e['commitment']}})
                    fields += 1
                    hit = True
            versions += hit
        return {'subject_id': subject, 'erased': True, 'erased_versions': versions, 'erased_fields': fields}

    def _stored_version(self, branch, oid, sel=None):
        from .store import tl_at
        st = self._state(branch, sel or {})[0]
        o = st['objects'].get(oid)
        if o is None:
            raise BpaError('IAM.NOT_FOUND', 'unknown object')
        seg = tl_at(o['timeline'], (sel or {}).get('asof') or self._default_valid(o['class']))
        if seg is None:
            raise BpaError('QRY.NOT_FOUND_AT_TIME')
        return self.versions[seg[2]]

    def stored_form(self, i):
        """The body as stored and hashed (STD-OBJ-010)."""
        v = self._stored_version(i.get('branch', 'main'), i.get('object_id'), i)
        return {'body': copy.deepcopy(v.get('stored') or v['body'])}

    def disclose(self, i):
        """A protected field's value with the salt of its commitment, for evidence."""
        v = self._stored_version(i.get('branch', 'main'), i.get('object_id'), i)
        f = i.get('field')
        e = (v.get('stored') or {}).get(f)
        if not isinstance(e, dict) or not e.get('enc'):
            raise BpaError('MODEL.INVALID_VALUE', 'the field is not encrypted')
        if is_marker(v['body'].get(f)):
            raise BpaError('PROOF.VALUE_ERASED', {'field': f})
        opened = self._open(e)
        return {'field': f, 'value': opened['value'], 'salt': opened['salt'], 'commitment': e['commitment']}

    # ---- relations and access rules (STD-LGS-050, -051) ----

    def _relation_fields(self, cls):
        return {n: f['authz_relation'] for n, f in self._cls(cls)['fields'].items() if f.get('authz_relation')}

    def _principal_field(self, cls):
        for n in self._cls(cls)['linearization']:
            d = self.decls.get(n) or {}
            if d.get('principal_field'):
                return d['principal_field']
        return None

    def _body_at(self, st, oid, asof=None):
        from .store import tl_at
        o = st['objects'].get(oid)
        if o is None:
            return None, None
        seg = tl_at(o['timeline'], asof or self._default_valid(o['class']))
        if seg is None or self.versions[seg[2]]['tombstone']:
            return o, None
        return o, self.versions[seg[2]]['body']

    def _targets(self, st, oid, relation, asof=None):
        o, body = self._body_at(st, oid, asof)
        if body is None:
            return []
        out = []
        for f, rel in sorted(self._relation_fields(o['class']).items()):
            if rel != relation:
                continue
            vals = body.get(f)
            for uri in (vals if isinstance(vals, list) else [vals]):
                if isinstance(uri, str):
                    tid = self._resolve(st, uri) if uri.startswith('ubos://') else (uri if uri in st['objects'] else None)
                    if tid and tid not in out:
                        out.append(tid)
        return out

    def relations(self, i):
        """STD-LGS-050: the authorization relations of an object at a snapshot."""
        branch = i.get('branch', 'main')
        st = self._state(branch, i)[0]
        o, body = self._body_at(st, i.get('object_id'), i.get('asof'))
        if body is None:
            raise BpaError('IAM.NOT_FOUND', 'unknown object')
        rels = {}
        for rel in sorted(set(self._relation_fields(o['class']).values())):
            rels[rel] = self._targets(st, i['object_id'], rel, i.get('asof'))
        out = {'object_id': i['object_id'], 'relations': rels}
        pf = self._principal_field(o['class'])
        if pf and body.get(pf):
            out['principal'] = body[pf]
        return out

    def _holds(self, st, oid, principal, path, asof=None):
        objs = [oid]
        for rel in path:
            nxt = []
            for x in objs:
                for t in self._targets(st, x, rel, asof):
                    if t not in nxt:
                        nxt.append(t)
            objs = nxt
        for x in objs:
            o, body = self._body_at(st, x, asof)
            pf = self._principal_field(o['class']) if body is not None else None
            if pf and body.get(pf) == principal:
                return True
        return False

    def _has_rules(self, st):
        for _, (_, body) in self._sheet_objects(st).items():
            for b in (body.get('ast') or {}).get('blocks') or []:
                if any(d.get('property') == 'access' for d in b.get('declarations') or []):
                    return True
        return False

    def _access(self, st, branch, oid, principal, action, ctx=None, asof=None):
        """(allowed, rule): the access rules of the object for the action; without rules the
        action is not restricted by relations."""
        if not self._has_rules(st):
            return True, None
        props, _ = self._effective(st, branch, oid, ctx or {}, asof or self._default_valid(st['objects'][oid]['class']))
        rules = [r for r in props.get('access') or [] if r['action'] in (action, '*')]
        if not rules:
            return True, None
        for r in rules:
            if self._holds(st, oid, principal, parse_access(r['allow_if']), asof):
                return True, r['allow_if']
        return False, None

    def check_access(self, i):
        """STD-LGS-051: whether a principal may take an action on an object, evaluated at the
        snapshot of the request."""
        branch = i.get('branch', 'main')
        st = self._state(branch, i)[0]
        oid = i.get('object_id')
        if oid not in st['objects']:
            raise BpaError('IAM.NOT_FOUND', 'unknown object')
        if not isinstance(i.get('principal'), str) or not isinstance(i.get('action'), str):
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'principal and action')
        ok, rule = self._access(st, branch, oid, i['principal'], i['action'], i.get('ctx'), i.get('asof'))
        return {'allowed': ok, 'rule': rule}

    def _viewer(self, v):
        if v is None:
            return None
        if not isinstance(v, dict) or not isinstance(v.get('principal'), str):
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'as is {principal, clearances, ctx}')
        cl = v.get('clearances', DEFAULT_CLEARANCES)
        if not isinstance(cl, list) or any(c not in LEVELS for c in cl):
            raise BpaError('MODEL.INVALID_VALUE', 'clearances are classification levels')
        return {'principal': v['principal'], 'clearances': cl, 'ctx': dict(v.get('ctx') or {}, principal=v['principal'])}

    # ---- masking (STD-LGS-052) ----

    def _mask_modes(self, st, branch, oid, viewer, asof=None):
        """{field: mode} for a viewer: the declared masks, and `hide` for fields whose
        classification the viewer's clearances do not cover."""
        o = st['objects'][oid]
        props, _ = self._effective(st, branch, oid, viewer['ctx'], asof or self._default_valid(o['class']))
        modes = {k[5:]: v for k, v in props.items() if k.startswith('mask.')}
        for f, d in self._cls(o['class'])['fields'].items():
            if f not in modes and not classification_covers(viewer['clearances'], d.get('classification', 'internal')):
                modes[f] = 'hide'
        return modes

    def _mask(self, body, modes):
        out = dict(body)
        for f, m in sorted(modes.items()):
            if f not in out or out[f] is None:
                continue
            v = out[f]
            if m == 'partial' and isinstance(v, str):
                out[f] = partial(v)
            elif m == 'hash' and self.lookup is not None and not is_marker(v):
                out[f] = 'hmac-sha256:' + hmac.new(bytes.fromhex(self.lookup), canonical(v).encode('utf-8'),
                                                   hashlib.sha256).hexdigest()
            else:
                del out[f]
        return out

    def lookup_key(self, i):
        """Set the tenant lookup key (STD-OBJ-012) used by `hash` masks."""
        crypto.unhex(i.get('key'), 32, 'key')
        self.lookup = i['key']
        return True

    def _view_read(self, st, branch, oid, out, sel):
        """Access and masking of a read made as a viewer (`as`)."""
        viewer = self._viewer(sel.get('as'))
        if viewer is None:
            return out
        ok, _ = self._access(st, branch, oid, viewer['principal'], 'read', viewer['ctx'], sel.get('asof'))
        if not ok:
            raise BpaError('IAM.NOT_FOUND', 'not visible')
        out['body'] = self._mask(out['body'], self._mask_modes(st, branch, oid, viewer, sel.get('asof')))
        return out

    def _view_filter(self, st, branch, oids, viewer, names, asof=None):
        """Row filtering inside a query and the refusal of masked fields in it."""
        keep = []
        for oid in oids:
            ok, _ = self._access(st, branch, oid, viewer['principal'], 'read', viewer['ctx'], asof)
            if not ok:
                continue
            hidden = set(self._mask_modes(st, branch, oid, viewer, asof)) & set(names)
            if hidden:
                raise BpaError('QRY.INVALID_QUERY', {'masked': sorted(hidden)})
            keep.append(oid)
        return keep

    @staticmethod
    def _this_members(node, out):
        """Fields of `this` an expression reads."""
        if isinstance(node, tuple):
            if node[0] == 'member' and isinstance(node[2], tuple) and node[2][0] == 'path' and node[2][2] == 'this':
                out.add(node[3])
            for x in node[2:]:
                Protection._this_members(x, out)
        elif isinstance(node, list):
            for x in node:
                Protection._this_members(x, out)
        return out

    def _view_effect(self, e, viewer):
        """An outbox effect as a subscriber sees it: its payload masked like its object."""
        if not e.get('object_id') or not isinstance(e.get('payload'), dict) or e.get('branch') not in self.branches:
            return e
        st = self._state(e['branch'])[0]
        if e['object_id'] not in st['objects']:
            return e
        return dict(e, payload=self._mask(e['payload'], self._mask_modes(st, e['branch'], e['object_id'], viewer)))
