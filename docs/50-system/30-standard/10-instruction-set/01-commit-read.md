---
id: UBS-STD-10-01
title: BPA Standard — Instructions commit.* and read.*
status: draft
phase: PH-0
depends_on: [UBS-STD-10]
---

# Instructions `commit.*` and `read.*`

All `commit.*` instructions add changes to the process's unit of work. They take effect at
flush (STD-TXN-004). Targets are canonical URIs or object IDs within the process's VAE. The
branch is the process branch unless a `branch` parameter is allowed and authorised.

## 1. `commit.*`

### STD-ISA-010 — `commit.create`
- **Clause:** Creates a new object.

  | Aspect | Value |
  |---|---|
  | Params | `class: uri`, `fields: map`, `slug: text?`, `valid: period?` (Definition), `object_id: uuid?` (import only) |
  | Result | `{object_id, version}` (the version is provisional until flush) |
  | Errors | `MODEL.CONSTRAINT_VIOLATED`, `MODEL.KEY_CONFLICT`, `MODEL.ABSTRACT_INSTANTIATION`, `MODEL.REFERENCE_INVALID`, `VER.CHANGE_SET_REQUIRED`, `IAM.DENIED` |
  | Determinism | `snapshot` (the object ID comes from the journaled entropy of the context) |
  | Cost | 50 + 1 per field |
  | Authorization | `create` on the class |

  Validation MAY be deferred to flush, but MUST be complete before commit.

  The object ID is drawn from the process entropy (STD-CTX-010) and the slug defaults to it; `version` is `null` until flush. Values written by logic carry a derivation record (STD-TXN-030) naming the asset version and the versions read (DEC-044).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3100…3119
- **Satisfies:** FR-MODEL-011, FR-MODEL-043, FR-TIME-021
- **Notes:** —

### STD-ISA-011 — `commit.update`
- **Clause:** Updates fields of an existing Definition or RawDocument object.

  | Aspect | Value |
  |---|---|
  | Params | `target: uri`, `set: map` (absent = unchanged, `null` = clear), `base: version?` (default: the version read in this process; required if not read), `timeline: {op: supersede_from \| set_period \| replace_future, from, to?}?` |
  | Result | `{version}` |
  | Errors | as for `commit.create`, plus `TXN.HEAD_MOVED` (at flush), `TIME.OVERLAPPING_VALIDITY`, `FLOW.STATE_DIRECT_WRITE`, `LEDG.IMMUTABLE_ENTRY` |
  | Determinism | `snapshot` |
  | Cost | 40 + 1 per field |
  | Authorization | `update` on the object, and field-level write permissions |

  Changes to an object created or changed earlier in the process are merged into that change (DEC-044).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3120…3149
- **Satisfies:** FR-TXN-021, FR-TIME-022, FR-FLOW-012
- **Notes:** —

### STD-ISA-012 — `commit.end`
- **Clause:** Ends an object from a valid date (STD-OBJ-005).

  | Aspect | Value |
  |---|---|
  | Params | `target: uri`, `from: date-or-instant?` (default: now), `base: version?`, `reason: text?` |
  | Result | `{version}` |
  | Errors | `MODEL.REFERENCE_RESTRICTED`, `TXN.HEAD_MOVED`, `IAM.DENIED` |
  | Determinism | `snapshot` |
  | Cost | 40 plus referential-rule work |
  | Authorization | `end` on the object |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3150…3164
- **Satisfies:** FR-MODEL-045
- **Notes:** —

### STD-ISA-013 — `commit.move`
- **Clause:** Changes the slug of an object (STD-OBJ-004).

  | Aspect | Value |
  |---|---|
  | Params | `target: uri`, `slug: text`, `base: version?` |
  | Result | `{version}` |
  | Errors | `MODEL.KEY_CONFLICT`, `TXN.HEAD_MOVED` |
  | Determinism | `snapshot` |
  | Cost | 40 |
  | Authorization | `update` on the object |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3165…3169
- **Satisfies:** FR-MODEL-014
- **Notes:** —

