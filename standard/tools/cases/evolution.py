"""Vector cases of BPA 1.0-rc8 for schema evolution and upstream changes (PH-2 step 1d,
DEC-092): the computed classification of class changes (STD-CLS-050), the lens operations
(STD-CLS-060), lens application to reads, writes and merges (STD-CLS-061), the totality check
(STD-CLS-062) and upstream change detection (STD-VER-082). Not required of the PH-0
prototype."""
import copy

from .common import NOW, cls, field, uid, create
from .common import commit
from .model import port
from .sheets import op

TIERS = ['gold', 'silver', 'bronze']
F_NAME = field('name', 'text', True)
F_EMAIL = field('email', 'text')
F_TIER = field('tier', 'enum', constraints={'allowed_values': TIERS})
F_LIMIT = field('limit', 'money')
F_CREDIT = field('credit', 'decimal')
CUST = cls('Customer', [F_NAME, F_EMAIL, F_TIER, F_LIMIT, F_CREDIT])
C1, C2 = uid(1), uid(2)
ACME = create(C1, 'Customer', 'acme', {'name': 'Acme Corp', 'email': 'ap@acme.example', 'tier': 'gold',
                                       'limit': {'amount': '5000.00', 'currency': 'USD'}, 'credit': '12.5'})


def v(n, fields, **extra):
    """Version n of the customer class with the given fields."""
    d = cls('Customer', fields, **extra)
    d['schema_version'] = n
    return d


def without(fields, name):
    return [f for f in fields if f['name'] != name]


def renamed(fields, old, new):
    return [dict(f, name=new, label=new, description=new) if f['name'] == old else f for f in fields]


BASE = CUST['fields']
V2_RENAME = v(2, renamed(BASE, 'email', 'email_address'))
V2_NOTES = v(2, BASE + [field('notes', 'text')])
V2_SPLIT = v(2, without(BASE, 'limit') + [field('limit_amount', 'decimal'), field('limit_currency', 'text')])
V3_SPLIT = v(3, without(V2_RENAME['fields'], 'limit') + [field('limit_amount', 'decimal'), field('limit_currency', 'text')])
V2_MAP = v(2, [F_NAME, F_EMAIL, field('tier', 'enum', constraints={'allowed_values': ['premium', 'bronze']}),
               F_LIMIT, F_CREDIT])


def lens(ops, frm=1, **extra):
    d = {'class': 'Customer', 'from': frm, 'to': frm + 1, 'ops': ops}
    d.update(extra)
    return d


RENAME = lens([{'op': 'rename', 'from': 'email', 'to': 'email_address'}])
SPLIT = {'op': 'split', 'from': 'limit', 'to': ['limit_amount', 'limit_currency'],
         'forward': ['amount(params.limit)', 'currency(params.limit)'],
         'backward': 'money(params.limit_amount, params.limit_currency)'}
MERGE_TIERS = lens([{'op': 'map_enum', 'field': 'tier', 'mapping': {'gold': 'premium', 'silver': 'premium',
                                                                     'bronze': 'bronze'}}],
                   one_way=True, one_way_reason='gold and silver both become premium')
FLOOR = lens([{'op': 'convert', 'field': 'credit', 'from_type': 'decimal', 'to_type': 'integer',
               'forward_expr': 'floor(params.credit)', 'backward_expr': 'params.credit * 1.0'}])
V2_FLOOR = v(2, without(BASE, 'credit') + [field('credit', 'integer')])
L1, L2 = uid(50), uid(51)


def lens_obj(oid, body, slug='customer-lens'):
    return create(oid, 'Lens', slug, body)


def change(decl, lens_id=None, error=None, label=None, details=False, **kw):
    inp = {'class': decl, 'branch': FB}
    if lens_id:
        inp['lens'] = lens_id
    inp.update(kw)
    s = op('change_class', label, error, **inp)
    if details:
        s['details'] = True
    return s


def rd(oid=C1, branch='main', label=None, **kw):
    return op('read', label, None, branch=branch, object_id=oid, **kw)


def upd(body, label_ref, oid=C1, branch='main', error=None, **kw):
    ch = {'op': 'update', 'object_id': oid, 'base': {'$ref': '%s/version_hash' % label_ref}, 'body': body}
    ch.update(kw)
    return commit([ch], branch=branch, error=error)


def apply(ops_lens, body, frm, to, direction='forward', error=None):
    return op('apply_lens', None, error, lens=ops_lens, body=body, direction=direction,
              **{'from': frm, 'to': to})


