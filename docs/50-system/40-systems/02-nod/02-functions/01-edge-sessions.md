---
id: UBS-SYS-NOD-02-01
title: UBOS Node — Edge, Protocols, Sessions and Identity
status: draft
phase: PH-1
depends_on: [UBS-SYS-NOD-01, UBS-STD-15]
---

# UBOS Node — Edge, Protocols, Sessions and Identity (DSN-NOD-0xx, DSN-NOD-1xx)

Reading order:
1. process architecture (001–008);
2. UBTP server (010–019);
3. REST facade and webhooks (030–037);
4. authentication and sessions (101–112);
5. provisioning (120–123);
6. secrets and keys (130–139);
7. API credentials and delegated clients (150–153).

## Process architecture

### DSN-NOD-001 — Async host with isolated execution pool
- **Statement:** `ubos-node` MUST run on a Tokio multi-threaded runtime for I/O and MUST dispatch DVM processes to the bounded execution pool (DSN-DVM-617). Blocking storage calls MUST use the async `sqlx` driver (PostgreSQL) or a dedicated SQLite writer thread. No handler MAY block an I/O thread for more than 1 ms.
- **Rationale:** Predictable latency under mixed load (NR-AVAIL-005).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given 64 CPU-bound processes and 5,000 idle subscriptions, when a health probe is sent, then it answers within 50 ms.
- **Verification:** BENCH
- **Origin:** NR-AVAIL-005, AR-032

### DSN-NOD-002 — Per-tenant fairness
- **Statement:** The execution pool MUST schedule work by weighted fair queuing across tenants, with weights from the tenant plan and per-tenant concurrency caps. A tenant exceeding its cap MUST queue, not fail, until the queue wait exceeds the profile limit, then fail with `OPS.QUOTA_EXCEEDED`.
- **Rationale:** FR-TEN-024, NR-PERF-022.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given one tenant sending 20× its fair share, when others run a standard workload, then their p95 latency rises by no more than NR-PERF-022 allows.
- **Verification:** BENCH
- **Origin:** FR-TEN-024, FR-EVT-034, NR-PERF-022

### DSN-NOD-003 — Stateless application nodes
- **Statement:** Every piece of node state MUST be either authoritative (in the store), operational and recoverable from the store (leases, cursors), or a cache. Killing any application node MUST lose no acknowledged work (AR-012).
- **Rationale:** Horizontal scaling and simple recovery.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a 3-node HA Server under load, when one node is killed every minute for an hour, then no acknowledged commit, job or event is lost.
- **Verification:** FAULT
- **Origin:** AR-012, FR-EVT-032, NR-DUR-001

### DSN-NOD-004 — Edition presets
- **Statement:** Server, Cell, Box and Worker editions MUST be produced from one code base with compile-time features and configuration presets. Features absent from an edition MUST be absent from its binary, not merely disabled.
- **Rationale:** FR-OPS-011, NR-MAINT-001.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given the Box binary, when inspected, then it contains no PostgreSQL driver or admin endpoint.
- **Verification:** INSP
- **Origin:** FR-OPS-011, FR-OPS-012

### DSN-NOD-005 — Minimal dependencies
- **Statement:** A single-node Server MUST run with only PostgreSQL and a file-system blob directory. The runtime store and object store MUST be optional (embedded in-memory runtime store and file-system blob adapter).
- **Rationale:** FR-OPS-012, NR-OPER-001.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a clean Linux host with PostgreSQL, when the Server is installed, then it is operational in under 30 minutes (NR-OPER-001).
- **Verification:** SCN
- **Origin:** FR-OPS-012, NR-OPER-001

### DSN-NOD-006 — Request pipeline
- **Statement:** Every inbound request MUST pass these stages: transport decode → authentication (session, token or mTLS) → context construction (tenant, VAE, principal, delegation, device, channel, trace) → rate limiting → DVM call → response encoding with error normalisation (AR-026). The context MUST be the only identity input to the DVM (AR-007).
- **Rationale:** One pipeline for every protocol.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the same operation through UBTP and REST, when traced, then both pass the same stages and produce the same DVM context.
- **Verification:** CONF, SEC
- **Origin:** AR-007, AR-026, FR-INT-015

