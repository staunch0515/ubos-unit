---
id: UBS-SYS-NOD-02-02
title: UBOS Node — Events, Jobs and Platform Services
status: draft
phase: PH-1
depends_on: [UBS-SYS-NOD-02-01]
---

# UBOS Node — Events, Jobs and Platform Services (DSN-NOD-2xx, DSN-NOD-3xx)

Reading order:
1. outbox release and event delivery (201–209);
2. jobs, schedules and timers (220–231);
3. workers (240–245);
4. connectors (301–306);
5. files (320–327);
6. search index adapters (340–344);
7. notifications and action messages (360–366);
8. retention, holds and WORM (380–383).

## Outbox release and event delivery

### DSN-NOD-201 — Outbox releaser
- **Statement:** Each node MUST run an outbox releaser that claims ready rows through `outbox_claim` (IF-DVM-033) with a lease (default 30 s), delivers them to their destinations (subscriptions, event handlers, webhooks, notifications, brokers, connectors), and acknowledges. Releasers on several nodes MUST partition work by aggregate-key hash so that per-key order holds.
- **Rationale:** FR-TXN-052, FR-EVT-014.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given 3 nodes and 100,000 events over 1,000 keys, when released, then every event is delivered at least once, and per-key order holds.
- **Verification:** SIM, FAULT
- **Origin:** FR-TXN-052, FR-EVT-014, NR-DUR-006
- **Phase note:** PH-1 step 7a (DEC-060): the in-process host releases the pending effects after every operation (`Vae::release_outbox`) to the `OutboxSink` service and acknowledges them; the polling releaser with claims follows with the Server of gate 1b. PH-1 step 15a (DEC-076): the Server releases through `outbox_claim` and `outbox_ack` with a lease and per-key order, and delivers what a crash left pending after recovery.

### DSN-NOD-202 — Event log
- **Statement:** Declared events (FR-EVT-011) and standard lifecycle events (FR-EVT-012) MUST be appended to an event log table after release, retained per event-type policy (FR-EVT-015), and queryable by type, object and time.
- **Rationale:** FR-EVT-011, FR-EVT-012, FR-EVT-015.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given an event type with 90-day retention, when the sweep runs, then older events are removed, and newer ones stay.
- **Verification:** CONF
- **Origin:** FR-EVT-011, FR-EVT-012, FR-EVT-015
- **Phase note:** PH-1 step 7a (DEC-060): released effects are appended to the `event_log` table once per effect identifier; subscriptions and fan-out follow with the Server of gate 1b. PH-1 step 15a (DEC-076): the event log is queryable by type, object and time, with a retention sweep per event type.

### DSN-NOD-203 — Subscriptions and listener registry
- **Statement:** Subscription objects (FR-EVT-021) MUST be compiled into a listener registry keyed by event type and branch scope (FR-EVT-023), rebuilt on commit of any subscription object.
- **Rationale:** FR-EVT-021, FR-EVT-022, FR-EVT-023.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a subscription scoped to `main`, when the same event occurs on a work branch, then the handler is not invoked.
- **Verification:** CONF
- **Origin:** FR-EVT-021, FR-EVT-022, FR-EVT-023

### DSN-NOD-204 — Handlers as processes
- **Statement:** Every handler invocation MUST run as a DVM process with the handler's service principal, an idempotency key `(handler, event ID)` (DSN-DVM-538) and the event as trigger (FR-EVT-024).
- **Rationale:** FR-EVT-024, FR-TXN-033.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a handler crash after commit but before ack, when the event is redelivered, then no second commit occurs.
- **Verification:** FAULT
- **Origin:** FR-EVT-024, FR-TXN-033

### DSN-NOD-205 — Delivery masking
- **Statement:** Event payloads MUST be masked per recipient principal at delivery time (FR-IAM-063), not at commit time.
- **Rationale:** Recipients with different rights see different payloads.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given two webhook subscribers with different rights, when an event with a masked field is delivered, then only the privileged one receives the value.
- **Verification:** SEC
- **Origin:** FR-IAM-063, FR-NTF-012

