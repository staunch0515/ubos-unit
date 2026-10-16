# SUITE-NOD-AUTH — local identity of the node (PH-1 step 14)

`ubos-auth-suite` (DEC-075, step 14d) checks the local identity of step 14c against the
acceptance criteria of DSN-NOD-101, -105, -106, -108, -109, -110, -111, -150 and -153, on the
memory backend and on PostgreSQL (migration 0008), and the Server's enforcement over its
bindings. Run on the development workstation (Intel Xeon @ 2.80 GHz, 4 vCPUs, 15 GiB, Linux; rustc 1.94.1, debug build); PostgreSQL 16 on the same host. Reports:
`auth-suite-memory.json`, `auth-suite-pg.json` (signed by ephemeral runner keys).

| Backend | Result | Time |
|---|---|---|
| memory | **13 of 13 cases pass** | 8.4 s |
| PostgreSQL | **13 of 13 cases pass** | 9.8 s |

The identity cases run on a fixed clock, so timeouts and lockouts are checked at the stated
durations without waiting. The three Server cases run over a memory VAE in both runs.

| Case | Clause | memory / PostgreSQL | ms |
|---|---|---|---|
| credentials_argon2id_totp | DSN-NOD-101 | pass / pass | 780 / 833 |
| brute_force_delay_and_lock | DSN-NOD-101 | pass / pass | 3552 / 3795 |
| sessions_and_timeouts | DSN-NOD-105 | pass / pass | 1174 / 1478 |
| revocation | DSN-NOD-106 | pass / pass | 550 / 778 |
| principal_keys_and_history | DSN-NOD-108 | pass / pass | 29 / 78 |
| principal_lifecycle | DSN-NOD-109 | pass / pass | 791 / 926 |
| service_principals_per_tenant | DSN-NOD-110 | pass / pass | 0 / 66 |
| security_log_chain | DSN-NOD-111 | pass / pass | 580 / 717 |
| api_keys | DSN-NOD-150 | pass / pass | 1 / 49 |
| scope_intersection | DSN-NOD-153 | pass / pass | 0 / 0 |
| server.server_preset_credentials | DSN-NOD-004 | pass / pass | 3 / 3 |
| server.read_only_scope_denies_mutate | DSN-NOD-153 | pass / pass | 198 / 207 |
| server.revocation_closes_connections | DSN-NOD-106 | pass / pass | 533 / 620 |

What the cases show:

- **credentials_argon2id_totp**: the stored hash is Argon2id with 64 MiB and 3 iterations, and the password is not stored. TOTP is required once confirmed and accepted at ±1 step. The secret is stored sealed.
- **brute_force_delay_and_lock** (DSN-NOD-101 acceptance 1): 20 failed sign-ins within a minute, each from a different source, are delayed progressively and locked at the 20th. The 21st, with the right password, is refused, and both the lock and the refusal are security events. The lock lapses after 15 minutes. A source failing on several accounts is throttled across them; another source is not.
- **sessions_and_timeouts** (DSN-NOD-105 acceptance 1): with a 15-minute idle timeout, a session idle for 16 minutes needs re-authentication. An active session ends at its absolute timeout. The concurrent limit evicts the oldest session. The token is stored hashed with its device and level, and is valid in its own tenant only.
- **revocation** (DSN-NOD-106): a revoked session fails its next validation. Suspending the principal revokes its sessions and API keys.
- **principal_keys_and_history** (DSN-NOD-108 acceptance 1): after a rotation, an old signature verifies against the historical key at its time and not after the rotation. The DID document keeps the history. The node does not sign with a client-held key.
- **principal_lifecycle** (DSN-NOD-109 acceptance 1): invited, active, suspended and deprovisioned states. A deprovisioned principal cannot sign in, and its signatures still verify. Deprovisioning is final and closes the keys.
- **service_principals_per_tenant** (DSN-NOD-110 acceptance 1): a service principal's key of tenant A is refused by tenant B.
- **security_log_chain** (DSN-NOD-111 acceptance 1): authentication, session, key, kernel decision and kernel security entries are chained, and no password or token reaches the log. A deleted row is reported as a gap.
- **api_keys** (DSN-NOD-150 acceptance 1): a 256-bit key is accepted from listed addresses and ranges. A key used from an unlisted address is rejected and logged. Keys expire, and listings never show the secret.
- **scope_intersection** (DSN-NOD-153): scopes never grant more than the principal holds, and a class scope narrows a broad grant.
- **server.server_preset_credentials** (DSN-NOD-004): the `server` preset refuses `dev` and in-process credentials, and the facade needs a credential.
- **server.read_only_scope_denies_mutate** (DSN-NOD-153 acceptance 1): a read-only API key of an administrator cannot mutate, though the administrator's own session can. The key can still read through the facade.
- **server.revocation_closes_connections** (DSN-NOD-106 acceptance 1, one node): revoking a session closes all three WebSocket connections bound to it within 2 ms, against the bound of 60 s (NR-SEC-005). The token is refused afterwards. Propagation to other nodes comes with CTR-008.

## Defects found and fixed

- **Refused attempts did not count.** An attempt refused during a progressive delay was not counted, so an attacker retrying through the delays never reached the lock. Refused attempts now count towards the lock; attempts refused while locked do not extend it.
- **Revocation left idle sockets open.** The WebSocket handler held its own sender of the push channel, so removing a revoked session did not close a connection that sent nothing. The session entry now holds the only sender.

## Reproduce

```
cargo build --locked -p ubos-node --bin ubos-auth-suite
./target/debug/ubos-auth-suite --report verify/reports/suite-nod-auth/auth-suite-memory.json
./target/debug/ubos-auth-suite --database "host=127.0.0.1 user=postgres dbname=bpa" \
    --report verify/reports/suite-nod-auth/auth-suite-pg.json
```
