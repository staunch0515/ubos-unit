---
id: UBS-STD-05
title: BPA Standard — Versioning (VER)
status: draft
phase: PH-0
depends_on: [UBS-STD-02, UBS-STD-04]
---

# BPA Standard — Versioning (VER)

## 1. Commits

### STD-VER-001 — Commit record
- **Clause:** A commit MUST conform to `DAT-Commit`. Its `commit_id` MUST be the content identifier of the canonical commit record without the `signatures` member (domain tag `ubos.commit.v1`).
- **Grammar / Schema:** `DAT-Commit`
- **Conformance vectors:** VER-CONF-1200…1219
- **Satisfies:** FR-VER-011, FR-VER-014
- **Notes:** —

```yaml
DAT-Commit:
  commit_id: { type: text, required: true, description: "content-id (computed)" }
  vae: { type: text, required: true, description: "tenant[.vae] authority" }
  branch: { type: text, required: true, description: "branch name" }
  parents: { type: list, required: true, description: "parent commit ids; first parent is the previous head of the branch; merge commits have 2" }
  author: { type: text, required: true, description: "principal DID" }
  on_behalf_of: { type: text, required: false, description: "principal DID" }
  process_id: { type: text, required: true, description: "UUID of the process" }
  tx_time: { type: instant, required: true, description: "transaction time (STD-TIME-001)" }
  changes: { type: list, required: true, description: "DAT-Change items" }
  root: { type: text, required: true, description: "Merkle root of the branch state after the commit (STD-PROOF-001)" }
  rule_binding: { type: text, required: true, description: "content-id of DAT-RuleBinding" }
  abi: { type: text, required: true, description: "instruction ABI version used" }
  message: { type: text, required: false, description: "free text" }
  reason_code: { type: text, required: false, description: "registered code; required for non-human initiators" }
  change_set: { type: reference, required: false, description: "change set merged by this commit" }
  signatures: { type: list, required: true, description: "DAT-Signature items: author, and node co-signature where required" }
DAT-Change:
  op: { type: enum, required: true, description: "create | update | end | move | append | hide | unhide" }
  target: { type: text, required: true, description: "object_id, or entry_id for append" }
  version: { type: text, required: true, description: "new version hash or entry hash" }
  base: { type: text, required: false, description: "base version hash (update, end, move, hide)" }
```

### STD-VER-002 — Commit immutability
- **Clause:** Commits, object versions and entries MUST NOT be altered or removed once committed, except for content disposal (STD-PROOF-060) and key destruction (STD-OBJ-011), both of which preserve all hashes.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1220…1224
- **Satisfies:** FR-VER-012
- **Notes:** —

### STD-VER-003 — First-parent history
- **Clause:** The history of a branch is the chain of first parents from its head. The transaction times along it MUST be strictly increasing (STD-TIME-001).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1225…1229
- **Satisfies:** FR-VER-015, FR-TIME-012
- **Notes:** —

## 2. Branches

### STD-VER-010 — Branch reference
- **Clause:** A branch MUST be represented by `DAT-BranchRef`. It is created by writing a reference to an existing commit (zero-copy) and moved only by commits on the branch (compare-and-set on `head`).
- **Grammar / Schema:** `DAT-BranchRef`
- **Conformance vectors:** VER-CONF-1230…1249
- **Satisfies:** FR-VER-021, FR-VER-027
- **Notes:** —

```yaml
DAT-BranchRef:
  vae: { type: text, required: true, description: "authority" }
  name: { type: text, required: true, description: "branch name (STD-VER-013)" }
  kind: { type: enum, required: true, description: "main | release | draft | feature | agent | overlay | loader | evolution | correction | sandbox | device" }
  head: { type: text, required: true, description: "commit id" }
  created_from: { type: text, required: true, description: "commit id at creation" }
  parent_line: { type: composite, required: false, description: "{branch, mode: live|pinned, pin: commit id} for inheritance" }
  owner: { type: text, required: true, description: "principal DID" }
  state: { type: enum, required: true, description: "active | frozen | archived" }
  expires_at: { type: instant, required: false, description: "lifetime for sandbox/agent kinds" }
```

