---
id: UBS-STD-18
title: BPA Standard — Conformance, Profiles, Extensions and Certification (CONF)
status: draft
phase: PH-0
depends_on: [UBS-STD-00]
---

# BPA Standard — Conformance (CONF)

### STD-CONF-001 — Conformance vector format
- **Clause:** A conformance vector MUST be a canonical JSON document (`DAT-ConformanceVector`) that is independent of implementation language. Vectors are grouped by clause and profile, and packaged with a signed manifest per standard release.
- **Grammar / Schema:** `DAT-ConformanceVector`
- **Conformance vectors:** —
- **Satisfies:** FR-STD-021, FR-STD-022
- **Notes:** —

```yaml
DAT-ConformanceVector:
  id: { type: text, required: true, description: "VER-CONF-nnnn" }
  clauses: { type: list, required: true, description: "STD clause ids exercised" }
  profile: { type: text, required: true, description: "profile name" }
  polarity: { type: enum, required: true, description: "positive | negative" }
  setup: { type: list, required: true, description: "ordered adapter operations establishing the initial state (genesis, classes, objects)" }
  steps: { type: list, required: true, description: "ordered operations with expected results {op, input, expect: {result | error_code, match: exact|subset|hash}}" }
  tags: { type: list, required: false, description: "labels such as `prototype` (the vector is required of the PH-0 prototype) (DEC-032)" }
  determinism: { type: composite, required: false, description: "fixed now, entropy seed, tzdb version" }
  description: { type: text, required: true, description: "what is checked" }
```

### STD-CONF-002 — Adapter protocol
- **Clause:** Implementations are driven by the runner through an adapter speaking UBTP (STD-PROTO) or an in-process adapter with the same message structures. The adapter MUST additionally support the test-only operations `reset`, `set_now`, `set_entropy_seed`, `load_genesis` and `export_state`. It MUST be disabled in production builds.
- **Grammar / Schema:** —
- **Conformance vectors:** —
- **Satisfies:** FR-STD-021, CTR-014
- **Notes:** —

### STD-CONF-003 — Runner report
- **Clause:** The runner MUST produce `DAT-ConformanceReport`, signed by the runner key, listing every vector with its outcome (`pass`, `fail`, `not_applicable` outside the claimed profiles) and failure details.
- **Grammar / Schema:** `DAT-ConformanceReport`
- **Conformance vectors:** —
- **Satisfies:** FR-STD-023
- **Notes:** —

```yaml
DAT-ConformanceReport:
  standard_version: { type: text, required: true, description: "BPA version" }
  implementation: { type: composite, required: true, description: "{name, vendor, version, build_hash, backends}" }
  profiles_claimed: { type: list, required: true, description: "profile names" }
  results: { type: list, required: true, description: "{vector, outcome, detail}" }
  summary: { type: map, required: true, description: "counts per profile and outcome" }
  environment: { type: map, required: true, description: "OS, CPU, database versions" }
  signature: { type: composite, required: true, description: "DAT-Signature (runner)" }
```

### STD-CONF-004 — Profile conformance
- **Clause:** An implementation conforms to a profile when every vector of the profile passes and every MUST clause of the profile is implemented. Claims MUST name the standard version and profiles, and SHOULD reference a public report.
- **Grammar / Schema:** —
- **Conformance vectors:** —
- **Satisfies:** FR-STD-014, FR-STD-051
- **Notes:** —

