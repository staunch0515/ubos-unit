---
id: UBS-STD-13
title: BPA Standard — Logic Governance Sheets and Decisions (LGS)
status: draft
phase: PH-0
depends_on: [UBS-STD-12]
---

# BPA Standard — Logic Governance Sheets and Decisions (LGS) `[Profile: Governance]`

## 1. Sheet language

### STD-LGS-001 — Textual grammar
- **Clause:** Governance sheets MUST conform to the grammar below.
- **Grammar / Schema:**
  ```
  sheet        = *( rule-block )
  rule-block   = selector-list "{" 1*declaration "}"
  selector-list= selector *( "," selector )
  selector     = type-sel *( qualifier )
  type-sel     = class-name / "*"
  qualifier    = "[" expr "]"                         ; L1 boolean over `this`
               / ":" state-name                        ; lifecycle state
               / ":tag(" slug ")"
               / ":ctx(" ctx-cond ")"                  ; branch_kind, channel, role, principal_kind, vae
               / ":rel(" member-name ")" "[" expr "]"  ; one reference hop
  ctx-cond     = ctx-key ( "=" / "!=" ) string / ctx-key "in" "[" string *( "," string ) "]"
  ctx-key      = "branch_kind" / "channel" / "role" / "principal_kind" / "vae"
  declaration  = property ":" value [ "!locked" ] ";"
  property     = property-name [ "." member-name ]    ; the member names one key of a map property
  property-name= member-name *( "." member-name )     ; kernel properties are plain, Buk properties namespaced
  value        = literal / list-lit / map-lit          ; a constant, typed per property schema
  map-lit      = "{" [ member-name ":" value *( "," member-name ":" value ) ] "}"
  comment      = "//" *( %x20-10FFFF ) LF             ; comments and whitespace are not part of the sheet
  ```
- **Conformance vectors:** VER-CONF-4800…4849
- **Satisfies:** FR-RULE-012, FR-RULE-015
- **Notes:** `DATA_TRANSACTION[amount > 10000] { audit_level: "strict"; }` from EXT-RFC maps to `Transaction[amount > 10000] { audit_level: "strict"; }`. Since 1.0-rc2 (DEC-066): `expr` in a `[…]` qualifier is an L1 boolean expression over `this` (STD-EXPR-001) and is type-checked at authoring against the class of the type selector (`*` gives `Object`, so only `ctx` is usable); `literal` and `list-lit` are those of STD-EXPR-001, and a map literal is added for map and composite values, since L1 has none. A syntax error is `RULE.PARSE_ERROR` with the position. A property path with a trailing member names one key of a map-typed property (`field_visibility.ssn`); the cascade runs per property path, so different keys never conflict. The canonical form of a sheet is one block per paragraph, selectors joined by `, `, one declaration per line indented by two spaces, values printed as literals; `source` holds this form (STD-LGS-002).

### STD-LGS-002 — Sheet object
- **Clause:** A sheet MUST be an object of class `GovernanceSheet` (`DAT-Sheet`), a genesis Definition class below `ConfigAsset`. Its `source` in textual form and its `ast` in JSON form MUST round-trip losslessly: the implementation derives `ast` from `source` at commit, stores `source` in its canonical form, and rejects an `ast` given with the source that differs from the derived one (`MODEL.INVALID_VALUE`). A sheet whose source does not parse (`RULE.PARSE_ERROR`), declares an unknown property (`RULE.UNKNOWN_PROPERTY`) or fails the checks of STD-LGS-020 and STD-LGS-022 MUST be rejected at commit.
- **Grammar / Schema:** `DAT-Sheet`
- **Conformance vectors:** VER-CONF-4850…4864
- **Satisfies:** FR-RULE-011, FR-RULE-061, FR-RULE-063
- **Notes:** Since 1.0-rc2 (DEC-066). The AST is `{blocks: [{selectors: [{type, qualifiers?: [{kind: attr | state | tag | ctx | rel, …}]}], declarations: [{property, value, locked?}]}]}`; qualifiers carry `expr` (text), `state`, `tag`, `key`, `op` and `value`, or `member` and `expr`; expressions stay text. `scope` is `{vaes?, branch_kinds?, classes?}`, each a list, an absent list meaning any (STD-LGS-020). As a `ConfigAsset` class, sheets on protected branches are written through change sets (STD-VER-012).

