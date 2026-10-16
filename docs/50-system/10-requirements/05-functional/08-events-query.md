---
id: UBS-REQ-05-08
title: Functional Requirements — EVT (Events, Jobs, Schedules, Workers) and QRY (Query and Search)
status: complete
phase: PH-1
depends_on: [UBS-REQ-05, UBS-REQ-05-04]
---

# Functional Requirements — EVT and QRY

## CAP-EVT-01 — Events

### FR-EVT-011 — Declared event types
- **Statement:** Classes MUST declare their business event types with payload schemas. Events MUST be emitted with the `emit.event` instruction and released only after commit (FR-TXN-051).
- **Rationale:** Decoupling through typed events (EXT-TRI "the nervous system").
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given an undeclared event type, when emitted, then it fails with `EVT.UNKNOWN_EVENT_TYPE`.
- **Verification:** CONF
- **Origin:** EXT-TRI, L40:SPEC-20

### FR-EVT-012 — Standard lifecycle events
- **Statement:** The platform MUST emit standard events `Created`, `Updated`, `Transitioned`, `Ended`, `EntryAppended` and `BecameEffective` (where declared) for classes that enable them, without custom logic.
- **Rationale:** Common reactions need no code.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given `Transitioned` enabled on `Invoice`, when approved, then an event with `from` and `to` states is emitted.
- **Verification:** CONF
- **Origin:** L40:SPEC-20

### FR-EVT-013 — CloudEvents representation
- **Statement:** Every event MUST have a CloudEvents 1.0 representation. The UBOS extension attributes are `ubosprocess`, `uboscommit`, `ubosvae`, `ubosbranch`, `ubosclass` and `ubosobject`.
- **Rationale:** Open standards at the edges (IMP-12).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, BRG
- **Personas:** PER-LogicDeveloper, PER-DataAnalyst
- **Acceptance:**
  1. Given any event, when serialised, then it validates against the CloudEvents JSON schema.
- **Verification:** CONF
- **Origin:** IMP-12

### FR-EVT-014 — Ordering guarantees
- **Statement:** Events of the same object MUST be delivered to each subscriber in commit order. No global order is guaranteed across objects.
- **Rationale:** Per-object consistency without a global bottleneck.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given 1,000 updates to one object, when delivered, then the subscriber observes them in commit order.
- **Verification:** SIM, CONF
- **Origin:** L40:SPEC-20

### FR-EVT-015 — Event log retention
- **Statement:** Emitted events MUST be stored in an event log with a retention policy per event type. The retention MUST be at least as long as the configured replay window.
- **Rationale:** Replay and audit (CAP-EVT-07).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a 30-day replay window, when an event is 29 days old, then it is replayable.
- **Verification:** CONF
- **Origin:** NEW

## CAP-EVT-02 — Subscriptions

### FR-EVT-021 — Subscription objects
- **Statement:** A subscription MUST be a Definition object with:
  - an event-type filter and an optional selector;
  - a handler (a port, logic asset, workflow start, connector or webhook);
  - a delivery policy (retries, ordering, concurrency) and an owner.
- **Rationale:** Listeners are first-class and visible (EXT-TRI).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a new subscription merged to `main`, when a matching event occurs, then the handler runs.
- **Verification:** CONF
- **Origin:** EXT-TRI, L40:SPEC-20
- **Phase note:** PH-1 step 15 (DEC-076): handlers of kind port and logic in PH-1; webhook, connector and workflow handlers follow in PH-2 with delivery masking (DSN-NOD-205).

### FR-EVT-022 — Listener registry
- **Statement:** The platform MUST answer "who listens to event type X" and "what does handler H listen to", including inherited and overlay subscriptions.
- **Rationale:** Understandable event flows.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, STU
- **Personas:** PER-LogicDeveloper, PER-BusinessArchitect
- **Acceptance:**
  1. Given 3 subscriptions to `NavPublished`, when queried, then all 3 are listed with owners.
- **Verification:** CONF
- **Origin:** EXT-TRI

### FR-EVT-023 — Branch-scoped subscriptions
- **Statement:** Subscriptions MUST fire only for events on the branches in their scope. Subscriptions committed on a draft MUST NOT fire for `main` events.
- **Rationale:** Drafts must not affect production.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a draft subscription, when a `main` event occurs, then no delivery happens.
- **Verification:** CONF
- **Origin:** L40:SPEC-14

