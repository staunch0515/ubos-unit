---
id: UBS-STD-15
title: BPA Standard — UBTP Protocol (PROTO)
status: draft
phase: PH-0
depends_on: [UBS-STD-10, UBS-STD-14]
---

# BPA Standard — UBTP Protocol (PROTO) `[Profile: Protocol]`

## 1. Envelope and encoding

### STD-PROTO-001 — Message envelope
- **Clause:** Every UBTP message MUST be a `DAT-UbtpEnvelope`, encoded as JSON (UTF-8) or, when negotiated, as CBOR (RFC 8949) with the same data model. Messages are correlated by `id`, and responses carry `reply_to`. A message that is not a well-formed envelope, a request before the handshake, a duplicate `id`, a request carrying `reply_to`, an unknown `type` or a server type sent by the client MUST be answered with `error` `INT.PROTOCOL_ERROR`; an unsupported `v` with `INT.VERSION_UNSUPPORTED`.
- **Grammar / Schema:** `DAT-UbtpEnvelope`
- **Conformance vectors:** VER-CONF-5400…5419
- **Satisfies:** FR-INT-011
- **Notes:** Since 1.0-rc2 (DEC-068): `v` is `"1.0"`; `id` is unique within the session for the client's messages, and the server numbers its own frames; the body of an `error` frame is the error object of STD-ERR-001 (`code`, `message`, `details`); `trace` is carried into the reply unchanged. The in-process binding uses JSON only; CBOR is negotiated by the Server. Since 1.0-rc5 (DEC-082): the `error` answering a `mutate` also carries `process_id` when the request's process was recorded (STD-PROTO-010).

```yaml
DAT-UbtpEnvelope:
  v: { type: text, required: true, description: "protocol version M.m" }
  id: { type: text, required: true, description: "client-unique message id" }
  reply_to: { type: text, required: false, description: "id of the request (responses and errors)" }
  type: { type: enum, required: true, description: "handshake | mutate | query | subscribe | unsubscribe | emit | sync | ack | error | result | ping | pong | cancel" }
  body: { type: json, required: true, description: "type-specific body" }
  trace: { type: text, required: false, description: "W3C traceparent" }
```

### STD-PROTO-002 — Handshake
- **Clause:** The first message on a connection MUST be `handshake`. It carries the supported protocol versions, the ABI range, the widget-catalogue version (clients with UI), encodings, features and credentials (a bearer token, an mTLS-bound identity, or an API key). The server MUST answer with the chosen versions, the session ID, the principal and its authority, and limits. Incompatibility MUST produce `error` with `INT.VERSION_UNSUPPORTED` naming the supported range. A second handshake on an open session is `INT.PROTOCOL_ERROR`.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5420…5439
- **Satisfies:** FR-INT-013, AR-008
- **Notes:** Since 1.0-rc2 (DEC-068). The request body is `{versions, abi: {min, max}, encodings, features, credentials, widget_catalogue?}`; the result body `{version, abi, session_id, encoding, principal: {did, kind, authority}, limits: {frame_bytes, window, retention}, features}`. The in-process binding (STD-PROTO-020) takes the credentials `{kind: "in_process", principal?, principal_kind?, role?, grants?}` of a trusted caller: the principal defaults to the adapter's default principal, and absent `grants` leave the caller unrestricted while a list restricts it to those permissions (`commit.raw` among them); bearer tokens, mTLS identities and API keys are verified by the Server (PH-1 step 14). The reference limits are a 16 MB frame, a window of 8 and a retention of 100 commits.

## 2. Operations

