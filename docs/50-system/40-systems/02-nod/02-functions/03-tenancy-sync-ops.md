---
id: UBS-SYS-NOD-02-03
title: UBOS Node — Tenancy, Sync and Operations
status: draft
phase: PH-1
depends_on: [UBS-SYS-NOD-02-02, UBS-STD-16]
---

# UBOS Node — Tenancy, Sync and Operations (DSN-NOD-4xx, DSN-NOD-5xx, DSN-NOD-6xx)

Reading order:
1. tenancy and VAEs (401–412);
2. genesis Buks and packages (420–423);
3. usage metering and licences on the node (430–433);
4. sync transport (501–509);
5. installation and configuration (601–606);
6. telemetry, health and diagnostics (610–616);
7. backup, restore and DR (620–627);
8. upgrades (630–634);
9. cluster coordination and admin endpoint (640–645).

## Tenancy and VAEs

### DSN-NOD-401 — Tenant registry
- **Statement:** The node MUST keep a tenant registry (tenant ID, state, plan, placement, keys reference, residency, VAEs) in a platform schema separate from business data. Tenant objects (FR-TEN-011) visible to tenant administrators MUST be projections of this registry into the tenant's VAE.
- **Rationale:** FR-TEN-011, AR-006.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a tenant plan change by CTL, when applied, then the tenant object in the VAE shows the new plan.
- **Verification:** CONF
- **Origin:** FR-TEN-011, AR-006
- **Phase note:** PH-1 step 7a (DEC-060): the `tenant` and `vae` tables, read by the node as the owner role.

### DSN-NOD-402 — Tenant creation
- **Statement:** Tenant creation (FR-TEN-051) MUST, idempotently: create the tenant KEK, register the tenant, create the first VAE through `create_vae` (IF-DVM-002), install genesis Buks, create the first administrator invitation, and emit a creation event. Each step MUST be resumable.
- **Rationale:** FR-TEN-051.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a crash after the VAE step, when creation is retried, then it completes without duplicate VAEs or keys.
- **Verification:** FAULT
- **Origin:** FR-TEN-051, FR-PKG-072, FR-TEN-012
- **Phase note:** PH-1 step 7a (DEC-060): `create_tenant` generates the tenant KEK wrapped by the root KEK; VAEs are created with their authority.

### DSN-NOD-403 — Suspension, archive and deletion
- **Statement:** Suspension MUST block sign-in and API access while keeping data and scheduled compliance jobs (FR-TEN-052). Archive MUST make the tenant read-only for export. Deletion MUST destroy the tenant KEK after the grace period and holds check, which renders all tenant data unreadable, and then physically purge rows and objects (FR-TEN-053).
- **Rationale:** FR-TEN-052, FR-TEN-053.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a deleted tenant after the grace period, when its backup is restored elsewhere, then no data can be decrypted.
- **Verification:** SEC
- **Origin:** FR-TEN-052, FR-TEN-053

### DSN-NOD-404 — Tenant configuration
- **Statement:** Tenant configuration (FR-TEN-054) that affects business behaviour MUST be stored as business data in the tenant's VAE (AR-027); only platform settings (placement, plan, keys) live in the registry.
- **Rationale:** FR-TEN-054, AR-027.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a change to a tenant's business calendar, when committed, then it is versioned and reviewable like any business data.
- **Verification:** SCN, INSP
- **Origin:** FR-TEN-054, AR-027

### DSN-NOD-405 — Storage isolation enforcement
- **Statement:** Every database session MUST set `ubos.tenant` before any query and reset it on return to the pool; the pool MUST refuse to hand out connections without a tenant for tenant-scoped work (FR-TEN-021).
- **Rationale:** FR-TEN-021, NR-SEC-003.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a code path that forgets to set the tenant, when it queries, then it gets zero rows, and a defect alert is raised.
- **Verification:** SEC
- **Origin:** FR-TEN-021, NR-SEC-003

