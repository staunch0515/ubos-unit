---
id: UBS-REQ-05-16
title: Functional Requirements — OPS (Operations) and BILL (Metering, Licensing, Billing)
status: complete
phase: PH-1
depends_on: [UBS-REQ-05, UBS-REQ-05-09]
---

# Functional Requirements — OPS and BILL

## CAP-OPS-01 — Installation and bootstrap

### FR-OPS-011 — Installable editions
- **Statement:** Each edition MUST have a supported installation path:
  - Box: signed desktop installers for Windows, macOS and Linux;
  - Server: OCI container images, Helm charts and a single-binary install for Linux;
  - Cell: infrastructure-as-code modules for the Control Plane.

  Bootstrap MUST follow SCN-001.
- **Rationale:** Adoption depends on easy installation.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator, PER-PersonalUser
- **Acceptance:**
  1. Given a clean Linux host, when the single binary is installed with a PostgreSQL URL, then SCN-001 passes.
- **Verification:** SCN
- **Origin:** L40:SPEC-31
- **Phase note:** PH-1 step 16 (DEC-077): the Server ships as a static Linux binary, an OCI image, a Helm chart and a Docker Compose file; the Box installers follow the Box in PH-4 (DEC-024).

### FR-OPS-012 — Minimal dependencies
- **Statement:** The Box MUST run with no external services. The Server MUST require only PostgreSQL and an S3-compatible object store. The runtime store MAY be embedded for small installations.
- **Rationale:** Operational simplicity.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a Server without Redis, when started in `small` mode, then it runs with the embedded runtime store.
- **Verification:** SCN
- **Origin:** L40:SPEC-31
- **Phase note:** PH-1 step 16 (DEC-077): PH-1 keeps blobs in PostgreSQL; the S3-compatible object store follows in PH-2.

### FR-OPS-013 — Air-gapped installation
- **Statement:** The Server MUST install and upgrade without Internet access, from signed offline bundles.
- **Rationale:** Regulated and sovereign deployments.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given an isolated network, when installed from a bundle, then the installation completes and verifies signatures.
- **Verification:** SCN
- **Origin:** EXT-TRI

## CAP-OPS-02 — Backup and restore

### FR-OPS-021 — Consistent backups and point-in-time restore
- **Statement:** The Server MUST support continuous backup of authoritative state and point-in-time restore to any instant within the retention window. The restored state MUST verify against the recorded head roots (SCN-017).
- **Rationale:** Durability.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given SCN-017, when executed, then the roots match.
- **Verification:** SCN, FAULT
- **Origin:** L40:SPEC-31

### FR-OPS-022 — Box backups
- **Statement:** The Box MUST offer encrypted backups to a user-chosen location and restore with verification.
- **Rationale:** Personal data safety.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-PersonalUser
- **Acceptance:**
  1. Given a backup, when restored on a new machine, then all data and history verify.
- **Verification:** SCN
- **Origin:** NEW
- **Phase note:** moved to PH-4 by DEC-024 and DEC-025 (SQLite, Box, devices and sync).

### FR-OPS-023 — Restore drills
- **Statement:** The platform MUST support automated restore drills into an isolated environment, with a report of verification results.
- **Rationale:** Untested backups are not backups.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD, CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a quarterly drill schedule, when run, then a signed drill report exists.
- **Verification:** SCN, AUDIT
- **Origin:** NEW

## CAP-OPS-03 — Upgrade

### FR-OPS-031 — Rolling upgrades
- **Statement:** Server clusters MUST upgrade node by node without downtime, within a compatibility window of adjacent minor versions (SCN-405).
- **Rationale:** Availability.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given SCN-405, when executed, then no failed client requests remain after retries.
- **Verification:** SCN, FAULT
- **Origin:** L40:SPEC-31

### FR-OPS-032 — Storage format evolution
- **Statement:** Storage-format changes MUST be backward-readable by the new version and MUST be applied lazily or by online migration. Blocking data conversions are not allowed.
- **Rationale:** No maintenance windows.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, DVM
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a new storage format, when upgraded, then old data remains readable immediately.
- **Verification:** CONF
- **Origin:** NEW