### STD-ISA-014 — `commit.append` `[Profile: Ledger]`
- **Clause:** Appends a ledger entry.

  | Aspect | Value |
  |---|---|
  | Params | `class: uri`, `ledger_key: map`, `fields: map`, `valid_at: date-or-instant?` (default: now), `posting_group: text?`, `source_refs: list<uri>?` |
  | Result | `{entry_id}` (the sequence is assigned at flush) |
  | Errors | `MODEL.CONSTRAINT_VIOLATED`, `LEDG.PERIOD_CLOSED`, `LEDG.UNBALANCED` (at flush), `RULE.INVARIANT_VIOLATED` (at flush) |
  | Determinism | `snapshot` |
  | Cost | 30 + 1 per field |
  | Authorization | `append` on the ledger class for the ledger key |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3170…3194
- **Satisfies:** FR-LEDG-012, FR-LEDG-013, FR-LEDG-051
- **Notes:** —

### STD-ISA-015 — `commit.reverse` and `commit.adjust` `[Profile: Ledger]`
- **Clause:**
  - `commit.reverse(entry: id, reason: text, valid_at?)` appends a reversal (STD-KIND-023).
  - `commit.adjust(corrects: list<id>, fields: map, valid_at?, reason: text)` appends an adjustment.

  | Aspect | Value |
  |---|---|
  | Errors | `LEDG.ALREADY_REVERSED`, `LEDG.REVERSAL_NOT_REVERSIBLE`, `LEDG.PERIOD_CLOSED`, `IAM.DENIED` |
  | Determinism | `snapshot` |
  | Cost | 40 |
  | Authorization | `correct` on the ledger class |

  In BPA 1.0-rc1 an adjustment corrects exactly one entry (DEC-044).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3195…3214
- **Satisfies:** FR-LEDG-021, FR-LEDG-022, FR-LEDG-023
- **Notes:** —

### STD-ISA-016 — `commit.link` and `commit.unlink`
- **Clause:** Creates or ends a relationship link. For relationships with an attributes class, this creates or ends the link object with valid time.

  | Aspect | Value |
  |---|---|
  | Params | `from: uri`, `relationship: text`, `to: uri`, `attributes: map?`, `valid: period?` |
  | Result | `{link: uri?}` |
  | Errors | `MODEL.CARDINALITY_VIOLATED` (at flush), `MODEL.REFERENCE_INVALID` |
  | Determinism | `snapshot` |
  | Cost | 40 |
  | Authorization | `update` on the source object |

  BPA 1.0-rc1 covers relationships without an attributes class; the link objects of attributes classes follow in 1.0-rc2 (DEC-044).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3215…3229
- **Satisfies:** FR-MODEL-061, FR-MODEL-062
- **Notes:** —

### STD-ISA-017 — `commit.transition`
- **Clause:** Executes a lifecycle transition as the effect of the current port. It is only callable from port logic whose declaration names the transition.

  | Aspect | Value |
  |---|---|
  | Params | `target: uri`, `transition: text` |
  | Result | `{from, to}` |
  | Errors | `FLOW.TRANSITION_NOT_ALLOWED`, `FLOW.GUARD_FAILED`, `FLOW.EFFECT_FAILED`, `FLOW.FIELD_RULE_VIOLATED` |
  | Determinism | `snapshot` |
  | Cost | 30 plus guards and effects |
  | Authorization | the port's permission (already checked) |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3230…3244
