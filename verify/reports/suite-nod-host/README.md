# SUITE-NOD-HOST — In-process host services on PostgreSQL (PH-1 step 7a)

| Item | Value |
|---|---|
| Suite | SUITE-NOD-HOST: the CTR-002 host services of the in-process node host (DEC-060) |
| Result | **12 of 12 cases pass** (`node-suite.json`, signed by an ephemeral runner key; 0.7 s) |
| Build | `cargo build --release --locked -p ubos-node --bin ubos-node-suite` |
| Database | PostgreSQL 16, development workstation; migration `0006_host.sql` applied by the node at start (DSN-NOD-602) |
| Root KEK | the development key (`UBOS_ROOT_KEK` unset) |

## Cases

| Case | Result | ms |
|---|---|---|
| tenant_registry | pass | 9 |
| key_hierarchy | pass | 9 |
| open_vae_signed_commits | pass | 61 |
| reopen_vae | pass | 100 |
| node_key_rotation | pass | 85 |
| blob_service_isolated | pass | 79 |
| secrets | pass | 24 |
| connector_journaled_and_replayed | pass | 126 |
| event_log_release | pass | 52 |
| jobs_lease_retry_poison | pass | 19 |
| telemetry_redaction | pass | 0 |
| entropy_from_host | pass | 87 |

What each case shows:

- **tenant_registry** — tenants and VAEs are created and found; duplicate slugs and authorities are rejected (DSN-NOD-401, -402).
- **key_hierarchy** — tenant KEKs unwrap only with the right root KEK; tenants derive different data keys; a data key rotation keeps old generations readable and rejects unknown ones (DSN-NOD-131, FR-IAM-033).
- **open_vae_signed_commits** — a VAE opened through the host signs its commit records, the genesis record included, with the key hierarchy's node key under the VAE's registered DID; the records verify against the node's DID document (DSN-NOD-134, STD-PROOF-011).
- **reopen_vae** — the declarations and the process counter survive reopening; commits continue (CTR-001 `open_vae`).
- **node_key_rotation** — `rotate_node_key` gives a contiguous key history; old records verify at their time and not after it; the history persists (STD-PROOF-010).
- **blob_service_isolated** — a record above the inline size is placed in the tenant's blob table and read back; another tenant sees no blobs (DSN-NOD-320, DSN-DVM-003).
- **secrets** — secrets round-trip through handles under the current data key, survive a data key rotation, are unknown to another tenant and can be deleted (DSN-NOD-130).
- **connector_journaled_and_replayed** — a host program calls a registered connector that resolves a secret; logic sees only its length; the call is journaled, the replay is identical and makes no connector call; an unregistered connector is `LOGIC.CONNECTOR_UNAVAILABLE` (DSN-NOD-301, -302, STD-CTX-011, SCN-011 A1).
- **event_log_release** — released effects reach the event log once and are acknowledged in the outbox (DSN-NOD-201, -202).
- **jobs_lease_retry_poison** — jobs are claimed under a lease, not claimed twice, requeued on failure and poison after `max_attempts`; completed jobs record their result (DSN-NOD-220, -221, -223).
- **telemetry_redaction** — only operational members survive; business data is dropped; every operation emits an event (DSN-NOD-610, -611).
- **entropy_from_host** — processes are seeded from the host's entropy, differ between runs and replay identically from the recorded seed (CTR-002 `entropy.fill`).