### STD-PROTO-010 — `mutate`
- **Clause:** The body is `{vae, branch?, port: {target, name, params} | commit: {changes}, base?, idempotency_key, validate_only?, message?, reason?}`. The response `result` MUST carry `{process_id, commits, versions, warnings, result}`, or `error`. A `require` decision MUST return `result` with `pending: {pending_change_id, steps}`; a `deny` decision is `error` `RULE.DECISION_DENIED`. `validate_only` on a `commit` body MUST return `{validated: true, violations, warnings}` without committing (STD-LGS-030). `base`, when given, is the head `cseq` the client saw; a moved head is `TXN.HEAD_MOVED`. A repeated `idempotency_key` of the same principal MUST return the first result for the same payload and `TXN.IDEMPOTENCY_MISMATCH` for another (STD-TXN-040). Since 1.0-rc5, an `error` answering a `mutate` whose process was recorded (STD-TXN-001) MUST carry that process's identifier as `process_id` in the error body, so that the caller can read the record, journal and metering of the failed process (STD-PROTO-015).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5440…5469
- **Satisfies:** FR-FLOW-022, FR-TXN-031, FR-FLOW-042
- **Notes:** Raw `commit` bodies are allowed only for principals with `commit.raw` permission (import and Forge). Since 1.0-rc2 (DEC-068): `port` maps to one process of `execute` and `commit` to the adapter's `commit`, with the session's principal, principal kind, role and channel `api` as the process and context; `message` and `reason` go to the process (STD-LGS-011 `audit_level`); `process_id` is assigned by the implementation. The stored pending change of a `require` outcome and its approval come with PH-2 (FR-FLOW-042, DSN-DVM-740): in 1.0-rc2 `pending_change_id` is the process identifier and nothing is stored. `validate_only` on a `port` body is the simulation of PH-2 (`LOGIC.NOT_AVAILABLE`). Since 1.0-rc4 (DEC-079): the versioning requests of STD-PROTO-014 are further alternatives to `port` and `commit`. Since 1.0-rc5 (DEC-082): the error body is `{code, message, details?, process_id?}`; `process_id` is present when the request reached its process (a raw commit or a port call that failed validation, a guard, a limit or a decision, a merge, revert or bulk process that failed) and absent for errors found before (the envelope, the body, the grants, `base`, the idempotency key). The versioning requests carry it under the same rule.

### STD-PROTO-011 — `query`
- **Clause:** The body is `{vae, read: uri | query: DAT-Query | view: {target, mode} | explain | history | timeline | diff}` with coordinates `{branch?, time?, commit?, asof?}`, exactly one request kind. The response carries the items, the cursor and `evaluated_at`.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5470…5499
- **Satisfies:** FR-QRY-011, FR-TIME-034, FR-UX-011
- **Notes:** Since 1.0-rc2 (DEC-068): `read` resolves the URI as the operation `resolve`, the message's coordinates completing the selectors the URI lacks, and returns one item; `query` takes a `DAT-Query` or its text (STD-EXPR-010) and returns the matching objects as read items with `object_id` and `class`, paging with the query cursor; `explain` is `{target, kind: cascade | decision, property | port}` (STD-LGS-023, -031); `history` `{target, from?, to?}` and `timeline` `{target, from?, to?}` return the commits and the valid-time segments of the object (STD-TIME-014); `diff` `{from, to}` the `DAT-DiffEntry` items (STD-VER-030). `view` payloads come with the views of PH-1 step 13 (`LOGIC.NOT_AVAILABLE`). Since 1.0-rc4 (DEC-079): the introspection requests of STD-PROTO-015 are further request kinds.

### STD-PROTO-012 — `subscribe`
- **Clause:** The body is `{vae, branch?, target: uri | query: DAT-Query | events: {types?}, from_cursor?}`, exactly one of the three. The response is `{subscription_id, cursor}`. The server MUST send `emit` messages containing changes in commit order per object, each with a cursor. A client reconnecting with a cursor within the retention window MUST receive every missed change exactly once. Beyond the window, the server MUST send `error` `INT.CURSOR_EXPIRED`, and the client re-queries. `unsubscribe {subscription_id}` ends a subscription.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5500…5529
- **Satisfies:** FR-INT-014, NR-PERF-015
- **Notes:** Since 1.0-rc2 (DEC-068): a cursor is opaque text denoting a commit of the branch (the reference binding uses `<branch>:<cseq>`); a subscription starts at the head unless `from_cursor` is given. An `emit` body is `{subscription_id, cursor, changes}` for object and query subscriptions, one frame per commit, each change `{object_id, class, op: added | changed | ended, version_hash, cseq}`; a query subscription reports the changed objects of the query's class that match it after the commit; an events subscription emits `{subscription_id, cursor, events}` with the released effects of the commit (STD-TXN-050) filtered by `types`. The in-process binding delivers the emits of a commit with the response that caused it and, for commits made outside the session, on the next call.