### STD-CONF-005 — Adapter operations of release 0.1
- **Clause:** Until the Protocol profile defines UBTP messages, vectors MUST use only the adapter operations below; an adapter MUST implement each operation of the profiles it claims. Inputs and results are canonical JSON. An operation fails with a registered error code (STD-ERR-002). `match: exact` compares the whole result, `subset` compares only the members present in the expected result (recursively for objects, element-wise for arrays of equal length).

  | Operation | Input | Result |
  |---|---|---|
  | `canonical_json` | `{value}` | canonical text of the value after NFC normalisation of every string (STD-FND-001…003) |
  | `content_id` | `{domain, value}` | content identifier of `domain 0x00 canonical(value)` (STD-FND-010, STD-FND-011) |
  | `normalize_text` | `{text}` | text in NFC (STD-FND-003) |
  | `check_name` | `{grammar, text}` | `true`; grammars `slug`, `class-name`, `member-name`, `namespace`, `branch-name`, `uuid`, `uuid-v7` |
  | `validate_value` | `{type, constraints, value}` | the canonical value (STD-FND-020…027) |
  | `decimal` | `{op, a, b, scale, rounding}` | decimal string; `op` is `canonical`, `add`, `sub`, `mul`, `div` or `round` (STD-FND-022) |
  | `money` | `{op, a, b}` | money value; `op` is `add` or `sub` (STD-FND-023) |
  | `local_to_instant` | `{date, time, tz, disambiguation}` | instant (STD-FND-024) |
  | `classification_covers` | `{clearances, level}` | boolean (STD-FND-027) |
  | `version_hash` | `{envelope, body}` | `{body_ref, version_hash}` (STD-OBJ-002) |
  | `entry_hash` | `{entry, body}` | `{body_ref, entry_hash}` (STD-KIND-020) |
  | `commit_id` | `{commit}` | commit identifier (STD-VER-001) |
  | `tree_root` | `{entries}` | root of the prolly tree over sorted `[key, hash]` pairs (STD-PROOF-001) |
  | `linearize` | `{classes, class}` | `L(class)` as a list of class names (STD-CLS-011) |
  | `effective_class` | `{classes, class}` | the effective definition (STD-CLS-001…014) |
  | `reset`, `set_now`, `load_genesis` | `{}`, `{now}`, `{classes}` | `true` (STD-CONF-002) |
  | `create_branch` | `{name, from}` | `{head}`, the cseq of the fork commit |
  | `commit` | `{branch, changes, process}` | `{cseq, versions, entries}` (`DAT-AdapterChange`) |
  | `read` | `{branch, object_id, asof, time}` | `{version_hash, body, valid, tombstone}` |
  | `read_ledger` | `{branch, class, ledger_key, asof, time}` | `{entries}`, each `{entry_id, seq, entry_hash, valid_at, body}` |
  | `merge` | `{source, target}` | `{cseq}`; on conflict `VER.MERGE_CONFLICT` with the conflict list |
  | `diff` | `{from, to}` | the diff of STD-VER-030 |
  | `export_state` | `{branch, time}` | `{root, entries}` of the state tree (STD-PROOF-001) |

  Conventions of the adapter operations:
  1. Absent inputs take their defaults. `time` is the `cseq` of a commit on the branch (`main` starts at `cseq` 0, empty), and `asof` is a date or an instant. Object and entry identifiers are chosen by the vector.
  2. A step MAY give its input as `input_text`, a JSON text that the adapter parses (duplicate member names are an error, `MODEL.INVALID_VALUE`); vectors carry non-canonical inputs, such as numbers with a fraction, this way. A step MAY carry a `label`. An input value `{"$ref": "<label>/<pointer>"}` is replaced by the member of that step's result at the JSON pointer (RFC 6901). An expectation `{"same_as": "<label>/<pointer>"}` requires the result to equal that value.
  3. In adapter inputs a class is referenced by its name. A trait is a declaration with `trait: true` and `applies_to`, a list of kinds. A ledger class declares `ledger: {key, amounts, double_entry, period_scope}`; `period_scope` names a class whose objects have the fields `start` and `end` (dates, half-open) and `state` (`open`, `closing`, `closed`).
  4. The business time zone of the adapter is UTC; the posting date of an append is the date of `now` (`set_now`).
  5. Version hashes, entry hashes, roots, commit identifiers, transaction times and signatures of stateful operations are compared only through `same_as` or through the pure operations, because they depend on the implementation's clock, keys and class identifiers. Before an `exact` comparison of a stateful result, the members `version_hash`, `entry_hash`, `root`, `head` and `versions` are removed from it.