### FR-EVT-024 — Handlers run as processes
- **Statement:** Each handler execution MUST run as its own process, with trigger lineage to the event and idempotency based on the delivery ID (FR-TXN-033).
- **Rationale:** Traceable asynchronous chains.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a handler commit, when its causality is queried, then the emitting process is its parent.
- **Verification:** CONF
- **Origin:** L40:SPEC-15

## CAP-EVT-03 — Durable jobs

### FR-EVT-031 — Jobs are objects
- **Statement:** Every asynchronous job MUST be a System-kind object with type, payload, state (`pending`, `leased`, `running`, `succeeded`, `failed`, `dead`, `cancelled`), attempts, lease owner and expiry, due time and priority.
- **Rationale:** Jobs are entities; queues are hints (L40 ADR-004).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given any queued job, when the job object is read, then its state and attempts are visible.
- **Verification:** CONF
- **Origin:** L40:SPEC-20

### FR-EVT-032 — Recovery without the runtime store
- **Statement:** When the runtime store (queues, caches) is lost, a sweeper MUST reconstruct every pending and leased job from job objects, with no job lost or run twice beyond its idempotency guarantee.
- **Rationale:** The runtime store is not authoritative.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given 10,000 pending jobs and a flushed runtime store, when the sweeper runs, then all 10,000 are re-queued.
- **Verification:** FAULT, SIM
- **Origin:** L40:SPEC-20

### FR-EVT-033 — Leases and heartbeats
- **Statement:** Running jobs MUST hold a lease renewed by heartbeat. An expired lease MUST return the job to `pending` for another attempt.
- **Rationale:** Recovery from crashed executors.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given an executor killed mid-job, when the lease expires, then the job is retried elsewhere.
- **Verification:** FAULT
- **Origin:** L40:SPEC-20

### FR-EVT-034 — Fairness and limits
- **Statement:** Job execution MUST enforce per-tenant concurrency limits and priorities, so that one tenant cannot starve others.
- **Rationale:** Multi-tenant fairness.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given tenant A with 100,000 queued jobs and tenant B with 10, when processed, then B's jobs start within the NR-PERF fairness bound.
- **Verification:** BENCH, SIM
- **Origin:** L40:SPEC-23

## CAP-EVT-04 — Schedules

### FR-EVT-041 — Schedule objects
- **Statement:** Schedules MUST be Definition objects with a cron expression or a business-calendar rule, a time zone, an active window, a misfire policy (`skip`, `fire_once`, `fire_all`) and a target (port, logic or workflow).
- **Rationale:** Rhythms are data (EXT-TRI "biological clock").
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-FundOperationsManager, PER-LogicDeveloper
- **Acceptance:**
  1. Given "18:00 America/New_York on business days", when DST changes, then the local firing time is preserved.
- **Verification:** CONF
- **Origin:** EXT-TRI, L40:SPEC-20

### FR-EVT-042 — Hot schedule changes
- **Statement:** A committed change to a schedule MUST take effect for the next firing without restart.
- **Rationale:** EXT-TRI "hot-swappable rhythms".
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given a daily schedule changed to hourly, when committed, then the next firing is within one hour.
- **Verification:** CONF
- **Origin:** EXT-TRI

### FR-EVT-043 — Exactly one firing per slot
- **Statement:** Each schedule slot MUST start exactly one process, even with several nodes and after failover.
- **Rationale:** No double NAV runs.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given 3 nodes and a failover at the slot time, when the slot passes, then exactly one process exists for it.
- **Verification:** SIM, FAULT
- **Origin:** L40:SPEC-20

### FR-EVT-044 — Pause and resume
- **Statement:** Authorised users MUST be able to pause and resume schedules. Missed slots MUST be handled by the misfire policy.
- **Rationale:** Operational control during incidents.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD, WSP
- **Personas:** PER-PlatformOperator, PER-FundOperationsManager
- **Acceptance:**
  1. Given a pause over 3 slots with policy `fire_once`, when resumed, then one catch-up firing occurs.
- **Verification:** CONF
- **Origin:** NEW

## CAP-EVT-05 — Retries and dead letters