### DSN-NOD-206 — Replay to a subscription
- **Statement:** Operators MUST be able to replay events of a time or `cseq` range to one subscription (FR-EVT-071), marked as replays so handlers with external effects can skip them (FR-EVT-073).
- **Rationale:** FR-EVT-071, FR-EVT-073.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a replay of 1,000 events to a notification subscription, when run, then no notifications are sent.
- **Verification:** CONF
- **Origin:** FR-EVT-071, FR-EVT-073

### DSN-NOD-207 — Projection backfill
- **Statement:** New asynchronous projections and search indexes MUST be backfilled by scanning committed state at a snapshot and then following commits from that snapshot (FR-EVT-072).
- **Rationale:** FR-EVT-072, FR-ANL-023.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a new index over 10 M objects with concurrent writes, when backfilled, then the final index equals a rebuild at the final head.
- **Verification:** SIM
- **Origin:** FR-EVT-072, FR-ANL-023

### DSN-NOD-208 — Became-effective scheduler
- **Statement:** A leader node per VAE MUST call `due_effective` (IF-DVM-030) every minute and emit became-effective events through a process with an idempotency key per segment.
- **Rationale:** DSN-DVM-325, NR-PERF-012.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given a leader failover at the due minute, when resolved, then exactly one event is emitted.
- **Verification:** FAULT
- **Origin:** FR-TIME-052, NR-PERF-012
- **Phase note:** PH-1 step 15 (DEC-076): the leader drives became-effective events every second, not every minute, to meet NR-PERF-012 (at most 5 s).

### DSN-NOD-209 — Dead letters
- **Statement:** Deliveries failing after their retry policy MUST move to a dead-letter set with the last error, visible to operators with retry, skip and edit-and-retry actions (FR-EVT-052).
- **Rationale:** FR-EVT-052.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a dead-lettered event, when retried after a fix, then it is delivered once.
- **Verification:** CONF
- **Origin:** FR-EVT-052
- **Phase note:** PH-1 step 15 (DEC-076): a dead letter is a job in state `dead`; operators retry, cancel or annotate it, and its payload is never edited (FR-EVT-052).

## Jobs, schedules and timers

### DSN-NOD-220 — Jobs as objects
- **Statement:** Jobs MUST be System-kind objects in the DVM store (FR-EVT-031) with a state, attempts, lease holder and lease expiry. The runtime store MAY hold queue hints, but a sweeper MUST recover every ready or expired job from the database (FR-EVT-032).
- **Rationale:** FR-EVT-031, FR-EVT-032.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given the runtime store wiped, when the sweeper runs, then all pending jobs resume.
- **Verification:** FAULT
- **Origin:** FR-EVT-031, FR-EVT-032
- **Phase note:** PH-1 step 7a (DEC-060): jobs in the `job` table with `enqueue`, `claim`, `complete` and `fail` through the `Jobs` service; jobs as governed objects and schedules follow with the Server of gate 1b. PH-1 step 15 (DEC-076): jobs are System-kind `Job` objects of the platform namespace; the lease, heartbeat and fencing token stay in the `job` table as the runtime hint.

### DSN-NOD-221 — Leases and heartbeats
- **Statement:** A job MUST be claimed with `UPDATE … SET lease_holder, lease_until WHERE state = ready AND (lease_until < now) … FOR UPDATE SKIP LOCKED` and heartbeated every lease/3 (FR-EVT-033). Results MUST be committed with a fencing check on the lease token.
- **Rationale:** No double execution after lease loss.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a paused node whose lease expired and was re-claimed, when it resumes and tries to commit, then its commit is rejected.
- **Verification:** SIM, FAULT
- **Origin:** FR-EVT-033
- **Phase note:** PH-1 step 7a (DEC-060): claims under a lease (`FOR UPDATE SKIP LOCKED`), expired leases reclaimable; heartbeats and fencing tokens follow with the Server of gate 1b. PH-1 step 15c (DEC-076): heartbeats every lease/3 and fencing tokens on result commits.

