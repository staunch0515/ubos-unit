"""Class declarations, C3 linearization, kinds and narrowing (STD-CLS-001…015).

In adapter inputs a class is referenced by its name, and a trait is a declaration with
`trait: true` and `applies_to` (a list of kinds) (STD-CONF-005)."""
from decimal import Decimal

from . import BpaError
from .values import GRAMMARS, LEVELS, TYPES

KINDS = ('definition', 'ledger', 'system', 'raw_document', 'inherit')
CONCRETE = ('definition', 'ledger', 'raw_document')

# Genesis model (STD-CLS-010): the root and the abstract base classes with their kinds.
GENESIS = {
    'Object': {'name': 'Object', 'kind': None, 'parent': None, 'abstract': True},
    'DataObject': {'name': 'DataObject', 'kind': None, 'parent': 'Object', 'abstract': True},
    'LogicAsset': {'name': 'LogicAsset', 'kind': 'definition', 'parent': 'Object', 'abstract': True},
    'ViewAsset': {'name': 'ViewAsset', 'kind': 'definition', 'parent': 'Object', 'abstract': True},
    'ConfigAsset': {'name': 'ConfigAsset', 'kind': 'definition', 'parent': 'Object', 'abstract': True},
    'SystemObject': {'name': 'SystemObject', 'kind': 'system', 'parent': 'Object', 'abstract': True},
}

CLASS_REQUIRED = ('name', 'namespace', 'kind', 'parent', 'label', 'description', 'schema_version', 'fields')
TRAIT_REQUIRED = ('name', 'namespace', 'applies_to', 'label', 'description', 'schema_version', 'fields')
FIELD_REQUIRED = ('name', 'type', 'label', 'description')
MIN_KEYS = ('min', 'min_length', 'min_items')
MAX_KEYS = ('max', 'max_length', 'max_items')


def _num(x):
    return Decimal(str(x)) if not isinstance(x, str) else Decimal(x)


def check_field(f):
    for k in FIELD_REQUIRED:
        if k not in f:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'field member %s' % k)
    if not GRAMMARS['member-name'].match(f['name']):
        raise BpaError('MODEL.INVALID_NAME', f['name'])
    if f['type'] not in TYPES:
        raise BpaError('MODEL.UNKNOWN_TYPE', f['type'])
    if 'classification' in f and f['classification'] not in LEVELS:
        raise BpaError('MODEL.INVALID_VALUE', 'classification')


def check_decl(d):
    required = TRAIT_REQUIRED if d.get('trait') else CLASS_REQUIRED
    for k in required:
        if k not in d:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'class member %s' % k)
    if not GRAMMARS['class-name'].match(d['name']):
        raise BpaError('MODEL.INVALID_NAME', d['name'])
    if not GRAMMARS['namespace'].match(d['namespace']):
        raise BpaError('MODEL.INVALID_NAME', d['namespace'])
    if not d.get('trait') and d['kind'] not in KINDS:
        raise BpaError('MODEL.INVALID_VALUE', 'kind')
    names = [f.get('name') for f in d['fields']]
    for f in d['fields']:
        check_field(f)
    if len(set(names)) != len(names):
        raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'duplicate field')


def _registry(classes):
    reg = dict(GENESIS)
    for d in classes:
        if d.get('name') in GENESIS:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'genesis class redeclared')
        reg[d.get('name')] = d
    return reg


def _c3_merge(seqs):
    seqs = [list(s) for s in seqs if s]
    out = []
    while seqs:
        for s in seqs:
            head = s[0]
            if not any(head in t[1:] for t in seqs):
                break
        else:
            raise BpaError('MODEL.LINEARIZATION_INCONSISTENT')
        out.append(head)
        seqs = [[x for x in s if x != head] for s in seqs]
        seqs = [s for s in seqs if s]
    return out


def linearize(classes, name, _reg=None, _stack=()):
    """L(C) = C + merge(L(T1), ..., L(Tn), L(P), [T1, ..., Tn, P]) (STD-CLS-011)."""
    reg = _reg or _registry(classes)
    if name in _stack:
        raise BpaError('MODEL.INHERITANCE_CYCLE')
    if name not in reg:
        raise BpaError('MODEL.REFERENCE_INVALID', 'unknown class %s' % name)
    d = reg[name]
    traits = list(d.get('traits', []))
    parents = traits + ([d['parent']] if d.get('parent') else [])
    lins = [linearize(classes, p, reg, _stack + (name,)) for p in parents]
    return [name] + _c3_merge(lins + [parents])


