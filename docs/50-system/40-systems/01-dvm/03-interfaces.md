---
id: UBS-SYS-DVM-03
title: DVM — Interfaces
status: draft
phase: PH-1
depends_on: [UBS-SYS-DVM-02, UBS-ARC-02]
---

# DVM — Interfaces (IF-DVM-*)

The DVM has three interface groups:

| Group | Range | Direction | Contract |
|---|---|---|---|
| Host API | IF-DVM-001…049 | host → DVM | CTR-001 |
| Host services | IF-DVM-050…079 | DVM → host | CTR-002 |
| Storage adapter | IF-DVM-080…099 | DVM → store | CTR-005 |

All host API operations are Rust library APIs in the `ubos-dvm` facade crate. The same
operations are exported through a C ABI (for other hosts) and through `wasm-bindgen`
(browser build). Requests and responses are canonical JSON when they cross a non-Rust
boundary. Every operation returns either its response or a `DAT-Error` (AR-026).

Conventions used below:
- `ctx` is always a `DAT-ExecutionContext` built by the host from authenticated data;
- "snapshot selector" means the selector set of STD-ADDR-002;
- idempotency keys are optional unless stated.

## Host API — lifecycle and processes

### IF-DVM-001 — `open_vae`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ tenant_id, vae_id, store: StoreHandle, config: DvmConfig }`
- **Response:** `VaeHandle` (holds registries, caches and the branch sequencers of this VAE on this node)
- **Errors:** `NOD.STORAGE_UNAVAILABLE`, `TEN.SUSPENDED`, `INT.VERSION_UNSUPPORTED`
- **Idempotency:** Opening an open VAE returns the existing handle.
- **Authorization:** Host-internal; no principal.
- **Satisfies:** FR-TEN-012, STD-FND-015

### IF-DVM-002 — `create_vae`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ tenant_id, vae_id, business_time_zone, template?: { vae, release } }`
- **Response:** `{ vae_id, genesis_commit_id, genesis_root }`
- **Errors:** `TXN.ALREADY_EXISTS`, `NOD.STORAGE_UNAVAILABLE`
- **Idempotency:** Keyed by `vae_id`; a repeat returns the same genesis.
- **Authorization:** `vae.create` on the tenant.
- **Satisfies:** FR-TEN-011, FR-TEN-012, STD-CLS-010

### IF-DVM-003 — `begin_process`
- **Kind:** library API
- **Caller → Callee:** NOD, FRG → DVM
- **Request:** `{ ctx: DAT-ExecutionContext, branch, profile, mode: standard | serializable, idempotency_key?, reexecutable: bool }`
- **Response:** `ProcessHandle`
- **Errors:** `LOGIC.CONTEXT_IMMUTABLE`, `IAM.DENIED`, `VER.INVALID_BRANCH_NAME`, `TXN.IDEMPOTENCY_MISMATCH`
- **Idempotency:** With a key, a completed process with the same request hash returns its recorded outcome instead of a handle.
- **Authorization:** `process.begin` on the branch.
- **Satisfies:** FR-TXN-011, FR-TXN-031, STD-TXN-001, STD-CTX-001

### IF-DVM-004 — `invoke_port`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ process: ProcessHandle, target: uri, port, args }`
- **Response:** `{ result, warnings[], needs_input?: FormRequest }`
- **Errors:** `MODEL.PORT_NOT_IMPLEMENTED`, `RULE.DECISION_DENIED`, `RULE.DECISION_REQUIRES`, `FLOW.GUARD_FAILED`, `LOGIC.*`, `IAM.DENIED`, `IAM.NOT_FOUND`
- **Idempotency:** Through the process idempotency key.
- **Authorization:** `port.invoke:<port>` on the target, then per instruction.
- **Satisfies:** FR-FLOW-022, STD-ISA-050

### IF-DVM-005 — `invoke_batch`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, branch, items: [{ target, port, args }], atomic: bool }`
- **Response:** `{ outcomes: [{ index, ok | error }], commit_ids[] }`
- **Errors:** as IF-DVM-004, per item
- **Idempotency:** Per batch key; per-item keys derived as `(batch key, index)`.
- **Authorization:** per item
- **Satisfies:** FR-FLOW-023

