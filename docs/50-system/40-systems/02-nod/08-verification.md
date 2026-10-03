---
id: UBS-SYS-NOD-08
title: UBOS Node — Verification Plan
status: draft
phase: PH-1
depends_on: [UBS-SYS-NOD-02, UBS-SYS-DVM-08]
---

# UBOS Node — Verification Plan

## 1. Suites

| Suite | Content | Method | Runs |
|---|---|---|---|
| SUITE-NOD-HOST | in-process host services on PostgreSQL: tenant registry, key hierarchy, node signer, blobs, secrets, journaled connectors, event log, jobs, telemetry redaction (PH-1 step 7a, DEC-060) | CONF, SEC | every commit |
| SUITE-NOD-PROTO | UBTP conformance on every binding; REST equivalence; version skew | CONF | every commit |
| SUITE-NOD-ISO | tenant isolation suite through every interface (DSN-NOD-406) | SEC | every commit, every release |
| SUITE-NOD-AUTH | authentication, sessions, revocation latency, SCIM compliance, OAuth flows, step-up | SEC, CONF | every commit (fast), nightly (full) |
| SUITE-NOD-JOBS | jobs, leases with fencing, schedules exactly-once, timers, dead letters, poison detection | SIM, FAULT | nightly |
| SUITE-NOD-EVENTS | outbox release ordering, webhooks, brokers, replay, masking per recipient | SIM, FAULT, SEC | nightly |
| SUITE-NOD-FILES | resumable uploads, scanning, previews, WORM, download links | CONF, SEC | nightly |
| SUITE-NOD-SYNC | device sync conformance, resumability, scope reduction and wipe, efficiency | CONF, SIM, BENCH | nightly (PH-4+, DEC-024) |
| SUITE-NOD-OPS | install, bootstrap, backup and PITR, failover, split-brain, rolling upgrade, rollback, DR drill | FAULT, SCN | weekly and release |
| SUITE-NOD-BENCH | end-to-end NR-PERF through the edge; fairness; subscription latency | BENCH | nightly and release |
| SUITE-NOD-PEN | external penetration test and egress SSRF tests | SEC | before PH-2 exit and yearly |

## 2. Key properties

| Property | Items | Suite |
|---|---|---|
| no acknowledged work lost when any node dies | DSN-NOD-003, DSN-NOD-201, DSN-NOD-220 | FAULT |
| exactly one firing per schedule slot | DSN-NOD-224, DSN-NOD-228 | SIM, FORM |
| fencing prevents double job commits | DSN-NOD-221 | SIM |
| per-key event order | DSN-NOD-201 | SIM |
| revocation within bound on all nodes | DSN-NOD-106 | BENCH |
| no cross-tenant access through any interface | DSN-NOD-405, DSN-NOD-406 | SEC |
| no secret or business value in logs and telemetry | DSN-NOD-130, DSN-NOD-611 | SEC (canary scan) |
| no writes to a demoted primary | DSN-NOD-625 | FAULT |

## 3. Phase exit contributions

| Phase | NOD evidence required |
|---|---|
| PH-1a | in-process host on PostgreSQL runs SUITE-DVM-CONF; host services pass their contract tests |
| PH-1b | SUITE-NOD-PROTO, -ISO, -AUTH (local), -JOBS, -EVENTS (subscriptions), -OPS (install, backup, PITR) green; single-node NR-PERF-001, -002, -011, -015 via the edge |
| PH-2 | HA failover and split-brain FAULT tests; REST, webhooks, OIDC, SAML, SCIM; notifications; files; text search; rolling upgrade and rollback; DR drill; penetration test with no open high findings (NR-SEC-001); bank statement parsers and NACHA generation |
| PH-3 | vector adapter; retention, holds and WORM; managed Server operations for the demonstration tenant |
| PH-4 | Box apps and SUITE-NOD-SYNC including NR-PERF-018 (DEC-024); Cell profile at NR-SCAL-002; worker role with result verification; usage chain to CTL; tenant migration within NR-AVAIL-004 |
| PH-5 | QUIC; cross-organisation sync with FED trust |

## 4. Environments

| Use | Environment |
|---|---|
| single-node functional and benchmark | ENV-REF-SMALL |
| HA, fault injection, fairness | ENV-REF-SERVER |
| Box | ENV-REF-BOX, ENV-REF-MOBILE |
| multi-tenant scale | ENV-REF-CELL |
| sync over poor networks (latency, loss) | ENV-REF-WAN |