OV = 'overlay/acme'
FB = 'feature/crm'


def cases(g):
    def S(clause, desc, steps, extra=(), classes=None):
        g.vector(clause, desc, setup=[{'op': 'set_now', 'input': {'now': NOW}},
                                      {'op': 'load_genesis', 'input': {'classes': classes or [CUST]}},
                                      commit([ACME], label='c'),
                                      op('create_branch', None, None, name=FB, **{'from': 'main'})] + list(extra),
                 steps=steps, prototype=False)

    def P(clause, desc, op_, inp, error=None):
        g.pure(clause, desc, op_, inp, error=error, prototype=False)

    # ---- STD-CLS-050: classification ----
    S('STD-CLS-050', 'a new optional field is additive and takes effect without a lens',
      [change(V2_NOTES), rd(label='r'), upd({'notes': 'key account'}, 'r'), rd()])
    S('STD-CLS-050', 'a renamed field is lens-required and takes effect with a total lens',
      [commit(branch=FB, changes=[lens_obj(L1, RENAME)]), change(V2_RENAME, L1)])
    S('STD-CLS-050', 'a lens-required change without a lens is refused',
      [change(V2_RENAME, error='MODEL.LENS_REQUIRED', details=True)])
    S('STD-CLS-050', 'a new required field without a default needs a lens',
      [change(v(2, BASE + [field('region', 'text', True)]), error='MODEL.LENS_REQUIRED', details=True)])
    S('STD-CLS-050', 'a narrower constraint that the stored versions satisfy is additive',
      [change(v(2, [dict(F_NAME, constraints={'max_length': 40})] + BASE[1:]))])
    S('STD-CLS-050', 'a narrower constraint that a stored version violates is breaking',
      [change(v(2, [dict(F_NAME, constraints={'max_length': 4})] + BASE[1:]), error='MODEL.MIGRATION_REQUIRED',
              details=True)])
    S('STD-CLS-050', 'a class change increments the schema version by one',
      [change(v(3, BASE + [field('notes', 'text')]), error='MODEL.INVALID_VALUE')])
    S('STD-CLS-050', 'a change names a declared class',
      [change(dict(V2_NOTES, name='Supplier'), error='MODEL.REFERENCE_INVALID')])
    P('STD-CLS-050', 'removed fields and changed types are lens-required; labels are additive', 'classify_change',
      {'from': CUST, 'to': v(2, [dict(F_NAME, label='Legal name'), F_TIER, F_LIMIT, field('credit', 'integer')])})
    P('STD-CLS-050', 'a new required field with a default is additive', 'classify_change',
      {'from': CUST, 'to': v(2, BASE + [field('region', 'text', True, default='US')])})
    P('STD-CLS-050', 'a removed port is breaking; a new port is additive', 'classify_change',
      {'from': dict(CUST, ports=[port('close'), port('freeze')]), 'to': v(2, BASE, ports=[port('close'), port('open')])})
    P('STD-CLS-050', 'a changed kind is breaking', 'classify_change',
      {'from': CUST, 'to': v(2, BASE, kind='ledger')})
    P('STD-CLS-050', 'a constraint change is classified against the stored versions', 'classify_change',
      {'from': CUST, 'to': v(2, [F_NAME, F_EMAIL, dict(F_TIER, constraints={'allowed_values': ['gold', 'silver']}),
                                 F_LIMIT, F_CREDIT]),
       'stored': [{'name': 'A', 'tier': 'gold'}, {'name': 'B', 'tier': 'bronze'}]})

    # ---- STD-CLS-060: lens operations ----
    body = {'name': 'Acme', 'email': 'ap@acme.example', 'tier': 'gold',
            'limit': {'amount': '5000.00', 'currency': 'USD'}, 'credit': '12.5'}
    P('STD-CLS-060', 'rename moves a value forward', 'apply_lens',
      {'lens': RENAME, 'from': CUST, 'to': V2_RENAME, 'body': body, 'direction': 'forward'})
    P('STD-CLS-060', 'rename moves the value back', 'apply_lens',
      {'lens': RENAME, 'from': CUST, 'to': V2_RENAME, 'direction': 'backward',
       'body': {'name': 'Acme', 'email_address': 'ap@acme.example'}})
    P('STD-CLS-060', 'split computes the parts with L1 expressions', 'apply_lens',
      {'lens': lens([SPLIT]), 'from': CUST, 'to': V2_SPLIT, 'body': body, 'direction': 'forward'})
    P('STD-CLS-060', 'split joins the parts backward', 'apply_lens',
      {'lens': lens([SPLIT]), 'from': CUST, 'to': V2_SPLIT, 'direction': 'backward',
       'body': {'name': 'Acme', 'limit_amount': '5000', 'limit_currency': 'USD'}})
    join = lens([{'op': 'join', 'from': ['limit_amount', 'limit_currency'], 'to': 'limit',
                  'forward': 'money(params.limit_amount, params.limit_currency)',
                  'backward': ['amount(params.limit)', 'currency(params.limit)']}], frm=2)
    v3_join = v(3, V2_SPLIT['fields'][:-2] + [F_LIMIT])
    P('STD-CLS-060', 'join combines the parts and splits them backward', 'apply_lens',
      {'lens': join, 'from': V2_SPLIT, 'to': v3_join, 'direction': 'forward',
       'body': {'name': 'Acme', 'limit_amount': '250.5', 'limit_currency': 'EUR'}})
    P('STD-CLS-060', 'convert applies the forward expression', 'apply_lens',
      {'lens': FLOOR, 'from': CUST, 'to': V2_FLOOR, 'direction': 'forward', 'body': {'name': 'Acme', 'credit': '12.5'}})
    dflt = lens([{'op': 'default', 'field': 'region', 'value': 'US'}])
    v2_region = v(2, BASE + [field('region', 'text', True)])
    P('STD-CLS-060', 'default sets an absent value forward', 'apply_lens',
      {'lens': dflt, 'from': CUST, 'to': v2_region, 'direction': 'forward', 'body': {'name': 'Acme'}})
    P('STD-CLS-060', 'default drops a value equal to the default backward', 'apply_lens',
      {'lens': dflt, 'from': CUST, 'to': v2_region, 'direction': 'backward', 'body': {'name': 'Acme', 'region': 'US'}})
    nest = lens([{'op': 'nest', 'fields': ['email', 'tier'], 'into': 'profile'}])
    v2_nest = v(2, [F_NAME, field('profile', 'json'), F_LIMIT, F_CREDIT])
    P('STD-CLS-060', 'nest moves fields into a composite', 'apply_lens',
      {'lens': nest, 'from': CUST, 'to': v2_nest, 'direction': 'forward', 'body': body})
    unnest = lens([{'op': 'unnest', 'from': 'profile', 'fields': ['email', 'tier']}], frm=2)
    P('STD-CLS-060', 'unnest flattens a composite', 'apply_lens',
      {'lens': unnest, 'from': v2_nest, 'to': v(3, BASE), 'direction': 'forward',
       'body': {'name': 'Acme', 'profile': {'email': 'ap@acme.example', 'tier': 'silver'}}})
    mp = lens([{'op': 'map_enum', 'field': 'tier', 'mapping': {'gold': 'premium', 'bronze': 'bronze'}}])
    P('STD-CLS-060', 'map_enum maps values and inverts an injective mapping', 'apply_lens',
      {'lens': mp, 'from': CUST, 'to': V2_MAP, 'direction': 'backward', 'body': {'name': 'Acme', 'tier': 'premium'}})
    drop = lens([{'op': 'drop_with_archive', 'field': 'credit'}])
    P('STD-CLS-060', 'drop_with_archive keeps the value in _archived', 'apply_lens',
      {'lens': drop, 'from': CUST, 'to': v(2, without(BASE, 'credit')), 'direction': 'forward', 'body': body})
    P('STD-CLS-060', 'drop_with_archive restores the value backward', 'apply_lens',
      {'lens': drop, 'from': CUST, 'to': v(2, without(BASE, 'credit')), 'direction': 'backward',
       'body': {'name': 'Acme', '_archived': {'credit': '12.5'}}})
    P('STD-CLS-060', 'a mapping that is not injective makes the lens one-way', 'apply_lens',
      {'lens': lens(MERGE_TIERS['ops']), 'from': CUST, 'to': V2_MAP, 'body': body}, error='MODEL.INVALID_VALUE')
    P('STD-CLS-060', 'a one-way lens states its reason', 'apply_lens',
      {'lens': lens(MERGE_TIERS['ops'], one_way=True), 'from': CUST, 'to': V2_MAP, 'body': body},
      error='MODEL.DECLARATION_INCOMPLETE')
    P('STD-CLS-060', 'a one-way lens has no backward mapping', 'apply_lens',
      {'lens': MERGE_TIERS, 'from': CUST, 'to': V2_MAP, 'direction': 'backward',
       'body': {'name': 'Acme', 'tier': 'premium'}}, error='MODEL.LENS_REQUIRED')
    P('STD-CLS-060', 'L2 residual functions are not available in this release', 'apply_lens',
      {'lens': dict(RENAME, residual_forward='ubos://acme/Logic/fix'), 'from': CUST, 'to': V2_RENAME, 'body': body},
      error='LOGIC.NOT_AVAILABLE')
    P('STD-CLS-060', 'an operation outside the nine is refused', 'apply_lens',
      {'lens': lens([{'op': 'uppercase', 'field': 'name'}]), 'from': CUST, 'to': CUST, 'body': body},
      error='MODEL.INVALID_VALUE')
    P('STD-CLS-060', 'a lens maps one schema version to the next', 'apply_lens',
      {'lens': dict(RENAME, to=3), 'from': CUST, 'to': V2_RENAME, 'body': body}, error='MODEL.INVALID_VALUE')
    P('STD-CLS-060', 'split gives one expression per part', 'apply_lens',
      {'lens': lens([dict(SPLIT, forward=['amount(params.limit)'])]), 'from': CUST, 'to': V2_SPLIT, 'body': body},
      error='MODEL.INVALID_VALUE')
    S('STD-CLS-060', 'a lens is checked when it is committed',
      [commit(branch=FB, changes=[lens_obj(L1, lens([{'op': 'rename', 'from': 'email'}]))], error='MODEL.DECLARATION_INCOMPLETE')])

    # ---- STD-CLS-061: application ----
    renamed_ = [commit(branch=FB, changes=[lens_obj(L1, RENAME)]), change(V2_RENAME, L1)]
    S('STD-CLS-061', 'a version stored in an older schema version is read in the current one, or in the version asked for',
      renamed_ + [rd(), rd(schema_version=1), rd(label='again')])
    S('STD-CLS-061', 'a version written in the current schema version is read backward',
      renamed_ + [commit([create(C2, 'Customer', 'globex', {'name': 'Globex', 'email_address': 'ap@globex.example'})]),
                  rd(C2, schema_version=1)])
    S('STD-CLS-061', 'a change in an older schema version is lensed forward before it is stored',
      renamed_ + [commit([create(C2, 'Customer', 'globex', {'name': 'Globex', 'email': 'ap@globex.example'},
                                 schema_version=1)]), rd(C2)])
    S('STD-CLS-061', 'an update in an older schema version patches its base seen in that version',
      renamed_ + [rd(label='r'), upd({'email': 'billing@acme.example'}, 'r', schema_version=1), rd()])
    S('STD-CLS-061', 'an update in the current schema version applies to its base lensed forward',
      renamed_ + [rd(label='r'), upd({'name': 'Acme Inc'}, 'r'), rd()])
    one_way = [commit(branch=FB, changes=[lens_obj(L1, MERGE_TIERS)]), change(V2_MAP, L1)]
    S('STD-CLS-061', 'reading backward through a one-way lens is refused',
      one_way + [rd(), commit([create(C2, 'Customer', 'globex', {'name': 'Globex', 'tier': 'premium'})]),
                 op('read', None, 'MODEL.LENS_REQUIRED', branch='main', object_id=C2, schema_version=1)])
    S('STD-CLS-061', 'an update in an older schema version through a one-way lens is refused',
      one_way + [rd(label='r'), upd({'credit': '1'}, 'r', schema_version=1, error='MODEL.LENS_REQUIRED')])
    S('STD-CLS-061', 'reading backward over an additive change drops the fields the older version lacks',
      [change(V2_NOTES), rd(label='r'), upd({'notes': 'key account'}, 'r'), rd(schema_version=1)])
    S('STD-CLS-061', 'lenses chain over several schema versions',
      renamed_ + [commit(branch=FB, changes=[lens_obj(L2, lens([SPLIT], frm=2), 'customer-lens-2')]), change(V3_SPLIT, L2), rd(),
                  rd(schema_version=1)])
    S('STD-CLS-061', 'a merge aligns base, source and target to the current schema version',
      [op('create_branch', None, None, name='feature/schema', **{'from': 'main'}),
       commit(branch='feature/schema', changes=[lens_obj(L1, RENAME)]), change(V2_RENAME, L1, branch='feature/schema'),
       rd(branch=FB, label='f'), upd({'email_address': 'billing@acme.example'}, 'f', branch=FB),
       rd(label='m'), upd({'name': 'Acme Inc'}, 'm'),
       op('merge', None, None, source=FB, target='main'), rd()])
    S('STD-CLS-061', 'a schema version outside the class history is refused',
      renamed_ + [op('read', None, 'MODEL.INVALID_VALUE', branch='main', object_id=C1, schema_version=3)])

    # ---- STD-CLS-062: totality ----
    P('STD-CLS-062', 'a total lens passes the round trips of generated values', 'check_lens',
      {'lens': lens([SPLIT]), 'from': CUST, 'to': V2_SPLIT})
    P('STD-CLS-062', 'failed round trips are reported with sample values', 'check_lens',
      {'lens': FLOOR, 'from': CUST, 'to': V2_FLOOR})
    P('STD-CLS-062', 'a forward mapping that does not conform to the target version is a failure', 'check_lens',
      {'lens': RENAME, 'from': CUST, 'to': v(2, without(renamed(BASE, 'email', 'email_address'), 'tier'))})
    P('STD-CLS-062', 'stored current versions are checked with the generated values', 'check_lens',
      {'lens': RENAME, 'from': CUST,
       'to': v(2, renamed(BASE, 'email', 'email_address')[:1] +
               [field('email_address', 'text', constraints={'max_length': 12})] + BASE[2:]),
       'stored': [{'name': 'Acme', 'email': 'ap@acme.example'}, {'name': 'Initech', 'email': 'a@b.example'}]})
    P('STD-CLS-062', 'a one-way lens is checked forward only', 'check_lens',
      {'lens': MERGE_TIERS, 'from': CUST, 'to': V2_MAP})
    S('STD-CLS-062', 'a class change whose lens is not total is breaking',
      [commit(branch=FB, changes=[lens_obj(L1, FLOOR)]), change(V2_FLOOR, L1, error='MODEL.MIGRATION_REQUIRED', details=True)])

    # ---- STD-VER-082: upstream changes ----
    ov = [op('create_branch', None, None, name=OV, **{'from': 'main', 'parent_line': {'branch': 'main', 'mode': 'live'}})]
    override = [rd(branch=OV, label='o'), upd({'name': 'Acme (EU)'}, 'o', branch=OV)]
    parent_change = [rd(label='p'), upd({'email': 'billing@acme.example'}, 'p')]

    def U(desc, steps):
        S('STD-VER-082', desc, steps, extra=ov)

    def uc(label=None):
        return op('upstream_changes', label, None, branch=OV)

    def resolve(mode, error=None, details=False, **kw):
        s = op('resolve_upstream', None, error, branch=OV, object_id=C1, mode=mode, **kw)
        if details:
            s['details'] = True
        return s

    U('a parent change of an overridden object is marked with the versions before and after',
      override + parent_change + [uc()])
    U('an override without a parent change is not marked', override + [uc()])
    U('an inherited object that is not overridden is not marked', parent_change + [uc(), rd(branch=OV)])
    U('acknowledgement clears the mark', override + parent_change + [resolve('acknowledge'), uc()])
    U('a three-way merge takes the parent change into the override and clears the mark',
      override + parent_change + [resolve('merge'), rd(branch=OV), uc()])
    U('a merge whose parent and override changed the same field conflicts',
      override + [rd(label='p'), upd({'name': 'Acme Holdings'}, 'p'), resolve('merge', 'VER.MERGE_CONFLICT', True)])
    U('a conflict is resolved with a recorded resolution',
      override + [rd(label='p'), upd({'name': 'Acme Holdings'}, 'p'),
                  resolve('merge', resolutions=[{'object_id': C1, 'path': '/name', 'choice': 'target',
                                                 'resolver': 'did:ubos:acme:01900000-0000-7000-8000-000000000021'}]),
                  rd(branch=OV), uc()])
    U('an object the parent line ended is acknowledged, not merged',
      override + [rd(label='p'), commit([{'op': 'end', 'object_id': C1, 'base': {'$ref': 'p/version_hash'}}]), uc(),
                  resolve('merge', 'MODEL.CONSTRAINT_VIOLATED'), resolve('acknowledge'), uc()])
    U('an object without an upstream change is not resolved', override + [resolve('acknowledge',
                                                                                    'MODEL.CONSTRAINT_VIOLATED')])
    U('a later parent change is marked from the acknowledged version',
      override + parent_change + [resolve('acknowledge'), rd(label='q'), upd({'tier': 'silver'}, 'q'), uc()])