### STD-VER-011 — Branch-kind policies
- **Clause:** Implementations MUST enforce the following policies per kind. Extra restrictions MAY be added by policy.

  | Kind | Created by | May merge into | Direct commits | Default lifetime |
  |---|---|---|---|---|
  | `main` | bootstrap only | — | per class write policy | permanent |
  | `release` | release managers | `main` (fixes) | per class write policy | permanent |
  | `draft` | any writer | `main`, `release`, `overlay` via change set | yes | 180 days inactive → archive |
  | `feature` | builders | `main`, `release`, `overlay`, `draft` via change set | yes | 180 days |
  | `agent` | Agent Hub for an agent run | protected targets via change set only | agent principal only | 30 days |
  | `overlay` | tenant administrators | — (receives merges) | per class write policy | permanent |
  | `loader` | installation processes | target line via change set | installation only | until install ends |
  | `evolution` | evolution analysers | protected targets via change set | analyser only | 90 days |
  | `correction` | authorised correctors | protected targets via change set | yes | 90 days |
  | `sandbox` | any writer | none | yes | 14 days |
  | `device` | sync (STD-SYNC) | target via integration rules | device principal | until integrated |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1250…1279
- **Satisfies:** FR-VER-022, FR-AI-031
- **Notes:** —

### STD-VER-012 — Write policy on protected branches
- **Clause:** `main`, `release` and `overlay` are protected branches. On them, a commit that creates or changes an object whose class has `write_policy: change_set` MUST be the merge commit of an approved change set. Otherwise it MUST fail with `VER.CHANGE_SET_REQUIRED`. Ledger appends follow the ledger class's write policy (default `direct`).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1280…1289
- **Satisfies:** FR-VER-024
- **Notes:** —

### STD-VER-013 — Branch names
- **Clause:** Branch names MUST match the grammar below. The first segment MUST equal the kind, except for `main`.
- **Grammar / Schema:**
  ```
  branch-name = "main" / kind-seg 1*( "/" slug )
  kind-seg    = "release" / "draft" / "feature" / "agent" / "overlay" / "loader"
              / "evolution" / "correction" / "sandbox" / "device"
  ```
- **Conformance vectors:** VER-CONF-1290…1299
- **Satisfies:** FR-VER-023
- **Notes:** —

## 3. Change sets

### STD-VER-020 — Change set object and lifecycle
- **Clause:** A change set MUST be a Definition object of class `ChangeSet` (`DAT-ChangeSet`) with this lifecycle:

  | From | Action | To | Condition |
  |---|---|---|---|
  | — | open | `open` | source and target exist; source is not protected |
  | `open` | submit | `submitted` | validation of the merge preview passes |
  | `submitted` | approve (final) | `approved` | approval rules satisfied (STD-TXN-060) |
  | `submitted` | reject | `rejected` | — |
  | `approved` | source or target change | `submitted` | approvals invalidated (STD-VER-021) |
  | `approved` | merge | `merged` | preview unchanged since approval |
  | `open`, `submitted` | abandon | `abandoned` | — |
- **Grammar / Schema:** `DAT-ChangeSet`
- **Conformance vectors:** VER-CONF-1300…1329
- **Satisfies:** FR-VER-031, FR-VER-034
- **Notes:** —

```yaml
DAT-ChangeSet:
  source: { type: text, required: true, description: "branch name" }
  target: { type: text, required: true, description: "branch name" }
  title: { type: text, required: true, description: "short title" }
  description: { type: document, required: false, description: "details" }
  preview: { type: text, required: false, description: "content-id of the latest DAT-MergePreview" }
  approvals: { type: list, required: false, description: "DAT-Approval references" }
  simulations: { type: list, required: false, description: "simulation run URIs" }
  selection: { type: list, required: false, description: "accepted object ids for partial acceptance" }
  merged_commit: { type: text, required: false, description: "commit id after merge" }
```

