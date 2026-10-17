---
id: UBS-SYS-SDK-02
title: Client SDKs — Design Requirements
status: draft
phase: PH-1
depends_on: [UBS-SYS-SDK-01]
---

# Client SDKs — Design Requirements (DSN-SDK-*)

### DSN-SDK-001 — Layered client
- **Statement:** Every SDK MUST be layered as: transport (bindings) → session (handshake, authentication, reconnection) → operations (mutate, query, subscribe, upload, sync) → typed API (generated). Lower layers MUST be usable without generated types.
- **Rationale:** FR-DEV-061, AR-003.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** SDK
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given an application without generated types, when it calls a port by name, then it works through the operations layer.
- **Verification:** CONF
- **Origin:** FR-DEV-061, AR-003

### DSN-SDK-002 — Automatic idempotency keys
- **Statement:** The SDK MUST generate an idempotency key (UUIDv7) for every mutation unless the caller supplies one, and MUST reuse it on retries of the same logical call.
- **Rationale:** AR-010, FR-TXN-031.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** SDK
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a network failure after the server committed, when the SDK retries, then one commit exists, and the caller receives its result.
- **Verification:** FAULT
- **Origin:** AR-010, FR-TXN-031

### DSN-SDK-003 — Retry policy
- **Statement:** The SDK MUST retry only errors classified retryable by the error registry (for example transport failures, `NOD.STORAGE_UNAVAILABLE`, `TXN.HEAD_MOVED` for re-executable operations), with exponential back-off and jitter, bounded by a deadline set by the caller.
- **Rationale:** Correct retry semantics.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** SDK
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given `MODEL.CONSTRAINT_VIOLATED`, when returned, then the SDK does not retry.
- **Verification:** CONF
- **Origin:** AR-026, STD-TXN-023

### DSN-SDK-004 — Reconnection and subscription resume
- **Statement:** On disconnect the SDK MUST reconnect with back-off, re-authenticate, and resume subscriptions from their last acknowledged cursors; on "reset required" it MUST notify the application and refetch.
- **Rationale:** FR-INT-014.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** SDK
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a 30-second network loss, when restored, then the application receives all missed updates in order.
- **Verification:** SIM, FAULT
- **Origin:** FR-INT-014, FR-EVT-014

### DSN-SDK-005 — Snapshot-consistent paging
- **Statement:** Query results MUST be exposed as async iterators that follow snapshot cursors transparently.
- **Rationale:** FR-QRY-071.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** SDK
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given 10,000 results and concurrent writes, when iterated, then the application sees exactly the snapshot's rows.
- **Verification:** PROP
- **Origin:** FR-QRY-071, FR-QRY-072

### DSN-SDK-006 — Temporal and branch selectors as first-class options
- **Statement:** Every read API MUST accept `branch`, `asof`, `time` and `commit` selectors, and results MUST expose version ID, commit and evaluation evidence.
- **Rationale:** CAP-TIME-03, CAP-VER-10 supporting role.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** SDK
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given `asof` and `time`, when reading, then the returned evidence names both coordinates.
- **Verification:** CONF
- **Origin:** FR-TIME-031, FR-VER-101, FR-VER-104

### DSN-SDK-007 — Typed errors
- **Statement:** Errors MUST be exposed as typed values per module and code with details and explanation, generated from the error registry.
- **Rationale:** AR-026, NR-USE-006.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** SDK
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a merge conflict, when caught in TypeScript, then it is an instance of the typed `VerMergeConflict` with the conflict list.
- **Verification:** CONF
- **Origin:** AR-026, NR-USE-006

### DSN-SDK-008 — Client cache keyed by version
- **Statement:** The SDK MAY cache objects keyed by version ID and invalidate entries on subscription updates; cached reads MUST declare their freshness (cursor) to the application and MUST NOT be used as CAS bases unless marked current by a subscription.
- **Rationale:** AR-028.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** SDK
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a cached object updated elsewhere, when a subscription update arrives, then the cache entry is replaced before the next read.
- **Verification:** PROP
- **Origin:** AR-028

### DSN-SDK-009 — Optimistic UI support
- **Statement:** The TypeScript SDK MUST offer pending-mutation tracking: local optimistic state that is confirmed or rolled back by the server response, with the base version recorded.
- **Rationale:** Responsive UIs (NR-USE-001).
- **Priority:** Should · **Phase:** PH-2 · **Systems:** SDK, WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a rejected mutation, when the response arrives, then the optimistic state is rolled back, and the error is surfaced.
- **Verification:** CONF
- **Origin:** NR-USE-001