```yaml
DAT-Sheet:
  name: { type: text, required: true, description: "namespaced name" }
  layer: { type: enum, required: true, description: "platform | industry | vendor | tenant | vae | unit | object" }
  priority: { type: integer, required: false, description: "default 0; higher wins among equals" }
  scope: { type: composite, required: false, description: "{vaes, branch_kinds, classes}" }
  source: { type: text, required: true, description: "textual sheet" }
  ast: { type: json, required: true, description: "parsed form" }
  cascading: { type: boolean, required: false, description: "applies in child VAEs at this layer (FR-TEN-042)" }
  tests: { type: list, required: false, description: "rule test URIs" }
  description: { type: text, required: true, description: "documentation" }
```

## 2. Properties

### STD-LGS-010 — Property registry
- **Clause:** Every declared property MUST be registered (`DAT-PropertyDecl`) with:
  - a value schema and an enforcement point;
  - a combination rule, used when a property is declared by several winning blocks of different selectors at the same rank, or explicitly marked combinable;
  - the owner.

  Declarations of unknown properties MUST be rejected (`RULE.UNKNOWN_PROPERTY`). The registry at a coordinate is the kernel properties (STD-LGS-011) plus the live objects of the genesis class `PropertyDecl` (`DAT-PropertyDecl`, below `ConfigAsset`), which register Buk properties; a Buk property name MUST be namespaced and MUST NOT shadow a kernel property. A declared value MUST validate against the property's schema (`MODEL.INVALID_VALUE`, `MODEL.CONSTRAINT_VIOLATED`).
- **Grammar / Schema:** `DAT-PropertyDecl`
- **Conformance vectors:** VER-CONF-4865…4879
- **Satisfies:** FR-RULE-013, FR-RULE-014
- **Notes:** Since 1.0-rc2 (DEC-066): `schema` is a field declaration `{type, constraints}` (STD-FND-020); a property of type `map` takes keyed declarations (STD-LGS-001). A combination rule other than `override` combines every matching candidate (STD-LGS-022).

```yaml
DAT-PropertyDecl:
  name: { type: text, required: true, description: "property path" }
  schema: { type: json, required: true, description: "field declaration {type, constraints} of the value" }
  enforcement: { type: enum, required: true, description: "commit | action | read | view | retention | notify" }
  combine: { type: enum, required: true, description: "override | max | min | union | all_true | any_true" }
  owner: { type: text, required: true, description: "kernel or Buk namespace" }
  handler: { type: reference, required: false, description: "L1/L2 handler for Buk properties" }
  description: { type: text, required: true, description: "documentation" }
```