### STD-VER-021 — Approval invalidation
- **Clause:** Each approval MUST record the content ID of the merge preview it approved. When the source head or the relevant target state changes so that the preview content ID changes, all approvals MUST be invalidated and the change set returned to `submitted`.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1330…1339
- **Satisfies:** FR-VER-032, FR-FLOW-045
- **Notes:** Unrelated target changes that do not alter the preview keep approvals valid.

### STD-VER-022 — Partial acceptance
- **Clause:** When `selection` is set, the merge MUST include only the selected objects and entries. The selection MUST be closed under references created in the change set: if a selected object references an object created in the source and absent from the target, that object MUST also be selected (`VER.SELECTION_NOT_CLOSED`). Unselected changes MUST remain on the source, in a new change set.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1340…1349
- **Satisfies:** FR-VER-035
- **Notes:** —

## 4. Diff

### STD-VER-030 — Diff definition
- **Clause:** The diff between states A and B MUST list, per object present in either state:
  - the operation: `added`, `ended`, `changed`, `moved` or `unchanged` (omitted);
  - for `changed`, the list of field paths with old and new values.

  Paths use JSON Pointer (RFC 6901). Keyed lists MUST be diffed by element key, reporting `added`, `removed`, `changed` and `moved` per element. `document` values MUST be diffed per block, with text-level changes within blocks.
- **Grammar / Schema:** `DAT-DiffEntry`
- **Conformance vectors:** VER-CONF-1350…1379
- **Satisfies:** FR-VER-041, FR-VER-042
- **Notes:** —

```yaml
DAT-DiffEntry:
  object_id: { type: text, required: true, description: "object or entry id" }
  class: { type: text, required: true, description: "class URI" }
  op: { type: enum, required: true, description: "added | ended | changed | moved | appended" }
  fields: { type: list, required: false, description: "{path, old, new, masked: boolean}" }
  versions: { type: composite, required: true, description: "{a: hash|null, b: hash|null}" }
```

### STD-VER-031 — Diff cost
- **Clause:** Implementations MUST compute diffs by comparing Merkle trees (STD-PROOF-001), descending only into subtrees whose hashes differ.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1380…1384
- **Satisfies:** FR-VER-045, NR-PERF-005
- **Notes:** —

## 5. Merge

### STD-VER-040 — Merge base
- **Clause:** The merge base of commits S (source head) and T (target head) MUST be the best common ancestor: a common ancestor not reachable from any other common ancestor. When several exist, the implementation MUST construct a virtual base by merging them one by one in ascending order of `commit_id`: the accumulated virtual state is merged with the next ancestor using, as base, the merge base of the ancestors merged so far and that ancestor (computed by this clause recursively). Inside a virtual merge, a conflicting field or object keeps its value in that base, and ledger entries are united by `entry_id` as in STD-VER-054 (DEC-032, OQ-005).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1400…1414
- **Satisfies:** FR-VER-051
- **Notes:** The recursive strategy handles criss-cross histories deterministically.

### STD-VER-050 — Three-way merge of Definition objects
- **Clause:** For each Definition object, with base version B, source S and target T (each possibly absent), the merge result MUST be determined per field path as follows:

  | Case | Result |
  |---|---|
  | S = B | take T |
  | T = B | take S |
  | S = T | take S |
  | S ≠ B, T ≠ B, S ≠ T | apply the field's merge strategy (STD-VER-051); with `three_way`, a conflict |

  Object-level cases:
  - created on one side only: take it;
  - created on both sides with different `object_id`s but the same natural key: key conflict;
  - ended on one side and changed on the other: conflict;
  - ended on both sides: ended.

  Keyed lists merge per element with the same rules.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1415…1459
- **Satisfies:** FR-VER-051
- **Notes:** —

### STD-VER-051 — Field merge strategies
- **Clause:** A field MAY declare one strategy. Every strategy MUST be deterministic.

  | Strategy | Result when both sides changed |
  |---|---|
  | `three_way` (default) | conflict |
  | `prefer_source` | S |
  | `prefer_target` | T |
  | `last_writer` | the value from the side whose changing commit has the later `tx_time`; ties broken by `commit_id` |
  | `set_union` | B + (S − B) + (T − B) − (B − S) − (B − T), for set-valued fields |
  | `counter_delta` | B + (S − B) + (T − B), for integer and decimal fields |
  | `manual` | always a conflict, even when S = T |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1460…1489