### FR-EVT-051 — Retry policies
- **Statement:** Handlers and jobs MUST declare retry policies (maximum attempts, backoff, jitter, retryable error classes). The standard default is 5 attempts with exponential backoff from 1 s.
- **Rationale:** Transient failure handling (EXT-WP reactive retry).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a handler failing twice and then succeeding, when observed, then 3 attempts are recorded with backoff delays.
- **Verification:** CONF
- **Origin:** EXT-WP, L40:SPEC-20

### FR-EVT-052 — Dead letters and operator actions
- **Statement:** Items that exhaust their retries MUST move to `dead`, with the last error, and operators MUST be able to re-drive, cancel or annotate them. Payloads MUST NOT be editable.
- **Rationale:** Controlled recovery without tampering.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD, CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a dead job, when re-driven after a fix, then a new attempt is recorded and linked.
- **Verification:** CONF
- **Origin:** L40:SPEC-20

### FR-EVT-053 — Poison detection
- **Statement:** The platform MUST detect items that repeatedly crash executors and isolate them without blocking other items.
- **Rationale:** One bad message must not stop a queue.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a payload that crashes the handler process, when it has crashed 3 times, then it is isolated, and other items continue.
- **Verification:** FAULT
- **Origin:** NEW

## CAP-EVT-06 — Worker pool (enterprise-internal, DEC-002)

### FR-EVT-061 — Worker registration
- **Statement:** Organisation-owned machines MUST be registrable as workers with a principal identity, attested runtime version, capacity, availability windows and a data-classification clearance.
- **Rationale:** DEC-002.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD, CTL
- **Personas:** PER-PlatformOperator, PER-SecurityOfficer
- **Acceptance:**
  1. Given a workstation, when registered, then it appears with clearance `internal` and its windows.
- **Verification:** CONF, SEC
- **Origin:** DEC-002

### FR-EVT-062 — Placement by classification and capability
- **Statement:** A job MUST be placed only on workers whose clearance covers the highest classification of its input data and whose capabilities match its requirements.
- **Rationale:** Data protection (SCN-410).
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a `confidential` job and only `internal` workers, when scheduled, then it runs on the Server, never on a worker.
- **Verification:** CONF, SEC
- **Origin:** DEC-002

### FR-EVT-063 — Result verification
- **Statement:** Worker results MUST carry a hash of inputs and outputs. The platform MUST re-execute a configurable sample (default 5%) on a second executor, and results MUST match.
- **Rationale:** Trust in results from less controlled machines.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a tampered result, when the sample includes it, then the mismatch is detected, and the worker is quarantined.
- **Verification:** CONF, SEC
- **Origin:** DEC-002

### FR-EVT-064 — Quarantine
- **Statement:** A worker with a mismatch, an attestation failure or repeated crashes MUST be quarantined automatically, and its recent results MUST be re-verified.
- **Rationale:** Containment.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a quarantined worker, when jobs are scheduled, then none are placed on it.
- **Verification:** CONF
- **Origin:** DEC-002

### FR-EVT-065 — Only verified components on workers
- **Statement:** Workers MUST execute only verified, signed WASM components (FR-LOGIC-054) in the sandbox. Workers MUST NOT hold authoritative state.
- **Rationale:** Minimal trust surface on workers.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given an unverified component, when submitted as a worker job, then it is rejected.
- **Verification:** SEC
- **Origin:** DEC-002

## CAP-EVT-07 — Replay and backfill

### FR-EVT-071 — Event replay to a subscription
- **Statement:** Authorised users MUST be able to replay events from the log for a time range and event types to one subscription, marked with a `replay` flag.
- **Rationale:** New subscribers and repair.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-LogicDeveloper, PER-PlatformOperator
- **Acceptance:**
  1. Given 30 days of `NavPublished`, when replayed to a new subscription, then each event is delivered once with `replay: true`.
- **Verification:** CONF
- **Origin:** NEW

### FR-EVT-072 — Projection backfill
- **Statement:** A new or changed projection MUST be backfilled from authoritative history, with progress reporting, and MUST switch to live maintenance without a gap.
- **Rationale:** Analytics evolve (IMP-09).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-DataAnalyst
- **Acceptance:**
  1. Given a new projection over 5,000,000 entries, when backfilled during live writes, then the result equals a full rebuild at the switch point.