### DSN-NOD-406 — Isolation test suite
- **Statement:** The node MUST ship an isolation test suite that creates two tenants and attempts cross-tenant access through every interface (UBTP, REST, search, files, events, webhooks, exports, workers) (FR-TEN-023).
- **Rationale:** FR-TEN-023.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the suite, when run in CI and against every release, then zero cross-tenant accesses succeed.
- **Verification:** SEC
- **Origin:** FR-TEN-023, NR-SEC-003

### DSN-NOD-407 — Overlay branches for customisation
- **Statement:** Tenant customisation of installed Buks MUST live on an overlay branch that reads through the Buk release (FR-TEN-031); Buk upgrades MUST be merged into overlays by a change set with the upstream report (FR-TEN-032, DSN-DVM-263), respecting overlay policies (FR-TEN-033).
- **Rationale:** FR-TEN-031, FR-TEN-032, FR-TEN-033.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, DVM
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given a customised fund class and a Buk upgrade, when upgraded, then the customisation is kept, and conflicts are shown in a change set.
- **Verification:** SCN
- **Origin:** FR-TEN-031, FR-TEN-032, FR-TEN-033, FR-PKG-041

### DSN-NOD-408 — Parent and child VAEs
- **Statement:** The node MUST support VAE hierarchies (FR-TEN-041) where parents publish policies and templates that cascade (FR-TEN-042) and may run aggregated queries across children within granted scopes (FR-TEN-043), subject to child autonomy settings (FR-TEN-044).
- **Rationale:** Group structures (fund families, subsidiaries).
- **Priority:** Should · **Phase:** PH-3 · **Systems:** NOD, DVM
- **Personas:** PER-Executive
- **Acceptance:**
  1. Given a parent with three children, when an aggregated AUM query runs, then it sums the children's authorised data.
- **Verification:** SCN
- **Origin:** FR-TEN-041, FR-TEN-042, FR-TEN-043, FR-TEN-044

### DSN-NOD-409 — Full and scoped export
- **Statement:** Tenant administrators MUST be able to export a full tenant (FR-TEN-061) or a scope (FR-TEN-063) in the portable archive format through DVM `export`, streamed to the object store with a signed manifest.
- **Rationale:** FR-TEN-061, FR-TEN-063, FR-PKG-082.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a full export, when imported into a fresh node (FR-TEN-062), then roots and query results are equal.
- **Verification:** CONF
- **Origin:** FR-TEN-061, FR-TEN-062, FR-TEN-063

### DSN-NOD-410 — Residency enforcement
- **Statement:** The node MUST refuse to place tenant data, backups, indexes or model calls in regions outside the tenant's residency policy (FR-TEN-073, NR-PRIV-004).
- **Rationale:** FR-TEN-073, NR-PRIV-004.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, CTL
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given an EU-resident tenant, when a backup target in the US is configured, then configuration is rejected.
- **Verification:** SEC
- **Origin:** FR-TEN-073, NR-PRIV-004

### DSN-NOD-411 — Online tenant migration (node side)
- **Statement:** For migration between Cells (FR-TEN-072) the node MUST support: export stream from the source, import on the target, catch-up of commits after the export snapshot, a write pause of at most NR-AVAIL-004, cut-over of routing, and source retirement. Each step MUST be idempotent and driven by CTL.
- **Rationale:** FR-TEN-072, NR-AVAIL-004.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD, CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a 50 GB tenant under write load, when migrated, then the write pause meets NR-AVAIL-004, and roots are equal on both sides at cut-over.
- **Verification:** FAULT, BENCH
- **Origin:** FR-TEN-072, NR-AVAIL-004

### DSN-NOD-412 — Personal to organisation onboarding
- **Statement:** A personal Box VAE MUST be attachable to an organisation Server as a scoped branch, with the user choosing what to share (FR-SYNC-024).
- **Rationale:** FR-SYNC-024.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-PersonalUser
- **Acceptance:**
  1. Given a personal VAE, when attached, then only the chosen scope is synced to the organisation.
- **Verification:** SCN
- **Origin:** FR-SYNC-024
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

## Genesis Buks and packages

