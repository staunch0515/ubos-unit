---
id: UBS-SYS-SDK-03
title: Client SDKs — Interfaces
status: draft
phase: PH-1
depends_on: [UBS-SYS-SDK-02]
---

# Client SDKs — Interfaces (IF-SDK-*)

The public API is shown in TypeScript form; other SDKs mirror it idiomatically.

### IF-SDK-001 — `connect`
- **Kind:** library API
- **Caller → Callee:** application → SDK
- **Request:** `connect({ url, vae, auth: { kind: 'oidc' | 'client_credentials' | 'api_key' | 'device', … }, transport?: 'ws' | 'http' | 'inproc' })`
- **Response:** `Client`
- **Errors:** `IntVersionUnsupported`, `IamDenied`, `TenSuspended`
- **Idempotency:** —
- **Authorization:** credentials supplied.
- **Satisfies:** FR-DEV-061, FR-INT-011

### IF-SDK-002 — Read and query
- **Kind:** library API
- **Caller → Callee:** application → SDK
- **Request:** `client.get(uri, { branch, asof, time, expand })`, `client.query(q, opts)` (async iterator), `client.aggregate`, `client.history`, `client.timeline`, `client.diff`, `client.explain`
- **Response:** typed objects with `version`, `commit`, `evidence`
- **Errors:** typed errors
- **Idempotency:** pure.
- **Authorization:** server-side.
- **Satisfies:** FR-QRY-011, FR-TIME-031

### IF-SDK-003 — Mutations
- **Kind:** library API
- **Caller → Callee:** application → SDK
- **Request:** `client.invoke(target, port, args, { branch, idempotencyKey?, deadline })`, `client.transaction(async tx => { tx.create(...); tx.update(...); tx.append(...) })`
- **Response:** `{ outcome, commitId, cseq, pendingChangeId, result, warnings }`
- **Errors:** typed errors
- **Idempotency:** automatic keys (DSN-SDK-002).
- **Authorization:** server-side.
- **Satisfies:** FR-FLOW-022, FR-TXN-012

### IF-SDK-004 — Versioning operations
- **Kind:** library API
- **Caller → Callee:** application → SDK
- **Request:** `client.branches.create/list/freeze`, `client.changeSets.open/submit/approve/merge`, `client.mergePreview`, `client.revert`
- **Response:** typed results
- **Errors:** `VerMergeConflict` and others
- **Idempotency:** keys per call.
- **Authorization:** server-side.
- **Satisfies:** FR-VER-021, FR-VER-031, FR-VER-057
- **Phase note:** PH-1 step 18c (DEC-079): the SDKs offer branch creation, change sets (open, submit, approve, reject, merge, abandon, get), merge preview, merge, revert and bulk processes over the versioning messages of BPA 1.0-rc4 (STD-PROTO-014), and the introspection queries of STD-PROTO-015. `branches.list` and `branches.freeze` follow when the Protocol carries branch listing and administration (IF-DVM-021); BPA 1.0-rc4 has no message for them.

### IF-SDK-005 — Subscriptions
- **Kind:** library API
- **Caller → Callee:** application → SDK
- **Request:** `client.subscribe(target, handler, { cursor? })`
- **Response:** `Subscription` with `cursor`, `close()`, events `change`, `reset`
- **Errors:** `IamNotFound`
- **Idempotency:** —
- **Authorization:** server-side per update.
- **Satisfies:** FR-INT-014

### IF-SDK-006 — Files
- **Kind:** library API
- **Caller → Callee:** application → SDK
- **Request:** `client.files.upload(blob, { field, onProgress })`, `client.files.download(ref, { range })`
- **Response:** content ID; byte stream
- **Errors:** quota and size errors
- **Idempotency:** resumable by upload ID.
- **Authorization:** server-side.
- **Satisfies:** FR-FILE-011
