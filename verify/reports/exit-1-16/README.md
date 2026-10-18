# EXIT-1-16 — Isolation, secret hygiene and security-event coverage (PH-1 step 19a)

| Item | Value |
|---|---|
| Criterion | EXIT-1-16: isolation suite 100% pass; 0 secret-scanning matches; 100% security event coverage (UBS-PH-1 §9) |
| Verification items | VER-SEC-2000 (SUITE-NOD-ISO: NR-SEC-003, FR-TEN-021, FR-TEN-023, DSN-NOD-405, DSN-NOD-406, DSN-DVM-010), VER-SEC-2001 (NR-SEC-009, NR-SEC-010, FR-AUD-012, DSN-NOD-111, DSN-NOD-130) |
| Result | **SUITE-NOD-ISO 544 of 544 cases, 0 cross-tenant accesses; 0 canary matches over 6 sources (127,459 bytes); 28 of 28 PH-1 security event types present; the security log's chain verifies** |
| Decision | DEC-085 (part 19a) |
| Runners | `ubos-iso-suite --database …` (`crates/ubos-node/src/bin/ubos-iso-suite.rs`); `crates/ubos-node/ops/hygiene_suite.py --binary target/debug/ubos-server` |
| Environment | development workstation (4 vCPU, linux x86_64), PostgreSQL 16, debug build, 2026-10-18; both run in the CI job `scenarios over UBTP` |
| Reports | `suite-nod-iso.json` (signed), `secret-hygiene.json` |

## 1. SUITE-NOD-ISO (VER-SEC-2000)

One Server on PostgreSQL serves three tenants: the operator's (the node's default tenant) and two
ordinary tenants A and B, each with the VAE `acme`, the genesis classes and twenty objects with
the same identifiers and slugs; B's bodies, principals and files carry a canary. A's
administrator, with its session token and with an API key, tries to reach B; a case passes
when the attempt is refused or answers from A's own VAE, and no response carries B's canary,
B's principal or its DID.

| Group | Attempts | Cases | Pass |
|---|---|---|---|
| UBTP over HTTP | B named by `Ubtp-Tenant`, by the handshake's `vae` or not at all; read, history and timeline of every object, a query | 246 | 246 |
| UBTP over WebSocket | B named by the `tenant` query parameter and by the header; a read of every object | 80 | 80 |
| sessions | B's session key presented with A's bearer token or tenant on the UBTP route and the facade; 60 guessed keys `sess-1…60`; 10 guessed stream sessions | 79 | 79 |
| REST facade | reads, a query and a port call naming B | 44 | 44 |
| forged tokens | B's tenant prefix on A's session token and API key, over UBTP, the identity and operator APIs | 44 | 44 |
| identity API | principals, keys, API keys, state, password, signing and the security log of B | 26 | 26 |
| operator API | B's events, jobs, schedules, leaders, listeners and tenant record; the node-wide diagnostics, health, telemetry and configuration | 14 | 14 |
| files | B's upload (GET, HEAD, PATCH, DELETE) and a download link for B's object | 10 | 10 |
| store | the operator's isolation self-check: no tenant pool without a tenant (DSN-NOD-405) | 1 | 1 |
| **total** | | **544** | **544** |

The store-level checks of row-level security (a pool without a tenant sees no rows, a tenant
sees no row of another) are cases of SUITE-DVM-STORE (`ubos-store-suite`: `two_tenants`,
`rls_no_tenant`, `rls_cross_tenant`).

## 2. Secret hygiene and event coverage (VER-SEC-2001)

A Server installed and bootstrapped by the steps of SUITE-NOD-OPS, with telemetry exported to
its log at level `debug`, runs a security scenario set: sign-ins with and without the second
factor, failures until the source is throttled, sessions opened and revoked, a session
policy, principals created and disabled, a password set, keys created, rotated and used, an
API key issued, used, rejected and revoked, secrets stored and deleted, configuration applied
and rejected, tenant settings, a policy committed on a branch that then refuses a UBTP read and
commit, a UBTP workload with failed requests, and a diagnostic bundle. Canaries: both
passwords and a wrong one, the TOTP secret, the session tokens, the API key, two secret-store
values, the root and backup keys, the database password and the invitation token.

| Source scanned | Bytes |
|---|---|
| node log (logs and spans) | 9,606 |
| telemetry span buffer | 9,677 |
| diagnostic bundle | 39,624 |
| security-log export | 40 |
| security log | 21,024 |
| client responses | 47,488 |

0 matches. Tokens, the API key and the TOTP secret appear only in the responses that issue them
to their owner, which the scan of client responses exempts.

| FR-AUD-012 category | PH-1 event types (all present) |
|---|---|
| authentication | `auth.signed_in`, `auth.failed`, `auth.throttled`, `auth.totp_required`, `api_key.rejected` |
| session | `session.opened`, `session.revoked`, `policy.session` |
| permission and policy changes | `principal.created`, `principal.state`, `credential.password_set`, `dvm.security:policy.change`, `dvm.security:authz.deny` |
| key operations | `key.created`, `key.rotated`, `key.signed`, `api_key.issued`, `api_key.revoked`, `credential.totp_enrolled`, `credential.totp_enabled` |
| administrative actions | `invitation.created`, `invitation.accepted`, `secret.put`, `secret.deleted`, `config.applied`, `config.rejected`, `tenant.settings` |
| exports | `diagnostics.bundle` |

Break-glass access is PH-2 (DSN-NOD-112). The kernel's security log reaches the tenant's log as
`dvm.security` entries (DSN-NOD-111).

## 3. Findings and fixes of part 19a

- **Session keys were sequential** (`sess-1`, `sess-2`, …) and a presented `Ubtp-Session` was
  accepted without regard to the caller: another tenant's caller could guess a key and read
  through that session. Session keys are now 128-bit random, and a session presented with a
  tenant named by `Ubtp-Tenant` or by the caller's bearer token must be that tenant's (UBTP over
  HTTP, the facade, the file service).
- **Node-wide operator routes** (diagnostics, health, telemetry, configuration) answered any
  tenant's administrator; they now belong to the operator's tenant, the node's default tenant,
  as the tenant registry route already did.
- **Policies did not apply to UBTP requests:** the kernel's UBTP binding read, queried and
  committed without the session's principal, so the policy set of a branch (DSN-DVM-804, -807)
  bound only host calls that named one. The binding now passes the session's principal, kind,
  role and authentication level to reads (authorized on the resolved object at the same
  commit), queries (row filter), history, timelines, raw commits and the versioning requests;
  ports already carried it. Without a policy set nothing changes: conformance 1,703 of 1,703,
  SUITE-NOD-PROTO 141 of 141 per binding, the SDK suites and the scenario runner unchanged.