- **Verification:** CONF, PROP
- **Origin:** IMP-09

### FR-EVT-073 — Replay safety for external effects
- **Statement:** Handlers MUST receive the `replay` flag, and connector calls from replayed handlers MUST be suppressed unless the subscription explicitly allows them.
- **Rationale:** Replays must not resend payments or emails.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a replay to a handler that sends email, when replayed, then no email is sent.
- **Verification:** CONF
- **Origin:** NEW

## CAP-QRY-01 — Criteria queries

### FR-QRY-011 — Query language
- **Statement:** The platform MUST provide a typed criteria query language with:
  - field comparisons, logical operators and relationship paths;
  - class filters (polymorphic by default) and lifecycle states;
  - sorting and field projection;
  - limits and cursors.

  It MUST be expressible as JSON and as a textual form.
- **Rationale:** Uniform retrieval for UI, logic, agents and integration.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, SDK
- **Personas:** PER-LogicDeveloper, PER-BusinessUser
- **Acceptance:**
  1. Given "open invoices over 10,000 of customers in region West sorted by due date", when expressed and run, then the correct ordered set is returned.
- **Verification:** CONF
- **Origin:** L40:SPEC-21

### FR-QRY-012 — Authorization inside queries
- **Statement:** Queries MUST apply row filtering and field masking of the caller, and aggregates MUST count only visible rows.
- **Rationale:** No inference through queries.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a user who sees 3 of 10 matching rows, when querying with count, then the count is 3.
- **Verification:** CONF, SEC
- **Origin:** L40:SPEC-22

### FR-QRY-013 — Temporal and branch parameters
- **Statement:** Queries MUST accept `branch`, `commit`, `tag`, `time` and `asof` with the semantics of FR-VER-101 and FR-TIME-031.
- **Rationale:** Consistent temporal behaviour.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given the same query at two `asof` values, when run, then the results reflect the valid-time state at each.
- **Verification:** CONF
- **Origin:** IMP-02

### FR-QRY-014 — Index use and scan limits
- **Statement:** Query execution MUST use declared indexes and projections, and MUST refuse unindexed scans larger than the profile limit, with `QRY.SCAN_LIMIT` and a suggestion.
- **Rationale:** Predictable performance.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given an unindexed filter over 10,000,000 objects with a scan limit of 100,000, when queried, then it fails with a suggestion naming a field to index.
- **Verification:** CONF, BENCH
- **Origin:** NEW

### FR-QRY-015 — Same semantics in logic
- **Statement:** The `search.query` instruction MUST have exactly the same semantics as client queries, evaluated in the process snapshot.
- **Rationale:** No surprises between UI and logic.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given the query vectors, when run from logic and from the SDK, then the results are identical.
- **Verification:** CONF
- **Origin:** L40:SPEC-21

## CAP-QRY-02 — Full-text search

### FR-QRY-021 — Text indexing and ranking
- **Statement:** Classes MUST be able to declare text-indexed fields and documents. Search MUST return ranked results with highlights and support language-specific analysis for at least English.
- **Rationale:** Users find records by words.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given contracts containing "limitation of liability", when searched, then the matching contracts are ranked with highlighted passages.
- **Verification:** CONF, USE
- **Origin:** NEW

### FR-QRY-022 — Permission-filtered search
- **Statement:** Search results, counts, snippets and suggestions MUST respect the caller's permissions and masking.
- **Rationale:** Search is a classic leakage path.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a hidden record matching the term, when searched, then it contributes neither a result nor a snippet nor a count.
- **Verification:** SEC
- **Origin:** NEW

### FR-QRY-023 — Index freshness
- **Statement:** Search responses MUST include the index watermark, and the index MUST catch up within the NR-PERF bound after a commit.
- **Rationale:** Users know whether a just-created record is searchable yet.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a commit, when searching after the bound, then the record is found.
- **Verification:** BENCH
- **Origin:** NEW

## CAP-QRY-03 — Semantic search

### FR-QRY-031 — Vector indexes
- **Statement:** Classes and knowledge objects MUST be able to declare vector-indexed content. Embeddings are produced through the model gateway and stored as a rebuildable projection with the model version.
- **Rationale:** Retrieval for agents and copilot (IMP-10).
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM, AGT
- **Personas:** PER-AiAgent
- **Acceptance:**
  1. Given a model change, when the projection is rebuilt, then all vectors carry the new model version.