### DSN-NOD-222 — Fairness and limits
- **Statement:** Job claiming MUST round-robin across tenants and queues and respect per-tenant concurrency limits (FR-EVT-034).
- **Rationale:** FR-EVT-034.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given one tenant with 100,000 queued jobs, when another tenant enqueues one job, then it starts within 1 s.
- **Verification:** BENCH
- **Origin:** FR-EVT-034, NR-PERF-022

### DSN-NOD-223 — Retry policies and poison detection
- **Statement:** Jobs MUST follow declared retry policies (fixed, exponential with jitter, max attempts) (FR-EVT-051). A job failing with the same error signature N times (default 3) across different nodes MUST be marked poison and dead-lettered early (FR-EVT-053).
- **Rationale:** FR-EVT-051, FR-EVT-053.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a job that crashes its process deterministically, when retried, then it is marked poison after 3 attempts.
- **Verification:** CONF
- **Origin:** FR-EVT-051, FR-EVT-053
- **Phase note:** PH-1 step 7a (DEC-060): a failed attempt requeues the job until `max_attempts`, after which it is poison; backoff policies follow with the Server of gate 1b. PH-1 step 15c (DEC-076): backoff policies with jitter; poison is the same error signature three times across the attempts of one node.

### DSN-NOD-224 — Schedules
- **Statement:** Schedule objects (FR-EVT-041) with cron or business-calendar rules MUST be evaluated by a leader per VAE. Each slot MUST be recorded in `schedule_state` with a unique `(schedule, slot)` key before its job is enqueued, guaranteeing exactly one firing per slot (FR-EVT-043).
- **Rationale:** FR-EVT-041, FR-EVT-043.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given two nodes both believing they are leader during a lease handover, when a slot is due, then exactly one job is created.
- **Verification:** SIM, FORM
- **Origin:** FR-EVT-041, FR-EVT-043, CTR-008

### DSN-NOD-225 — Hot schedule changes, pause and resume
- **Statement:** Changes to schedules MUST take effect at the next evaluation without restart (FR-EVT-042). Paused schedules MUST record skipped slots and, on resume, apply the declared catch-up policy (none, last, all) (FR-EVT-044).
- **Rationale:** FR-EVT-042, FR-EVT-044.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given a schedule paused for 3 slots with catch-up "last", when resumed, then one job runs.
- **Verification:** CONF
- **Origin:** FR-EVT-042, FR-EVT-044
- **Phase note:** PH-1 step 15 (DEC-076): the catch-up policies are FR-EVT-041's `skip`, `fire_once` and `fire_all` (none, last and all).

### DSN-NOD-226 — Missed slots after outage
- **Statement:** After a full outage, the leader MUST compute missed slots since the last recorded slot and apply the catch-up policy.
- **Rationale:** Deterministic behaviour after downtime.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a 3-hour outage over an hourly schedule with catch-up "all", when restarted, then three jobs run in slot order.
- **Verification:** FAULT
- **Origin:** FR-EVT-043

### DSN-NOD-227 — Timer service
- **Statement:** The node MUST claim due timers through `timers_claim` (IF-DVM-034) at least every second and start the target step processes (DSN-DVM-751).
- **Rationale:** Workflow timers and escalations.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given 10,000 timers due in the same minute, when processed, then all fire within 60 s.
- **Verification:** BENCH
- **Origin:** FR-FLOW-063, FR-FLOW-082

