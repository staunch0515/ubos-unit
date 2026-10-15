"""Class declarations, C3 linearization, kinds and narrowing (STD-CLS-001…015).

In adapter inputs a class is referenced by its name, and a trait is a declaration with
`trait: true` and `applies_to` (a list of kinds) (STD-CONF-005)."""
import re
from decimal import Decimal

from . import BpaError
from .values import GRAMMARS, LEVELS, TYPES

KINDS = ('definition', 'ledger', 'system', 'raw_document', 'inherit')
CONCRETE = ('definition', 'ledger', 'raw_document')

# Genesis model (STD-CLS-010): the root and the abstract base classes with their kinds.
def _f(name, type_, required=False, **extra):
    f = {'name': name, 'type': type_, 'label': name.replace('_', ' '), 'description': name}
    if required:
        f['required'] = True
    f.update(extra)
    return f


GENESIS = {
    'Object': {'name': 'Object', 'kind': None, 'parent': None, 'abstract': True},
    'DataObject': {'name': 'DataObject', 'kind': None, 'parent': 'Object', 'abstract': True},
    'LogicAsset': {'name': 'LogicAsset', 'kind': 'definition', 'parent': 'Object', 'abstract': True},
    'ViewAsset': {'name': 'ViewAsset', 'kind': 'definition', 'parent': 'Object', 'abstract': True},
    'ConfigAsset': {'name': 'ConfigAsset', 'kind': 'definition', 'parent': 'Object', 'abstract': True},
    'SystemObject': {'name': 'SystemObject', 'kind': 'system', 'parent': 'Object', 'abstract': True},
    # STD-CLS-030 (BPA 1.0-rc1): extensions are objects of this class; in adapter inputs the
    # target is a class name and the member lists are JSON.
    'ClassExtension': {'name': 'ClassExtension', 'kind': 'definition', 'parent': 'ConfigAsset', 'abstract': False,
                       'fields': [
                           {'name': 'target', 'type': 'text', 'required': True, 'label': 'target', 'description': 'target'},
                           {'name': 'namespace', 'type': 'text', 'required': True, 'label': 'namespace',
                            'description': 'namespace'},
                           {'name': 'order', 'type': 'integer', 'required': True, 'label': 'order', 'description': 'order'},
                           {'name': 'fields', 'type': 'json', 'label': 'fields', 'description': 'fields'},
                           {'name': 'refinements', 'type': 'json', 'label': 'refinements', 'description': 'refinements'},
                           {'name': 'ports', 'type': 'json', 'label': 'ports', 'description': 'ports'},
                           {'name': 'views', 'type': 'json', 'label': 'views', 'description': 'views'},
                           # since 1.0-rc2 (STD-LGS-030, DEC-067): validations added by the extension
                           {'name': 'validations', 'type': 'json', 'label': 'validations',
                            'description': 'validations'}]},
    # STD-VER-020, -061 (BPA 1.0-rc1, DEC-041): change sets and releases are Definition objects.
    'ChangeSet': {'name': 'ChangeSet', 'kind': 'definition', 'parent': 'Object', 'abstract': False,
                  'fields': [_f('source', 'text', True), _f('target', 'text', True), _f('title', 'text', True),
                             _f('description', 'text'),
                             _f('state', 'enum', True, constraints={'allowed_values': [
                                 'open', 'submitted', 'approved', 'rejected', 'merged', 'abandoned']}),
                             _f('preview', 'text'), _f('approvals', 'json'), _f('simulations', 'json'),
                             _f('selection', 'json'), _f('merged_commit', 'text')]},
    # STD-CTX-022 (BPA 1.0-rc1, DEC-044): logic assets are objects of this class (DAT-LogicAsset).
    'Logic': {'name': 'Logic', 'kind': 'definition', 'parent': 'LogicAsset', 'abstract': False,
              'fields': [_f('name', 'text', True),
                         _f('tier', 'enum', True, constraints={'allowed_values': ['l2', 'l3', 'host']}),
                         _f('abi', 'text', True), _f('source', 'json', True), _f('source_ref', 'text'),
                         _f('exports', 'json', True), _f('imports', 'json'), _f('capabilities', 'json', True),
                         _f('profile', 'enum', True, constraints={'allowed_values': [
                             'strict', 'standard', 'batch', 'simulation', 'agent']}),
                         _f('tests', 'json'),
                         _f('state', 'enum', True, constraints={'allowed_values': [
                             'draft', 'committed', 'verified', 'deprecated', 'retired']}),
                         _f('verification', 'text'), _f('publisher_signature', 'json')]},
    # STD-LGS-002, -010 (BPA 1.0-rc2, DEC-066): sheets and Buk property declarations are
    # Definition objects below ConfigAsset; `ast` is derived from `source` at commit.
    'GovernanceSheet': {'name': 'GovernanceSheet', 'kind': 'definition', 'parent': 'ConfigAsset', 'abstract': False,
                        'fields': [_f('name', 'text', True),
                                   _f('layer', 'enum', True, constraints={'allowed_values': [
                                       'platform', 'industry', 'vendor', 'tenant', 'vae', 'unit', 'object']}),
                                   _f('priority', 'integer'), _f('scope', 'json'), _f('source', 'text', True),
                                   _f('ast', 'json'), _f('cascading', 'boolean'), _f('tests', 'json'),
                                   _f('description', 'text', True)]},
    'PropertyDecl': {'name': 'PropertyDecl', 'kind': 'definition', 'parent': 'ConfigAsset', 'abstract': False,
                     'fields': [_f('name', 'text', True), _f('schema', 'json', True),
                                _f('enforcement', 'enum', True, constraints={'allowed_values': [
                                    'commit', 'action', 'read', 'view', 'retention', 'notify']}),
                                _f('combine', 'enum', True, constraints={'allowed_values': [
                                    'override', 'max', 'min', 'union', 'all_true', 'any_true']}),
                                _f('owner', 'text', True), _f('handler', 'text'), _f('description', 'text', True)]},
    # STD-CLS-004 (BPA 1.0-rc3, DEC-071): the lifecycle of a class is a Definition object below
    # ConfigAsset that its declaration names by URI.
    'Lifecycle': {'name': 'Lifecycle', 'kind': 'definition', 'parent': 'ConfigAsset', 'abstract': False,
                  'fields': [_f('name', 'text', True), _f('states', 'json', True), _f('initial', 'text', True),
                             _f('final', 'json'), _f('transitions', 'json', True), _f('effects', 'json'),
                             _f('field_rules', 'json'), _f('description', 'text', True)]},
    # DSN-DVM-803 (PH-1 step 13b, DEC-073): policies are Definition objects below ConfigAsset;
    # the kernel validates `source` against its entity model (no standard clause until 1.0.0).
    'Policy': {'name': 'Policy', 'kind': 'definition', 'parent': 'ConfigAsset', 'abstract': False,
               'fields': [_f('name', 'text', True), _f('source', 'text', True), _f('enabled', 'boolean'),
                          _f('description', 'text', True)]},
    'Release': {'name': 'Release', 'kind': 'definition', 'parent': 'Object', 'abstract': False,
                'fields': [_f('version', 'text', True), _f('tag', 'text', True), _f('notes', 'text'),
                           _f('change_sets', 'json'), _f('approvals', 'json')]},
}
# Custom value types (STD-FND-026) known to the store that loads the declarations.
CUSTOM_TYPES = {}
PORT_REQUIRED = ('name', 'kind', 'permission', 'description')
CARDINALITY = {'one_to_one': 'reference', 'many_to_one': 'reference', 'one_to_many': 'list', 'many_to_many': 'list'}

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
    if f['type'] not in TYPES and f['type'] not in CUSTOM_TYPES:
        raise BpaError('MODEL.UNKNOWN_TYPE', f['type'])
    if 'classification' in f and f['classification'] not in LEVELS:
        raise BpaError('MODEL.INVALID_VALUE', 'classification')
    if 'lookup' in f and f['lookup'] not in ('none', 'equality'):
        raise BpaError('MODEL.INVALID_VALUE', 'lookup')
    rel = f.get('relationship')
    if rel is not None:
        check_relationship(f, rel)