### DSN-NOD-420 — Embedded genesis Buks
- **Statement:** The node binary MUST embed signed genesis Buks (ontology core, identity and access, notifications, files, tasks) (FR-PKG-071) and install them into new VAEs idempotently (FR-PKG-072).
- **Rationale:** FR-PKG-071, FR-PKG-072.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a VAE installed twice with the same genesis version, when compared, then roots are equal.
- **Verification:** CONF
- **Origin:** FR-PKG-071, FR-PKG-072

### DSN-NOD-421 — Genesis upgrades
- **Statement:** A node upgrade carrying newer genesis Buks MUST propose upgrade change sets per VAE rather than changing `main` directly (FR-PKG-073).
- **Rationale:** FR-PKG-073.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a node upgrade, when started, then each VAE has a pending genesis upgrade change set.
- **Verification:** CONF
- **Origin:** FR-PKG-073

### DSN-NOD-422 — Buk retrieval and verification
- **Statement:** The node MUST fetch Buks from EXC (CTR-054) or local files, verify signatures and manifests (`PKG.SIGNATURE_INVALID`, `PKG.MANIFEST_INVALID`), and install them through a change set.
- **Rationale:** Supply-chain integrity (NR-SEC-007).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a Buk with a modified file, when installed, then installation fails before any commit.
- **Verification:** SEC
- **Origin:** NR-SEC-007, CTR-054, CR-NIST-003

### DSN-NOD-423 — Air-gapped package import
- **Statement:** Air-gapped nodes MUST import Buks, licences and upgrades from signed bundles on removable media (FR-OPS-013, FR-PKG-083).
- **Rationale:** FR-OPS-013, FR-PKG-083.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given no network, when a bundle is imported, then its Buks install and its licence activates.
- **Verification:** SCN
- **Origin:** FR-OPS-013, FR-PKG-083

### DSN-NOD-424 — Ontology Buk
- **Statement:** The genesis `ontology` Buk MUST provide the base business ontology (FR-MODEL-091): party, person, organisation, role, account, instrument, location, document, agreement, obligation and event classes, with the party–role model (FR-MODEL-092). Its evolution MUST follow Buk versioning and overlay rules (FR-MODEL-094); tenants extend it and never redefine it (FR-MODEL-093).
- **Rationale:** A shared vocabulary for all Buks.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given the fund and contract Buks, when installed, then both extend the same party and role classes without redefining them.
- **Verification:** CONF, SCN
- **Origin:** FR-MODEL-091, FR-MODEL-092, FR-MODEL-093, FR-MODEL-094

## Usage metering and licences on the node

### DSN-NOD-430 — Usage records
- **Statement:** The node MUST aggregate DVM metering records, storage sizes, active users and AI costs into hourly usage records per tenant (FR-BILL-011), hash-chained per node (FR-BILL-012), and push them to CTL (CTR-051) or export them as files when air-gapped.
- **Rationale:** FR-BILL-011, FR-BILL-012.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD, CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a removed usage record, when CTL verifies the chain, then the gap is detected.
- **Verification:** SEC, CONF
- **Origin:** FR-BILL-011, FR-BILL-012, NR-COST-004

### DSN-NOD-431 — Entitlement enforcement
- **Statement:** The node MUST cache the tenant's entitlements from CTL and enforce them at the edge and in the DVM context (`BILL.NOT_ENTITLED`), with a grace period when CTL is unreachable (FR-BILL-022).
- **Rationale:** FR-BILL-022.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given CTL unreachable for 3 days, when users work, then entitlements continue under grace.
- **Verification:** FAULT
- **Origin:** FR-BILL-022

### DSN-NOD-432 — Licence checks never hold data hostage
- **Statement:** When a Buk licence lapses (`BILL.LICENCE_INACTIVE`), the node MUST disable the Buk's mutating ports but keep reads, exports and compliance jobs working (FR-PKG-082).
- **Rationale:** FR-PKG-081, FR-PKG-082.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a lapsed licence, when a user exports data, then the export succeeds.
- **Verification:** CONF
- **Origin:** FR-PKG-081, FR-PKG-082