### FR-OPS-033 — Automatic rollback
- **Statement:** Failed post-upgrade health checks MUST trigger automatic rollback of the affected node and halt the plan.
- **Rationale:** SCN-405 A1.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given an injected health failure, when upgrading, then the node rolls back.
- **Verification:** FAULT
- **Origin:** NEW

## CAP-OPS-04 — Telemetry

### FR-OPS-041 — OpenTelemetry
- **Statement:** Nodes MUST emit metrics, traces and logs through OpenTelemetry. Traces MUST propagate across UBTP calls, processes, jobs and connector calls, with the process ID as an attribute.
- **Rationale:** Standard observability (IMP-12).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD, DVM
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a user action that triggers a job, when traced, then one trace links the request, commit and job.
- **Verification:** CONF
- **Origin:** IMP-12

### FR-OPS-042 — No business data in telemetry
- **Statement:** Telemetry MUST NOT contain business field values, except explicitly allow-listed non-sensitive identifiers.
- **Rationale:** Operators must not read business data.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a log scan for field values in the test corpus, when run, then there are no matches.
- **Verification:** SEC
- **Origin:** NEW

### FR-OPS-043 — Standard dashboards and alerts
- **Statement:** The platform MUST ship standard dashboards and alert rules for SLOs, saturation, errors, queues, projections lag, sync and anchoring.
- **Rationale:** Operability from day one.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD, CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given an installation, when monitoring is connected, then the dashboards render.
- **Verification:** INSP
- **Origin:** NEW

## CAP-OPS-05 — Health and diagnostics

### FR-OPS-051 — Health endpoints
- **Statement:** Nodes MUST expose liveness, readiness and dependency health endpoints.
- **Rationale:** Orchestration and load balancing.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a database outage, when readiness is checked, then it reports not ready.
- **Verification:** FAULT
- **Origin:** NEW

### FR-OPS-052 — Self-checks
- **Statement:** Nodes MUST run periodic self-checks: hash-chain samples, projection checksums, job-sweeper health, clock skew and WORM configuration.
- **Rationale:** Early detection.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given an injected projection drift, when the self-check runs, then an alert is raised.
- **Verification:** FAULT
- **Origin:** NEW

### FR-OPS-053 — Diagnostic bundles
- **Statement:** Operators MUST be able to produce diagnostic bundles (configuration, versions, metrics, logs) that exclude business data by construction.
- **Rationale:** Supportability without data exposure.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a bundle, when scanned, then it contains no business field values.
- **Verification:** SEC
- **Origin:** NEW

## CAP-OPS-06 — Disaster recovery

### FR-OPS-061 — Replication and failover
- **Statement:** The Server MUST support a standby site with streaming replication and a documented failover procedure that meets the NR-DUR RPO and RTO targets (SCN-406).
- **Rationale:** Business continuity.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given SCN-406, when executed, then the measured RPO and RTO are within targets.
- **Verification:** SCN, FAULT
- **Origin:** NEW

### FR-OPS-062 — Split-brain protection
- **Statement:** After failover, the former primary MUST be fenced. Any commits made there after the fork MUST be exported for review and MUST NOT merge automatically.
- **Rationale:** SCN-406 A1.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a returning former primary, when detected, then it is fenced, and divergent commits are listed.
- **Verification:** FAULT, SIM
- **Origin:** NEW

### FR-OPS-063 — DR documentation and tests
- **Statement:** DR runbooks MUST exist per edition, and DR tests MUST run at least twice a year with reports.
- **Rationale:** SOC 2 availability evidence.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given the DR test schedule, when a year passes, then two reports exist.
- **Verification:** AUDIT, INSP
- **Origin:** NEW

## CAP-OPS-07 — Capacity and tiering

### FR-OPS-071 — Capacity reporting
- **Statement:** The platform MUST report storage by tenant, VAE, class and tier, with growth trends and forecasts.
- **Rationale:** Full history needs capacity management.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** CTL, NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a month of data, when reported, then per-tenant growth and forecast are shown.
- **Verification:** CONF
- **Origin:** IMP-08