### DSN-NOD-228 — Leadership leases
- **Statement:** Leadership (schedules, became-effective, sweeps) MUST use database rows with lease expiry and a monotonically increasing fencing token (CTR-008). Default lease 15 s, renewal every 5 s.
- **Rationale:** At most one leader per duty.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a network partition isolating the leader, when the lease expires, then another node takes over within 20 s, and the old leader stops acting.
- **Verification:** FAULT, FORM
- **Origin:** CTR-008, FR-OPS-062

### DSN-NOD-229 — Operator job controls
- **Statement:** Operators MUST be able to list, pause, cancel, re-prioritise and retry jobs per tenant and queue, with every action audited.
- **Rationale:** FR-EVT-052.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a cancelled running job, when cancelled, then its process is cancelled within NR-PERF-019, and the audit log records the operator.
- **Verification:** CONF
- **Origin:** FR-EVT-052, NR-PERF-019, CR-SOX-004

### DSN-NOD-230 — Bulk and saga coordination
- **Statement:** Bulk processes (DSN-DVM-560) and saga coordinators (DSN-DVM-564) MUST run as jobs, so they resume after node failure through the lease mechanism.
- **Rationale:** Uniform durability.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a saga coordinator node killed mid-saga, when the lease expires, then another node continues the saga.
- **Verification:** FAULT
- **Origin:** FR-TXN-015, FR-TXN-061
- **Phase note:** PH-1 step 15 (DEC-076): bulk processes run as jobs in PH-1; saga coordinators follow DSN-DVM-564 in PH-2.

### DSN-NOD-231 — Due-date and SLA jobs
- **Statement:** The node MUST run periodic jobs that evaluate task due dates and escalation chains (DSN-DVM-744) and emit reminders.
- **Rationale:** FR-FLOW-081, FR-FLOW-082.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given an overdue task, when the job runs, then the escalation step is applied once.
- **Verification:** CONF
- **Origin:** FR-FLOW-081, FR-FLOW-082

## Workers

### DSN-NOD-240 — Worker registration
- **Statement:** Workers MUST register with a worker principal, an attested capability profile (CPU, memory, available components) and allowed classification levels (FR-EVT-061).
- **Rationale:** FR-EVT-061.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given an unregistered worker, when it requests a job, then it is refused.
- **Verification:** SEC
- **Origin:** FR-EVT-061, CTR-070

### DSN-NOD-241 — Placement by classification
- **Statement:** Jobs MUST be offered only to workers whose allowed classification covers the job's inputs and whose capabilities match (FR-EVT-062). Inputs MUST be fetched through UBTP with the worker principal and classification checks.
- **Rationale:** FR-EVT-062.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a job with restricted inputs, when only internal-level workers are available, then it waits.
- **Verification:** SEC
- **Origin:** FR-EVT-062

### DSN-NOD-242 — Verified components only
- **Statement:** Workers MUST run only verified logic assets (FR-EVT-065), checked by hash and verification record before execution.
- **Rationale:** FR-EVT-065.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a tampered component on a worker, when a job starts, then it is refused.
- **Verification:** SEC
- **Origin:** FR-EVT-065, FR-LOGIC-054

### DSN-NOD-243 — Result verification
- **Statement:** Worker results MUST carry input and output hashes and the journal. The node MUST re-execute a configurable sample (default 1%) on a trusted node and compare hashes (FR-EVT-063).
- **Rationale:** FR-EVT-063.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a worker returning a wrong result, when sampled, then the mismatch is detected, and the worker is quarantined.
- **Verification:** SEC, FAULT
- **Origin:** FR-EVT-063, FR-EVT-064

### DSN-NOD-244 — Worker quarantine
- **Statement:** Workers with mismatches, repeated failures or expired attestations MUST be quarantined (FR-EVT-064) and their in-flight jobs re-queued.
- **Rationale:** FR-EVT-064.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a quarantined worker, when it requests work, then it is refused until released.
- **Verification:** CONF
- **Origin:** FR-EVT-064