### DSN-NOD-433 — Budgets and caps
- **Statement:** The node MUST enforce tenant budgets and hard caps for metered resources (AI spend, storage, compute) set in CTL (FR-BILL-051), warning at thresholds and blocking at caps with `OPS.QUOTA_EXCEEDED`.
- **Rationale:** FR-BILL-051, FR-OPS-073.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given an AI cap reached, when an agent calls a model, then the call is refused.
- **Verification:** CONF
- **Origin:** FR-BILL-051, FR-OPS-073

## Sync transport

### DSN-NOD-501 — Sync endpoint
- **Statement:** The Server MUST expose the UBTP `Sync` operation (CTR-060) that drives the DVM `sync_session` (IF-DVM-045): head exchange, have/want rounds, pack transfer in chunks of at most 4 MB, device-branch push and integration, rejection return.
- **Rationale:** FR-SYNC-011, STD-SYNC-001…004.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-FieldWorker
- **Acceptance:**
  1. Given the sync conformance suite, when run between Box and Server, then all cases pass.
- **Verification:** CONF
- **Origin:** FR-SYNC-011, FR-SYNC-012, STD-SYNC-001, STD-SYNC-002, STD-SYNC-003, STD-SYNC-004
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

### DSN-NOD-502 — Resumable and idempotent transfer
- **Statement:** Transfers MUST be resumable at pack-chunk granularity and idempotent by content address (FR-SYNC-014).
- **Rationale:** FR-SYNC-014.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-FieldWorker
- **Acceptance:**
  1. Given a connection dropped at 70% of a 500 MB sync, when resumed, then at most one chunk is resent.
- **Verification:** FAULT
- **Origin:** FR-SYNC-014
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

### DSN-NOD-503 — Authorised scope
- **Statement:** The Server MUST compute each device's scope from its sync scope object and the user's permissions, re-evaluated at each session (FR-SYNC-013, FR-SYNC-041); scope reductions MUST be pushed as wipe instructions (FR-SYNC-042).
- **Rationale:** FR-SYNC-013, FR-SYNC-041, FR-SYNC-042.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a user removed from a fund team, when the device next syncs, then the fund's data is removed from the device.
- **Verification:** SEC
- **Origin:** FR-SYNC-013, FR-SYNC-041, FR-SYNC-042
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

### DSN-NOD-504 — Background and scheduled sync
- **Statement:** The Box MUST sync in the background on connectivity changes and on schedule (FR-SYNC-023), with bandwidth and battery policies on mobile.
- **Rationale:** FR-SYNC-023.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-FieldWorker
- **Acceptance:**
  1. Given a device regaining connectivity, when detected, then sync starts within 30 s.
- **Verification:** SCN
- **Origin:** FR-SYNC-023
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

### DSN-NOD-505 — Governed integration of device branches
- **Statement:** Device branches MUST be integrated on the Server by a merge process under the device user's principal and the target branch's policies; if a change set is required, the integration MUST create one instead of merging (FR-SYNC-022).
- **Rationale:** FR-SYNC-022.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD, DVM
- **Personas:** PER-FieldWorker
- **Acceptance:**
  1. Given a device edit to a review-required class, when synced, then a change set is opened, and the user sees it pending.
- **Verification:** SCN
- **Origin:** FR-SYNC-022, FR-SYNC-033
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

### DSN-NOD-506 — Sync efficiency
- **Statement:** Sync MUST meet NR-PERF-018 by exchanging tree summaries top-down and transferring only missing nodes and chunks.
- **Rationale:** NR-PERF-018.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-FieldWorker
- **Acceptance:**
  1. Given the NR-PERF-018 workload, when synced, then the transferred bytes meet the target.
- **Verification:** BENCH
- **Origin:** NR-PERF-018
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

### DSN-NOD-507 — Cross-organisation sync transport
- **Statement:** The node MUST support sync between organisations' nodes (CTR-062) with mutual TLS using `did:ubos` identities and trust relationships from FED (FR-SYNC-053, FR-SYNC-054).
- **Rationale:** FR-SYNC-053, FR-SYNC-054.
- **Priority:** Must · **Phase:** PH-5 · **Systems:** NOD, FED
- **Personas:** PER-ExternalParty
- **Acceptance:**
  1. Given two organisations without a trust relationship, when sync is attempted, then it is refused.
