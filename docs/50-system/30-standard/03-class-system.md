---
id: UBS-STD-03
title: BPA Standard — Class System (CLS)
status: draft
phase: PH-0
depends_on: [UBS-STD-02]
---

# BPA Standard — Class System (CLS)

## 1. Class declarations

### STD-CLS-001 — Class declaration schema
- **Clause:** A class MUST be an object of the meta-class `Class` whose body conforms to `DAT-ClassDecl`. Class objects follow all object and versioning rules. The genesis model defines `Class` itself (self-describing).
- **Grammar / Schema:** `DAT-ClassDecl`
- **Conformance vectors:** VER-CONF-0500…0529
- **Satisfies:** FR-MODEL-011, FR-MODEL-012
- **Notes:** —

```yaml
DAT-ClassDecl:
  name: { type: text, required: true, description: "PascalCase class name" }
  namespace: { type: text, required: true, description: "owning namespace (Buk or tenant)" }
  kind: { type: enum, required: true, description: "definition | ledger | system | raw_document | inherit" }
  parent: { type: reference, required: true, description: "parent class URI (absent only for Object)" }
  traits: { type: list, required: false, description: "ordered trait URIs" }
  abstract: { type: boolean, required: false, description: "no direct instances" }
  label: { type: text, required: true, description: "display label (translatable)" }
  description: { type: text, required: true, description: "documentation" }
  schema_version: { type: integer, required: true, description: "incremented on every change" }
  fields: { type: list, required: true, description: "DAT-FieldDecl items (own fields only)" }
  keys: { type: list, required: false, description: "unique keys: lists of field names" }
  ports: { type: list, required: false, description: "DAT-PortDecl items" }
  lifecycle: { type: reference, required: false, description: "lifecycle object URI" }
  views: { type: list, required: false, description: "view object URIs by mode" }
  knowledge: { type: list, required: false, description: "knowledge object URIs" }
  projections: { type: list, required: false, description: "projection declaration URIs" }
  write_policy: { type: enum, required: false, description: "direct | change_set (default direct for Ledger, change_set for classes in ConfigAsset, else direct)" }
  merge: { type: map, required: false, description: "per-field merge strategies (STD-VER-051)" }
  valid_time: { type: enum, required: false, description: "date | instant (Definition kind; default date)" }
  subject_key: { type: text, required: false, description: "field path that identifies the data subject for personal fields" }
  classification_default: { type: enum, required: false, description: "default classification for fields" }
  overlay_policy: { type: map, required: false, description: "per member: overridable | extend_only | locked" }
  lens: { type: reference, required: false, description: "lens from the previous schema version (STD-CLS-060)" }
  ledger: { type: composite, required: false, description: "Ledger kind only: {key: [field], amounts: [field], double_entry: boolean, period_scope: text}" }
```

### STD-CLS-002 — Field declaration schema
- **Clause:** Each field MUST conform to `DAT-FieldDecl`.
- **Grammar / Schema:** `DAT-FieldDecl`
- **Conformance vectors:** VER-CONF-0530…0559
- **Satisfies:** FR-MODEL-041, FR-MODEL-043, FR-MODEL-047
- **Notes:** —

```yaml
DAT-FieldDecl:
  name: { type: text, required: true, description: "snake_case member name" }
  type: { type: text, required: true, description: "value type name (STD-FND-020) or custom value type URI" }
  required: { type: boolean, required: false, description: "default false" }
  constraints: { type: map, required: false, description: "type-specific constraints" }
  default: { type: json, required: false, description: "literal default or {expr: L1}" }
  computed: { type: text, required: false, description: "L1 expression; computed fields are read-only" }
  materialized: { type: boolean, required: false, description: "store computed value (STD-OBJ-003)" }
  classification: { type: enum, required: false, description: "STD-FND-027 level" }
  lookup: { type: enum, required: false, description: "none | equality (STD-OBJ-012)" }
  label: { type: text, required: true, description: "display label" }
  description: { type: text, required: true, description: "documentation" }
  relationship: { type: composite, required: false, description: "for reference/list-of-reference fields: {target, cardinality, inverse, ownership, on_end, attributes_class}" }
  log_reads: { type: boolean, required: false, description: "access logging (default true for personal levels)" }
```

