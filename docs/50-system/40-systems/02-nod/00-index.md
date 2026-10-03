---
id: UBS-SYS-NOD-00
title: UBOS Node — Overview
status: draft
phase: PH-1
depends_on: [UBS-SYS-00, UBS-SYS-DVM-00, UBS-ARC-01, UBS-ARC-03]
---

# UBOS Node (NOD) — Overview

## 1. Purpose

The Node is the host process that turns the DVM library into a running system. It owns
everything the kernel must not own (AR-032):
- network transports (UBTP, REST facade, webhooks, admin endpoint);
- authentication, sessions and devices;
- secrets, keys and connector I/O;
- clocks, schedules, jobs, workers and outbox release;
- notifications, files and derived search indexes;
- sync transport between devices and servers;
- telemetry, health, backups, upgrades and configuration.

The Node holds no business truth of its own (AR-005): every business fact lives in the DVM
store; the Node's own state is operational and recoverable.

## 2. Editions and roles

| Edition or role | Crate | Store | Phase |
|---|---|---|---|
| In-process host (gate 1a) | `ubos-node` library behind `ubos-dvm::host` | PostgreSQL, blobs in a table (DEC-060) | PH-1a |
| Server (single node) | `ubos-node` | PostgreSQL + S3-compatible or file-system blobs | PH-1b |
| Server (HA) | `ubos-node` | PostgreSQL with standby + object store + runtime store cluster | PH-2 |
| Cell (multi-tenant) | `ubos-node` with Cell profile | clustered PostgreSQL, tiered object store | PH-4 |
| Box (desktop, mobile, headless) | `ubos-box` | SQLite + local blob directory | headless PH-1a, apps PH-3 |
| Worker (enterprise-internal) | `ubos-worker` | none (stateless) | PH-4 |

All editions share one code base; edition differences are configuration presets and
compiled features, never forks.

## 3. Responsibilities and non-responsibilities

| Responsible for | Not responsible for |
|---|---|
| transport, authentication, sessions, rate limits | business semantics, validation, authorization decisions (DVM) |
| host services for the DVM (CTR-002) | canonical formats and proofs (DVM, BPA) |
| jobs, schedules, timers, outbox release, event delivery | workflow semantics (DVM flow engine) |
| secrets, key custody, connector execution | model routing and agent policies (AGT) |
| files: upload, scanning, previews, extraction | rendering of views (WSP) |
| search index adapters (text, vector) | analytics sinks and migrations (BRG) |
| device and server sync transport | federation registry (FED) |
| telemetry, health, backups, upgrades, configuration | fleet orchestration and billing (CTL) |

## 4. Personas served

PER-PlatformOperator (runs it), PER-TenantAdministrator (configures identity, connectors,
notifications), PER-SecurityOfficer (keys, sessions, logs), PER-IsvDeveloper (protocols,
webhooks), PER-PersonalUser and PER-FieldWorker (Box).

## 5. Phase map (DEC-014)

| Gate or phase | NOD scope |
|---|---|
| PH-0 | transport and host-service skeleton for the storage prototype |
| PH-1a | in-process host for the kernel on PostgreSQL (DEC-024); CLI host services |
| PH-1b | Server single node: UBTP, local authentication, sessions, secrets, jobs, schedules, outbox release, events, blobs, telemetry, health, backups, configuration, genesis Buks |
| PH-2 | HA Server, REST facade, webhooks, OIDC and SAML, SCIM, notifications, file scanning and previews, text search, DR, upgrades, US bank statement and payment file formats (DEC-022) |
| PH-3 | managed Server for one demonstration tenant, vector search adapter, legal-hold and retention jobs |
| PH-4 | Box (headless, desktop, mobile) and device sync (DEC-024); Cell profile, worker role, fleet agent for CTL, usage metering, tiering |
| PH-5 | QUIC transport, federation module hooks, browser host support services |

## 6. Crate and module map

| Module in `ubos-node` | Content |
|---|---|
| `edge` | listeners, TLS, UBTP codec, REST facade, admin endpoint |
| `session` | authentication, sessions, devices, API credentials |
| `host` | implementation of CTR-002 host services |
| `exec` | process execution pool, per-tenant fairness |
| `events` | outbox releaser, subscriptions, event log, webhooks |
| `jobs` | jobs, schedules, timers, retries, dead letters, worker coordination |
| `connectors` | connector runtime, egress proxy client, rate limits |
| `secrets` | secret store, key service, KMS adapters |
| `files` | blob service, scanning, previews, extraction |
| `index` | text and vector index adapters |
| `notify` | notification rules runtime, channels, providers |
| `sync` | device sync and server sync transport |
| `ops` | configuration, telemetry, health, diagnostics, backup, upgrade, cluster coordination |
| `tenancy` | tenant and VAE registry, tenant lifecycle, admin operations |

## 7. Files

| File | Content |
|---|---|
| `01-context.md` | context and contracts |
| `02-functions/` | DSN-NOD-* by sub-area |
| `03-interfaces.md` | IF-NOD-* |
| `04-data.md` | operational tables and stores |
| `05-state-machines.md` | session, job, schedule slot, sync session, tenant, upgrade |
| `06-configuration.md` | node configuration |
| `07-operations.md` | deployment, scaling, backup, monitoring, runbooks |
| `08-verification.md` | NOD verification plan |