### DSN-NOD-007 — Rate limiting
- **Statement:** The edge MUST apply token-bucket rate limits per principal, per API credential and per tenant, configured by plan, stored in the runtime store with local fallback buckets when it is unavailable. Rejections MUST carry a retry-after hint.
- **Rationale:** FR-INT-043.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a credential limited to 10 requests per second, when it sends 50, then 40 are rejected with retry-after.
- **Verification:** CONF
- **Origin:** FR-INT-043, FR-OPS-073

### DSN-NOD-008 — Tenant routing
- **Statement:** The edge MUST resolve the tenant and VAE from the hostname, UBTP handshake or token claims, and MUST open the VAE through the tenancy registry. Requests for suspended tenants MUST fail with `TEN.SUSPENDED`.
- **Rationale:** FR-TEN-013, FR-TEN-052.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a suspended tenant, when any user connects, then `TEN.SUSPENDED` is returned without opening the VAE.
- **Verification:** CONF
- **Origin:** FR-TEN-013, FR-TEN-052

## UBTP server

### DSN-NOD-010 — Bindings
- **Statement:** The UBTP server MUST implement the WebSocket, HTTP (request/response and server-sent events) and in-process bindings of STD-PROTO in PH-1b, and TCP with TLS 1.3 in PH-2. Semantics MUST be binding-independent (FR-INT-015).
- **Rationale:** FR-INT-011, FR-INT-012.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given the UBTP conformance suite, when run against every binding, then results are identical.
- **Verification:** CONF
- **Origin:** FR-INT-011, FR-INT-012, FR-INT-015

### DSN-NOD-011 — Handshake and version negotiation
- **Statement:** The handshake MUST authenticate the client, negotiate the protocol version and ABI (FR-INT-013), announce the widget catalogue version, and return session parameters. Unsupported versions MUST fail with `INT.VERSION_UNSUPPORTED` listing the supported range.
- **Rationale:** FR-INT-013, NR-COMPAT-002.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a client one minor version behind, when it connects, then it is served with the older version's semantics.
- **Verification:** CONF
- **Origin:** FR-INT-013, NR-COMPAT-002

### DSN-NOD-012 — Mutate
- **Statement:** A `Mutate` message MUST map to one DVM process (port invocation, instruction batch or change-set operation) with the message's idempotency key. The response MUST include the commit ID, `cseq` and warnings, or the error.
- **Rationale:** FR-INT-011, FR-TXN-031.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a network drop after the server commits, when the client retries with the same key, then it receives the original result.
- **Verification:** FAULT, CONF
- **Origin:** FR-INT-011, FR-TXN-031, AR-010

### DSN-NOD-013 — Query and paging
- **Statement:** A `Query` message MUST map to the DVM query API with snapshot cursors. The server MUST stream large results in frames of at most 256 KB.
- **Rationale:** FR-QRY-071.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a 100 MB result, when queried, then memory on the node stays under 64 MB for the request.
- **Verification:** BENCH
- **Origin:** FR-QRY-071, FR-INT-011

### DSN-NOD-014 — Subscriptions and fan-out
- **Statement:** `Subscribe` MUST register a live query or object subscription with a resumable cursor `(branch, cseq)`. After each commit, the node MUST compute affected subscriptions from `changes_since` (IF-DVM-018), re-authorize per subscriber and push `Emit` frames. Across nodes, commit notifications MUST travel over the runtime-store pub/sub, with a database poll fallback every 1 s.
- **Rationale:** FR-INT-014, NR-PERF-015.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given 10,000 subscriptions on a 3-node Server, when a matching commit occurs, then subscribers receive updates within NR-PERF-015.
  2. Given the runtime store down, when commits occur, then updates still arrive within 2 s.
- **Verification:** BENCH, FAULT
- **Origin:** FR-INT-014, NR-PERF-015, CTR-007