### STD-CLS-003 — Port declaration schema
- **Clause:** Each port MUST conform to `DAT-PortDecl`. A port's implementation MUST be an L1 expression set, an L2 logic asset or an L3 component.
- **Grammar / Schema:** `DAT-PortDecl`
- **Conformance vectors:** VER-CONF-0560…0579
- **Satisfies:** FR-FLOW-021
- **Notes:** —

```yaml
DAT-PortDecl:
  name: { type: text, required: true, description: "snake_case port name" }
  kind: { type: enum, required: true, description: "query | command" }
  params: { type: list, required: false, description: "DAT-FieldDecl-shaped parameter declarations" }
  result: { type: text, required: false, description: "result type" }
  errors: { type: list, required: false, description: "declared error codes" }
  states: { type: list, required: false, description: "lifecycle states where available (default: all)" }
  transition: { type: text, required: false, description: "lifecycle transition triggered, if any" }
  guards: { type: list, required: false, description: "guard references (L1 or L2)" }
  impl: { type: reference, required: false, description: "logic asset URI; absent for abstract ports" }
  permission: { type: text, required: true, description: "policy action name" }
  idempotent: { type: boolean, required: false, description: "safe to retry without key (query ports are always)" }
  ui: { type: map, required: false, description: "label, icon, confirmation text" }
  description: { type: text, required: true, description: "documentation" }
```

## 2. Inheritance

### STD-CLS-010 — Root and base classes
- **Clause:** The genesis model MUST define `Object` (abstract) and, directly below it, the abstract base classes `DataObject`, `LogicAsset`, `ViewAsset`, `ConfigAsset` and `SystemObject`. Every class MUST have exactly one parent, except `Object`. `LogicAsset`, `ViewAsset` and `ConfigAsset` have the kind `definition`, `SystemObject` the kind `system`; `Object` and `DataObject` have no kind (STD-CLS-014) (DEC-032).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0600…0604
- **Satisfies:** FR-MODEL-021
- **Notes:** —

### STD-CLS-011 — C3 linearization
- **Clause:** The linearization `L(C)` of a class C with parent P and traits T1 … Tn MUST be:

  `L(C) = C + merge(L(T1), …, L(Tn), L(P), [T1, …, Tn, P])`

  `merge` is the standard C3 merge. It repeatedly takes the first head that does not appear in the tail of any list.
  - Traits come before the parent, so trait members override parent members.
  - A class whose merge fails MUST be rejected with `MODEL.LINEARIZATION_INCONSISTENT`.
  - A cycle MUST be rejected with `MODEL.INHERITANCE_CYCLE`.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0605…0629
- **Satisfies:** FR-MODEL-022
- **Notes:** Traits are classes with `abstract: true` and a `trait: true` marker. They have no parent except `Object`, and they declare `applies_to` kinds.

### STD-CLS-012 — Effective definition
- **Clause:** The effective definition of C MUST be computed by folding over `reverse(L(C))` (most general first), then applying extensions in effect on the branch in extension order. Each later declaration of a member refines the earlier one under the narrowing rules (STD-CLS-013). The result is a pure function of the class versions and extension versions involved.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0630…0649
- **Satisfies:** FR-MODEL-025, FR-MODEL-101
- **Notes:** Implementations may cache the result keyed by the content IDs of the inputs (AR-028).

### STD-CLS-013 — Narrowing rules
- **Clause:** A refinement of an inherited member MUST satisfy the matrix below. Any other refinement MUST be rejected with `MODEL.NARROWING_VIOLATION`.

  | Aspect | Allowed refinement |
  |---|---|
  | field type | same type; or a custom value type whose base is the inherited type; or a subclass target for references |
  | `required` | `false` → `true` only |
  | `min`, `min_length`, `min_items` | increase only |
  | `max`, `max_length`, `max_items` | decrease only |
  | `pattern` | add a pattern only when none existed (a combined pattern means both must match) |
  | `allowed_values` | subset only |
  | `scale` | equal only |
  | `classification` | raise only |
  | `default` | may be set or changed |
  | `computed` | may not be added to or removed from an inherited stored field |
  | `label`, `description` | may be changed |
  | port parameters | STD-CLS-021 |
  | lifecycle | add states, transitions, guards and effects only (FR-FLOW-015) |
  | `write_policy` | `direct` → `change_set` only |
  | removal of any member | never |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0650…0699