### IF-DVM-006 — `stage`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ process, instruction: commit.* , args }` (direct instruction invocation for generic create, update, end, append, link, transition)
- **Response:** `{ object_id, staged_version_preview }`
- **Errors:** `MODEL.*`, `TIME.*`, `LEDG.*`, `FLOW.STATE_DIRECT_WRITE`, `IAM.DENIED`
- **Idempotency:** Staging is local to the process.
- **Authorization:** per instruction
- **Satisfies:** STD-ISA-010…017

### IF-DVM-007 — `validate`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ process }`
- **Response:** `{ violations: [DAT-Error detail], warnings[] }`
- **Errors:** none (violations are data)
- **Idempotency:** Pure.
- **Authorization:** none beyond the process.
- **Satisfies:** FR-RULE-035, FR-UX-031

### IF-DVM-008 — `commit`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ process, reason_code?, message? }`
- **Response:** `{ outcome: committed | deferred | empty, commit_id?, cseq?, pending_change_id?, metering: DAT-MeteringRecord }`
- **Errors:** `TXN.HEAD_MOVED`, `TXN.READ_SET_CHANGED`, `MODEL.*`, `RULE.INVARIANT_VIOLATED`, `LEDG.*`, `VER.CHANGE_SET_REQUIRED`, `NOD.CLOCK_SKEW`, `NOD.STORAGE_UNAVAILABLE`
- **Idempotency:** Via the process idempotency record written in the flush.
- **Authorization:** already checked per instruction; branch write policy re-checked.
- **Satisfies:** FR-TXN-012, FR-TXN-013, STD-TXN-003, STD-TXN-004

### IF-DVM-009 — `abort` and `cancel`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ process, reason }`
- **Response:** `{ aborted: true, metering }`
- **Errors:** none
- **Idempotency:** Repeat calls are no-ops.
- **Authorization:** caller owns the process or holds `process.cancel`.
- **Satisfies:** STD-CTX-005, NR-PERF-019

## Host API — reads and queries

### IF-DVM-010 — `read`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, uri with snapshot selector, expand?: [path], lens?: current | stored }`
- **Response:** `{ object, version_id, provenance, evidence: EvaluationEvidence }`
- **Errors:** `IAM.NOT_FOUND`, `QRY.NOT_FOUND_AT_TIME`, `ADDR.CONFLICTING_SELECTORS`, `MODEL.LENS_REQUIRED`, `PROOF.VALUE_ERASED` (per field), `IAM.KEY_UNAVAILABLE`
- **Idempotency:** Pure for a fixed snapshot.
- **Authorization:** `read` on the object, masking applied.
- **Satisfies:** FR-VER-101, FR-TIME-031, STD-ISA-030, STD-TIME-012

### IF-DVM-011 — `history` and `timeline`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, uri, range?, page }`
- **Response:** `{ entries: [{ commit_id, cseq, tx_time, version_id, valid_period, author, reason }], cursor }`
- **Errors:** `IAM.NOT_FOUND`, `INT.CURSOR_EXPIRED`
- **Idempotency:** Pure.
- **Authorization:** `read.history` on the object.
- **Satisfies:** FR-VER-015, FR-TIME-025, STD-ISA-031, STD-ISA-032

### IF-DVM-012 — `query`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, query: DAT-Query, snapshot selector, page_size, cursor? }`
- **Response:** `{ rows, cursor?, plan_summary }`
- **Errors:** `QRY.INVALID_QUERY`, `QRY.SCAN_LIMIT`, `INT.CURSOR_EXPIRED`
- **Idempotency:** Pure for a fixed snapshot.
- **Authorization:** residual filter by partial evaluation.
- **Satisfies:** FR-QRY-011, FR-QRY-012, FR-QRY-071, STD-ISA-080

### IF-DVM-013 — `aggregate` and `traverse`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, query, group_by?, measures? }` or `{ ctx, start, relation path, depth, limit }`
- **Response:** aggregate rows or traversal nodes and edges
- **Errors:** `QRY.*`, `MODEL.CURRENCY_MISMATCH`
- **Idempotency:** Pure.
- **Authorization:** as IF-DVM-012, per hop for traversals.
- **Satisfies:** FR-QRY-041, FR-MODEL-065, STD-ISA-081, STD-ISA-082