def is_reference_list(f):
    item = (f.get('constraints') or {}).get('item_type') or {}
    return f['type'] == 'list' and item.get('type') == 'reference'


def check_relationship(f, rel):
    """STD-CLS-040: a relationship on a reference or list-of-reference field."""
    if not isinstance(rel, dict) or 'target' not in rel or 'cardinality' not in rel:
        raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'relationship needs target and cardinality')
    shape = CARDINALITY.get(rel['cardinality'])
    if shape is None:
        raise BpaError('MODEL.INVALID_VALUE', 'cardinality')
    if (shape == 'reference' and f['type'] != 'reference') or (shape == 'list' and not is_reference_list(f)):
        raise BpaError('MODEL.INVALID_VALUE', 'cardinality does not fit the field type')
    if rel.get('ownership', 'association') not in ('association', 'composition'):
        raise BpaError('MODEL.INVALID_VALUE', 'ownership')
    if rel.get('on_end', 'restrict') not in ('restrict', 'cascade', 'detach'):
        raise BpaError('MODEL.INVALID_VALUE', 'on_end')
    if 'inverse' in rel and not GRAMMARS['member-name'].match(rel['inverse']):
        raise BpaError('MODEL.INVALID_NAME', rel['inverse'])


PORT_MEMBERS = ('name', 'kind', 'params', 'result', 'errors', 'states', 'transition', 'guards', 'impl', 'permission',
                'idempotent', 'ui', 'description', 'emits')