### STD-PROTO-013 — Flow control and cancellation
- **Clause:** Clients MUST acknowledge `emit` batches (`ack` with `{subscription_id, cursor}`). Servers MUST NOT exceed the negotiated window of unacknowledged `emit` frames per subscription; further emits wait for an acknowledgement. `cancel {id}` MUST abort an in-flight request; a request already completed answers `result {cancelled: false}`. For a running process, cancellation propagates as `LOGIC.CANCELLED`. `ping` is answered with `pong`.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5530…5544
- **Satisfies:** FR-INT-014, FR-AI-022
- **Notes:** Since 1.0-rc2 (DEC-068): the window is the `limits.window` of the handshake; an `ack` releases every emit up to its cursor. The in-process binding handles each message to completion, so cancellation of a running process is verified with the Server (PH-1 step 14, SUITE-NOD-PROTO).

### STD-PROTO-014 — Versioning messages
- **Clause:** Since 1.0-rc4, a `mutate` body MAY carry, in place of `port` or `commit`, exactly one of the versioning requests below; the body's `branch` (default `main`) names the branch they act on, and `base`, `idempotency_key`, `message` and `reason` apply as in STD-PROTO-010. The response `result` MUST carry `{commits, versions, warnings, result}`, with `process_id` when the request runs a process (`merge`, `revert`, `bulk`); `commits` lists the identifiers of the commits the request made in order (empty when none), `versions` the version hashes of the objects they changed, and `result` the outcome of the operation named below.

  | Request | Body | Operation and `result` | Permission |
  |---|---|---|---|
  | `create_branch` | `{name, from?, time?}` | creates branch `name` from branch `from` (default: the body's `branch`) at its head or at transaction time `time` (STD-VER-010, -013); `{branch, head}` | `branch.create` |
  | `change_set` | `{action, id, source?, target?, title?, description?, resolutions?}` | the change set action `open`, `get`, `submit`, `approve`, `reject`, `merge` or `abandon` (STD-VER-020, -021); the approver of `approve` and `reject` is the session's principal; `{state, preview?, approvals?, merged_commit?, cseq?}` | `changeset.approve` for `approve` and `reject`, `branch.merge` for `merge` |
  | `merge` | `{source, resolutions?}` | merges branch `source` into the body's `branch` (STD-VER-050…058); the merge result | `branch.merge` |
  | `revert` | `{process_id? \| change_set? \| time? \| commit?, reversal_ids?, dry_run?}` | reverts the commits of a process, a merged change set or one commit of the body's `branch` (STD-VER-070, -071); `{reverted, dependents, commits?}` | `revert` |
  | `bulk` | `{changes, max_changes?, mode?, staging?, resume?}` | the bulk process of STD-TXN-070 on the body's `branch`, its identifier the `process_id`; `resume` names the `process_id` of a failed bulk process to resume; `{state, children, merge?, error?}` | `commit.raw` |

  A body with more than one of `port`, `commit` and the requests above, or with none, is `MODEL.INVALID_VALUE`; so is `validate_only` with a versioning request. A session restricted by its grants (STD-PROTO-002) that lacks the permission of the request MUST receive `IAM.DENIED` before anything is changed. The errors of the operation are returned as `error` frames.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5600…5649
- **Satisfies:** FR-INT-011, FR-VER-021, FR-VER-031, FR-VER-051, FR-VER-071, FR-TXN-015
- **Notes:** Since 1.0-rc4 (DEC-079): the requests map to the adapter operations of the same names (STD-CONF-005) with the session's principal, principal kind, role and channel `api` as the process; the emits of the commits they make follow the response (STD-PROTO-012). Change sets carry their `id` from the client because the change set is an object of the target branch (STD-VER-020). The pending change of a `require` decision and its approval remain PH-2 (STD-PROTO-010).

### STD-PROTO-015 — Introspection queries
- **Clause:** Since 1.0-rc4, a `query` body MAY carry, in place of the request kinds of STD-PROTO-011, exactly one of the introspection requests below, with the coordinates `{branch?, time?, commit?, asof?}` of STD-PROTO-011. The response MUST carry `items` and `evaluated_at` as in STD-PROTO-011, and the items MUST be those the named operation gives at the coordinates.

  | Request | Body | Items |
  |---|---|---|
  | `class` | `{name}` | one item: the effective class `name` at the coordinates, with the extensions in force (STD-CLS-012, STD-CLS-030) |
  | `dispatch` | `{class, port, after?}` | one item: the port resolution of STD-CLS-020 at the coordinates |
  | `commit_info` | `{}` | one item: the commit the coordinates select, `{branch, cseq, commit_id, tx_time, parents, rule_binding, process_id?, change_set?, resolutions?, reverts?, root}`, `root` the state root of its commit record (STD-VER-001, STD-PROOF-003) |
  | `merge_preview` | `{source}` | one item: the merge preview of the head of branch `source` into the head of the coordinates' branch (STD-VER-057); `time` and `commit` do not apply |
  | `ledger` | `{class, ledger_key}` | the entries of the ledger key at the coordinates in sequence order, filtered by `asof` (STD-KIND-020, -022) |
  | `process` | `{process_id, journal?, metering?}` | one item: the process record (STD-TXN-001), with its execution journal (STD-CTX-011) when `journal` is true and its metering record (STD-CTX-006) when `metering` is true |
  | `replay` | `{process_id}` | one item: the outcome of re-executing the process from its journal, `{outcome: identical}` or `{outcome: diverged, divergence}` (STD-CTX-012) |

  A body with more than one request kind, or with none, is `MODEL.INVALID_VALUE`. An unknown process or branch is `IAM.NOT_FOUND` and an unknown class `MODEL.REFERENCE_INVALID`, as for the operations.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5650…5699
- **Satisfies:** FR-INT-011, FR-QRY-013, FR-MODEL-025, FR-VER-057, FR-LEDG-042, FR-LOGIC-063, FR-LOGIC-072, FR-LOGIC-073
- **Notes:** Since 1.0-rc4 (DEC-079): the requests map to the adapter operations `class_at`, `dispatch` (over the class declarations and extensions in force), `commit_info` with the `root` of `commit_record`, `merge_preview`, `read_ledger`, `process_info`, `journal`, `metering` and `replay` (STD-CONF-005). `replay` re-executes without changing state; the test hooks of the adapter operation (`tamper`, `drop`) are not part of the protocol. A process record, journal and metering record are readable by the process's initiator and by principals with `process.read`; the in-process binding does not restrict them.

## 3. Bindings

### STD-PROTO-020 — Transport bindings
- **Clause:** Implementations MUST support the bindings below with identical semantics:

  | Binding | Mapping |
  |---|---|
  | WebSocket | subprotocol `ubtp.v1`, one message per frame |
  | HTTP | `POST /ubtp/v1` for request–response messages; `GET /ubtp/v1/stream?cursor=` as server-sent events for `emit` |
  | TCP | length-prefixed frames (4-byte big-endian length), TLS 1.3 |
  | in-process | direct calls on the host API (CTR-001) with the same message structures |
  | QUIC `[from PH-5]` | one bidirectional stream per request and one unidirectional stream per subscription, TLS 1.3 with mTLS |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5545…5579
- **Satisfies:** FR-INT-012, FR-INT-015, FR-SYNC-054
- **Notes:** Since 1.0-rc2 (DEC-068): the in-process binding is the adapter operation `ubtp` (STD-CONF-005), which feeds a message sequence to a session and returns the frames produced in order; its vectors verify that the same message structures give the results of the direct operations. **Phase note:** PH-1 step 10d (DEC-068): the WebSocket, HTTP and TCP bindings are implemented by the Server of step 14, which replays these vectors over each binding (SUITE-NOD-PROTO); `sync` frames come with PH-3.

### STD-PROTO-021 — REST facade mapping
- **Clause:** The REST facade MUST map resources and ports to UBTP as follows:
  - `GET /api/v1/{vae}/{Class}/{slug}` maps to a read;
  - `GET /api/v1/{vae}/{Class}?q=` maps to a query;
  - `POST /api/v1/{vae}/{Class}/{slug}/{port}` maps to a mutate.

  `Idempotency-Key` headers are carried through. Errors use RFC 9457 with the `code` and `explanation` extensions.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5580…5599
- **Satisfies:** FR-INT-021, FR-INT-023
- **Notes:** —