### IF-DVM-014 — `resolve_view`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, uri, view_kind: detail | form | list | card, device class, staged_edits? }`
- **Response:** `ViewPayload { view_id, data, field_states, actions, violations }`
- **Errors:** `IAM.NOT_FOUND`
- **Idempotency:** Pure.
- **Authorization:** `read`; field states reflect write permissions.
- **Satisfies:** FR-UX-011, FR-UX-013, STD-CLS-023

### IF-DVM-015 — `available_actions`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, uri }`
- **Response:** `{ actions: [{ name, available, reasons[] }] }`
- **Errors:** `IAM.NOT_FOUND`
- **Idempotency:** Pure.
- **Authorization:** `read`.
- **Satisfies:** FR-FLOW-024

### IF-DVM-016 — `explain`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, uri, what: cascade | decision | lineage | why_changed, property? }`
- **Response:** `DAT-CascadeExplanation` or lineage graph or causality chain
- **Errors:** `IAM.NOT_FOUND`
- **Idempotency:** Pure.
- **Authorization:** `read.explain`; explanations are masked.
- **Satisfies:** FR-RULE-022, FR-AUD-021, FR-AUD-031, FR-TXN-043, STD-ISA-035

### IF-DVM-017 — `reflect`
- **Kind:** library API
- **Caller → Callee:** NOD, FRG → DVM
- **Request:** `{ ctx, snapshot selector, scope: classes | class | ports | sheets | instructions, format: native | json_schema | openapi | graph }`
- **Response:** reflection document
- **Errors:** `IAM.DENIED`
- **Idempotency:** Pure.
- **Authorization:** `model.read`.
- **Satisfies:** FR-MODEL-101, FR-MODEL-102, FR-MODEL-103, FR-MODEL-104

### IF-DVM-018 — `changes_since`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ branch, from_cseq, to_cseq }`
- **Response:** `{ changed_objects[], affected_saved_queries[], outbox_range }`
- **Errors:** none
- **Idempotency:** Pure.
- **Authorization:** host-internal (subscription fan-out applies authorization per subscriber).
- **Satisfies:** NR-PERF-015, FR-QRY-051

## Host API — versioning

### IF-DVM-020 — `branch_create`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, name, kind, from: snapshot selector, read_through: bool }`
- **Response:** `DAT-BranchRef`
- **Errors:** `VER.INVALID_BRANCH_NAME`, `TXN.ALREADY_EXISTS`, `VER.INHERITANCE_CYCLE`
- **Idempotency:** Keyed by name; repeat with the same source returns the branch.
- **Authorization:** `branch.create` on the source branch.
- **Satisfies:** FR-VER-021, FR-VER-022, FR-VER-027

### IF-DVM-021 — `branch_update`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, branch, op: freeze | unfreeze | archive | delete | advance_upstream, upstream_target? }`
- **Response:** `DAT-BranchRef` plus, for `advance_upstream`, an override report
- **Errors:** `IAM.DENIED`, `VER.*`
- **Idempotency:** State-setting operations are idempotent.
- **Authorization:** `branch.admin`.
- **Satisfies:** FR-VER-025, FR-VER-086

### IF-DVM-022 — `diff`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, from: selector, to: selector, filter?, semantic: bool, cursor? }`
- **Response:** `{ entries: [DAT-DiffEntry], cursor?, redacted_counts }`
- **Errors:** `INT.CURSOR_EXPIRED`
- **Idempotency:** Pure.
- **Authorization:** `read` per object; masked.
- **Satisfies:** FR-VER-041, FR-VER-042, FR-VER-044, STD-ISA-033

### IF-DVM-023 — `change_set`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, op: open | update | submit | approve | reject | comment | withdraw | accept_partial, change_set_id?, args }`
- **Response:** `DAT-ChangeSet` with the current preview hash and summary
- **Errors:** `VER.SELECTION_NOT_CLOSED`, `IAM.DENIED`, `FLOW.TRANSITION_NOT_ALLOWED`
- **Idempotency:** Approvals keyed by `(change set, approver, preview hash)`.
- **Authorization:** per operation (`changeset.approve` subject to SoD).
- **Satisfies:** FR-VER-031…036, STD-VER-020…022

### IF-DVM-024 — `merge_preview`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, source, target, resolutions?: [DAT-Resolution] }`
- **Response:** `DAT-MergePreview`
- **Errors:** `IAM.DENIED`
- **Idempotency:** Pure and cached.
- **Authorization:** `read` on both branches.
- **Satisfies:** FR-VER-057, STD-VER-057