ERROR_CODE = re.compile(r'\A[A-Z]+\.[A-Z_]+\Z')


def check_port(p):
    """STD-CLS-003: a `DAT-PortDecl` (its optional members since 1.0-rc2, DEC-067)."""
    for k in PORT_REQUIRED:
        if k not in p:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'port member %s' % k)
    for k in p:
        if k not in PORT_MEMBERS:
            raise BpaError('MODEL.INVALID_VALUE', 'unknown port member %s' % k)
    if not isinstance(p['name'], str) or not GRAMMARS['member-name'].match(p['name']):
        raise BpaError('MODEL.INVALID_NAME', str(p['name']))
    if p['kind'] not in ('query', 'command'):
        raise BpaError('MODEL.INVALID_VALUE', 'port kind')
    if not isinstance(p.get('params', []), list):
        raise BpaError('MODEL.INVALID_VALUE', 'params is a list')
    for q in p.get('params', []):
        check_field(q)
    if 'result' in p and (not isinstance(p['result'], str) or not p['result']):
        raise BpaError('MODEL.INVALID_VALUE', 'result is a type name')
    for e in p.get('errors') or []:
        if not isinstance(e, str) or not ERROR_CODE.match(e):
            raise BpaError('MODEL.INVALID_VALUE', 'an error is a registered code')
    for st in p.get('states') or []:
        if not isinstance(st, str) or not GRAMMARS['member-name'].match(st):
            raise BpaError('MODEL.INVALID_NAME', 'state %s' % st)
    if 'transition' in p and (not isinstance(p['transition'], str) or not GRAMMARS['member-name'].match(p['transition'])):
        raise BpaError('MODEL.INVALID_NAME', 'transition')
    if 'transition' in p and p['kind'] != 'command':
        raise BpaError('MODEL.INVALID_VALUE', 'only a command port triggers a transition')
    for g in p.get('guards') or []:
        if not isinstance(g, str) or not g:
            raise BpaError('MODEL.INVALID_VALUE', 'a guard is a reference')
    if 'impl' in p and (not isinstance(p['impl'], str) or not p['impl'].strip()):
        raise BpaError('MODEL.INVALID_VALUE', 'impl references a logic asset')
    if not isinstance(p['permission'], str) or not p['permission']:
        raise BpaError('MODEL.INVALID_VALUE', 'permission is a policy action name')
    if 'idempotent' in p and not isinstance(p['idempotent'], bool):
        raise BpaError('MODEL.INVALID_VALUE', 'idempotent is boolean')
    if p['kind'] == 'query' and p.get('idempotent') is False:
        raise BpaError('MODEL.INVALID_VALUE', 'a query port is always idempotent')
    if 'ui' in p and not isinstance(p['ui'], dict):
        raise BpaError('MODEL.INVALID_VALUE', 'ui is a map')
    if not isinstance(p['description'], str) or not p['description']:
        raise BpaError('MODEL.INVALID_VALUE', 'description')


OVERLAY = ('overridable', 'extend_only', 'locked')
SEVERITIES = ('error', 'warning', 'info')


def check_validations(items):
    """STD-LGS-030: `validations` of a class or an extension (1.0-rc2, DEC-067)."""
    if not isinstance(items, list):
        raise BpaError('MODEL.INVALID_VALUE', 'validations is a list')
    for v in items:
        if not isinstance(v, dict) or not isinstance(v.get('name'), str) or not isinstance(v.get('rule'), str) \
                or v.get('severity') not in SEVERITIES:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'a validation has name, rule and severity')
        from . import expr as _expr
        _expr.parse(v['rule'])