### DSN-SDK-010 — Authentication helpers
- **Statement:** SDKs MUST implement browser sign-in (OIDC redirect through NOD, PKCE), OAuth client credentials, API keys and device-bound tokens, with token refresh.
- **Rationale:** FR-INT-081, FR-IAM-104.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** SDK
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given an expiring access token, when a call is made, then the SDK refreshes it transparently.
- **Verification:** CONF, SEC
- **Origin:** FR-INT-081, FR-IAM-104
- **Phase note:** PH-1 step 17 (DEC-078): the PH-1 SDKs implement password sign-in, sessions with refresh and API keys; browser OIDC with PKCE and OAuth client credentials follow with DSN-NOD-103 and DSN-NOD-151 in PH-2, device-bound tokens with DSN-NOD-107 in PH-4.

### DSN-SDK-011 — Uploads and downloads
- **Statement:** SDKs MUST provide resumable upload and ranged download helpers with progress events and hash verification.
- **Rationale:** FR-FILE-011, FR-FILE-012.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** SDK
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given an interrupted upload, when resumed, then it continues from the last acknowledged offset.
- **Verification:** FAULT
- **Origin:** FR-FILE-011, FR-FILE-012

### DSN-SDK-012 — Generated typed API
- **Statement:** Generated code (DSN-FRG-102) MUST provide typed classes, port methods, query builders that compile to DAT-Query, and discriminated unions for polymorphic results.
- **Rationale:** FR-DEV-031, FR-MODEL-034.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** SDK, FRG
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a polymorphic query, when results are iterated in TypeScript, then narrowing by class works in the type checker.
- **Verification:** CONF
- **Origin:** FR-DEV-031, FR-MODEL-034

### DSN-SDK-013 — Decimal and money types
- **Statement:** SDKs MUST represent decimals and money with exact types (string-backed decimal libraries), never native floating point, and MUST reject floats at the API boundary.
- **Rationale:** STD-FND-002, STD-FND-022.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** SDK
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a JavaScript number passed as a money amount, when sent, then the SDK throws before sending.
- **Verification:** CONF
- **Origin:** STD-FND-002, STD-FND-022, FR-MODEL-042

### DSN-SDK-014 — Semantic versioning and compatibility
- **Statement:** SDKs MUST follow semantic versioning (FR-DEV-063) and support the protocol versions of the current and previous node minor (NR-COMPAT-002).
- **Rationale:** FR-DEV-063, NR-COMPAT-002.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** SDK
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given SDK N and node N−1, when the protocol suite runs, then it passes.
- **Verification:** CONF
- **Origin:** FR-DEV-063, NR-COMPAT-002
- **Phase note:** PH-1 step 17 (DEC-078): the SDKs negotiate the protocol version and pass the version-skew cases; the test against two node minors runs when protocol 1.1 exists.

### DSN-SDK-015 — Additional languages
- **Statement:** The Python SDK MUST follow in PH-3 (FR-DEV-062) with the same layers and conformance; further languages MUST pass the SDK protocol suite before being called official.
- **Rationale:** FR-DEV-062.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** SDK
- **Personas:** PER-DataAnalyst
- **Acceptance:**
  1. Given the Python SDK, when the protocol suite runs, then it passes.
- **Verification:** CONF
- **Origin:** FR-DEV-062

### DSN-SDK-016 — Telemetry propagation
- **Statement:** SDKs MUST propagate W3C trace context in UBTP frames and MUST NOT send business values to any telemetry endpoint.
- **Rationale:** NR-OBS-001, FR-OPS-042.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** SDK
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given an application trace, when a mutation is made, then the server spans join the same trace.
- **Verification:** CONF
- **Origin:** NR-OBS-001, FR-OPS-042

### DSN-SDK-017 — Bundle size (browser)
- **Statement:** The TypeScript SDK core MUST be tree-shakeable, with the browser core under 60 KB gzipped excluding generated types.
- **Rationale:** WSP performance.
- **Priority:** Should · **Phase:** PH-1 · **Systems:** SDK
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a minimal application, when bundled, then the SDK contributes less than 60 KB gzipped.
- **Verification:** BENCH
- **Origin:** NR-USE-001

### DSN-SDK-018 — Reference documentation completeness
- **Statement:** Every public UBTP message, instruction, SDK API and CLI command MUST have generated reference documentation with at least one example, and a CI check MUST fail when any public item lacks it (NR-MAINT-005).
- **Rationale:** NR-MAINT-005.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** SDK, FRG
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a new SDK method without an example, when CI runs, then the documentation check fails.
- **Verification:** CONF
- **Origin:** NR-MAINT-005