- **Verification:** SEC
- **Origin:** FR-SYNC-053, FR-SYNC-054

### DSN-NOD-508 — Remote parent fallback
- **Statement:** A node whose parent VAE is remote MUST run the network fallback state machine (STD-SYNC-010): serve from the last synced parent state when unreachable, mark reads as possibly stale, and reconcile on reconnection (FR-SYNC-062, FR-SYNC-063).
- **Rationale:** FR-SYNC-061, FR-SYNC-062, FR-SYNC-063.
- **Priority:** Should · **Phase:** PH-5 · **Systems:** NOD, FED
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given the parent unreachable for 1 hour, when users work, then reads succeed with a staleness marker.
- **Verification:** SIM
- **Origin:** FR-SYNC-061, FR-SYNC-062, FR-SYNC-063, STD-SYNC-010

### DSN-NOD-509 — Sync observability
- **Statement:** The Server MUST report per device: last sync, lag, pending rejections and scope size.
- **Rationale:** Operators and users know device state.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a device offline for 3 days, when viewed, then its lag and last sync are shown.
- **Verification:** CONF
- **Origin:** FR-SYNC-023
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

## Installation and configuration

### DSN-NOD-601 — Installable packages
- **Statement:** The Server MUST ship as a static Linux binary (x86-64, ARM64), an OCI image, a Helm chart and a Docker Compose file; the Box as signed desktop installers and store packages (FR-OPS-011).
- **Rationale:** FR-OPS-011.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given each package, when installed on its target, then the installation smoke test passes.
- **Verification:** SCN
- **Origin:** FR-OPS-011, NR-PORT-002

### DSN-NOD-602 — Bootstrap
- **Statement:** First start MUST run a bootstrap that checks prerequisites (database version, extensions, object store access, clock sync, key service), applies storage migrations, creates the platform schema, and prints a one-time administrator link.
- **Rationale:** NR-OPER-001.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a missing PostgreSQL extension, when bootstrapped, then the error names the extension and the fix.
- **Verification:** SCN
- **Origin:** NR-OPER-001, NR-USE-006
- **Phase note:** PH-1 step 7a (DEC-060): the node applies the migrations at start before any tenant is opened.

### DSN-NOD-603 — Versioned node configuration
- **Statement:** Node configuration MUST be a versioned document (file or CTL-managed) with a schema, validated at start and reload (FR-OPS-091, NR-OPER-003). Every applied version MUST be recorded with its hash and author (FR-OPS-093).
- **Rationale:** FR-OPS-091, FR-OPS-093.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given an invalid configuration, when reloaded, then it is rejected, and the running configuration stays.
- **Verification:** CONF
- **Origin:** FR-OPS-091, FR-OPS-093, NR-OPER-003

### DSN-NOD-604 — Hot reload
- **Statement:** Keys marked reloadable MUST take effect without restart (FR-OPS-092); others MUST report "restart required".
- **Rationale:** FR-OPS-092.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a changed rate limit, when reloaded, then it applies to new requests within 5 s.
- **Verification:** CONF
- **Origin:** FR-OPS-092

### DSN-NOD-605 — Secrets in configuration
- **Statement:** Configuration MUST reference secrets by environment variable, file path or KMS reference, never inline values; the validator MUST reject inline secrets.
- **Rationale:** NR-SEC-009.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a configuration with an inline database password, when validated, then it is rejected.
- **Verification:** SEC
- **Origin:** NR-SEC-009

### DSN-NOD-606 — Air-gapped installation
- **Statement:** Installation MUST work without internet access from an offline bundle containing binaries, images, genesis Buks and the time-zone database (FR-OPS-013).
- **Rationale:** FR-OPS-013.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a host without network, when installed from the bundle, then it is operational.
- **Verification:** SCN
- **Origin:** FR-OPS-013