### STD-LGS-011 — Kernel properties (ABI 1.0)
- **Clause:** Implementations MUST provide these properties with the stated semantics:

  | Property | Schema | Enforcement | Combine | Semantics |
  |---|---|---|---|---|
  | `audit_level` | enum `basic`, `standard`, `strict` | commit | override | `strict`: read logging on, commit message required, reason code required |
  | `require_signature` | boolean | action | override | command ports require a signature step (`require: signature`) |
  | `approvals` | integer or composite `{count, roles, order}` | action | max (by `count`) | approval requirement (STD-TXN-060) |
  | `maker_checker` | boolean | action | override | the initiator cannot approve |
  | `read_only` | boolean | commit | override | changes to matched objects are rejected |
  | `field_required` | list of fields | commit | union | additional required fields |
  | `field_default` | map | commit | override per key | defaults applied on create |
  | `field_visibility` | map field → `visible`, `hidden`, `read_only` | view | override per key | view payload flags |
  | `mask` | map field → `hide`, `partial`, `hash` | read | override per key | masking (FR-IAM-061) |
  | `retention` | composite `{min, max, trigger}` | retention | override | FR-AUD-071 |
  | `classification` | map field → level (raise only) | commit | max per key (raise only) | FR-MODEL-047 |
  | `notify` | list of notification rules | notify | union | FR-NTF-011 |
  | `write_policy` | `direct` or `change_set` (raise only) | commit | max (`direct` < `change_set`, raise only) | FR-VER-024 |
  | `rate_limit` | composite `{per, count}` | action | override | per principal and port |
  | `validations` | list of `{name, rule, severity, message}` | commit | union | validations of the matched objects (STD-LGS-030); since 1.0-rc2 |
  | `invariants` | list of `{name, scope, rule, severity, message}` | commit | union | invariants over the objects of the scope classes (STD-LGS-030); since 1.0-rc2 |
  | `decisions` | list of `{name, when, outcome, steps, message}` | action | union | decisions before a command port (STD-LGS-031); since 1.0-rc2 |
  | `access` | list of `{action, allow_if}` | action | union | relation-based access rules (STD-LGS-051); since 1.0-rc8 |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-4880…4939
- **Satisfies:** FR-RULE-013, SCN-009, SCN-012
- **Notes:** Since 1.0-rc2 (DEC-066): `approvals` accepts an integer as the shorthand of `{count}`; `retention` members are ISO 8601 durations and a trigger name; `notify` items are JSON rules. The vectors of this clause cover the value schemas and the combination rules; the enforcement of each property is verified where its enforcement point is implemented: the `commit` properties and the `action` properties `approvals`, `require_signature` and `maker_checker` with STD-LGS-030 and STD-LGS-031 (PH-1 step 10c, DEC-067), `rate_limit` with the Server of step 14 (it counts calls per principal and port), `read` and `view` properties with the queries and views of step 13, `retention` and `notify` in PH-2. Commit enforcement (DEC-067): `read_only` rejects an update or end of a matched object and `field_required` a matched object without the field, both with `RULE.DECLARATION_UNSATISFIED` naming the property; `field_default` fills the absent fields of a created object before validation; `audit_level: strict` requires the commit's process to carry `message` and `reason` (STD-TXN-001); `write_policy: change_set` requires a change set on a protected branch like the class policy (`VER.CHANGE_SET_REQUIRED`); `classification` raises the effective classification of the field. The objects of `ConfigAsset` and `LogicAsset` classes (sheets, extensions, logic) are not subject to the declarations they carry: write policy and change sets govern them.

## 3. Cascade

### STD-LGS-020 — Matching
- **Clause:** A block matches object o in context c if any selector in its list matches:
  - the type selector matches o's linearization (STD-CLS-022);
  - every qualifier evaluates to `true` on o (and on the referenced object for `:rel`);
  - the sheet's scope includes c;
  - the sheet is effective at `(asof, time)` (FR-RULE-061).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-4940…4969
- **Satisfies:** FR-RULE-012
- **Notes:** Since 1.0-rc2 (DEC-066). The context is `{vae, branch_kind, channel, role, principal_kind}`; an absent key matches no `=` or `in` condition; `branch_kind` defaults to the kind of the branch evaluated. A sheet's `scope` lists are each optional and mean any when absent; `classes` applies when it intersects the object's linearization. A sheet is effective at a coordinate when the sheet object is live there, as a read of it would be (STD-TIME-013): its valid time covers `asof` and it exists at `time`. `:state` matches the object's `state` member (a field named `state`, or the lifecycle state once the class has a lifecycle); `:tag(x)` matches when the object's `tags` member (a list of texts) contains `x`; `:rel(m)[e]` resolves the reference member `m` at the same coordinate and evaluates `e` on the referenced object, any of them for a list, and does not match an empty or dangling reference. A qualifier whose evaluation fails (a null member, a type error at run time) does not match. At authoring, a qualifier is type-checked against the class of its type selector (`LOGIC.TYPE_ERROR`), `:rel` needs a reference member (`MODEL.INVALID_VALUE`), and an attribute or relation qualifier on a `RawDocument` class is `RULE.UNTYPED_ACCESS`.