### DSN-NOD-245 — Worker results commit on the server
- **Statement:** Workers MUST NOT commit directly; results are submitted to the server, which commits them in a process with the job's service principal after verification.
- **Rationale:** Only the commit path writes authoritative state (AR-004).
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a worker result, when committed, then the commit's author is the job's service principal, and the worker is recorded as executor.
- **Verification:** CONF
- **Origin:** AR-004, CTR-070

## Connectors

### DSN-NOD-301 — Connector runtime
- **Statement:** The connector runtime MUST implement `connector.call` (IF-DVM-053) for connector types HTTP/REST, SOAP, SFTP, SMTP/IMAP, database (read-only), message brokers and file drops (FR-INT-041), resolving secrets from handles only inside the call.
- **Rationale:** FR-INT-041, FR-LOGIC-101.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given one connector of each type against test endpoints, when called, then responses are returned and journaled.
- **Verification:** CONF
- **Origin:** FR-INT-041, FR-LOGIC-101
- **Phase note:** PH-1 step 7a (DEC-060): connectors are Rust functions registered in the node, called through the `Connectors` service with the tenant's secrets, and journaled by the kernel before use; the response-only replay of SCN-011 A1 is verified by SUITE-NOD-HOST. The connector runtime with external egress follows with the Server of gate 1b.

### DSN-NOD-302 — Egress through the allow-list
- **Statement:** All connector traffic MUST go through the egress path with a per-tenant allow-list of hosts and ports (AR-011). DNS resolution MUST be pinned per call to prevent rebinding, and private address ranges MUST be blocked unless explicitly allowed.
- **Rationale:** FR-LOGIC-104, AR-011.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a connector host that resolves to 169.254.169.254, when called, then the call is blocked.
- **Verification:** SEC
- **Origin:** FR-LOGIC-104, AR-011, NR-SEC-001
- **Phase note:** PH-1 step 7a (DEC-060): the registry of the node is the allow-list; an unregistered connector is `LOGIC.CONNECTOR_UNAVAILABLE`.

### DSN-NOD-303 — Resilience
- **Statement:** The runtime MUST apply per-connector timeouts, retries (only for idempotent operations), circuit breakers and bulkheads per tenant (FR-LOGIC-105).
- **Rationale:** FR-LOGIC-105.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given one slow connector, when called heavily, then other connectors of the same tenant are unaffected.
- **Verification:** FAULT
- **Origin:** FR-LOGIC-105

### DSN-NOD-304 — Mapping definitions
- **Statement:** Mapping definitions (FR-INT-042) MUST be compiled to expression-language transforms and evaluated in the DVM, so mappings are deterministic and replayable.
- **Rationale:** FR-INT-042.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, DVM
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given a mapping, when replayed with a recorded response, then the same objects result.
- **Verification:** CONF
- **Origin:** FR-INT-042

### DSN-NOD-305 — Bank and fund file formats
- **Statement:** The node MUST include parsers for US bank statements in BAI2 and ISO 20022 camt.053/camt.054, plus MT940/MT942 (FR-INT-071, FR-INT-072); generators for NACHA ACH files, ISO 20022 pain.001 and positive-pay files (FR-INT-074); and a configurable CSV/fixed-width profile for other institution files (FR-INT-073). Every parser MUST produce idempotent ingestion keys (DSN-DVM-446).
- **Rationale:** Basic Finance needs bank reconciliation and payments (DEC-022).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, BRG
- **Personas:** PER-Accountant
- **Acceptance:**
  1. Given sample BAI2, camt.053 and MT940 files, when ingested twice, then statement lines exist once, and parsing errors are reported per line.
  2. Given a payment run, when a NACHA file is generated, then it passes a NACHA format validator.
- **Verification:** CONF, SCN
- **Origin:** FR-INT-071, FR-INT-072, FR-INT-073, FR-INT-074, FR-LEDG-074