## Telemetry, health and diagnostics

### DSN-NOD-610 — OpenTelemetry
- **Statement:** The node MUST export traces, metrics and logs through OTLP (FR-OPS-041) with trace propagation from clients through the DVM spans (NR-OBS-001).
- **Rationale:** FR-OPS-041, NR-OBS-001.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a UBTP mutate, when traced, then the trace includes edge, DVM flush steps and storage spans.
- **Verification:** CONF
- **Origin:** FR-OPS-041, NR-OBS-001
- **Phase note:** PH-1 step 7a (DEC-060): one event per operation and node action, kept in a bounded ring (and on standard error with `UBOS_TELEMETRY=stderr`); OpenTelemetry export follows with the Server of gate 1b.

### DSN-NOD-611 — No business data in telemetry
- **Statement:** Telemetry MUST NOT contain field values, object names or document content (FR-OPS-042). A telemetry scrubber MUST enforce an allow-list of attribute keys.
- **Rationale:** FR-OPS-042, NR-SEC-009.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a canary value written in business data, when telemetry is searched, then it is not found.
- **Verification:** SEC
- **Origin:** FR-OPS-042, NR-SEC-009
- **Phase note:** PH-1 step 7a (DEC-060): events are redacted to an allow-list of operational members before they are kept; SUITE-NOD-HOST checks that business data is dropped.

### DSN-NOD-612 — Metrics catalogue and dashboards
- **Statement:** The node MUST publish the metrics catalogue (NR-OBS-002) and ship standard dashboards and alert rules (FR-OPS-043) with SLO burn alerts (NR-OBS-004).
- **Rationale:** FR-OPS-043, NR-OBS-002, NR-OBS-004.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given the shipped alert rules, when an SLO burn is simulated, then the alert fires.
- **Verification:** CONF
- **Origin:** FR-OPS-043, NR-OBS-002, NR-OBS-004

### DSN-NOD-613 — Health endpoints
- **Statement:** The node MUST expose liveness, readiness and startup endpoints (FR-OPS-051); readiness MUST reflect database, key service and object store reachability and degraded modes.
- **Rationale:** FR-OPS-051.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given the database down, when readiness is probed, then it reports not ready with the reason.
- **Verification:** FAULT
- **Origin:** FR-OPS-051, NR-AVAIL-006

### DSN-NOD-614 — Self-checks
- **Statement:** The node MUST run periodic self-checks (clock skew, certificate expiry, key access, disk space, backup age, DVM verifiers) (FR-OPS-052) and raise alerts.
- **Rationale:** FR-OPS-052.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a certificate expiring in 10 days, when self-checks run, then a warning is raised.
- **Verification:** CONF
- **Origin:** FR-OPS-052

### DSN-NOD-615 — Diagnostic bundles
- **Statement:** The node MUST produce a diagnostic bundle (configuration without secrets, versions, recent logs, metrics snapshot, self-check results) within NR-OPER-005 (FR-OPS-053).
- **Rationale:** FR-OPS-053, NR-OPER-005.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a bundle, when inspected, then it contains no secret or business value.
- **Verification:** SEC, BENCH
- **Origin:** FR-OPS-053, NR-OPER-005

### DSN-NOD-616 — Degraded modes
- **Statement:** The node MUST switch to read-only mode when the database refuses writes or storage degrades (NR-AVAIL-006), and to "effects paused" mode when the egress path fails, informing clients through `Emit` banners.
- **Rationale:** NR-AVAIL-005, NR-AVAIL-006.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given the database primary read-only during failover, when users read, then reads work, and writes show a clear message.
- **Verification:** FAULT
- **Origin:** NR-AVAIL-005, NR-AVAIL-006

## Backup, restore and DR

### DSN-NOD-620 — Consistent backups and point-in-time restore
- **Statement:** Server backups MUST combine PostgreSQL base backups with continuous WAL archiving and object-store versioning, giving point-in-time restore (FR-OPS-021) within NR-DUR-002 and NR-DUR-003.
- **Rationale:** FR-OPS-021, NR-DUR-002, NR-DUR-003.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a restore to 10 minutes ago, when completed, then the DVM verification of the last 1,000 commits passes.
- **Verification:** FAULT
- **Origin:** FR-OPS-021, NR-DUR-002, NR-DUR-003, CR-SOX-004

