---
id: UBS-SYS-NOD-06
title: UBOS Node — Configuration
status: draft
phase: PH-1
depends_on: [UBS-SYS-NOD-02]
---

# UBOS Node — Configuration

Node configuration is a versioned TOML or YAML document validated against a published JSON
Schema (DSN-NOD-603). Environment variables `UBOS_<SECTION>_<KEY>` override file values.
Secrets are references (`env:`, `file:`, `kms:`), never inline (DSN-NOD-605). The embedded
`dvm` section is the DVM configuration (UBS-SYS-DVM-06).

| Key | Default | Range | Reload | Design |
|---|---|---|---|---|
| `edition` | server | server, cell, box, worker | no | DSN-NOD-004 |
| `listen.ubtp` | 0.0.0.0:7443 | address | no | DSN-NOD-010 |
| `listen.rest` | same port, path `/v1` | address | no | DSN-NOD-030 |
| `listen.admin` | 127.0.0.1:7444 | management address | no | DSN-NOD-641 |
| `tls.cert`, `tls.key` | — | file or KMS reference | yes | DSN-NOD-010 |
| `database.url` | — | reference | no | CTR-005 |
| `database.pool_size` | 4 × cores | 4–512 | no | DSN-NOD-001 |
| `blobs.backend` | fs | fs, s3, azure, gcs | no | CTR-006 |
| `blobs.bucket`, `blobs.worm_bucket` | — | names | no | DSN-NOD-381 |
| `runtime_store.url` | embedded | embedded or reference | no | CTR-007 |
| `keys.root` | local-file (dev) | kms reference | no | DSN-NOD-131 |
| `keys.data_key_cache_minutes` | 60 | 1–240 | yes | DSN-NOD-131 |
| `exec.pool_threads` | cores | 1–256 | no | DSN-NOD-001 |
| `exec.tenant_concurrency_default` | 16 | 1–1024 | yes | DSN-NOD-002 |
| `ratelimit.principal_rps` | 50 | 1–10000 | yes | DSN-NOD-007 |
| `ratelimit.tenant_rps` | plan | 1–100000 | yes | DSN-NOD-007 |
| `ubtp.frame_max_mb` | 16 | 1–64 | yes | DSN-NOD-018 |
| `ubtp.send_queue_frames` | 1000 | 100–100000 | yes | DSN-NOD-016 |
| `session.idle_minutes` | 30 | 5–1440 | yes | DSN-NOD-105 |
| `session.absolute_hours` | 12 | 1–720 | yes | DSN-NOD-105 |
| `auth.password.argon2_memory_mb` | 64 | 32–1024 | yes | DSN-NOD-101 |
| `auth.lockout_threshold` | 10 | 3–100 | yes | DSN-NOD-101 |
| `outbox.lease_seconds` | 30 | 5–600 | yes | DSN-NOD-201 |
| `outbox.releasers` | 4 | 1–64 | yes | DSN-NOD-201 |
| `webhooks.retry_hours` | 24 | 1–168 | yes | DSN-NOD-033 |
| `jobs.lease_seconds` | 60 | 10–3600 | yes | DSN-NOD-221 |
| `jobs.poison_threshold` | 3 | 2–20 | yes | DSN-NOD-223 |
| `leases.duration_seconds` | 15 | 5–120 | yes | DSN-NOD-228 |
| `timers.poll_ms` | 1000 | 100–10000 | yes | DSN-NOD-227 |
| `egress.proxy` | — | URL | yes | DSN-NOD-302 |
| `egress.block_private_ranges` | true | bool | yes | DSN-NOD-302 |
| `files.scan.backend` | clamav | clamav, cloud, none (dev only) | yes | DSN-NOD-322 |
| `files.download_link_seconds` | 300 | 30–3600 | yes | DSN-NOD-327 |
| `search.text.backend` | tantivy | tantivy, opensearch | no | DSN-NOD-340 |
| `search.vector.backend` | pgvector | pgvector, hnsw | no | DSN-NOD-342 |
| `notify.providers.*` | — | provider references | yes | DSN-NOD-362 |
| `sync.pack_chunk_mb` | 4 | 1–64 | yes | DSN-NOD-501 |
| `telemetry.otlp_endpoint` | — | URL | yes | DSN-NOD-610 |
| `telemetry.attribute_allowlist` | built-in | list | yes | DSN-NOD-611 |
| `server.tls_certificate` | — | path | yes | DSN-NOD-614 |
| `selfcheck.interval_secs` | 60 | 5–3600 | yes | DSN-NOD-614 |
| `selfcheck.disk_min_free_mb` | 1024 | 0–10000000 | yes | DSN-NOD-614 |
| `backup.wal_archive` | — | target reference | no | DSN-NOD-620 |
| `backup.retention_days` | 35 | 7–3650 | yes | DSN-NOD-623 |
| `ctl.endpoint` | — | URL (optional) | no | DSN-NOD-643 |
| `ctl.heartbeat_seconds` | 30 | 10–300 | yes | DSN-NOD-642 |
| `residency.region` | — | region code | no | DSN-NOD-410 |

## Presets

| Edition | Differences |
|---|---|
| Server single | embedded runtime store, fs blobs allowed, 1 releaser per queue |
| Server HA | external runtime store required, s3 blobs required, leader leases on |
| Cell | opensearch text search, per-tenant fairness weights from CTL plans, admin endpoint on |
| Box | SQLite, local blobs, hnsw vectors, no admin endpoint, background sync on |
| Worker | no listeners except outbound UBTP, no database |