- **Satisfies:** FR-VER-053
- **Notes:** —

### STD-VER-052 — Conflict report
- **Clause:** If any conflict exists, the merge MUST fail as a whole with `VER.MERGE_CONFLICT` and a list of `DAT-MergeConflict` covering every conflict.
- **Grammar / Schema:** `DAT-MergeConflict`
- **Conformance vectors:** VER-CONF-1490…1499
- **Satisfies:** FR-VER-052
- **Notes:** —

```yaml
DAT-MergeConflict:
  object_id: { type: text, required: true, description: "object id" }
  path: { type: text, required: false, description: "JSON Pointer of the field; absent for object-level conflicts" }
  kind: { type: enum, required: true, description: "field | ended_vs_changed | key | raw_document | schema" }
  base: { type: json, required: false, description: "base value (masked per reader)" }
  source: { type: json, required: false, description: "source value" }
  target: { type: json, required: false, description: "target value" }
  source_commit: { type: text, required: false, description: "commit that set the source value" }
  target_commit: { type: text, required: false, description: "commit that set the target value" }
```

### STD-VER-053 — Kind-specific merge
- **Clause:** System-kind objects MUST NOT be merged; they stay on their branch. RawDocument objects merge as whole values: if both sides changed the body, the result is a conflict of kind `raw_document`.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1500…1504
- **Satisfies:** FR-MODEL-054, FR-MODEL-055
- **Notes:** —

### STD-VER-054 — Ledger entries are appended on merge `[Profile: Ledger]`
- **Clause:** Entries present on the source and absent from the target (by `entry_id`) MUST be appended to the target in source sequence order per ledger key. They receive new target sequence numbers and `source_seq`. Entries are never field-merged. If a corrected entry (`corrects`) is not present on the target, the merge MUST fail with `LEDG.CORRECTION_TARGET_MISSING`.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1505…1524
- **Satisfies:** FR-VER-054, FR-SYNC-032
- **Notes:** —

### STD-VER-055 — Validation of the merged result
- **Clause:** Before writing a merge commit, the implementation MUST evaluate against the merged state:
  - constraints, validations and invariants;
  - governance-sheet declarations of the commit enforcement point;
  - referential integrity and key uniqueness;
  - double-entry balance and period rules.

  Any error-severity failure MUST abort the merge.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1525…1544
- **Satisfies:** FR-VER-055
- **Notes:** —

### STD-VER-056 — Merge commit
- **Clause:** A successful merge MUST produce exactly one commit on the target, with parents `[T, S]`, the change list relative to T, and `change_set` set when applicable. The merge is atomic (STD-TXN-003).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1545…1549
- **Satisfies:** FR-VER-056
- **Notes:** —

### STD-VER-057 — Merge preview
- **Clause:** A merge preview (`DAT-MergePreview`) MUST contain the base, the result changes, the conflicts and the validation results. It MUST be a pure function of (B, S, T, class versions in effect), so that equal inputs give equal preview content IDs.
- **Grammar / Schema:** `DAT-MergePreview`
- **Conformance vectors:** VER-CONF-1550…1559
- **Satisfies:** FR-VER-057, FR-VER-032
- **Notes:** —

```yaml
DAT-MergePreview:
  base: { type: text, required: true, description: "base commit id or virtual base id" }
  source_head: { type: text, required: true, description: "commit id" }
  target_head: { type: text, required: true, description: "commit id" }
  changes: { type: list, required: true, description: "DAT-DiffEntry items relative to target" }
  conflicts: { type: list, required: true, description: "DAT-MergeConflict items" }
  validation: { type: list, required: true, description: "violations with explanations" }
  summary: { type: map, required: true, description: "counts by class and op; risk flags" }
```