### IF-DVM-025 — `merge`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, change_set_id | { source, target }, expected_preview_hash, resolutions? }`
- **Response:** `{ merge_commit_id, cseq, ledger_mapping_ref }`
- **Errors:** `VER.MERGE_CONFLICT`, `VER.CHANGE_SET_REQUIRED`, `TXN.HEAD_MOVED`, `IAM.DENIED`
- **Idempotency:** A repeated merge of a merged change set returns the merge commit.
- **Authorization:** `branch.merge` on the target plus valid approvals.
- **Satisfies:** FR-VER-051…056, STD-VER-056

### IF-DVM-026 — `revert`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, target: commit | process | change_set, preview: bool }`
- **Response:** preview with dependent processes, or revert commit ID
- **Errors:** `VER.MERGE_CONFLICT`, `LEDG.PERIOD_CLOSED`, `LEDG.ALREADY_REVERSED`
- **Idempotency:** Keyed by target.
- **Authorization:** `revert` on the branch.
- **Satisfies:** FR-VER-071…075

### IF-DVM-027 — `tag` and `release`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, name, commit, sign: bool, release?: { notes, compatibility } }`
- **Response:** tag or release object
- **Errors:** `VER.TAG_IMMUTABLE`
- **Idempotency:** Same name and commit returns the existing tag.
- **Authorization:** `tag.create`.
- **Satisfies:** FR-VER-061…064

### IF-DVM-028 — `pending_change`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, op: approve | reject | withdraw | read, pending_change_id, expected_preview_hash? }`
- **Response:** `DAT-PendingChange` or commit result
- **Errors:** `IAM.DENIED`, `TXN.HEAD_MOVED`
- **Idempotency:** Keyed by `(pending change, approver, preview hash)`.
- **Authorization:** per approval spec.
- **Satisfies:** STD-TXN-060, FR-FLOW-042

## Host API — time, ledger and operations

### IF-DVM-030 — `due_effective`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ branch, from, to }`
- **Response:** `{ segments: [{ object_id, version_id, valid_from }] }`
- **Errors:** none
- **Idempotency:** Pure.
- **Authorization:** host-internal.
- **Satisfies:** FR-TIME-051, FR-TIME-052

### IF-DVM-031 — `impact` and `restate`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, change: commit | pending change, mode: compute | restate }`
- **Response:** impact set, or `{ correction_branch, proposals }`
- **Errors:** `IAM.DENIED`
- **Idempotency:** `restate` keyed by the change.
- **Authorization:** `impact.read`, `restate.run`.
- **Satisfies:** FR-TIME-042, FR-TIME-043, FR-TIME-045

### IF-DVM-032 — `balance`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, projection, key, asof, snapshot selector }`
- **Response:** `{ balances: [{ currency, amount }], evidence }`
- **Errors:** `IAM.NOT_FOUND`
- **Idempotency:** Pure.
- **Authorization:** `read` on the ledger key.
- **Satisfies:** FR-LEDG-031, FR-LEDG-032

### IF-DVM-033 — `outbox_claim` and `outbox_ack`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ n, lease_ms }` / `{ ids[] }`
- **Response:** `{ rows: [{ id, branch, cseq, index, kind, payload, attempts }] }` / `{ acked }`
- **Errors:** `NOD.STORAGE_UNAVAILABLE`
- **Idempotency:** Ack is idempotent; claims expire.
- **Authorization:** host-internal.
- **Satisfies:** FR-TXN-051, FR-TXN-052

### IF-DVM-034 — `timers_claim`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ now, n, lease_ms }`
- **Response:** `{ timers: [{ id, due_at, target, step }] }`
- **Errors:** `NOD.STORAGE_UNAVAILABLE`
- **Idempotency:** Firing is idempotent per `(instance, step, attempt)`.
- **Authorization:** host-internal.
- **Satisfies:** FR-FLOW-063

### IF-DVM-035 — `replay`
- **Kind:** library API
- **Caller → Callee:** NOD, FRG → DVM
- **Request:** `{ ctx, journal_ref | process_id, substitutions?: { sheets?, assets? }, debug: bool }`
- **Response:** `{ reproduced: bool, outcome_hash, divergence?, trace? }`
- **Errors:** `LOGIC.REPLAY_INPUT_MISSING`
- **Idempotency:** Pure.
- **Authorization:** `replay.run`.
- **Satisfies:** FR-LOGIC-073, FR-RULE-071