### DSN-NOD-621 — Box backups
- **Statement:** The Box MUST back up its encrypted database and blobs to a user-chosen target (local disk, cloud folder or Server) with online SQLite backup (FR-OPS-022).
- **Rationale:** FR-OPS-022.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-PersonalUser
- **Acceptance:**
  1. Given a Box restored from backup on a new device, when unlocked, then data equals the backup point.
- **Verification:** SCN
- **Origin:** FR-OPS-022
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

### DSN-NOD-622 — Restore drills
- **Statement:** The node MUST support automated restore drills into an isolated environment, reporting restore time and verification results (FR-OPS-023).
- **Rationale:** FR-OPS-023.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a monthly drill, when run, then a report with RTO and verification outcome is produced.
- **Verification:** SCN
- **Origin:** FR-OPS-023

### DSN-NOD-623 — Backup encryption and retention
- **Statement:** Backups MUST be encrypted with a backup key separate from the tenant KEKs, retained per NR-DUR-004, and stored in a separate failure domain.
- **Rationale:** NR-DUR-004, NR-SEC-004.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a backup copy, when inspected without the backup key, then it is unreadable.
- **Verification:** SEC
- **Origin:** NR-DUR-004, NR-SEC-004

### DSN-NOD-624 — Replication and failover
- **Statement:** HA Servers MUST use a PostgreSQL synchronous standby in another zone and automatic failover (Patroni or managed service) (FR-OPS-061). Nodes MUST reconnect to the new primary and resume without lost acknowledged commits.
- **Rationale:** FR-OPS-061, NR-AVAIL-002.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a primary crash under load, when failover completes, then every acknowledged commit is present.
- **Verification:** FAULT
- **Origin:** FR-OPS-061, NR-AVAIL-002, NR-DUR-001

### DSN-NOD-625 — Split-brain protection
- **Statement:** Writes MUST be possible only on the primary holding the current timeline; nodes MUST verify the database's primary status before each flush batch and refuse writes to a demoted primary (FR-OPS-062).
- **Rationale:** FR-OPS-062.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a network partition creating two primaries, when nodes write, then only the legitimate primary accepts commits.
- **Verification:** FAULT, FORM
- **Origin:** FR-OPS-062

### DSN-NOD-626 — Disaster recovery
- **Statement:** DR MUST restore a Server or Cell in another region from backups and the asynchronous replica within NR-DUR-002 and NR-DUR-003, with documented and tested procedures (FR-OPS-063).
- **Rationale:** FR-OPS-063.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a region loss drill, when executed, then service resumes within the targets.
- **Verification:** SCN
- **Origin:** FR-OPS-063, NR-DUR-002, NR-DUR-003

### DSN-NOD-627 — Effect delivery after recovery
- **Statement:** After restore or failover, the outbox MUST resume from its persisted state; effects released but not acknowledged before the failure MAY be re-delivered and MUST carry their original IDs (NR-DUR-006).
- **Rationale:** NR-DUR-006.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a failover during webhook delivery, when resumed, then receivers see duplicates only with identical event IDs.
- **Verification:** FAULT
- **Origin:** NR-DUR-006

## Upgrades

### DSN-NOD-630 — Rolling upgrades
- **Statement:** HA Servers and Cells MUST upgrade node by node with no downtime (FR-OPS-031, NR-AVAIL-003), with mixed versions supported for one minor step (NR-COMPAT-002).
- **Rationale:** FR-OPS-031, NR-AVAIL-003.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given continuous load during an upgrade, when completed, then no request fails beyond retryable errors.
- **Verification:** FAULT
- **Origin:** FR-OPS-031, NR-AVAIL-003, NR-COMPAT-002