### DSN-NOD-015 — Resume after disconnect
- **Statement:** A client reconnecting with a subscription cursor MUST receive every change after the cursor (or a "reset required" signal if the cursor is older than the retention window), in commit order per object.
- **Rationale:** FR-INT-014, FR-EVT-014.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-FieldWorker
- **Acceptance:**
  1. Given a client offline for 10 minutes, when it resumes, then it receives all missed changes and no duplicates beyond the cursor.
- **Verification:** SIM
- **Origin:** FR-INT-014, FR-EVT-014

### DSN-NOD-016 — Backpressure
- **Statement:** Each connection MUST have a bounded send queue (default 1,000 frames). When full, the server MUST coalesce object updates to the latest version per object, and if still full, close the subscription with "reset required".
- **Rationale:** Slow clients do not exhaust node memory.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a client that stops reading, when 100,000 updates occur, then node memory for that connection stays bounded.
- **Verification:** FAULT
- **Origin:** NR-AVAIL-005

### DSN-NOD-017 — Emit for server push
- **Statement:** The server MUST use `Emit` frames for subscription updates, task and notification pushes, and session events (revocation, forced re-authentication).
- **Rationale:** One push channel.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a session revoked by an administrator, when revoked, then the client receives a revocation frame within NR-SEC-005.
- **Verification:** BENCH
- **Origin:** FR-IAM-102, NR-SEC-005

### DSN-NOD-018 — Message size and compression
- **Statement:** Frames MUST be limited to 16 MB; uploads above 8 MB MUST use the blob upload path (DSN-NOD-320). Frames MAY be compressed with zstd when negotiated.
- **Rationale:** Predictable resource use.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a 20 MB frame, when received, then the connection returns an error and remains usable.
- **Verification:** CONF
- **Origin:** FR-FILE-012

### DSN-NOD-019 — QUIC binding
- **Statement:** From PH-5 the server MUST offer a QUIC binding with connection migration for mobile clients.
- **Rationale:** Mobile resilience.
- **Priority:** Could · **Phase:** PH-5 · **Systems:** NOD
- **Personas:** PER-FieldWorker
- **Acceptance:**
  1. Given a mobile client switching from Wi-Fi to cellular, when streaming, then the session continues without re-handshake.
- **Verification:** SCN
- **Origin:** FR-INT-012

## REST facade and webhooks

### DSN-NOD-030 — Generated REST endpoints
- **Statement:** The REST facade MUST generate resource endpoints per class (`GET/POST/PATCH /v1/{vae}/{class}`, `GET /…/{id}`, history and timeline sub-resources) and port endpoints (`POST /…/{id}/ports/{port}`) from the model reflection. Each call MUST map to the same DVM operation as the UBTP equivalent.
- **Rationale:** FR-INT-021, CTR-004.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a fixture model, when the REST and UBTP conformance cases run, then results are equivalent.
- **Verification:** CONF
- **Origin:** FR-INT-021, FR-INT-015

### DSN-NOD-031 — OpenAPI and problem details
- **Statement:** The facade MUST serve an OpenAPI 3.1 document per VAE and API version (FR-INT-022), and map errors to RFC 9457 problem details carrying the platform code (FR-INT-023).
- **Rationale:** FR-INT-022, FR-INT-023.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given the served OpenAPI document, when linted, then there are no errors.
- **Verification:** CONF
- **Origin:** FR-INT-022, FR-INT-023

### DSN-NOD-032 — Idempotency and concurrency headers
- **Statement:** The facade MUST accept `Idempotency-Key` on POST and PATCH, and `If-Match` with the version ID for updates (mapping to the base version).
- **Rationale:** AR-010, STD-TXN-020.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a PATCH with a stale `If-Match`, when sent, then HTTP 412 with `TXN.HEAD_MOVED` is returned.
- **Verification:** CONF
- **Origin:** AR-010, STD-TXN-020