### IF-DVM-036 — `simulate`
- **Kind:** library API
- **Caller → Callee:** NOD, STU → DVM
- **Request:** `{ ctx, base: selector, changes: change set | sheets, workload: query + period, budget }`
- **Response:** `{ run_id, changed_outcomes[], coverage, budget_used }`
- **Errors:** `LOGIC.LIMIT_EXCEEDED`
- **Idempotency:** Keyed by run request hash.
- **Authorization:** `simulate.run`.
- **Satisfies:** FR-RULE-071…074

### IF-DVM-037 — `verify`
- **Kind:** library API
- **Caller → Callee:** NOD, FRG → DVM
- **Request:** `{ what: indexes | projections | sequences | signatures | roots, branch, range }`
- **Response:** verification report
- **Errors:** none (findings are data)
- **Idempotency:** Pure.
- **Authorization:** host-internal or `ops.verify`.
- **Satisfies:** NR-DUR-005, FR-ANL-022, FR-LEDG-043

### IF-DVM-038 — `rebuild`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ what: index | projection, name, branch }`
- **Response:** `{ rows, duration, swapped: bool }`
- **Errors:** `NOD.STORAGE_UNAVAILABLE`
- **Idempotency:** Safe to repeat.
- **Authorization:** `ops.rebuild`.
- **Satisfies:** FR-ANL-021, FR-ANL-023

## Host API — proofs, export, import and sync

### IF-DVM-040 — `prove`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, kind: inclusion | non_inclusion | consistency | tenant, commit, key? | other_commit? }`
- **Response:** `DAT-InclusionProof` or consistency proof
- **Errors:** `IAM.NOT_FOUND`
- **Idempotency:** Pure.
- **Authorization:** `proof.read`.
- **Satisfies:** FR-PROOF-032, FR-PROOF-033, FR-PROOF-034

### IF-DVM-041 — `evidence_pack`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, scope: query + selector, include: { journals, receipts }, redact: [classification] }`
- **Response:** streamed archive plus `DAT-EvidenceManifest`
- **Errors:** `IAM.DENIED`
- **Idempotency:** Reproducible output (DSN-DVM-941).
- **Authorization:** `evidence.export`, logged.
- **Satisfies:** FR-PROOF-051…054

### IF-DVM-042 — `seal`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, document_hash, source: selector, view_id }`
- **Response:** `DAT-Seal`
- **Errors:** `IAM.DENIED`
- **Idempotency:** Keyed by `(document hash, source)`.
- **Authorization:** `seal.create`.
- **Satisfies:** STD-PROOF-055

### IF-DVM-043 — `export`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, scope: vae | branch | range, format: portable_archive }`
- **Response:** streamed archive
- **Errors:** `IAM.DENIED`
- **Idempotency:** Reproducible for a fixed selector.
- **Authorization:** `vae.export`, logged.
- **Satisfies:** NR-PORT-003, STD-PROOF-070

### IF-DVM-044 — `import`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, archive stream, target: new vae | branch }`
- **Response:** `{ commits, roots_verified: true }`
- **Errors:** `SYNC.INTEGRITY_FAILURE`, `PROOF.SIGNATURE_INVALID`
- **Idempotency:** Content-addressed; re-import is a no-op.
- **Authorization:** `vae.import`.
- **Satisfies:** NR-PORT-003, FR-MIG-013

### IF-DVM-045 — `sync_session`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, scope: DAT-SyncScope, peer_heads, have/want messages }`
- **Response:** have/want answers, packs, integration results, rejection records
- **Errors:** `SYNC.INTEGRITY_FAILURE`, `SYNC.UPSTREAM_UNAVAILABLE`, `IAM.DENIED`
- **Idempotency:** Resumable; packs are content-addressed.
- **Authorization:** scope authorization per STD-SYNC-001.
- **Satisfies:** FR-SYNC-011…014, STD-SYNC-001…005