### STD-VER-058 — Recorded resolutions
- **Clause:** Conflict resolutions MUST be recorded as a `DAT-Resolution` list referenced by the merge commit, stating path, chosen value (or its hash when masked), resolver and optional reason.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1560…1564
- **Satisfies:** FR-VER-058
- **Notes:** —

```yaml
DAT-Resolution:
  object_id: { type: text, required: true, description: "object id" }
  path: { type: text, required: false, description: "JSON Pointer; absent for object-level conflicts" }
  choice: { type: enum, required: true, description: "source | target | base | custom" }
  value_hash: { type: text, required: true, description: "content-id of the chosen value" }
  resolver: { type: text, required: true, description: "principal DID" }
  reason: { type: text, required: false, description: "free text" }
```

## 6. Tags and releases

### STD-VER-060 — Tags
- **Clause:** A tag MUST bind a slug name to one commit, MUST be signed by its creator, and MUST NOT be moved. Deleting a tag MUST leave a tombstone that prevents reuse of the name.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1565…1574
- **Satisfies:** FR-VER-061, FR-VER-064
- **Notes:** —

### STD-VER-061 — Releases
- **Clause:** A release MUST be a tag plus a `Release` object (semantic version, notes, included change sets, approvals). Its semantic version MUST be unique per branch line.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1575…1579
- **Satisfies:** FR-VER-062
- **Notes:** —

## 7. Revert

### STD-VER-070 — Revert of a commit
- **Clause:** `revert(C)` on branch X MUST produce a compensating commit on X:
  - for every Definition field that C changed, restore the value before C, unless the field has changed since C (a conflict);
  - for objects created by C, end them unless changed since (a conflict);
  - for objects ended by C, restore their last version;
  - for ledger entries appended by C, append reversals (STD-KIND-023).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1580…1604
- **Satisfies:** FR-VER-071, FR-VER-074
- **Notes:** —

### STD-VER-071 — Revert of a process or change set
- **Clause:** Reverting a process MUST revert all its commits on each branch, as one process, in reverse commit order. Reverting a merged change set MUST revert its merge commit relative to the first parent. The implementation MUST report dependent processes (those triggered by the reverted ones) before execution.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1605…1614
- **Satisfies:** FR-VER-072, FR-VER-073, FR-VER-075
- **Notes:** —

## 8. Branch inheritance

### STD-VER-080 — Resolution through the parent line
- **Clause:** For a branch X with `parent_line` P (mode live or pinned), resolving object o at snapshot s MUST:
  1. look for o on X at s;
  2. if X has a version (including a hide marker), use it;
  3. otherwise resolve o on P at P's head (live, as of the time of s) or at the pin (pinned), recursively.

  A hide marker (`op: hide`) MUST make o not found on X without affecting P. Chains MUST support depth 8 and reject cycles (`VER.INHERITANCE_CYCLE`).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1615…1644
- **Satisfies:** FR-VER-081, FR-VER-082, FR-VER-083, FR-VER-084
- **Notes:** Merkle roots of X cover only X's own entries plus the parent-line reference (commit ID). Proofs of inherited objects chain through the parent root.

### STD-VER-081 — Provenance of resolution
- **Clause:** Every read MUST be able to report the branch and commit from which each returned version was resolved.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1645…1649
- **Satisfies:** FR-VER-085
- **Notes:** —

### STD-VER-082 — Upstream change detection
- **Clause:** When the parent line changes an object that X overrides, the implementation MUST mark the override as `upstream_changed`, recording the parent versions before and after, until X resolves it by a three-way merge (base = the parent version at the time of the override) or by acknowledgement.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1650…1659
- **Satisfies:** FR-VER-086
- **Notes:** —

## 9. Points in history

### STD-VER-090 — Snapshot selection
- **Clause:** A snapshot of branch X MUST be selected by one of:
  - `commit=C`: C must be on X's history;
  - `tag=N`;
  - `time=T`: the latest commit on X's first-parent history with `tx_time ≤ T`;
  - no selector: the head at request start.

  Resolution MUST be identical for all readers.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1660…1674
- **Satisfies:** FR-VER-101, FR-VER-104
- **Notes:** —