- **Satisfies:** FR-MODEL-023, FR-MODEL-072
- **Notes:** Substitutability: every valid instance of a subclass is a valid instance of each ancestor.

### STD-CLS-014 — Kind inheritance
- **Clause:** A class's kind MUST equal its parent's kind, except that direct children of `DataObject` MUST declare a concrete kind. Traits MUST declare `applies_to` kinds, and a class MUST NOT include a trait that does not apply to its kind.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0700…0709
- **Satisfies:** FR-MODEL-024
- **Notes:** —

### STD-CLS-015 — Minimum depths
- **Clause:** Implementations MUST support a linearization length of at least 48 (inheritance depth 32 plus 16 traits).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0710…0714
- **Satisfies:** FR-MODEL-026
- **Notes:** —

## 3. Polymorphism

### STD-CLS-020 — Port dispatch
- **Clause:** Invoking port `p` on object `o` of class C MUST execute the implementation declared by the first class in `L(C)`, after applying extensions, that provides an implementation of `p`. When no class provides one, the invocation MUST fail with `MODEL.PORT_NOT_IMPLEMENTED`. `run.super` MUST continue the search from the class after the current one in `L(C)`.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0720…0739
- **Satisfies:** FR-MODEL-031, FR-MODEL-033
- **Notes:** —

### STD-CLS-021 — Port override compatibility
- **Clause:** An override of port `p` MUST:
  - declare the same parameter names;
  - declare each parameter type equal to or more general than the overridden declaration (the reverse of the narrowing matrix);
  - declare a result type equal to or narrower;
  - declare an error-code set that is a subset.

  Otherwise the class commit MUST fail with `MODEL.PORT_SIGNATURE_INCOMPATIBLE`.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0740…0754
- **Satisfies:** FR-MODEL-032
- **Notes:** —

### STD-CLS-022 — Polymorphic queries and selectors
- **Clause:** A class filter in queries and a type selector in governance sheets MUST match every object whose class has the named class in its linearization, unless `only: true` is given. The specificity contribution of a type selector MUST equal the position depth of the named class in the hierarchy (depth of `Object` = 0).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0755…0769
- **Satisfies:** FR-MODEL-034, FR-MODEL-035
- **Notes:** Because traits appear in linearizations, a selector on a trait matches every class that includes it.

### STD-CLS-023 — View resolution
- **Clause:** For a mode `m`, the view of object `o` MUST be the view declared for `m` by the first class in `L(C)` that declares one, refined by context variants (device class, role, locale) when declared. Otherwise it is the generated view of the effective definition.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0770…0779
- **Satisfies:** FR-MODEL-036, FR-UX-012, FR-UX-015
- **Notes:** —

## 4. Extensions

### STD-CLS-030 — Extension objects
- **Clause:** An extension MUST be an object of class `ClassExtension` that names a target class and adds fields, ports, views, constraints and merge strategies under the narrowing rules. Members added by extensions MUST be stored under the key `<namespace>.<member>`. An extension is effective only on branches where it is visible (by commit or inheritance).
- **Grammar / Schema:** `DAT-ClassExtension`
- **Conformance vectors:** VER-CONF-0780…0799
- **Satisfies:** FR-MODEL-071 … FR-MODEL-074
- **Notes:** —

```yaml
DAT-ClassExtension:
  target: { type: reference, required: true, description: "class URI" }
  namespace: { type: text, required: true, description: "owner namespace of the added members" }
  order: { type: integer, required: true, description: "application order among extensions of the same target (ties broken by namespace)" }
  fields: { type: list, required: false, description: "added DAT-FieldDecl items" }
  refinements: { type: list, required: false, description: "narrowing refinements of existing members" }
  ports: { type: list, required: false, description: "added DAT-PortDecl items" }
  views: { type: list, required: false, description: "added or overriding views, subject to overlay_policy" }
```

### STD-CLS-031 — Overlay policy enforcement
- **Clause:** A member declared `locked` in the target's `overlay_policy` MUST NOT be refined by extensions or overridden on child branches. A member declared `extend_only` MUST NOT be overridden, but MAY receive additive refinements (for example extra validations). Violations MUST fail with `TEN.OVERRIDE_LOCKED`.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0800…0809
- **Satisfies:** FR-TEN-033, FR-RULE-023
- **Notes:** —

## 5. Relationships