### IF-DVM-046 — `erase_subject`
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, subject_id, reason, request_ref }`
- **Response:** `{ erased_fields_count, certificate }`
- **Errors:** `AUD.LEGAL_HOLD_ACTIVE`, `IAM.DENIED`
- **Idempotency:** Repeat returns the same certificate.
- **Authorization:** `privacy.erase`, requires approval.
- **Satisfies:** FR-PROOF-073, FR-PROOF-075, NR-PRIV-001

### IF-DVM-047 — `authorize` and access analysis
- **Kind:** library API
- **Caller → Callee:** NOD → DVM
- **Request:** `{ ctx, action, resource }` or `{ who_can | what_can, … }`
- **Response:** decision with reasons, or principal and action lists
- **Errors:** none
- **Idempotency:** Pure.
- **Authorization:** `access.analyse` for analysis queries.
- **Satisfies:** FR-IAM-081, AR-024

## Host services (implemented by each host)

### IF-DVM-050 — `clock.now`
- **Kind:** library API
- **Caller → Callee:** DVM → NOD
- **Request:** `{}`
- **Response:** `{ instant, source, uncertainty_ms }`
- **Errors:** `NOD.CLOCK_SKEW`
- **Idempotency:** Not idempotent; journaled.
- **Authorization:** —
- **Satisfies:** STD-TIME-001, STD-CTX-010

### IF-DVM-051 — `entropy.fill`
- **Kind:** library API
- **Caller → Callee:** DVM → NOD
- **Request:** `{ bytes }`
- **Response:** random bytes from the OS CSPRNG
- **Errors:** —
- **Idempotency:** Not idempotent; journaled.
- **Authorization:** —
- **Satisfies:** STD-CTX-010

### IF-DVM-052 — `blob.put` and `blob.get`
- **Kind:** library API
- **Caller → Callee:** DVM → NOD
- **Request:** `{ tenant, content_id, bytes }` / `{ tenant, content_id, range? }`
- **Response:** `{ durable: true }` / bytes
- **Errors:** `NOD.BLOB_UNAVAILABLE`, `NOD.STORAGE_UNAVAILABLE`
- **Idempotency:** Content-addressed.
- **Authorization:** tenant-scoped credentials held by the host.
- **Satisfies:** CTR-006, STD-OBJ-008

### IF-DVM-053 — `connector.call`
- **Kind:** library API
- **Caller → Callee:** DVM → NOD
- **Request:** `{ connector_version_id, operation, request, secret_handle, timeout }`
- **Response:** `{ status, response, duration }`
- **Errors:** `LOGIC.CONNECTOR_ERROR`, `LOGIC.CONNECTOR_UNAVAILABLE`
- **Idempotency:** Per connector declaration; request carries an idempotency header when supported.
- **Authorization:** checked by the DVM before the call.
- **Satisfies:** FR-LOGIC-101…105

### IF-DVM-054 — `model.ask`
- **Kind:** library API
- **Caller → Callee:** DVM → NOD (model gateway, AGT)
- **Request:** `{ purpose, model_class, prompt parts with classifications, output schema, budget }`
- **Response:** `{ output, provider, model, tokens, cost }`
- **Errors:** `AI.PROVIDER_UNAVAILABLE`, `AI.BUDGET_EXHAUSTED`
- **Idempotency:** Not idempotent; journaled.
- **Authorization:** classification checked by the DVM.
- **Satisfies:** FR-AI-011, FR-AI-091

### IF-DVM-055 — `keys`
- **Kind:** library API
- **Caller → Callee:** DVM → NOD (key service)
- **Request:** `sign(key_ref, bytes)`, `wrap/unwrap(subject key)`, `destroy(subject key)`, `hmac(key_ref, bytes)`
- **Response:** signature, key material handle, confirmation, token
- **Errors:** `IAM.KEY_UNAVAILABLE`
- **Idempotency:** `destroy` is idempotent.
- **Authorization:** host-internal.
- **Satisfies:** STD-PROOF-010, FR-IAM-092, FR-PROOF-073

### IF-DVM-056 — `index.text` and `index.vector`
- **Kind:** library API
- **Caller → Callee:** DVM → NOD, BRG
- **Request:** `{ tenant, class, query, residual filter, limit }`
- **Response:** candidate hits with object IDs and scores
- **Errors:** `QRY.CLASSIFICATION_NOT_EMBEDDABLE`
- **Idempotency:** Pure.
- **Authorization:** hits re-checked by the DVM.
- **Satisfies:** FR-QRY-021, FR-QRY-031

### IF-DVM-057 — `telemetry.emit`
- **Kind:** library API
- **Caller → Callee:** DVM → NOD
- **Request:** spans, metrics and log records (no field values)
- **Response:** —
- **Errors:** — (best effort)
- **Idempotency:** —
- **Authorization:** —
- **Satisfies:** NR-OBS-001, NR-SEC-009

### IF-DVM-058 — `audit.emit`
- **Kind:** library API
- **Caller → Callee:** DVM → NOD
- **Request:** decision logs, read logs, export logs
- **Response:** `{ accepted }`
- **Errors:** `NOD.STORAGE_UNAVAILABLE` (the DVM fails the operation when audit is mandatory)
- **Idempotency:** Records carry IDs.
- **Authorization:** —
- **Satisfies:** FR-AUD-012, FR-AUD-051, FR-AUD-052

## Storage adapter

### IF-DVM-080 — `StoreTx` flush operations
- **Kind:** library API
- **Caller → Callee:** DVM → PostgreSQL or SQLite adapter
- **Request:** one transaction with `put_chunks`, `put_versions`, `put_entries`, `put_commit`, `cas_branch_head`, `upsert_index_rows`, `put_tree_nodes`, `alloc_sequences`, `outbox_append`, `idempotency_put`
- **Response:** `{ committed: true }`
- **Errors:** `TXN.HEAD_MOVED`, `NOD.STORAGE_UNAVAILABLE`
- **Idempotency:** Content-addressed rows use insert-if-absent; the head CAS makes the transaction single-shot.
- **Authorization:** database role with RLS.
- **Satisfies:** CTR-005, DEC-020

### IF-DVM-081 — `StoreRead` snapshot reads
- **Kind:** library API
- **Caller → Callee:** DVM → adapter
- **Request:** point, range and index scans with `(branch, cseq)` filters
- **Response:** rows
- **Errors:** `NOD.STORAGE_UNAVAILABLE`
- **Idempotency:** Pure.
- **Authorization:** database role with RLS.
- **Satisfies:** CTR-005, STD-TXN-002

### IF-DVM-082 — Storage conformance suite
- **Kind:** library API
- **Caller → Callee:** FRG test runner → adapter
- **Request:** the suite `SUITE-DVM-STORE`
- **Response:** report
- **Errors:** —
- **Idempotency:** —
- **Authorization:** —
- **Satisfies:** NR-PORT-003, CST-002

## Model export formats (DSN-DVM-132)

Built in PH-2 step 7f (DEC-133) in `crates/ubos-dvm/src/export.rs`. The adapter operations
are `export_openapi {scope?}` and `export_model_graph {scope?}`. The Server serves them at
`POST /api/v1/admin/model/export {format: openapi | graph, scope?}`, and `forge export`
runs them on a project's model (offline) or on a Server. A scope is a namespace and the
namespaces below it; without one, every class outside the kernel's genesis model is
exported.

**OpenAPI 3.1.** The document describes the Server's REST binding under
`/api/v1/<authority>`:
- `GET /<Class>`: a criteria query in `q`, on `branch`;
- `GET /<Class>/{slug}`: a read;
- `POST /<Class>/{slug}/<port>`: one per port of a concrete class, with the port's
  parameters as the request body and `PortResult` as the response.

Every class has a `<Class>Body` schema: its effective fields as JSON Schema, with the value
types of STD-FND-020 mapped as `money` → `{amount, currency}`, `date` → `format: date`,
`reference` → `format: uri`, and so on. Concrete classes also get a `<Class>Object` schema
for the stored version. Errors are `application/problem+json` (RFC 9457), and the
security scheme is a bearer token. The extensions `x-ubos-namespace`,
`x-ubos-schema-version`, `x-ubos-port-kind` and `x-ubos-impl` carry what OpenAPI has no
member for. The `info.version` is derived from the schemas, so the same model gives the
same document.

**Model graph `ubos.model-graph.v1`.** A JSON object `{format, scope, nodes, edges}`:
- **Node:** `{id, kind (a kind or "trait"), namespace, abstract, schema_version,
  description, fields (own), ports (effective), linearization}`.
- **Edge:** `{from, to, type}`, where `type` is:
  - `extends`: the parent class;
  - `has_trait`: a trait the class takes;
  - `references`: a reference field to its `target_class`, with `field`;
  - `implemented_by`: a port the class declares and the logic asset that implements it,
    with `port`.