### DSN-NOD-306 — Credential lifecycle for connectors
- **Statement:** Connector credentials (API keys, OAuth tokens with refresh, certificates) MUST be refreshed automatically before expiry and alert on failure (FR-INT-044).
- **Rationale:** FR-INT-044.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given an OAuth token expiring in 5 minutes, when the refresh job runs, then a new token is stored, and calls continue.
- **Verification:** CONF
- **Origin:** FR-INT-044

## Files

### DSN-NOD-320 — Blob service and uploads
- **Statement:** Files MUST be uploaded by resumable chunked upload (tus-compatible), chunked and content-addressed per STD-OBJ-008, encrypted with the tenant data key, and made durable before the referencing commit (FR-FILE-011, FR-FILE-012).
- **Rationale:** FR-FILE-011, FR-FILE-012, CTR-006.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a 2 GB upload interrupted at 60%, when resumed, then only the remaining 40% is sent.
- **Verification:** FAULT
- **Origin:** FR-FILE-011, FR-FILE-012, FR-PROOF-014
- **Phase note:** PH-1 step 7a (DEC-060): the blob service is the tenant's `blob` table under row-level security (content-addressed paths deduplicate); uploads, S3-compatible stores and scanning follow with the Server of gate 1b.

### DSN-NOD-321 — Deduplication
- **Statement:** Identical chunks within a tenant MUST be stored once. Cross-tenant deduplication MUST NOT be used (it leaks information).
- **Rationale:** FR-PROOF-014, NR-SEC-003.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the same file uploaded by two tenants, when stored, then two physical copies exist.
- **Verification:** SEC
- **Origin:** FR-PROOF-014, NR-SEC-003

### DSN-NOD-322 — Malware scanning and type checks
- **Statement:** Uploaded files MUST be quarantined until scanned (ClamAV-compatible scanner service or cloud scanner adapter) (FR-FILE-021) and type-checked by content sniffing against the declared field types (FR-FILE-022). Infected files MUST be rejected and reported.
- **Rationale:** FR-FILE-021, FR-FILE-022.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the EICAR test file, when uploaded, then it is rejected, and no commit references it.
- **Verification:** SEC
- **Origin:** FR-FILE-021, FR-FILE-022

### DSN-NOD-323 — Previews
- **Statement:** The node MUST generate previews (images, PDF first pages, office document renderings through a sandboxed converter) asynchronously and store them as derived chunks (FR-FILE-031).
- **Rationale:** FR-FILE-031.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a DOCX upload, when processed, then a preview is available within 30 s.
- **Verification:** BENCH
- **Origin:** FR-FILE-031

### DSN-NOD-324 — Text extraction
- **Statement:** Text extraction (PDF, office formats, OCR for images when enabled) MUST run in a sandboxed process and feed the text index (FR-FILE-032), respecting classification.
- **Rationale:** FR-FILE-032.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a scanned PDF with OCR enabled, when processed, then its text is searchable.
- **Verification:** CONF
- **Origin:** FR-FILE-032

### DSN-NOD-325 — File classification and encryption
- **Statement:** Files MUST inherit the classification of their field; personal-classified files MUST be encrypted with the subject key (DSN-DVM-830) (FR-FILE-013).
- **Rationale:** FR-FILE-013.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a subject erasure, when the subject's ID document is read, then it is erased.
- **Verification:** SEC
- **Origin:** FR-FILE-013, FR-PROOF-071

### DSN-NOD-326 — Path aliases and mountable views
- **Statement:** The node MUST expose VFS path aliases over objects (STD-ADDR-004) (FR-FILE-041) and a WebDAV-compatible read-only mount of saved queries and document folders (FR-FILE-042).
- **Rationale:** FR-FILE-041, FR-FILE-042.
- **Priority:** Could · **Phase:** PH-3 · **Systems:** NOD
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a mounted folder, when opened in a desktop file manager, then files are listed with permission filtering.
- **Verification:** SCN
- **Origin:** FR-FILE-041, FR-FILE-042, STD-ADDR-004