def resolve_kind(reg, name):
    d = reg[name]
    if d.get('trait'):
        return None
    parent = d.get('parent')
    if parent is None:
        return None
    pk = resolve_kind(reg, parent)
    k = d.get('kind')
    if parent == 'DataObject' and name not in GENESIS:
        if k not in CONCRETE:
            raise BpaError('MODEL.KIND_MISMATCH', 'children of DataObject declare a concrete kind')
        return k
    if name in GENESIS:
        return k if k is not None else pk
    if k == 'inherit' or k == pk:
        return pk
    raise BpaError('MODEL.KIND_MISMATCH')


def refine(old, new):
    """Apply the narrowing rules of STD-CLS-013 to a refinement of an inherited field."""
    if 'type' in new and new['type'] != old['type']:
        raise BpaError('MODEL.NARROWING_VIOLATION', 'type')
    if old.get('required') and new.get('required') is False:
        raise BpaError('MODEL.NARROWING_VIOLATION', 'required')
    for k, v in new.items():
        if k not in ('type', 'required', 'constraints', 'default', 'classification', 'label', 'description') \
                and v != old.get(k):
            raise BpaError('MODEL.NARROWING_VIOLATION', k)   # computed and every unlisted aspect
    oc, nc = old.get('constraints', {}), new.get('constraints', {})
    for k in nc:
        if k not in MIN_KEYS + MAX_KEYS + ('pattern', 'allowed_values', 'scale') and nc[k] != oc.get(k):
            raise BpaError('MODEL.NARROWING_VIOLATION', k)
    for k in MIN_KEYS:
        if k in oc and k in nc and _num(nc[k]) < _num(oc[k]):
            raise BpaError('MODEL.NARROWING_VIOLATION', k)
    for k in MAX_KEYS:
        if k in oc and k in nc and _num(nc[k]) > _num(oc[k]):
            raise BpaError('MODEL.NARROWING_VIOLATION', k)
    if 'pattern' in oc and 'pattern' in nc and nc['pattern'] != oc['pattern']:
        raise BpaError('MODEL.NARROWING_VIOLATION', 'pattern')
    if 'allowed_values' in oc and 'allowed_values' in nc and not set(nc['allowed_values']) <= set(oc['allowed_values']):
        raise BpaError('MODEL.NARROWING_VIOLATION', 'allowed_values')
    if 'scale' in oc and 'scale' in nc and nc['scale'] != oc['scale']:
        raise BpaError('MODEL.NARROWING_VIOLATION', 'scale')
    if 'classification' in old and 'classification' in new and \
            LEVELS.index(new['classification']) < LEVELS.index(old['classification']):
        raise BpaError('MODEL.NARROWING_VIOLATION', 'classification')
    merged = dict(old)
    merged.update({k: v for k, v in new.items() if k != 'constraints'})
    if oc or nc:
        merged['constraints'] = dict(oc, **nc)
    return merged


def effective_class(classes, name):
    """Effective definition of a class: linearization, kind, write policy and fields
    (STD-CLS-001, -002, -011…-014)."""
    for d in classes:
        check_decl(d)
    reg = _registry(classes)
    lin = linearize(classes, name, reg)
    kind = resolve_kind(reg, name)
    for c in lin:
        d = reg[c]
        if d.get('trait') and kind is not None and kind not in d['applies_to']:
            raise BpaError('MODEL.KIND_MISMATCH', 'trait does not apply to the kind')
    fields, policy = {}, None
    for c in reversed(lin):
        d = reg[c]
        wp = d.get('write_policy')
        if wp is not None:
            if policy == 'change_set' and wp == 'direct':
                raise BpaError('MODEL.NARROWING_VIOLATION', 'write_policy')
            policy = wp
        for f in d.get('fields', []):
            fields[f['name']] = refine(fields[f['name']], f) if f['name'] in fields else dict(f)
    if policy is None:
        policy = 'change_set' if 'ConfigAsset' in lin else 'direct'
    return {'linearization': lin, 'kind': kind, 'write_policy': policy, 'fields': fields}