### STD-LGS-021 — Specificity
- **Clause:** The specificity of a selector MUST be the tuple `(d, a, s, x)`, compared lexicographically:
  - `d` = depth of the type-selector class (0 for `*`);
  - `a` = number of attribute (`[…]`) and `:rel` qualifiers;
  - `s` = number of state and tag qualifiers;
  - `x` = number of `:ctx` qualifiers.

  The specificity of a block for an object MUST be that of its most specific matching selector.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-4970…4984
- **Satisfies:** FR-RULE-021, FR-MODEL-035
- **Notes:** Since 1.0-rc2 (DEC-066): `d` is the depth of STD-CLS-022 (`Object` 0, one more per parent step, a trait 1, `*` 0); the adapter operation `specificity` returns the tuple of one selector.

### STD-LGS-022 — Cascade order and ambiguity
- **Clause:** For each property, the winning declaration MUST be the one with the highest rank, where rank compares in order:
  1. locked flag (a locked declaration from a lower layer beats any unlocked declaration from a higher layer);
  2. layer (`platform` < `industry` < `vendor` < `tenant` < `vae` < `unit` < `object`);
  3. specificity;
  4. sheet priority;
  5. position (later block wins, then later declaration).

  Two candidates of equal rank from different sheets with different values MUST be rejected at authoring (`RULE.AMBIGUOUS_CASCADE`), unless the property's `combine` rule is not `override`, in which case the values of every matching candidate are combined. A higher-layer declaration that tries to override a locked one MUST be rejected at authoring (`RULE.LOCKED_OVERRIDE`).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-4985…5024