### DSN-NOD-033 — Signed outbound webhooks
- **Statement:** Outbound webhooks MUST be released from the outbox, signed with HMAC-SHA256 over `timestamp.body` using a per-endpoint secret (Standard Webhooks format), retried with exponential back-off for 24 hours, and dead-lettered afterwards.
- **Rationale:** FR-INT-031.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given an endpoint down for 2 hours, when it recovers, then all events are delivered in order per aggregate.
- **Verification:** FAULT, CONF
- **Origin:** FR-INT-031, FR-INT-052

### DSN-NOD-034 — Verified inbound webhooks
- **Statement:** Inbound webhook endpoints MUST verify the sender's signature scheme declared on the connector, reject replays older than 5 minutes, and start a handler process with an idempotency key derived from the sender's event ID.
- **Rationale:** FR-INT-032.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a replayed webhook, when received, then it is rejected, and no process starts.
- **Verification:** SEC
- **Origin:** FR-INT-032, AR-010

### DSN-NOD-035 — Webhook management
- **Statement:** Webhook subscriptions MUST be Definition objects (FR-INT-033) with endpoint, event filter, secret handle and state; the node MUST expose delivery history and manual redelivery.
- **Rationale:** FR-INT-033.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, STU
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a failed delivery, when redelivered manually, then it is sent with a new attempt number and the original event ID.
- **Verification:** CONF
- **Origin:** FR-INT-033

### DSN-NOD-036 — External brokers
- **Statement:** The node MUST publish events to external brokers (Kafka, AMQP, NATS) through broker connectors with at-least-once delivery and per-key ordering (FR-INT-051, FR-INT-052).
- **Rationale:** FR-INT-051, FR-INT-052.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD, BRG
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a Kafka broker restart, when events continue, then none is lost, and order per key is kept.
- **Verification:** FAULT
- **Origin:** FR-INT-051, FR-INT-052

### DSN-NOD-037 — CloudEvents representation
- **Statement:** Events delivered externally MUST use the CloudEvents 1.0 JSON format with UBOS extension attributes (`ubosbranch`, `uboscommit`, `uboscseq`) (FR-EVT-013).
- **Rationale:** Interoperability.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given an outbound event, when validated against the CloudEvents schema, then it passes.
- **Verification:** CONF
- **Origin:** FR-EVT-013

## Authentication and sessions

### DSN-NOD-101 — Local credentials
- **Statement:** Local password credentials MUST be stored as Argon2id hashes (memory ≥ 64 MB, iterations ≥ 3), with TOTP and WebAuthn second factors, and with brute-force protection by progressive delay and lockout per account and per source (FR-IAM-013).
- **Rationale:** FR-IAM-011, FR-IAM-013, NR-SEC-004.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given 20 failed logins in a minute, when a 21st is attempted, then it is delayed or locked, and a security event is logged.
- **Verification:** SEC
- **Origin:** FR-IAM-011, FR-IAM-013, NR-SEC-004

### DSN-NOD-102 — Box local unlock
- **Statement:** The Box MUST unlock its database key with an OS key-store-protected key and a local factor (PIN, biometrics or password) (FR-IAM-014).
- **Rationale:** FR-IAM-014, FR-SYNC-043.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-PersonalUser
- **Acceptance:**
  1. Given the database file copied to another machine, when opened, then it cannot be decrypted.
- **Verification:** SEC
- **Origin:** FR-IAM-014, FR-SYNC-043
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

### DSN-NOD-103 — OIDC and SAML federation
- **Statement:** The node MUST support OIDC (authorization code with PKCE) and SAML 2.0 (SP-initiated, signed assertions) per tenant, map claims to principals and groups, and support just-in-time provisioning (FR-IAM-023).
- **Rationale:** FR-IAM-021, FR-IAM-022, FR-IAM-023.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given Entra ID and Okta test tenants, when users sign in, then principals are created or matched, and groups are mapped.
- **Verification:** SCN, SEC
- **Origin:** FR-IAM-021, FR-IAM-022, FR-IAM-023, CTR-052