- **Satisfies:** FR-FLOW-011, FR-FLOW-012, FR-FLOW-013
- **Notes:** **Phase note:** PH-1 step 10c (DEC-067): the instruction is in BPA 1.0-rc2; its vectors are written with the lifecycles of step 12, which implement it. Since 1.0-rc3 (DEC-071): the target is an object the process has read (as `commit.update`); the transition is looked up in the lifecycle in force for the object's class (STD-CLS-004) and is allowed when the calling port names it in `transition` and the object's current state is among the states it leaves, else `FLOW.TRANSITION_NOT_ALLOWED` with `{transition, from}`; then every guard of the transition is evaluated with `this` the object (its pending changes applied), `params` the port's parameters and `ctx` the decision context, and when any fails the instruction raises `FLOW.GUARD_FAILED` with `{transition, failed: [{name, message}]}` listing every failing guard and nothing is staged; then the exit effects of the state left, the state change and the entry effects of the state entered run in that order, each `set` assigning the value of its expression to the field and each `logic` effect running its function in the same process (a failing effect is `FLOW.EFFECT_FAILED` with `{state, effect, code}` and the transition stages nothing); then the required fields of the entered state must be present (`FLOW.FIELD_RULE_VIOLATED`). The staged change carries the new state and the fields the effects set; the commit records the guards evaluated in the rule binding (STD-TIME-040 `guards`: `{guard: "<transition>/<name>", result: true}`).

## 2. `read.*`

### STD-ISA-030 — `read.get`
- **Clause:** Reads an object at the process snapshot, or at explicit coordinates.

  | Aspect | Value |
  |---|---|
  | Params | `target: uri` (selectors allowed: `commit`, `tag`, `time`, `asof`, `branch`), `fields: list<text>?`, `schema_version: integer?` |
  | Result | `{object_id, class, version, fields, state, valid, resolved_from}`, masked per policy |
  | Errors | `IAM.NOT_FOUND`, `QRY.NOT_FOUND_AT_TIME`, `PROOF.VALUE_ERASED` (only when a required field is erased) |
  | Determinism | `snapshot` |
  | Cost | 10 + 1 per KiB |
  | Authorization | `read` on the object, field-level masking |

  The first read of a branch fixes the process snapshot (STD-TXN-002) (DEC-044).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3300…3329
- **Satisfies:** FR-VER-101, FR-TIME-031, FR-VER-085
- **Notes:** Reads at explicit past coordinates do not extend the process's serializable read set.

### STD-ISA-031 — `read.history`
- **Clause:** Returns the versions of an object (or entries of a ledger key) in a transaction-time range, with commit metadata, paged.

  | Aspect | Value |
  |---|---|
  | Params | `target: uri`, `from_time: instant?`, `to_time: instant?`, `limit`, `cursor` |
  | Errors | `IAM.NOT_FOUND` |
  | Determinism | `snapshot` |
  | Cost | 10 + 2 per item |
  | Authorization | `read_history` on the object |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3330…3339
- **Satisfies:** FR-VER-015
- **Notes:** —

### STD-ISA-032 — `read.timeline`
- **Clause:** Returns `TL(o, X, t)` (STD-TIME-011), optionally clipped to a valid range.

  | Aspect | Value |
  |---|---|
  | Params | `target: uri`, `time: instant?`, `valid_from?`, `valid_to?` |
  | Determinism | `snapshot` |
  | Cost | 10 + 2 per segment |
  | Authorization | `read` on the object |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3340…3349
- **Satisfies:** FR-TIME-025
- **Notes:** —

### STD-ISA-033 — `read.diff`
- **Clause:** Computes a diff (STD-VER-030) between two coordinates for an object, a class or a branch, with a summary and paging.

  | Aspect | Value |
  |---|---|
  | Params | `scope: uri`, `a: coordinates`, `b: coordinates`, `summary_only: boolean?`, `limit`, `cursor` |
  | Determinism | `snapshot` |
  | Cost | 20 + 2 per entry |
  | Authorization | `read` on the scope, with masking |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3350…3359
- **Satisfies:** FR-VER-041, FR-VER-044
- **Notes:** —

### STD-ISA-034 — `read.effective`
- **Clause:** Returns the effective class definition, the effective sheet declarations for an object, or the BPU descriptor.

  | Aspect | Value |
  |---|---|
  | Params | `what: class \| declarations \| descriptor`, `target: uri` |
  | Determinism | `snapshot` |
  | Cost | 20 |
  | Authorization | `reflect` on the class |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3360…3369
- **Satisfies:** FR-MODEL-101, FR-RULE-022
- **Notes:** Since 1.0-rc2 (DEC-067) `declarations` returns the effective sheet declarations of an object, `{properties, bindings}` as the adapter operation `cascade`; the 1.0-rc1 vector VER-CONF-3361, which expected `LOGIC.NOT_AVAILABLE`, is superseded (STD-CONF-030).

