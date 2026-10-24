# BPA 1.0.0-rc8 — Release notes

Release candidate 8 of BPA 1.0 (PH-2 step 1, DEC-089…096), the candidate for the PH-2 kernel.
Tag `bpa-v1.0.0-rc8`, signed by the owner (DSN-BPA-001). The manifest
(`release/1.0-rc8/manifest.json`) lists every released file with its SHA-256; its `content_id`
identifies this release. BPA 1.0-rc7 stays frozen as tagged. The standard precedes the code
(AR-033): the PH-2 kernel implements this frozen text in its later steps.

## Content

| Item | Value |
|---|---|
| Profiles | Core, Bitemporal, Ledger, Proof, Governance, Protocol |
| Clauses | 238 (scope by phase, `scope.json`): the 201 clauses of BPA 1.0-rc7, 23 that satisfy PH-2 requirements, and 14 written in parts 1b–1e |
| MUST clauses with a vector range | 200, 200 covered (`coverage.md`) |
| Vectors | 2,053: those of BPA 1.0-rc7 without VER-CONF-1528 and VER-CONF-3002 (superseded), 45 of part 1b, 53 of part 1c, 122 of part 1d, 43 of part 1e, 51 of part 1f and 38 of part 1g |

## Changes since BPA 1.0-rc7

- The scope rule names a phase (DEC-089 item 1). Added clauses:
  - addressing: STD-ADDR-003, -004;
  - class system and models: STD-CLS-023, -050, -060, -061, -062;
  - execution: STD-CTX-031;
  - instructions: STD-ISA-053, -071, -081, -082, -083, -090;
  - decision tables: STD-LGS-040;
  - encrypted fields: STD-OBJ-010, -011;
  - REST facade: STD-PROTO-021;
  - re-evaluation: STD-TIME-041;
  - transactions: STD-TXN-051, -060, -080;
  - upstream change detection: STD-VER-082.
- Part 1b (DEC-090), money and periods:
  - **Period transitions** (STD-KIND-025): an update moves a period between `open` and `closing`; `closed` is entered by `close_period` and left by `reopen_period` with a reason.
  - **Close artefacts** (STD-KIND-026, new): balances per ledger key, `as_of`, and a signed tag `close-<slug>-<n>`.
  - **Matching** (STD-KIND-027, new) and **match records** (STD-KIND-028, new) with unmatch.
  - **Re-evaluation modes** for logic assets (STD-TIME-041) and **restatement** on a correction branch (STD-TIME-042, new).
  - **Adapter operations** `close_period`, `reopen_period`, `match`, `unmatch` and `restate`.
  - **Supersession:** VER-CONF-1528 closed a period by an update. It is superseded by VER-CONF-1108, which closes the period with `close_period`.
- Part 1c (DEC-091), approvals and rules:
  - **Deferred commits** with `defer` (STD-TXN-060).
  - **Approval requirements, separation of duties and approval evidence** (STD-LGS-041…043, new).
  - **Decision tables** with hit policies and an authoring check (STD-LGS-040, STD-ISA-053).
  - **Simulation of rule changes** (STD-LGS-044, new).
  - **Adapter operations** `decide_table`, `check_table`, `pending_change`, `approve` and `simulate`.
- Part 1d (DEC-092), long-running work and models:
  - **Workflows** (STD-TXN-090…092, new): versioned definitions with eight step types, System-kind instances pinned to their definition version, idempotent steps, a join that fires once, events matched by type and correlation key with a buffer window, timers, human tasks and explicit migration.
  - **Sagas** (STD-TXN-080): steps as linked processes, backward or forward recovery, committed state transitions and resumption without repeated steps.
  - **Quarantine** (STD-CTX-031): a per-VAE threshold of failures and limit terminations, fail-fast calls, an alert effect and release by an authorised principal.
  - **Schema evolution** (STD-CLS-050, -060…062): computed classification of class changes, the nine lens operations, lensed reads, writes and merges, and the totality check.
  - **Upstream changes** (STD-VER-082): overrides marked when the parent line changes them, resolved by acknowledgement or three-way merge.
  - **Adapter operations** `start_workflow`, `deliver_event`, `complete_task`, `migrate_workflow`, `run_saga`, `quarantine_policy`, `release_quarantine`, `change_class`, `classify_change`, `apply_lens`, `check_lens`, `upstream_changes` and `resolve_upstream`; `read` and `commit` take `schema_version`.
  - **Moved:** STD-TXN-051 (pre-commit external calls) is written in part 1f with `call.connector`.
