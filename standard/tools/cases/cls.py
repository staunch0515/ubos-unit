"""Vector cases for the class system (STD-CLS-*). Not required of the PH-0 prototype."""
from .common import cls, field


def trait(name, applies_to, fields=()):
    return {'name': name, 'namespace': 'acme.finance', 'trait': True, 'applies_to': applies_to,
            'label': name, 'description': name, 'schema_version': 1, 'fields': list(fields)}


def cases(g):
    def E(clause, desc, classes, name, error=None, op='effective_class'):
        g.pure(clause, desc, op, {'classes': classes, 'class': name}, error=error, prototype=False)

    A = cls('Account', [field('number', 'text', True)])
    # STD-CLS-001 class declarations
    E('STD-CLS-001', 'a complete class declaration is accepted', [A], 'Account')
    for member in ('name', 'namespace', 'kind', 'parent', 'label', 'description', 'schema_version', 'fields'):
        E('STD-CLS-001', 'a class declaration without %s is incomplete' % member,
          [{k: v for k, v in A.items() if k != member}], 'Account' if member != 'name' else 'Account',
          error='MODEL.DECLARATION_INCOMPLETE')
    E('STD-CLS-001', 'a class name that is not PascalCase is rejected', [dict(A, name='account')], 'account',
      error='MODEL.INVALID_NAME')
    E('STD-CLS-001', 'a namespace outside the grammar is rejected', [dict(A, namespace='Acme Finance')], 'Account',
      error='MODEL.INVALID_NAME')
    E('STD-CLS-001', 'an unknown kind is rejected', [dict(A, kind='table')], 'Account', error='MODEL.INVALID_VALUE')
    E('STD-CLS-001', 'a declaration may add optional members', [dict(A, abstract=False, keys=[['number']],
                                                                   valid_time='date')], 'Account')
    E('STD-CLS-001', 'two fields with the same name are rejected',
      [dict(A, fields=[field('number', 'text'), field('number', 'integer')])], 'Account', error='MODEL.DECLARATION_INCOMPLETE')
    E('STD-CLS-001', 'a class under an unknown parent is rejected', [cls('X', [], parent='Missing')], 'X',
      error='MODEL.REFERENCE_INVALID')
    E('STD-CLS-001', 'redeclaring a genesis class is rejected', [cls('Object', [])], 'Object',
      error='MODEL.DECLARATION_INCOMPLETE')
    # STD-CLS-002 field declarations
    for member in ('name', 'type', 'label', 'description'):
        f = {k: v for k, v in field('number', 'text').items() if k != member}
        E('STD-CLS-002', 'a field declaration without %s is incomplete' % member, [dict(A, fields=[f])], 'Account',
          error='MODEL.DECLARATION_INCOMPLETE')
    E('STD-CLS-002', 'a field name that is not snake_case is rejected', [dict(A, fields=[field('Number', 'text')])],
      'Account', error='MODEL.INVALID_NAME')
    E('STD-CLS-002', 'a field of an unknown value type is rejected', [dict(A, fields=[field('n', 'float')])], 'Account',
      error='MODEL.UNKNOWN_TYPE')
    E('STD-CLS-002', 'a field with an unknown classification is rejected',
      [dict(A, fields=[field('n', 'text', classification='secret')])], 'Account', error='MODEL.INVALID_VALUE')
    E('STD-CLS-002', 'a field with constraints, default and classification is accepted',
      [dict(A, fields=[field('iban', 'text', True, constraints={'max_length': 34}, default='',
                             classification='personal', lookup='equality')])], 'Account')
    for t in ('decimal', 'money', 'date', 'instant', 'period', 'enum', 'list', 'map', 'composite', 'document',
              'blob', 'json', 'boolean', 'duration', 'quantity', 'time', 'integer'):
        E('STD-CLS-002', 'a field of type %s is accepted' % t, [dict(A, fields=[field('v', t)])], 'Account')
    # STD-CLS-010 root and base classes
    for base in ('DataObject', 'LogicAsset', 'ViewAsset', 'ConfigAsset', 'SystemObject'):
        E('STD-CLS-010', 'the genesis class %s lies directly below Object' % base, [], base, op='linearize')
    # STD-CLS-011 C3 linearization
    B = cls('Party', [field('name', 'text')])
    C = cls('Customer', [], parent='Party', kind='inherit')
    T1, T2 = trait('Auditable', ['definition']), trait('Taggable', ['definition'])
    E('STD-CLS-011', 'single inheritance chain', [B, C], 'Customer', op='linearize')
    E('STD-CLS-011', 'traits come before the parent', [B, T1, dict(C, traits=['Auditable'])], 'Customer', op='linearize')
    E('STD-CLS-011', 'two traits keep their declared order', [B, T1, T2, dict(C, traits=['Auditable', 'Taggable'])],
      'Customer', op='linearize')
    E('STD-CLS-011', 'reversed trait order gives the reversed linearization',
      [B, T1, T2, dict(C, traits=['Taggable', 'Auditable'])], 'Customer', op='linearize')
    T3 = dict(trait('Versioned', ['definition']), traits=['Auditable'])
    E('STD-CLS-011', 'a trait with its own trait', [B, T1, T3, dict(C, traits=['Versioned'])], 'Customer', op='linearize')
    E('STD-CLS-011', 'a trait reached twice appears once', [B, T1, T3, dict(C, traits=['Versioned', 'Auditable'])],
      'Customer', op='linearize')
    E('STD-CLS-011', 'listing a trait of the parent again before the parent is inconsistent',
      [dict(B, traits=['Auditable']), T1, T2, dict(C, traits=['Taggable', 'Auditable'])], 'Customer', op='linearize',
      error='MODEL.LINEARIZATION_INCONSISTENT')
    E('STD-CLS-011', 'an inconsistent order is rejected',
      [T1, T2, dict(trait('X1', ['definition']), traits=['Auditable', 'Taggable']),
       dict(trait('X2', ['definition']), traits=['Taggable', 'Auditable']), B,
       dict(C, traits=['X1', 'X2'])], 'Customer', op='linearize', error='MODEL.LINEARIZATION_INCONSISTENT')
    E('STD-CLS-011', 'a cycle is rejected', [cls('P', [], parent='Q', kind='inherit'), cls('Q', [], parent='P', kind='inherit')],
      'P', op='linearize', error='MODEL.INHERITANCE_CYCLE')
    E('STD-CLS-011', 'a class that is its own parent is a cycle', [cls('P', [], parent='P', kind='inherit')], 'P',
      op='linearize', error='MODEL.INHERITANCE_CYCLE')
    E('STD-CLS-011', 'a three-level chain with a trait at each level',
      [B, T1, T2, T3, dict(C, traits=['Auditable']), cls('Vip', [], parent='Customer', kind='inherit', traits=['Taggable'])],
      'Vip', op='linearize')
    # STD-CLS-012 effective definition
    E('STD-CLS-012', 'fields of parents and traits are inherited', [B, dict(T1, fields=[field('audited_by', 'text')]),
                                                                  dict(C, traits=['Auditable'], fields=[field('segment', 'text')])], 'Customer')
    E('STD-CLS-012', 'a trait refinement overrides the parent declaration',
      [dict(B, fields=[field('name', 'text', constraints={'max_length': 100})]),
       dict(T1, fields=[field('name', 'text', constraints={'max_length': 50})]), dict(C, traits=['Auditable'])], 'Customer')
    E('STD-CLS-012', 'the class refinement is applied last',
      [dict(B, fields=[field('name', 'text', constraints={'max_length': 100})]),
       dict(C, fields=[field('name', 'text', True, constraints={'max_length': 80})])], 'Customer')
    E('STD-CLS-012', 'the effective write policy of a ConfigAsset class is change_set',
      [cls('TaxRule', [field('rate', 'decimal')], parent='ConfigAsset', kind='inherit')], 'TaxRule')
    # STD-CLS-013 narrowing
    def N(desc, old, new, error=None):
        E('STD-CLS-013', desc, [dict(B, fields=[dict(field('name', 'text'), **old)]),
                                 dict(C, fields=[dict(field('name', 'text'), **new)])], 'Customer', error=error)
    N('required false to true is allowed', {}, {'required': True})
    N('required true to false is a violation', {'required': True}, {'required': False}, 'MODEL.NARROWING_VIOLATION')
    N('min_length may increase', {'constraints': {'min_length': 1}}, {'constraints': {'min_length': 3}})
    N('min_length may not decrease', {'constraints': {'min_length': 3}}, {'constraints': {'min_length': 1}},
      'MODEL.NARROWING_VIOLATION')
    N('max_length may decrease', {'constraints': {'max_length': 50}}, {'constraints': {'max_length': 20}})
    N('max_length may not increase', {'constraints': {'max_length': 20}}, {'constraints': {'max_length': 50}},
      'MODEL.NARROWING_VIOLATION')
    N('a pattern may be added when none existed', {}, {'constraints': {'pattern': '[A-Z].*'}})
    N('a pattern may not be replaced', {'constraints': {'pattern': '[A-Z].*'}}, {'constraints': {'pattern': '.*'}},
      'MODEL.NARROWING_VIOLATION')
    N('classification may be raised', {'classification': 'internal'}, {'classification': 'personal'})
    N('classification may not be lowered', {'classification': 'personal'}, {'classification': 'internal'},
      'MODEL.NARROWING_VIOLATION')
    N('the default may be changed', {'default': 'a'}, {'default': 'b'})
    N('label and description may be changed', {}, {'label': 'Legal name', 'description': 'registered name'})
    N('the type may not change', {}, {'type': 'integer'}, 'MODEL.NARROWING_VIOLATION')
    N('computed may not be added to a stored field', {}, {'computed': 'upper(name)'}, 'MODEL.NARROWING_VIOLATION')
    N('an unlisted aspect may not change', {'lookup': 'none'}, {'lookup': 'equality'}, 'MODEL.NARROWING_VIOLATION')

    def NV(desc, t, old, new, error=None):
        E('STD-CLS-013', desc, [dict(B, fields=[field('v', t, constraints=old)]),
                                 dict(C, fields=[field('v', t, constraints=new)])], 'Customer', error=error)
    NV('allowed_values may shrink to a subset', 'enum', {'allowed_values': ['a', 'b', 'c']}, {'allowed_values': ['a', 'b']})
    NV('allowed_values may not grow', 'enum', {'allowed_values': ['a', 'b']}, {'allowed_values': ['a', 'b', 'c']},
       'MODEL.NARROWING_VIOLATION')
    NV('the scale must stay equal', 'decimal', {'scale': 2}, {'scale': 4}, 'MODEL.NARROWING_VIOLATION')
    NV('an equal scale is allowed', 'decimal', {'scale': 2}, {'scale': 2})
    NV('min may increase', 'integer', {'min': 0}, {'min': 10})
    NV('max may not increase', 'integer', {'max': 10}, {'max': 20}, 'MODEL.NARROWING_VIOLATION')
    NV('max_items may decrease', 'list', {'max_items': 10}, {'max_items': 5})
    NV('min_items may not decrease', 'list', {'min_items': 2}, {'min_items': 1}, 'MODEL.NARROWING_VIOLATION')
    E('STD-CLS-013', 'write_policy direct to change_set is allowed',
      [dict(B, write_policy='direct'), dict(C, write_policy='change_set')], 'Customer')
    E('STD-CLS-013', 'write_policy change_set to direct is a violation',
      [dict(B, write_policy='change_set'), dict(C, write_policy='direct')], 'Customer', error='MODEL.NARROWING_VIOLATION')
    # STD-CLS-014 kinds
    E('STD-CLS-014', 'a child of DataObject declares a concrete kind', [cls('Doc', [], kind='raw_document')], 'Doc')
    E('STD-CLS-014', 'a child of DataObject with kind inherit is rejected', [cls('Doc', [], kind='inherit')], 'Doc',
      error='MODEL.KIND_MISMATCH')
    E('STD-CLS-014', 'a child of DataObject cannot be of System kind', [cls('Doc', [], kind='system')], 'Doc',
      error='MODEL.KIND_MISMATCH')
    E('STD-CLS-014', 'a subclass inherits the kind of its parent', [B, C], 'Customer')
    E('STD-CLS-014', 'a subclass may repeat the kind of its parent', [B, dict(C, kind='definition')], 'Customer')
    E('STD-CLS-014', 'a subclass with another kind is rejected', [B, dict(C, kind='ledger')], 'Customer',
      error='MODEL.KIND_MISMATCH')
    E('STD-CLS-014', 'a subclass of SystemObject has System kind', [cls('Job', [], parent='SystemObject', kind='inherit')], 'Job')
    E('STD-CLS-014', 'a trait must apply to the kind of the class',
      [B, trait('LedgerOnly', ['ledger']), dict(C, traits=['LedgerOnly'])], 'Customer', error='MODEL.KIND_MISMATCH')
    E('STD-CLS-014', 'a trait that applies to several kinds is accepted',
      [B, trait('Commented', ['definition', 'raw_document']), dict(C, traits=['Commented'])], 'Customer')
    # STD-CLS-015 minimum depths
    chain = [cls('L0', [field('f0', 'text')])] + [cls('L%d' % i, [field('f%d' % i, 'text')], parent='L%d' % (i - 1),
                                                     kind='inherit') for i in range(1, 32)]
    traits_ = [trait('Tr%d' % i, ['definition']) for i in range(16)]
    E('STD-CLS-015', 'a linearization of 48 classes plus genesis is supported',
      chain + traits_ + [cls('Deep', [], parent='L31', kind='inherit', traits=['Tr%d' % i for i in range(16)])], 'Deep',
      op='linearize')
    E('STD-CLS-015', 'the effective definition of a depth-32 class collects all fields', chain, 'L31')