- **Verification:** CONF
- **Origin:** IMP-10

### FR-QRY-032 — Hybrid search
- **Statement:** Search MUST support combining vector similarity with text and criteria filters in one request.
- **Rationale:** Precise retrieval.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** DVM
- **Personas:** PER-AiAgent, PER-BusinessUser
- **Acceptance:**
  1. Given "clauses similar to X in active agreements", when searched, then only active agreements' clauses are ranked by similarity.
- **Verification:** CONF
- **Origin:** NEW

### FR-QRY-033 — Classification-aware embedding
- **Statement:** Content classified `sensitive_personal` MUST NOT be embedded, and `personal` content MUST be embedded only when the VAE policy allows it and the model gateway route keeps the data in approved regions.
- **Rationale:** Privacy (DEC-003).
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM, AGT
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a sensitive field declared for vector indexing, when committed, then it fails with `QRY.CLASSIFICATION_NOT_EMBEDDABLE`.
- **Verification:** CONF, SEC
- **Origin:** NEW

## CAP-QRY-04 — Aggregation

### FR-QRY-041 — Aggregate queries
- **Statement:** Queries MUST support grouping and the aggregate functions `count`, `sum`, `avg`, `min`, `max` and `percentile`, with temporal parameters.
- **Rationale:** Lists with totals, dashboards and reports.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-DataAnalyst
- **Acceptance:**
  1. Given invoices, when grouped by customer with sum, then the totals match a full scan.
- **Verification:** CONF, PROP
- **Origin:** L40:SPEC-21

### FR-QRY-042 — Currency-safe aggregation
- **Statement:** Aggregating `money` MUST group by currency unless an explicit conversion with FX-rate references is requested.
- **Rationale:** No meaningless cross-currency sums.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given USD and EUR amounts, when summed without conversion, then two totals are returned.
- **Verification:** CONF
- **Origin:** NEW

## CAP-QRY-05 — History queries

### FR-QRY-051 — Change queries
- **Statement:** The platform MUST query versions and changes by time range, author, process, class and changed field.
- **Rationale:** Audit and investigation (SCN-007).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given "all changes to `bank_account` fields in October by non-finance users", when queried, then exactly those changes are returned.
- **Verification:** CONF
- **Origin:** L40:SPEC-14

### FR-QRY-052 — Transition queries
- **Statement:** The platform MUST query lifecycle transitions (object, from, to, time, actor) over a range.
- **Rationale:** Process analytics and SLA measurement.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given approvals in October, when queried, then each transition to `approved` is returned with its actor.
- **Verification:** CONF
- **Origin:** NEW

## CAP-QRY-06 — Saved queries and list views

### FR-QRY-061 — Saved queries
- **Statement:** Users MUST be able to save parameterised queries as objects with an owner and sharing (private, role, VAE). Saved queries are versioned.
- **Rationale:** Reusable lists.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a shared saved query, when a colleague opens it, then it runs with the colleague's permissions.
- **Verification:** CONF
- **Origin:** NEW

### FR-QRY-062 — Reuse across surfaces
- **Statement:** Saved queries MUST be usable by list views, reports, dashboards, agents (as tools) and exports.
- **Rationale:** One definition, many uses.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-DataAnalyst
- **Acceptance:**
  1. Given a saved query, when used in a report and as an agent tool, then both return the same rows for the same principal.
- **Verification:** CONF
- **Origin:** NEW

## CAP-QRY-07 — Consistent pagination

### FR-QRY-071 — Snapshot cursors
- **Statement:** Pagination MUST use opaque cursors bound to the snapshot of the first page. Subsequent pages MUST reflect the same snapshot.
- **Rationale:** No duplicates or gaps under concurrent writes.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given concurrent inserts, when paging through 10,000 results, then the union equals the snapshot result set.
- **Verification:** PROP
- **Origin:** L40:SPEC-21

### FR-QRY-072 — Stable ordering
- **Statement:** Every sort order MUST be made total by appending the object UUID as a final tie-breaker.
- **Rationale:** Deterministic pagination.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given many rows with equal sort keys, when paged twice, then the order is identical.
- **Verification:** CONF
- **Origin:** NEW