- **Satisfies:** FR-RULE-021, FR-RULE-023, FR-TEN-042
- **Notes:** Authoring-time ambiguity detection covers selectors that can match the same object. Implementations use a conservative overlap analysis. Since 1.0-rc2 (DEC-066): among locked declarations the lowest layer wins and among unlocked declarations the highest layer wins; position compares within one sheet only (a later block, then a later declaration). Equal-rank candidates from different sheets with the same value are not ambiguous. The checks run when a sheet or property declaration is committed, over the live sheets of the branch: two selectors may overlap unless their type selectors are disjoint (two non-trait classes, neither in the other's linearization, with no concrete class carrying both), or their `:state` qualifiers name different states, or a `:ctx` condition on the same key with `=` or `in` admits no common value; attribute, tag and relation qualifiers are assumed to overlap. The commit that creates an overlap of a locked declaration with a declaration of the same property path in a higher layer fails with `RULE.LOCKED_OVERRIDE`, whichever sheet it writes; an ambiguity met at evaluation that the analysis did not detect is `RULE.AMBIGUOUS_CASCADE` at evaluation. Since 1.0-rc7 (DEC-084): the checks also run on the sheets and property declarations a merge or a revert changes in the target, over the live sheets of the merged state, and fail the merge or revert with the same errors; `merge_preview` reports them among its validation results (STD-VER-055, -057).

### STD-LGS-023 — Explanation object
- **Clause:** For any object and property, the implementation MUST produce `DAT-CascadeExplanation`: the matching candidates in rank order, each with `won` and a `reason` among `won`, `combined`, `same_value`, `locked_elsewhere`, `lower_layer`, `less_specific`, `lower_priority` and `earlier_position`.
- **Grammar / Schema:** `DAT-CascadeExplanation`
- **Conformance vectors:** VER-CONF-5025…5034
- **Satisfies:** FR-RULE-022
- **Notes:** Since 1.0-rc2 (DEC-066): `object` is the canonical URI and `version_hash` the version explained; `sheet` is the sheet's URI and `sheet_version` its version hash; `effective` is `null` without candidates. The adapter operations are `cascade` (every effective property path with the sheet versions bound) and `explain_cascade` (STD-CONF-005).

```yaml
DAT-CascadeExplanation:
  object: { type: text, required: true, description: "canonical URI of the object" }
  version_hash: { type: text, required: true, description: "version explained" }
  property: { type: text, required: true, description: "property path" }
  effective: { type: json, required: true, description: "effective value" }
  candidates: { type: list, required: true, description: "{sheet, sheet_version, block, selector, layer, specificity, priority, locked, value, won: boolean, reason}" }
```

## 4. Validations and decisions

### STD-LGS-030 — Validations and invariants
- **Clause:** Validations (object-scoped) and invariants (query-scoped with declared scope classes) MUST be evaluated at commit for the objects changed. Invariants MUST be evaluated for every object in their declared scope affected by the commit. Each evaluation yields `ok`, or a violation with severity and explanation. All error-severity violations up to the maximum MUST be reported together (`RULE.VALIDATION_FAILED` with `violations`); warning and info violations are returned with the commit result as `warnings`. Validations are declared by the `validations` property of sheets (STD-LGS-011) and by the `validations` member of class declarations and extensions; invariants by the `invariants` property. A rule is an L1 boolean over `this` and `old`; a rule that cannot be evaluated counts as violated.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5035…5064
- **Satisfies:** FR-RULE-031 … FR-RULE-034, FR-LEDG-034
- **Notes:** Since 1.0-rc2 (DEC-067). The validations of an object are those of its effective class (declarations and extensions along the linearization) plus the effective `validations` of its cascade in the commit's context (STD-LGS-020: `branch_kind`, `channel`, `principal_kind`, `role` of the process); `old` is the base version of the object, with every member null on a create. An invariant is evaluated for each object written by the commit whose linearization meets its `scope`, with `this` bound to that object; invariants over ledger balances (FR-LEDG-034) need the balance functions of the synchronous projections and follow with the kernel of PH-1 step 11. The maximum of reported violations is `set_limits.max_violations` (default 100). A violation item is `{code, object_id, name, severity, message}` with `code` `RULE.VALIDATION_FAILED` or `RULE.INVARIANT_VIOLATED`. The preview of FR-RULE-035 is the adapter operation `validate` and the instruction `run.validate` (STD-ISA-054): the same evaluation without a commit. Since 1.0-rc7 (DEC-084): the `sheets` of the commit's rule binding list the versions of the sheets that apply to the objects the commit enforcement evaluated (the sheet versions `cascade` binds), in object-identifier order of the sheets (STD-TIME-040).

### STD-LGS-031 — Decision evaluation and combination
- **Clause:** Before a command port executes, the implementation MUST evaluate all applicable decisions (action-enforcement properties and decision objects). Each yields `allow`, `deny(explanations)` or `require(steps)`. Decisions combine as follows:
  - any `deny` gives `deny`;
  - otherwise the union of the steps gives `require`;
  - otherwise `allow`.

  Decisions apply identically to all principal kinds, and MAY condition on `principal_kind`. Decisions are declared by the `decisions` property of sheets (STD-LGS-011): each item has an optional `when` (an L1 boolean over `this`, `params` and `ctx`), an `outcome` and, for `require`, its `steps`; the action properties `approvals`, `require_signature` and `maker_checker` require the steps `approval`, `signature` and `second_person`. A `deny` outcome fails the port call with `RULE.DECISION_DENIED` and a `require` outcome with `RULE.DECISION_REQUIRES`, both carrying the `DAT-DecisionResult`; query ports are not subject to decisions.
- **Grammar / Schema:** `DAT-DecisionResult`
- **Conformance vectors:** VER-CONF-5065…5089
- **Satisfies:** FR-RULE-051, FR-RULE-052, FR-RULE-053
- **Notes:** Since 1.0-rc2 (DEC-067). The context of a decision is `{principal_kind, channel, vae, branch_kind, role?}` (`role` is an optional member of the execution context since 1.0-rc2, STD-CTX-001); `explanations` lists the decisions that fired as `{name, outcome, message, steps}`, with `value` for an action property; `steps` is present for `require` in the order collected. The adapter operation `decide` evaluates the decisions of a port call without running it; `execute` evaluates them before a command port and before `run.port` (DSN-DVM-720). The deferred commit that a `require` outcome leads to, and the approval objects, belong to PH-2 (DSN-DVM-740); decision tables (STD-LGS-040) too.

```yaml
DAT-DecisionResult:
  outcome: { type: enum, required: true, description: "allow | deny | require" }
  steps: { type: list, required: false, description: "approval | signature | reason | step_up | second_person" }
  explanations: { type: list, required: true, description: "explanation objects" }
  bindings: { type: list, required: true, description: "content-ids of sheets and decision objects evaluated" }
```

### STD-LGS-040 — Decision tables
- **Clause:** Decision tables MUST declare typed inputs, outputs, rows (L1 conditions per input column, or `-` for any) and a hit policy (`unique`, `first`, `priority`, `collect` with an aggregator). Authoring MUST report gaps and hit-policy overlaps for enumerable and range inputs (FR-RULE-042).
- **Grammar / Schema:** `DAT-DecisionTable`
- **Conformance vectors:** VER-CONF-5090…5129
- **Satisfies:** FR-RULE-041, FR-RULE-042
- **Notes:** Since 1.0-rc8 (DEC-091):
  - **Storage.** A decision table is an object of the genesis Definition class `DecisionTable` below `ConfigAsset`; it is evaluated in its version valid at `asof`.
  - **Conditions.** A condition cell is an L1 boolean over `params`, the inputs, which are validated against their declared types (`MODEL.INVALID_VALUE`). A missing cell or `-` matches any value. Every row gives every output.
  - **Hit policies.**
    - `unique`: exactly one row matches; none fails with `RULE.NO_MATCH`, several with `RULE.MULTIPLE_MATCH`.
    - `first`: the first matching row in row order.
    - `priority`: the matching row with the highest `priority`; the first of equal priorities.
    - `collect`: the outputs of every matching row, in row order. The aggregator `sum`, `min` or `max` reduces each numeric output, and `count` gives `{count}`. An aggregator with another policy is `MODEL.INVALID_VALUE`.
  - **Result.** `{outputs, rows}`, where `rows` lists the indexes of the rows that decided.
  - **Authoring check.** It evaluates the rows on the representative values of each input:
    - for `boolean` and `enum` inputs, the domain;
    - for `integer`, `decimal` and `date` inputs, every literal the conditions compare the input with, the values one unit (1, 0.01 or one day) below and above it, and the midpoints between them.

    It reports, for at most 10,000 combinations:
    - `gaps`: combinations no row matches (not for `collect`);
    - `overlaps`: combinations several rows match for `unique`, or several rows of the top priority match for `priority`.

    Other inputs, or more combinations, are reported as `unchecked`.

```yaml
DAT-DecisionTable:
  name: { type: text, required: true, description: "table name" }
  inputs: { type: list, required: true, description: "{name, type, constraints} items" }
  outputs: { type: list, required: true, description: "{name, type, constraints} items" }
  rows: { type: list, required: true, description: "{when: {input: L1 condition | -}, then: {output: value}, priority?} items" }
  hit_policy: { type: enum, required: true, description: "unique | first | priority | collect" }
  aggregator: { type: enum, required: false, description: "sum | min | max | count (collect only)" }
  description: { type: text, required: true, description: "description" }
```

### STD-LGS-041 — Approval requirements
- **Clause:** The steps of a `require` outcome (STD-LGS-031) MUST become the requirements of the pending change (STD-TXN-060). The kernel property `approvals` specifies the approval step:
  - `count`, the number of approvals;
  - `roles` or `principals`, the eligible approvers;
  - `order`: `any`, or `sequence`, in which the named principals approve in their listed order;
  - `incompatible` duties (STD-LGS-042);
  - `expires_in`, a duration after which the pending change expires.

  An `approval` step of a decision without the property requires one approval. Eligibility:
  - an approver who is neither a named principal nor holds an eligible role is denied (`IAM.DENIED`, details `{approval: not_eligible}`);
  - an approver out of sequence is denied (details `{approval: out_of_sequence}`).

  The change is applied when every step is met:
  - `approval` by `count` approvals;
  - `second_person` by one approval;
  - `signature` by one approval that carries a signature.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5130…5149
- **Satisfies:** FR-FLOW-041, DSN-DVM-740
- **Notes:** Each requirement is `{step, object_id, port}` with `count`, `order`, `roles`, `principals`, `incompatible` and `expires_in` for `approval`. Since 1.0-rc8 (DEC-091).

### STD-LGS-042 — Separation of duties
- **Clause:** An approval MUST be denied (`IAM.DENIED` with `details.sod`) when one of these holds for the approver or the principal named in `on_behalf_of`:
  - it initiated the pending change (`maker_checker`);
  - it already decided on the pending change (`one_decision_per_principal`);
  - it initiated a committed call of a port the requirement lists as `incompatible` on the same object (`incompatible_duty`, with the `duty`).

  These checks apply to every approval of the chain, and through delegations.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5150…5164
- **Satisfies:** FR-FLOW-043
- **Notes:** The incompatible-duty matrix is declared per action through `approvals.incompatible`; organisation-wide matrices are sheets that set it on the classes concerned. Since 1.0-rc8 (DEC-091).

### STD-LGS-043 — Approval evidence
- **Clause:** Every approval or rejection MUST be recorded as a `DAT-Approval` item with:
  - the approver, `on_behalf_of` when delegated, and the decision;
  - the preview it decided on, the time and the authentication level;
  - the comment and the signature when given.

  Change-set approvals (STD-VER-021) and pending-change approvals MUST use the same item. A pending change's approvals become void when it is invalidated (STD-TXN-060), and the voided items stay visible. The commit that applies a pending change MUST reference the pending change, its preview and the approvers.
- **Grammar / Schema:** `DAT-Approval`
- **Conformance vectors:** VER-CONF-5165…5179
- **Satisfies:** FR-FLOW-044, FR-FLOW-045, FR-FLOW-046, DSN-DVM-742
- **Notes:** The default authentication level of the adapter is `basic`. A pending-change approval also records its `step`. Change-set approvals record `auth_level` and `comment` when given. Since 1.0-rc8 (DEC-091).

### STD-LGS-044 — Simulation of rule changes
- **Clause:** A simulation MUST:
  - select the historical port calls of a target branch in a window and by an optional port, in commit order;
  - re-run each, up to a budget of processes, in the `simulation` profile on a sandbox branch created from the target as of just before the call's commit, with the candidate rules of the source branch: the objects of `GovernanceSheet`, `PropertyDecl` and `DecisionTable` on it;
  - compare each simulated outcome (`committed` with the result hash, or `failed` with the error code) with the actual one;
  - report `changed` per call.

  Effects of the re-runs MUST NOT be released. When the budget ends before the selection, the result MUST state the coverage: the calls covered, those selected, `complete` and the dates covered. Every run MUST be stored as an object of the genesis System class `SimulationRun` with:
  - the source, the target and the window;
  - the budget, the coverage and the results;
  - the versions of the candidate rules.

  A run given a change set MUST be linked from the change set's `simulations`.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5180…5199
- **Satisfies:** FR-RULE-071, FR-RULE-072, FR-RULE-073, DSN-DVM-760, DSN-DVM-761, DSN-DVM-762
- **Notes:** The budget counts processes; CPU and time budgets are the node's (DSN-DVM-762). The sandbox branches stay as evidence. Since 1.0-rc8 (DEC-091).

## 6. Relations, access and masking

### STD-LGS-050 — Authorization relations
- **Clause:** A reference field, or a list of references, MAY declare `authz_relation: <name>`. The authorization relations of an object MUST be derived from its version at the snapshot of the request, with one tuple `(object, relation, target)` per referenced object; they MUST NOT be kept in a separate store, so a commit that changes a reference changes the relation at that commit. A class MAY declare `principal_field`, a text field holding the DID of the principal an object stands for.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5310…5329
- **Satisfies:** FR-IAM-051, DSN-DVM-810
- **Notes:** Since 1.0-rc8 (DEC-093):
  - **Declarations.** `authz_relation` on another type fails with `MODEL.INVALID_VALUE`, and so does a `principal_field` that names no text field.
  - **Operation.** The adapter operation `relations {branch, object_id}` returns `{object_id, relations, principal}`, with the targets of each relation as object IDs in field and list order. It takes the snapshot selectors.

### STD-LGS-051 — Relation-based access rules
- **Clause:** The kernel property `access` MUST hold rules `{action, allow_if}` where `allow_if` is a relation path `principal in this(.relation)*` of at most four relations; a longer or malformed path MUST fail with `RULE.PARSE_ERROR` when the sheet is committed. When rules for an action (or `*`) apply to an object, the action MUST be allowed only if a rule's path, followed from the object at the snapshot of the request, reaches an object whose `principal_field` holds the principal. A refused read MUST fail with `IAM.NOT_FOUND`, and a refused port call with `IAM.DENIED`. Actions without rules are not restricted by relations.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5330…5354
- **Satisfies:** FR-IAM-052, FR-IAM-053, FR-IAM-062, DSN-DVM-811, DSN-DVM-812
- **Notes:** Since 1.0-rc8 (DEC-093):
  - **Actions.** The action of a read is `read`; the action of a port call is the port name.
  - **Combination.** `access` combines by union across the cascade, so a rule of any matching sheet can allow.
  - **Operations.** `check_access {branch, object_id, principal, action, ctx}` returns `{allowed, rule}` with the rule that allowed. A `read` or `query` given `as: {principal, clearances, ctx}` is made as that principal: the read is refused when no rule allows it, and a query leaves out, inside its execution, the objects the principal may not read, so pages and counts agree (FR-IAM-062).
  - **Consistency.** The rules and the relations are evaluated at the snapshot of the request, so a selector of an earlier commit sees the relations of that commit (FR-IAM-053).

### STD-LGS-052 — Field masking
- **Clause:** Reads made as a viewer MUST pass every body through masking after authorization:
  - a field with a declared `mask` (STD-LGS-011) is shown in its mode: `hide` removes it, `partial` keeps the last four characters of a text and replaces the others with `*`, `hash` gives `hmac-sha256:` and the keyed hash of STD-OBJ-012 under the tenant lookup key;
  - a field whose classification the viewer's clearances do not cover (STD-FND-027) is hidden unless a mask declares another mode.

  The same masking MUST apply to query results, to event payloads delivered to the viewer and to every other output that carries bodies. A query made as a viewer that filters or sorts on a field masked for the viewer MUST fail with `QRY.INVALID_QUERY`.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5355…5384
- **Satisfies:** FR-IAM-061, FR-IAM-063, DSN-DVM-821, DSN-DVM-822
- **Notes:** Since 1.0-rc8 (DEC-093):
  - **Viewer.** A viewer is `{principal, clearances, ctx}`; clearances default to `internal`, and `ctx` selects masks with context qualifiers. A read without a viewer is the adapter's system read and is not masked.
  - **Modes.** `partial` on a value that is not a text hides it, and so does `hash` without a tenant lookup key (adapter operation `lookup_key`). An erased marker (STD-OBJ-011) is masked like a value.
  - **Events.** `outbox` given `as` masks each payload by the masks and classifications of the effect's object.