### DSN-NOD-104 — Step-up authentication
- **Statement:** When a DVM decision requires a stronger authentication level (for example for approvals above a threshold or break-glass), the node MUST challenge the user for step-up and record the level in the context (FR-IAM-024).
- **Rationale:** FR-IAM-024, NR-SEC-011.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given an approval requiring WebAuthn, when approved with a password session, then a step-up challenge is issued first.
- **Verification:** SEC
- **Origin:** FR-IAM-024, NR-SEC-011

### DSN-NOD-105 — Sessions
- **Statement:** Sessions MUST be server-side records with an opaque token, idle and absolute timeouts per session policy (FR-IAM-101), device binding and authentication level. Tokens MUST be stored hashed.
- **Rationale:** FR-IAM-101.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given an idle timeout of 15 minutes, when a session is idle for 16, then the next request requires re-authentication.
- **Verification:** SEC
- **Origin:** FR-IAM-101

### DSN-NOD-106 — Immediate revocation
- **Statement:** Revoking a session, principal, role or key MUST be published to all nodes (CTR-008) and take effect within NR-SEC-005. Each node MUST also re-check a session revocation list on every request with a local cache TTL no longer than the NR bound.
- **Rationale:** FR-IAM-102, NR-SEC-005.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a revoked user with open connections on 3 nodes, when revoked, then all connections are closed within the bound.
- **Verification:** BENCH, SEC
- **Origin:** FR-IAM-102, NR-SEC-005

### DSN-NOD-107 — Device registration and remote wipe
- **Statement:** Devices (Box, mobile) MUST be registered with a device key; a remote wipe command MUST be delivered at the next contact and trigger scope reduction to empty (DSN-DVM-954) and key destruction.
- **Rationale:** FR-IAM-103.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a wiped device, when it next connects, then local data is deleted, and the wipe is confirmed.
- **Verification:** SEC
- **Origin:** FR-IAM-103, FR-SYNC-042
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

### DSN-NOD-108 — Principal keys
- **Statement:** The node MUST create an Ed25519 signing key per principal at first need, custodial (wrapped by the tenant key in the key service) by default, or client-held for devices and agents that bring their own. Keys MUST be registered in the principal's key history under `did:ubos` (STD-ADDR-010).
- **Rationale:** FR-IAM-032, FR-IAM-033, FR-IAM-034.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a key rotation, when old commits are verified, then they verify against the historical key.
- **Verification:** CONF, SEC
- **Origin:** FR-IAM-032, FR-IAM-033, FR-IAM-034, STD-ADDR-010

### DSN-NOD-109 — Principal lifecycle
- **Statement:** Principal states (invited, active, suspended, deprovisioned) MUST be enforced at the edge and in the DVM context; deprovisioned principals keep their history and keys for verification.
- **Rationale:** FR-IAM-035.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a deprovisioned user, when their historical commits are verified, then verification succeeds, and they cannot sign in.
- **Verification:** SEC
- **Origin:** FR-IAM-035

### DSN-NOD-110 — Service principals per tenant
- **Statement:** Platform services (jobs, flows, agents, connectors, BRG, NTY) MUST act under tenant-scoped service principals with explicit grants (AR-009); no global superuser principal MAY exist for business data.
- **Rationale:** AR-009, AR-006.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a job in tenant A, when it attempts to read tenant B, then it is denied.
- **Verification:** SEC
- **Origin:** AR-009, AR-006, NR-SEC-003

### DSN-NOD-111 — Security event log
- **Statement:** Authentication events, session events, administrative actions, key operations and DVM decision logs MUST be written to a hash-chained `security_log` and exported to SIEM (FR-AUD-012, FR-AUD-013) within NR-OBS-003.
- **Rationale:** FR-AUD-012, FR-AUD-013, NR-SEC-010.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a deleted log row, when the chain is verified, then the gap is detected.
- **Verification:** SEC, FAULT
- **Origin:** FR-AUD-012, FR-AUD-013, FR-AUD-014, NR-SEC-010, NR-OBS-003, CR-NIST-004