### STD-CLS-040 — Relationship semantics
- **Clause:** A relationship is declared on a reference or list-of-reference field (`DAT-FieldDecl.relationship`). The implementation MUST:
  1. enforce cardinality on commit;
  2. maintain the inverse for queries and traversal;
  3. enforce `on_end` rules;
  4. for composition, end owned objects when the owner ends.

  When `attributes_class` is declared, each link MUST be an object of that class (Definition kind), with its own versions and valid time, referencing both ends.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0810…0839
- **Satisfies:** FR-MODEL-061, FR-MODEL-062
- **Notes:** —

### STD-CLS-041 — Referential integrity at valid time
- **Clause:** On commit, every reference in a Definition version with period `[f, t)` MUST resolve on the same branch to a target that is not ended for the whole period `[f, t)` as known at the commit. Ledger entries' references MUST resolve at the entry's valid time. Violations MUST fail with `MODEL.REFERENCE_INVALID`.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0840…0859
- **Satisfies:** FR-MODEL-063
- **Notes:** A later retroactive ending of a target does not invalidate committed versions. It is reported by impact analysis instead.

## 6. Schema evolution

### STD-CLS-050 — Change classification
- **Clause:** Every class change MUST be classified as follows. The classification MUST be computed by the implementation, not declared by the author.
  - `additive`: new optional fields, new ports, new states, label or description changes, or narrowing of constraints that all stored current versions satisfy;
  - `lens_required`: renames, type conversions, splits, joins, moves, or state renames and removals;
  - `breaking`: any change for which no total lens exists.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0860…0879
- **Satisfies:** FR-MODEL-081
- **Notes:** Narrowing a constraint that current data violates is `breaking` until the data is migrated.

### STD-CLS-060 — Lens language
- **Clause:** A lens MUST be an object of class `Lens` with a `from` and `to` schema version and an ordered list of operations from the table below, plus an optional L2 residual function for each direction. Each operation defines a forward and a backward mapping. Operations marked one-way make the lens one-way.

  | Operation | Parameters | Forward | Backward |
  |---|---|---|---|
  | `rename` | from, to | move the value | move back |
  | `split` | from, to[], expr | compute parts | join parts (expr) |
  | `join` | from[], to, expr | combine | split (expr) |
  | `convert` | field, from_type, to_type, forward_expr, backward_expr | apply the forward expression | apply the backward expression |
  | `default` | field, value | set when absent | drop if equal to the default |
  | `nest` | fields[], into | move into a composite | flatten |
  | `unnest` | from, fields[] | flatten | nest |
  | `map_enum` | field, mapping | map values | inverse mapping (must be injective, else one-way) |
  | `drop_with_archive` | field | move into `_archived.<field>` | restore from the archive |
- **Grammar / Schema:** `DAT-Lens`
- **Conformance vectors:** VER-CONF-0880…0919
- **Satisfies:** FR-MODEL-082, FR-MODEL-083
- **Notes:** `drop_with_archive` keeps round trips total.

```yaml
DAT-Lens:
  class: { type: reference, required: true, description: "class URI" }
  from: { type: integer, required: true, description: "source schema version" }
  to: { type: integer, required: true, description: "target schema version (from + 1)" }
  ops: { type: list, required: true, description: "ordered operations" }
  residual_forward: { type: reference, required: false, description: "L2 function for cases not covered" }
  residual_backward: { type: reference, required: false, description: "L2 function" }
  one_way: { type: boolean, required: false, description: "true if backward is not total" }
  one_way_reason: { type: text, required: false, description: "required when one_way" }
```

### STD-CLS-061 — Lens application
- **Clause:** Reading a version stored at schema version `s` in schema version `t` MUST apply the lenses `s→s+1→…→t` forward (or backward for `t < s`). The result MUST NOT be stored. Lens application MUST be deterministic.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0920…0939
- **Satisfies:** FR-MODEL-083, FR-MODEL-084, FR-MODEL-085
- **Notes:** —

### STD-CLS-062 — Lens totality check
- **Clause:** Before a class change with a lens becomes effective on a protected branch, the implementation MUST verify round trips `backward(forward(x)) = x` for generated values of every affected field type, and for a sample of stored current versions (at least the lesser of 10,000 and all). It MUST report failures with sample values.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0940…0949
- **Satisfies:** FR-MODEL-086
- **Notes:** —