def check_overlay_policy(d):
    pol = d.get('overlay_policy')
    if pol is None:
        return
    if not isinstance(pol, dict) or any(v not in OVERLAY for v in pol.values()):
        raise BpaError('MODEL.INVALID_VALUE', 'overlay_policy maps members to overridable, extend_only or locked')


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
    ports = [p.get('name') for p in d.get('ports', [])]
    for p in d.get('ports', []):
        check_port(p)
    if len(set(ports)) != len(ports):
        raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'duplicate port')
    if 'validations' in d:
        check_validations(d['validations'])
    check_overlay_policy(d)
    # STD-CLS-004 (1.0-rc3): the lifecycle is a URI; a class with a lifecycle has no field state
    if 'lifecycle' in d:
        if not isinstance(d['lifecycle'], str) or not d['lifecycle']:
            raise BpaError('MODEL.INVALID_VALUE', 'lifecycle is the URI of a Lifecycle object')
        if 'state' in names:
            raise BpaError('MODEL.INVALID_VALUE', 'a class with a lifecycle has no field state')


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


def check_override(general, special):
    """STD-CLS-021: an override `special` of port `general`."""
    gp = {q['name']: q for q in general.get('params', [])}
    sp = {q['name']: q for q in special.get('params', [])}
    if set(gp) != set(sp):
        raise BpaError('MODEL.PORT_SIGNATURE_INCOMPATIBLE', 'parameter names')
    for n in gp:
        try:
            refine(sp[n], gp[n])        # the inherited parameter must narrow the override's
        except BpaError:
            raise BpaError('MODEL.PORT_SIGNATURE_INCOMPATIBLE', 'parameter %s' % n)
    if special.get('result') != general.get('result'):
        raise BpaError('MODEL.PORT_SIGNATURE_INCOMPATIBLE', 'result type')
    if not set(special.get('errors', [])) <= set(general.get('errors', [])):
        raise BpaError('MODEL.PORT_SIGNATURE_INCOMPATIBLE', 'error codes')
    if special['kind'] != general['kind']:
        raise BpaError('MODEL.PORT_SIGNATURE_INCOMPATIBLE', 'port kind')


def check_extension(ext):
    """DAT-ClassExtension as given in adapter inputs (STD-CLS-030)."""
    for k in ('target', 'namespace', 'order'):
        if k not in ext:
            raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'extension member %s' % k)
    if not GRAMMARS['namespace'].match(ext['namespace']):
        raise BpaError('MODEL.INVALID_NAME', ext['namespace'])
    for f in ext.get('fields') or []:
        check_field(f)
    for p in ext.get('ports') or []:
        check_port(p)
    if 'validations' in ext:
        check_validations(ext['validations'])