### FR-OPS-072 — Tiering policies
- **Statement:** Operators and tenants MUST be able to configure tiering (FR-VER-091) within plan limits.
- **Rationale:** Cost control.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** CTL
- **Personas:** PER-PlatformOperator, PER-TenantAdministrator
- **Acceptance:**
  1. Given a policy change, when applied, then the next tiering job uses it.
- **Verification:** CONF
- **Origin:** IMP-08

### FR-OPS-073 — Quotas
- **Statement:** Storage and compute quotas MUST be enforceable per tenant, with warnings before hard limits.
- **Rationale:** Predictable operations.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** CTL, NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given 90% of quota, when reached, then administrators are warned. At 100%, writes fail with `OPS.QUOTA_EXCEEDED`, and reads continue.
- **Verification:** CONF
- **Origin:** NEW

## CAP-OPS-08 — Fleet management

### FR-OPS-081 — Control Plane fleet operations
- **Statement:** The Control Plane MUST manage nodes and cells: registration, health, version inventory, upgrade waves, tenant placement and migration. It MUST NOT be able to read business data.
- **Rationale:** Operating many nodes safely (IMP-08).
- **Priority:** Must · **Phase:** PH-4 · **Systems:** CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a Control Plane credential, when used against a node's business API, then it is denied.
- **Verification:** SEC, SCN
- **Origin:** IMP-08

### FR-OPS-082 — Upgrade waves
- **Statement:** Upgrades across the fleet MUST proceed in waves with health gates and automatic halt on regression.
- **Rationale:** Blast-radius control.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a failing gate in wave 2, when observed, then waves 3 and later do not start.
- **Verification:** SCN
- **Origin:** NEW

### FR-OPS-083 — Customer-operated nodes
- **Statement:** Customer-operated Server nodes MAY register with the Control Plane for licence, version and health reporting only. Registration MUST be optional for air-gapped installations.
- **Rationale:** Hybrid operation.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** CTL, NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given an unregistered node, when running, then all features work with an offline licence.
- **Verification:** CONF
- **Origin:** NEW

## CAP-OPS-09 — Configuration management

### FR-OPS-091 — Versioned node configuration
- **Statement:** Node configuration MUST be declarative, versioned and validated before apply. Invalid configuration MUST be rejected, and the node MUST keep running on the last valid configuration.
- **Rationale:** Safe changes.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given an invalid configuration, when applied, then it is rejected, and the node is unaffected.
- **Verification:** CONF
- **Origin:** NEW

### FR-OPS-092 — Hot reload
- **Statement:** Configuration keys marked reloadable MUST take effect without restart.
- **Rationale:** Availability.
- **Priority:** Should · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a change to the log level, when applied, then it takes effect within seconds.
- **Verification:** CONF
- **Origin:** NEW

### FR-OPS-093 — Configuration audit
- **Statement:** Configuration changes MUST be recorded in the security audit log with a diff.
- **Rationale:** Change management evidence.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a change, when audited, then the diff and actor are present.
- **Verification:** CONF
- **Origin:** NEW

## CAP-BILL-01 — Metering

### FR-BILL-011 — Usage records
- **Statement:** The platform MUST produce usage records for:
  - active users;
  - storage by tier;
  - compute (metering units from FR-LOGIC-063);
  - AI usage, API calls and connector calls;
  - Buk usage per licensed feature.

  Each record MUST be attributed to tenant, VAE and period.
- **Rationale:** Billing and cost transparency (fiat only, DEC-012).
- **Priority:** Must · **Phase:** PH-4 · **Systems:** CTL, NOD
- **Personas:** PER-TenantAdministrator, PER-ExchangeOperator
- **Acceptance:**
  1. Given a month, when usage is exported, then it reconciles to the underlying metering records.
- **Verification:** CONF, AUDIT
- **Origin:** NEW