### STD-ISA-035 — `read.explain`
- **Clause:** Returns the explanation object for a declaration cascade, a validation outcome, a decision or a value's lineage.

  | Aspect | Value |
  |---|---|
  | Params | `kind: cascade \| validation \| decision \| lineage`, `target: uri`, `property: text?`, `depth: integer?` |
  | Determinism | `snapshot` |
  | Cost | 30 + 2 per node |
  | Authorization | `explain` on the target, with non-leaking redaction (FR-AUD-033) |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3370…3379
- **Satisfies:** FR-RULE-022, FR-AUD-021, FR-AUD-031
- **Notes:** Since 1.0-rc2 (DEC-067): `cascade` returns `DAT-CascadeExplanation` of `property`; `validation` the `{violations, warnings}` of the unit of work on the target (as `run.validate`); `decision` the `DAT-DecisionResult` of the port named by `property`; `lineage` follows in PH-2 (`LOGIC.NOT_AVAILABLE`).

### STD-ISA-036 — `read.actions`
- **Clause:** Lists the ports of an object with their availability for the calling principal now, and the reason when unavailable (FR-FLOW-024).

  | Aspect | Value |
  |---|---|
  | Params | `target: uri` |
  | Result | `{state, actions: list<action>}` |
  | Errors | `IAM.NOT_FOUND` |
  | Determinism | `snapshot` |
  | Cost | 20 + 1 per port |
  | Authorization | `read` |

  An action is `{port, kind, transition?, to?, available, reason?, guards?, decision?}`: `available` is false with `reason` `state` when the port declares `states` that exclude the object's lifecycle state or names a transition that does not leave it, `permission` when the principal lacks the port's permission, `decision` when the decisions of the port (STD-LGS-031) deny or require (`decision` carries the outcome), or `guard` when a guard of the port or of its transition fails with the port's parameters absent (`guards` names them), in that order of precedence. Query ports are subject to `states` and `permission` only. `state` is the lifecycle state, or null without a lifecycle.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3380…3389
- **Satisfies:** FR-FLOW-024
- **Notes:** Since 1.0-rc3 (DEC-071). The adapter operation `actions` gives the same list without a principal (every port permitted), with the context given (STD-CONF-005). The actions are listed in port-name order.

### STD-ISA-037 — `read.projection` `[Profile: Ledger]`
- **Clause:** Reads a projection (STD-TIME-021) at the process snapshot, or at explicit coordinates.

  | Aspect | Value |
  |---|---|
  | Params | `projection: text`, `group: map?`, `asof: date?`, `rollup: boolean?`, `at_least: integer?`, `timeout_ms: integer?` |
  | Result | `{projection, groups, watermark}` |
  | Errors | `IAM.NOT_FOUND`, `QRY.INVALID_QUERY`, `QRY.WATERMARK_TIMEOUT` |
  | Determinism | `snapshot` |
  | Cost | 10 + 1 per group |
  | Authorization | `read` on the source class |

  `group` keeps the groups whose key has the given values. With `rollup`, the projection MUST group by the `account` field of its source class (STD-KIND-029) and have only `count` and `sum` aggregates (otherwise `QRY.INVALID_QUERY`); the result then holds, besides the groups of the leaves, one group per account above them with the sums over the leaves below it, at the `asof` date's hierarchy. The watermark is `{cseq, commit_id}`.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3390…3399
- **Satisfies:** FR-LEDG-031, FR-LEDG-033, DSN-DVM-441
- **Notes:** Since 1.0-rc9 (DEC-098):
  - **Names.** `projection` is the `name` of a live projection; an unknown name fails with `IAM.NOT_FOUND`.
  - **Defaults.** Without `asof`, entries of every valid time count. `timeout_ms` defaults to 0, so a read with `at_least` above the watermark fails at once.
  - **Snapshot.** The first read of a branch fixes the process snapshot (STD-TXN-002), as for `read.get`.