def effective_class(classes, name, extensions=()):
    """Effective definition of a class: linearization, kind, write policy, fields and ports,
    with the extensions of the classes in its linearization (STD-CLS-001…-030)."""
    for d in classes:
        check_decl(d)
    reg = _registry(classes)
    lin = linearize(classes, name, reg)
    kind = resolve_kind(reg, name)
    for c in lin:
        d = reg[c]
        if d.get('trait') and kind is not None and kind not in d['applies_to']:
            raise BpaError('MODEL.KIND_MISMATCH', 'trait does not apply to the kind')
    fields, ports, policy = {}, {}, None
    validations, overlay = [], {}
    for c in reversed(lin):
        d = reg[c]
        validations += list(d.get('validations') or [])
        overlay.update(d.get('overlay_policy') or {})
        wp = d.get('write_policy')
        if wp is not None:
            if policy == 'change_set' and wp == 'direct':
                raise BpaError('MODEL.NARROWING_VIOLATION', 'write_policy')
            policy = wp
        for f in d.get('fields', []):
            fields[f['name']] = refine(fields[f['name']], f) if f['name'] in fields else dict(f)
        for p in d.get('ports', []):
            if p['name'] in ports:
                check_override(ports[p['name']], p)
            ports[p['name']] = dict(p, declared_by=c,
                                    impl_by=c if p.get('impl') else ports.get(p['name'], {}).get('impl_by'))
    # Extensions of the classes in L(C): most general target first, then order, then namespace.
    pos = {c: i for i, c in enumerate(reversed(lin))}
    exts = [e for e in extensions if e.get('target') in pos]
    for e in sorted(exts, key=lambda e: (pos[e['target']], e['order'], e['namespace'])):
        check_extension(e)
        for f in e.get('fields') or []:
            key = '%s.%s' % (e['namespace'], f['name'])
            if key in fields:
                raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'duplicate member %s' % key)
            fields[key] = dict(f, name=key)
        for r in e.get('refinements') or []:
            if r.get('name') not in fields:
                raise BpaError('MODEL.NARROWING_VIOLATION', 'refinement of an unknown member')
            # STD-CLS-031: the overlay policy of the member (1.0-rc2, DEC-067)
            pol = overlay.get(r['name'], 'overridable')
            if pol == 'locked':
                raise BpaError('TEN.OVERRIDE_LOCKED', r['name'])
            if pol == 'extend_only' and any(k not in ('name', 'constraints', 'validations') for k in r):
                raise BpaError('TEN.OVERRIDE_LOCKED', '%s accepts additive refinements only' % r['name'])
            fields[r['name']] = refine(fields[r['name']], {k: v for k, v in r.items() if k != 'validations'})
            validations += list(r.get('validations') or [])
        validations += list(e.get('validations') or [])
        for v in e.get('views') or []:
            if overlay.get('views', 'overridable') == 'locked' or \
                    (isinstance(v, dict) and overlay.get('views.%s' % v.get('mode'), 'overridable') == 'locked'):
                raise BpaError('TEN.OVERRIDE_LOCKED', 'views')
        for p in e.get('ports') or []:
            key = '%s.%s' % (e['namespace'], p['name'])
            if key in ports:
                raise BpaError('MODEL.DECLARATION_INCOMPLETE', 'duplicate member %s' % key)
            ports[key] = dict(p, name=key, declared_by=e['target'], impl_by=e['target'] if p.get('impl') else None)
    if policy is None:
        policy = 'change_set' if 'ConfigAsset' in lin else 'direct'
    out = {'linearization': lin, 'kind': kind, 'write_policy': policy, 'fields': fields}
    # STD-CLS-004 (1.0-rc3): the lifecycle in force is the nearest one named along the linearization
    lifecycle = next((reg[c].get('lifecycle') for c in lin if reg[c].get('lifecycle')), None)
    if lifecycle:
        if 'state' in fields:
            raise BpaError('MODEL.INVALID_VALUE', 'a class with a lifecycle has no field state')
        out['lifecycle'] = lifecycle
    if validations:
        out['validations'] = validations
    if overlay:
        out['overlay_policy'] = overlay
    if ports:
        out['ports'] = {k: {m: v for m, v in p.items() if m not in ('declared_by', 'impl_by')} for k, p in ports.items()}
    out['_ports'] = ports
    return out


def public(eff):
    """The effective definition as returned by the adapter (internal members removed)."""
    return {k: v for k, v in eff.items() if not k.startswith('_')}


def dispatch(classes, name, port, after=None, extensions=()):
    """STD-CLS-020: the class whose implementation of `port` runs for an object of `name`;
    with `after`, the search continues after that class (run.super)."""
    eff = effective_class(classes, name, extensions)
    reg = _registry(classes)
    lin = eff['linearization']
    start = lin.index(after) + 1 if after in lin else 0
    if after is not None and after not in lin:
        raise BpaError('MODEL.REFERENCE_INVALID', 'class %s is not in the linearization' % after)
    if '.' in port:
        p = eff['_ports'].get(port)
        if p and p.get('impl') and p['declared_by'] in lin[start:]:
            return {'class': p['declared_by'], 'impl': p['impl']}
        raise BpaError('MODEL.PORT_NOT_IMPLEMENTED', port)
    for c in lin[start:]:
        for p in reg[c].get('ports', []):
            if p['name'] == port and p.get('impl'):
                return {'class': c, 'impl': p['impl']}
    raise BpaError('MODEL.PORT_NOT_IMPLEMENTED', port)


def depth(reg, name):
    """Position depth in the hierarchy: Object 0, each parent step +1; traits have depth 1."""
    d = reg[name]
    if d.get('trait'):
        return 1
    n = 0
    while reg[name].get('parent'):
        name = reg[name]['parent']
        n += 1
    return n


def type_selector(classes, name, selector, only=False):
    """STD-CLS-022: whether a type selector matches an object of class `name`, and its
    specificity contribution."""
    reg = _registry(classes)
    if selector not in reg:
        raise BpaError('MODEL.REFERENCE_INVALID', 'unknown class %s' % selector)
    lin = linearize(classes, name, reg)
    return {'matches': selector == name if only else selector in lin, 'specificity': depth(reg, selector)}