- Part 1e (DEC-093), protection:
  - **Encrypted fields** (STD-OBJ-010): personal fields of classes with `subject_key` stored under AES-256-GCM with field commitments; hashes cover the stored form; key rotation and disclosure. The commitment member is named `commitment`.
  - **Erasure** (STD-OBJ-011): destroying a subject key erases its fields in every version with hashes and proofs unchanged; unavailable keys are distinct from erasure.
  - **Authorization relations** (STD-LGS-050, new), **relation-based access rules** with the kernel property `access` (STD-LGS-051, new) and **field masking** for viewers (STD-LGS-052, new).
  - **Adapter operations** `subject_key`, `key_availability`, `erase_subject`, `stored_form`, `disclose`, `verify_disclosure`, `lookup_key`, `relations` and `check_access`; `read`, `query` and `outbox` take `as`.
  - **Rule 6 of STD-CONF-005:** `nonce`, `ct`, `salt` and `commitment` are implementation-dependent members.
- Part 1f (DEC-094), integration and addressing:
  - **Connectors** (STD-ISA-090): `Connector` objects with an egress allow-list, credential handles, schemas, classification checks, retry, breaker and rate limit, redacted records and journaled responses; `after_commit` calls through the outbox. Assets name their connectors as capabilities `connector:<uri>`.
  - **Compensation of pre-commit calls** (STD-TXN-051): reverse order, idempotency keys, at least once through the outbox.
  - **Notifications** (STD-ISA-071): `NotificationTemplate` objects; recipients by DID, role and relation, rendered per recipient with masking.
  - **Short forms and VFS paths** (STD-ADDR-003, -004) in `resolve` and `resolve_path`.
  - **REST facade** (STD-PROTO-021) over the UBTP session, with `Idempotency-Key` and RFC 9457 problems.
  - **Adapter operations** `egress_allow`, `connector_stub`, `exchanges`, `principal`, `resolve_path` and `rest`.
- Part 1g (DEC-095), search and views:
  - **Descriptors:** `search.aggregate`, `search.traverse` and `search.text` declare their parameters. VER-CONF-3002 (the catalogue of 1.0-rc3) is superseded by VER-CONF-3003.
  - **Aggregates** (STD-ISA-081) with money per currency or converted by rates, nearest-rank percentiles and visible rows only.
  - **Traversal** (STD-ISA-082), breadth-first to depth 8 and authorised per node.
  - **Text search** (STD-ISA-083) over `text_index` fields with ranking, `<em>` snippets, masking, a watermark with `lagging`, and journaled results.
  - **View resolution** (STD-CLS-023) with context variants and generated views; genesis class `View`.
  - **Adapter operations** `resolve_view` and `index_lag`.

## Superseded vectors

Two vectors of BPA 1.0-rc7 are kept as released and excluded from this release
(`vectors/superseded.txt`, STD-CONF-030):

- VER-CONF-1528 closed a period by an update; periods are closed by `close_period` (VER-CONF-1108, DEC-090).
- VER-CONF-3002 asserted the instruction catalogue of 1.0-rc3; the search instructions declare their parameters (VER-CONF-3003, DEC-095).

## Known limits

- **Kernel.** The vectors added in 1.0-rc8 are not tagged `prototype` and have not run on the kernel. The kernel's conformance test keeps claiming the clauses it implements and runs these vectors as the later PH-2 steps implement them (EXIT-2-01, VER-CONF-8405).
- **Simulated outside world.** Connector responses are scripted (`connector_stub`). The text index of the reference model is current unless `index_lag` freezes it. Subject keys are derived by the reference model, where implementations use their key service.
- **Not in this release:**
  - L2 residual functions of lenses (`LOGIC.NOT_AVAILABLE`);
  - business-calendar timers of workflows (FR-FLOW-063);
  - the legal-hold check before erasure (DSN-DVM-831, PH-3);
  - personal-data read logging (DSN-DVM-823, kernel);
  - short forms in the reference values of changes (VER-CONF-0849).
- **Bindings.** The Protocol vectors exercise the in-process binding and the REST facade over it; the other bindings replay them with the Server.