### DSN-NOD-327 — Download links
- **Statement:** Download links MUST be short-lived signed URLs (default 5 minutes) bound to the principal and object version, and every download of personal-classified content MUST be logged.
- **Rationale:** Safe distribution of files.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given an expired link, when used, then it is refused.
- **Verification:** SEC
- **Origin:** FR-AUD-051, FR-AUD-052

## Search index adapters

### DSN-NOD-340 — Text index adapter
- **Statement:** The text index adapter MUST implement `index.text` (IF-DVM-056) with an embedded Tantivy index per tenant on Server and Box, and an OpenSearch adapter for Cell. Index documents MUST carry authorization attributes for residual filtering.
- **Rationale:** FR-QRY-021, FR-QRY-022.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given 1 M documents, when searched with a residual filter, then only visible hits are returned before the DVM re-check.
- **Verification:** CONF, BENCH
- **Origin:** FR-QRY-021, FR-QRY-022

### DSN-NOD-341 — Index freshness
- **Statement:** Indexers MUST follow commits through the outbox and meet NR-PERF-013 freshness; the index MUST expose its watermark `cseq`.
- **Rationale:** FR-QRY-023, NR-PERF-013.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given continuous writes, when searched, then the watermark lags by less than the NR target.
- **Verification:** BENCH
- **Origin:** FR-QRY-023, NR-PERF-013

### DSN-NOD-342 — Vector index adapter
- **Statement:** The vector adapter MUST implement `index.vector` with pgvector (Server) or an embedded HNSW index (Box), with embeddings computed through AGT only for classifications allowed to be embedded (FR-QRY-033).
- **Rationale:** FR-QRY-031, FR-QRY-033.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NOD, AGT
- **Personas:** PER-AiAgent
- **Acceptance:**
  1. Given a restricted field, when indexing, then no embedding is created.
- **Verification:** SEC
- **Origin:** FR-QRY-031, FR-QRY-033

### DSN-NOD-343 — Tenant isolation of derived stores
- **Statement:** Search indexes MUST be separate per tenant (separate index or partition with tenant filter enforced by the adapter) (FR-TEN-022).
- **Rationale:** FR-TEN-022.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the isolation test suite, when run against search, then no cross-tenant hit occurs.
- **Verification:** SEC
- **Origin:** FR-TEN-022, FR-TEN-023

### DSN-NOD-344 — Index rebuild
- **Statement:** Every derived index MUST be rebuildable from committed state (AR-015) with an online rebuild that swaps atomically.
- **Rationale:** AR-015.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a deleted index directory, when rebuilt, then search results equal those before deletion.
- **Verification:** FAULT
- **Origin:** AR-015, FR-ANL-021

## Notifications and action messages

### DSN-NOD-360 — Notification rules runtime
- **Statement:** Notification rules (FR-NTF-011) MUST be evaluated for released events and deadline timers, resolving recipients through saved queries and relations, and producing notification records.
- **Rationale:** FR-NTF-011.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a rule "notify fund managers when NAV is approved", when a NAV is approved, then each manager receives one notification.
- **Verification:** CONF
- **Origin:** FR-NTF-011

### DSN-NOD-361 — Permission-aware content
- **Statement:** Notification content MUST be rendered per recipient with that recipient's permissions and masking (FR-NTF-012); recipients who cannot see the object get no notification.
- **Rationale:** FR-NTF-012.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a recipient who lost access before delivery, when delivered, then no notification is sent.
- **Verification:** SEC
- **Origin:** FR-NTF-012, FR-IAM-063

### DSN-NOD-362 — Channels and fallbacks
- **Statement:** Channels MUST include in-app, e-mail (SMTP and provider APIs), mobile push (APNs, FCM), Slack and Microsoft Teams (FR-NTF-021), with declared fallbacks (FR-NTF-022).
- **Rationale:** FR-NTF-021, FR-NTF-022, CTR-071.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given push delivery failure, when fallback is e-mail, then an e-mail is sent.
- **Verification:** FAULT
- **Origin:** FR-NTF-021, FR-NTF-022