### FR-BILL-012 — Tamper-evident usage
- **Statement:** Usage records MUST be ledger entries with hash chaining, so that billing disputes can be resolved from evidence.
- **Rationale:** Trustworthy billing.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** CTL
- **Personas:** PER-ExchangeOperator
- **Acceptance:**
  1. Given a disputed invoice, when evidence is produced, then the usage entries verify.
- **Verification:** CONF
- **Origin:** NEW

## CAP-BILL-02 — Plans and entitlements

### FR-BILL-021 — Plans
- **Statement:** Plans MUST define included quotas, features, profiles and prices, and MUST be assignable per tenant with effective dates.
- **Rationale:** Commercial packaging.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** CTL
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a plan upgrade effective next month, when the month starts, then the new entitlements apply.
- **Verification:** CONF
- **Origin:** NEW

### FR-BILL-022 — Entitlement enforcement
- **Statement:** Entitlements MUST be enforced at the relevant points (profiles, AI budgets, storage, features), with clear errors.
- **Rationale:** Consistent commercial boundaries.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a feature outside the plan, when used, then it fails with `BILL.NOT_ENTITLED`.
- **Verification:** CONF
- **Origin:** NEW

## CAP-BILL-03 — Licences

### FR-BILL-031 — Licence objects
- **Statement:** Licences for Buks and for the kernel (embedding) MUST be signed objects with licensee, scope, term, limits and entitlements, verifiable offline.
- **Rationale:** ISV and ARM-style licensing (EXT-BPU).
- **Priority:** Must · **Phase:** PH-4 · **Systems:** EXC, CTL
- **Personas:** PER-IsvDeveloper, PER-TenantAdministrator
- **Acceptance:**
  1. Given a licence file, when verified offline, then the signature and scope check succeed.
- **Verification:** CONF
- **Origin:** EXT-BPU, EXT-TRI

### FR-BILL-032 — Licence lifecycle
- **Statement:** Licences MUST support issuance, renewal, upgrade, suspension and expiry with grace periods and notifications.
- **Rationale:** Commercial operations.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** EXC
- **Personas:** PER-ExchangeOperator
- **Acceptance:**
  1. Given expiry in 30 days, when reached, then the licensee is notified.
- **Verification:** CONF
- **Origin:** NEW

## CAP-BILL-04 — Invoicing and payouts

### FR-BILL-041 — Customer invoicing in fiat
- **Statement:** The platform MUST compute invoices from plans, usage and licences in fiat currency (USD first), with tax lines supplied by an external tax service and exported to the payment processor.
- **Rationale:** Commercial operation, no tokens (DEC-012).
- **Priority:** Must · **Phase:** PH-4 · **Systems:** CTL, EXC
- **Personas:** PER-ExchangeOperator
- **Acceptance:**
  1. Given a month of usage, when invoiced, then each line traces to usage or licence records.
- **Verification:** CONF, AUDIT
- **Origin:** DEC-012

### FR-BILL-042 — Publisher payouts
- **Statement:** The Exchange MUST compute publisher payouts from licence and usage revenue minus fees, with statements per period.
- **Rationale:** An ISV ecosystem.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** EXC
- **Personas:** PER-IsvDeveloper, PER-ExchangeOperator
- **Acceptance:**
  1. Given revenue, when payouts are computed, then the statement totals reconcile.
- **Verification:** CONF, AUDIT
- **Origin:** EXT-TRI

## CAP-BILL-05 — Cost controls

### FR-BILL-051 — Budgets and caps
- **Statement:** Tenants MUST be able to set budgets with alerts and hard caps per tenant, VAE, agent and AI route.
- **Rationale:** No surprise bills.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** CTL, AGT
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a hard cap on AI spend, when reached, then further AI calls fail with `AI.BUDGET_EXHAUSTED`.
- **Verification:** CONF
- **Origin:** NEW

### FR-BILL-052 — Cost transparency
- **Statement:** Tenants MUST see current-period cost estimates by category and driver.
- **Rationale:** Informed usage.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** CTL
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given mid-month, when viewed, then the estimate by category is shown.
- **Verification:** CONF
- **Origin:** NEW