### DSN-NOD-112 — Break-glass accounts
- **Statement:** Each tenant MAY define break-glass local accounts usable when the IdP is unavailable; use MUST require step-up, raise an alert and grant a time-bounded break-glass context (DSN-DVM-808).
- **Rationale:** AR-025, CTR-052 failure behaviour.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given the IdP down, when a break-glass account signs in, then an alert is raised, and access expires after the configured time.
- **Verification:** SEC
- **Origin:** AR-025

## Provisioning

### DSN-NOD-120 — SCIM 2.0 server
- **Statement:** The node MUST implement SCIM 2.0 Users and Groups endpoints per tenant, mapping to principals and group relations through DVM processes (FR-IAM-071).
- **Rationale:** FR-IAM-071.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given the SCIM compliance test suite, when run, then all required tests pass.
- **Verification:** CONF
- **Origin:** FR-IAM-071, CTR-052

### DSN-NOD-121 — Deprovisioning propagation
- **Statement:** A SCIM deactivation MUST suspend the principal, revoke sessions and API credentials, and reassign or flag open tasks, within NR-SEC-005 (FR-IAM-072).
- **Rationale:** FR-IAM-072.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a deactivated user with open sessions and tasks, when processed, then sessions end, and tasks are flagged.
- **Verification:** SCN
- **Origin:** FR-IAM-072

### DSN-NOD-122 — Group-to-role mapping
- **Statement:** IdP groups MUST map to roles through mapping objects per tenant (FR-IAM-073), evaluated at sign-in and on SCIM updates.
- **Rationale:** FR-IAM-073.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a user added to an IdP group, when SCIM pushes the change, then the mapped role is effective in the next process.
- **Verification:** CONF
- **Origin:** FR-IAM-073

### DSN-NOD-123 — Access review support
- **Statement:** The node MUST run access review campaigns (FR-IAM-082) as workflows that list grants per reviewer from who-can analysis (IF-DVM-047) and apply revocations as processes.
- **Rationale:** FR-IAM-082.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** NOD, DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a campaign, when a reviewer revokes a grant, then the revocation is committed with the campaign as reason.
- **Verification:** SCN
- **Origin:** FR-IAM-082

## Secrets and keys

### DSN-NOD-130 — Secret store and handles
- **Statement:** Secrets MUST be stored encrypted by the tenant key and referenced by handles; logic, sheets, views and APIs MUST only ever see handles (FR-IAM-091). Secret values MUST be resolved only inside connector execution and never logged (NR-SEC-009).
- **Rationale:** FR-IAM-091.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a log search for all secret values after the test run, when executed, then no match is found.
- **Verification:** SEC
- **Origin:** FR-IAM-091, NR-SEC-009
- **Phase note:** PH-1 step 7a (DEC-060): secrets of a tenant are sealed under the current data key with the handle as associated data (`secret` table) and resolved only inside connector execution through the `Secrets` service; the Server of gate 1b adds the operator interface.

### DSN-NOD-131 — Envelope encryption hierarchy
- **Statement:** The key hierarchy MUST be: root KEK (KMS, HSM or OS key store) → tenant KEK → tenant data keys (chunk AEAD, secret store) and subject keys (DSN-DVM-830). Data keys MUST be cached in memory for at most 1 hour.
- **Rationale:** FR-IAM-092, NR-SEC-006.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the root KEK revoked, when the cache expires, then encrypted reads fail with `IAM.KEY_UNAVAILABLE`.
- **Verification:** SEC, FAULT
- **Origin:** FR-IAM-092, NR-SEC-006
- **Phase note:** PH-1 step 7a (DEC-060): root KEK from `UBOS_ROOT_KEK` (or a file; development key otherwise) → tenant KEK stored wrapped in the registry → data keys and node signing keys derived with HKDF-SHA-256, never stored; data key generations rotate. A KMS or HSM root follows with the Server of gate 1b.