- **Grammar / Schema:** `DAT-AdapterChange`
- **Conformance vectors:** —
- **Satisfies:** FR-STD-021, FR-STD-022
- **Notes:** The operations are test interfaces; STD-CONF-002 applies (disabled in production builds).

```yaml
DAT-AdapterChange:
  op: { type: enum, required: true, description: "create | update | end | append | reverse | adjust" }
  object_id: { type: text, required: false, description: "Definition or RawDocument object (create, update, end)" }
  class: { type: text, required: false, description: "class name (create, append)" }
  slug: { type: text, required: false, description: "locator slug (create; update when it changes)" }
  base: { type: text, required: false, description: "base version hash (update, end); STD-TXN-020" }
  body: { type: json, required: false, description: "own field values (create, update, append)" }
  valid: { type: map, required: false, description: "timeline operation {kind: create|supersede_from|set_period|replace_future|end, from, to} (STD-TIME-011)" }
  entry_id: { type: text, required: false, description: "entry to append, or entry to reverse or adjust" }
  new_entry_id: { type: text, required: false, description: "identifier of the correcting entry (reverse, adjust)" }
  ledger_key: { type: map, required: false, description: "ledger key values (append)" }
  valid_at: { type: text, required: false, description: "date or instant of business effect (append)" }
  posting_group: { type: text, required: false, description: "double-entry group (append)" }
```

### STD-CONF-010 — Extensions
- **Clause:** Vendor extensions (instructions, value types, properties, widgets, error codes) MUST use names prefixed `x-<vendor>.`, MUST be registered in the extension registry with schema and vectors, and MUST NOT change the semantics of standard items. Standard names without the prefix are reserved.
- **Grammar / Schema:** —
- **Conformance vectors:** —
- **Satisfies:** FR-STD-043, FR-STD-071
- **Notes:** —

### STD-CONF-011 — Algorithm registry
- **Clause:** Algorithm identifiers for hashes, signatures and AEAD are registered by the standard. Version 1.0 registers `sha256`, `ed25519`, `aes-256-gcm` and `xchacha20-poly1305`. Adding an algorithm is a minor release. Implementations MUST verify content produced with every registered algorithm of their major version.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-6200…6209
- **Satisfies:** FR-PROOF-012, NR-SEC-004
- **Notes:** —

### STD-CONF-020 — Certification procedure
- **Clause:** Certification MUST follow these steps:
  1. the vendor submits the signed report, the claimed profiles, the extension list and the build identity;
  2. the steward reproduces the run on a certification environment with a build from the same source;
  3. a certificate (`DAT-Certificate`) is issued for the passing profiles, signed by the steward, listed publicly and valid for at most 24 months or until the next major version.
- **Grammar / Schema:** `DAT-Certificate`
- **Conformance vectors:** —
- **Satisfies:** FR-STD-051, FR-STD-052, FR-STD-053, SCN-407
- **Notes:** —

```yaml
DAT-Certificate:
  certificate_id: { type: text, required: true, description: "UUID" }
  implementation: { type: composite, required: true, description: "{name, vendor, version, build_hash}" }
  standard_version: { type: text, required: true, description: "BPA version" }
  profiles: { type: list, required: true, description: "certified profiles" }
  issued_at: { type: instant, required: true, description: "time" }
  expires_at: { type: instant, required: true, description: "time" }
  report: { type: text, required: true, description: "content-id of the reproduced report" }
  status: { type: enum, required: true, description: "valid | revoked | expired" }
  signature: { type: composite, required: true, description: "DAT-Signature (standards steward)" }
```

### STD-CONF-030 — Standard change process
- **Clause:** Every change to the standard MUST be proposed with rationale, clause diffs and new or changed vectors. Changes are classified as `patch` (errata, clarifications without behaviour change), `minor` (additive) or `major` (breaking). A change is published only with its vectors, and an erratum MUST reference the clause and vectors affected.
- **Grammar / Schema:** —
- **Conformance vectors:** —
- **Satisfies:** FR-STD-011, FR-STD-012, AR-033
- **Notes:** —