### DSN-NOD-631 — Storage format evolution
- **Statement:** Physical schema migrations MUST follow expand–migrate–contract across releases (FR-OPS-032, AR-030); the node MUST refuse to start against a schema newer than it supports.
- **Rationale:** FR-OPS-032.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given an old node and a contracted schema, when started, then it refuses with a clear message.
- **Verification:** CONF
- **Origin:** FR-OPS-032, AR-030

### DSN-NOD-632 — Automatic rollback
- **Statement:** An upgrade step whose health checks fail within the watch window MUST roll back the binary automatically while the schema is still in the expand state (FR-OPS-033).
- **Rationale:** FR-OPS-033.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a faulty release, when deployed, then the nodes roll back without operator action.
- **Verification:** FAULT
- **Origin:** FR-OPS-033

### DSN-NOD-633 — Upgrade effort
- **Statement:** A single-node Server upgrade MUST be one command with automatic backup and verification (NR-OPER-002).
- **Rationale:** NR-OPER-002.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a minor upgrade, when run, then it completes with one command and a verification report.
- **Verification:** SCN
- **Origin:** NR-OPER-002

### DSN-NOD-634 — Box updates
- **Statement:** The Box MUST update through signed update channels (Tauri updater) with staged rollout and rollback.
- **Rationale:** NR-SEC-007.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-PersonalUser
- **Acceptance:**
  1. Given an unsigned update package, when offered, then it is refused.
- **Verification:** SEC
- **Origin:** NR-SEC-007
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

## Cluster coordination and admin endpoint

### DSN-NOD-640 — Cache invalidation
- **Statement:** Node-level caches of mutable state (sessions, entitlements, registry) MUST be invalidated through pub/sub messages (CTR-008), and fully reset if a node misses messages (sequence gap).
- **Rationale:** CTR-008, AR-028.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a dropped invalidation message, when the gap is detected, then the node resets the affected caches.
- **Verification:** FAULT
- **Origin:** CTR-008, AR-028

### DSN-NOD-641 — Admin endpoint
- **Statement:** The admin endpoint (CTR-050) MUST listen only in the management zone with mTLS for CTL's service identity, expose only operational operations (no business data) and audit every call (AR-006).
- **Rationale:** AR-006, AR-011.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a review of the admin API, when inspected, then no operation returns business data.
- **Verification:** INSP, SEC
- **Origin:** AR-006, AR-011, CTR-050

### DSN-NOD-642 — Heartbeat and version report
- **Statement:** Nodes MUST send heartbeats with version, health summary and capacity metrics to CTL every 30 s (CTR-051).
- **Rationale:** Fleet visibility.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD, CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a node stopped, when 90 s pass, then CTL shows it as missing.
- **Verification:** CONF
- **Origin:** CTR-051, FR-OPS-081

### DSN-NOD-643 — Customer-operated nodes
- **Statement:** Customer-operated nodes MAY connect to CTL for fleet services with an outbound-only connection, or run without CTL (FR-OPS-083).
- **Rationale:** FR-OPS-083.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a node behind a firewall allowing only outbound HTTPS, when connected, then CTL can manage it.
- **Verification:** SCN
- **Origin:** FR-OPS-083

### DSN-NOD-644 — Capacity reporting and tiering execution
- **Statement:** Nodes MUST report capacity per tenant (storage by category, commit rate, active sessions) (FR-OPS-071) and execute tiering policies decided by CTL (FR-OPS-072).
- **Rationale:** FR-OPS-071, FR-OPS-072.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a tiering policy "history older than 2 years", when executed, then the chunks move to the cold tier and remain provable.
- **Verification:** CONF
- **Origin:** FR-OPS-071, FR-OPS-072, FR-VER-092

### DSN-NOD-645 — Quotas
- **Statement:** The node MUST enforce per-tenant quotas (storage, objects, branches, API rate) (FR-OPS-073) with warnings at 80% and `OPS.QUOTA_EXCEEDED` at 100%.
- **Rationale:** FR-OPS-073.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a branch quota reached, when a branch is created, then it is refused.
- **Verification:** CONF
- **Origin:** FR-OPS-073