### DSN-NOD-132 — KMS adapters and customer-managed keys
- **Statement:** The key service MUST support AWS KMS, Azure Key Vault, Google Cloud KMS, HashiCorp Vault Transit, PKCS#11 HSMs and a local file-based KEK (development only). Customer-managed keys MUST be configurable per tenant (FR-IAM-093).
- **Rationale:** FR-IAM-093.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a tenant with a customer-managed key that the customer disables, when data is read, then reads fail, and the Control Plane shows the tenant as key-locked.
- **Verification:** SEC
- **Origin:** FR-IAM-093

### DSN-NOD-133 — Rotation
- **Statement:** Tenant KEKs MUST be rotatable online by re-wrapping data keys; data keys MUST be rotatable by writing new chunks with the new key and lazily re-encrypting on read or by a background job (FR-IAM-094). Content IDs never change on rotation (DSN-DVM-002).
- **Rationale:** FR-IAM-094.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a KEK rotation on a tenant with 10 M chunks, when completed, then all data is readable, and roots are unchanged.
- **Verification:** SEC, CONF
- **Origin:** FR-IAM-094, AR-020

### DSN-NOD-134 — Signing service
- **Statement:** The signing service (IF-DVM-055) MUST sign with custodial keys unwrapped in memory (at most 1 hour) or delegate to an HSM. Each signature operation MUST be counted per principal for anomaly detection.
- **Rationale:** DSN-DVM-520, NR-SEC-006.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given 1,000 commits per second, when signed with custodial keys, then signing adds under 0.2 ms p95.
- **Verification:** BENCH
- **Origin:** NR-SEC-006, STD-PROOF-010
- **Phase note:** PH-1 step 7a (DEC-060): the node signer of `ubos-node` signs commit records and verification records through the store's signer hook with keys derived from the tenant KEK; the key history is `node_key` and the DID is the VAE's registered form.

### DSN-NOD-135 — Subject key destruction
- **Statement:** Destroying a subject key MUST remove it from the key store, add a tombstone that key-store restores apply (DSN-DVM-832), and produce an erasure certificate.
- **Rationale:** FR-PROOF-073, FR-PROOF-074.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a key-store restore from before a destruction, when completed, then the tombstone is re-applied.
- **Verification:** FAULT, SEC
- **Origin:** FR-PROOF-073, FR-PROOF-074

## API credentials and delegated clients

### DSN-NOD-150 — API keys
- **Statement:** API keys MUST be random 256-bit secrets shown once, stored hashed, bound to a principal, scopes and optional IP ranges, with expiry (FR-IAM-012, FR-INT-082).
- **Rationale:** FR-IAM-012, FR-INT-082.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a key used from an unlisted IP, when called, then it is rejected, and a security event is logged.
- **Verification:** SEC
- **Origin:** FR-IAM-012, FR-INT-082

### DSN-NOD-151 — OAuth client credentials
- **Statement:** The node MUST act as an OAuth 2.1 authorization server for client credentials and authorization code with PKCE for delegated clients (FR-INT-081, FR-IAM-104), issuing short-lived access tokens (default 15 minutes) with scopes.
- **Rationale:** FR-INT-081, FR-IAM-104.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given an expired token, when used, then HTTP 401 is returned with a clear error.
- **Verification:** SEC, CONF
- **Origin:** FR-INT-081, FR-IAM-104

### DSN-NOD-152 — Rotation without downtime
- **Statement:** Credentials MUST support two concurrently valid secrets during a rotation window (FR-INT-083, FR-INT-044).
- **Rationale:** FR-INT-083.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a rotation, when both old and new secrets are used during the window, then both work; after it, only the new one.
- **Verification:** CONF
- **Origin:** FR-INT-083, FR-INT-044

### DSN-NOD-153 — Scope enforcement
- **Statement:** Credential scopes MUST be intersected with the principal's permissions in the DVM context, so a credential can never exceed its principal.
- **Rationale:** Least privilege for integrations.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a read-only scope, when a mutation is attempted, then it is denied even though the principal could write.
- **Verification:** SEC
- **Origin:** FR-INT-082, FR-IAM-041