### DSN-NOD-363 — Delivery records
- **Statement:** Each delivery attempt MUST be recorded with channel, status, provider message ID and time (FR-NTF-013).
- **Rationale:** FR-NTF-013.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a notification, when inspected, then all attempts and statuses are visible.
- **Verification:** CONF
- **Origin:** FR-NTF-013

### DSN-NOD-364 — Preferences and digests
- **Statement:** Delivery MUST respect user preferences and quiet hours, and group low-priority notifications into digests on schedule (FR-NTF-031, FR-NTF-032).
- **Rationale:** FR-NTF-031, FR-NTF-032.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD, WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a user with a daily digest, when 20 low-priority events occur, then one digest is sent.
- **Verification:** CONF
- **Origin:** FR-NTF-031, FR-NTF-032

### DSN-NOD-365 — Action messages
- **Statement:** Action messages (FR-OFFICE-061) MUST carry signed, single-use, version-bound action tokens (FR-OFFICE-063). Executing the action MUST authenticate the user (or require sign-in) and run through the DVM like any action (FR-OFFICE-062).
- **Rationale:** FR-OFFICE-061, FR-OFFICE-062, FR-OFFICE-063.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given an approve link for version V, when the object changed to V+1, then the action is refused and the user sees the new version.
- **Verification:** SEC, CONF
- **Origin:** FR-OFFICE-061, FR-OFFICE-062, FR-OFFICE-063

### DSN-NOD-366 — Inbound e-mail
- **Statement:** Inbound e-mail to per-object addresses MUST create records or thread messages through a verified handler, with attachments scanned (FR-INT-061, FR-OFFICE-064).
- **Rationale:** FR-INT-061, FR-OFFICE-064.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-ContractManager
- **Acceptance:**
  1. Given an e-mail to a contract's address, when received, then it appears in the contract's thread.
- **Verification:** SCN
- **Origin:** FR-INT-061, FR-OFFICE-064

## Retention, holds and WORM

### DSN-NOD-380 — Retention jobs
- **Statement:** The node MUST run retention jobs that evaluate retention schedules (FR-AUD-071), skip objects under hold (FR-AUD-062), and dispose content through the DVM with certificates (FR-AUD-072).
- **Rationale:** FR-AUD-071, FR-AUD-072.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NOD, DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given 1,000 expired records, 10 under hold, when the job runs, then 990 are disposed with certificates.
- **Verification:** CONF
- **Origin:** FR-AUD-071, FR-AUD-072, FR-AUD-062, CR-SEC-004

### DSN-NOD-381 — WORM mode
- **Statement:** Classes marked WORM MUST have their chunks written to object-lock buckets in compliance mode with the retention period (FR-AUD-073).
- **Rationale:** FR-AUD-073, SEC 17a-4 style requirements.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a WORM record, when deletion is attempted through the storage API, then the object store refuses.
- **Verification:** SEC
- **Origin:** FR-AUD-073, CR-SEC-006

### DSN-NOD-382 — Hold lifecycle
- **Statement:** Hold objects (FR-AUD-061) MUST be applied by query scope and propagate to new matching objects until released (FR-AUD-063).
- **Rationale:** FR-AUD-061, FR-AUD-063.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NOD, DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a hold on a counterparty, when a new document for it is created, then it is under hold.
- **Verification:** CONF
- **Origin:** FR-AUD-061, FR-AUD-063

### DSN-NOD-383 — Retention reporting
- **Statement:** The node MUST report, per class and schedule, counts retained, due, held and disposed (FR-AUD-074).
- **Rationale:** FR-AUD-074.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given the report, when compared with the store, then counts match.
- **Verification:** CONF
- **Origin:** FR-AUD-074
