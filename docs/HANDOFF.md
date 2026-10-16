# Handoff — status of the UBOS specification work

Last updated: 2026-10-15. This file lets a new Claude session (or a new engineer) continue
the work without the earlier conversations. Read `CLAUDE.md` first for the working rules.

## 1. Background in brief

- The owner earlier built eight experimental repositories of the same platform idea with AI
  help. They diverged. The research volume (`docs/10-inventory` … `docs/40-spec`) analysed
  them; the **system specification** `docs/50-system/` replaced them as the single
  authority (DEC-001, DEC-008).
- The core idea: a public standard (BPA) for how business state changes over time — typed,
  versioned, bitemporal, governed and provable — run by a reference kernel (DVM); business
  behaviour ships as data in business packages (Buks). Principle: *harden the mechanism,
  soften the policy.*
- 13 systems: DVM, NOD, CTL, FRG, SDK, WSP, STU, AGT, EXC, NTY, BRG, FED, BPA.
- 6 phases: PH-0 foundations; PH-1 kernel and single-node server (gates 1a and 1b); PH-2
  first business package and production server; PH-3 CPA audit drill, evidence and governed
  AI; PH-4 devices, scale and ecosystem; PH-5 federation. **PH-4 and PH-5 are specified but
  not scheduled** (DEC-023).

## 2. Decisions that shape the current plan (all Accepted)

| Decision | Content |
|---|---|
| DEC-022 | The first business package is **US Basic Finance** for one company (GL, journals with approval, periods and close, AR, AP with Form 1099, bank reconciliation, fixed assets, multi-currency ASC 830, basic state sales tax, four statements, audit trail and controls). Fund operations and contract management are **Deferred** with IDs kept; contract-only capabilities move to PH-5. Excluded: payroll, inventory, consolidation, budgeting, project accounting, complex ASC 606, ASC 842. |
| DEC-023 | The product is **not for sale**; it demonstrates the capabilities the platform imposes and is compared with the owner's Java system. PH-3 acceptance is an external CPA audit drill, not a customer pilot. |
| DEC-024 | **PostgreSQL only** until PH-4; SQLite, Box and device sync move to PH-4. |
| DEC-025 | PH-2 is **Web only**, professional and practical; Smart Grid (bulk journal grid) is in PH-2; desktop and mobile apps in PH-4. |
| DEC-026 | The finance requirements are a separate, implementation-neutral English document (`docs/finance-requirements/`, IDs `FIN-*`) shared by both implementations, with a sample company, expected results, acceptance scenarios and a comparison protocol. |
| DEC-019 | Licensing: standard text CC BY 4.0; vectors, schemas, SDKs and verifier Apache-2.0; kernel, node and Forge AGPL-3.0 plus commercial; first-party Buks commercial. |
| DEC-027 | The specification and finance requirements moved with history to `staunch0515/ubos-unit`; `ubos-platform` is the archive. |
| DEC-018 | Component layout: `ubos` (kernel and clients), `bpa-standard`, `ubos-verify`, `ubos-buks`; revised by DEC-028. |
| DEC-028 | `ubos-unit` is the single implementation monorepo: `standard/`, `verify/`, `crates/`, `packages/`, `buks/` next to `docs/`; the DEC-018 names remain component names (UBS-ARC-07 §1). |
| DEC-030 | PH-0 is executed by the owner with AI sessions in eight steps (one pull request each), with PH-0 verification work requested, external review at acceptance, benchmark hardware rented for step 5 and owner-signed releases. |
| DEC-029 | Finance requirements are self-contained, **frozen at version 1.0** (tag `fin-req-v1.0`), copied unchanged into other repositories; work items recorded per `40-comparison-protocol.md` §2 (system codes U and J). |
| DEC-034 | The Rhai sandbox spike blocks 100 of 100 initial escape cases; ASM-008 is confirmed for L2 with conditions (empty engine with pure packages, per-execution memory accounting, uncatchable host errors, fine-grained time checks), recorded in DSN-DVM-612. |
| DEC-035 | EXIT-0-05 (storage benchmark) is carried over into PH-1 because hosts cannot be rented now; it is due before the PH-1 storage layout is fixed, at the latest at gate 1a. |
| DEC-036 | PH-1 re-baselined after PH-0: every finding dispositioned (UBS-PHR-0), work items W-1…W-12, OQ-007 and OQ-008 resolved, RSK-004 closed, RSK-016 and RSK-017 added. |
| DEC-037 | PH-0 accepted with EXIT-0-05 carried over; external review together with the carried benchmark. |
| DEC-073 | PH-1 step 13 in parts 13a–13d: `ubos-query`, field indexes, the index path of `query`, `search.query` (13a); `ubos-authz` with Cedar (13b); history queries, audit trail, view payloads, overlays (13c); MET-PERF-016 benchmark (13d); the `ubos-query` dependency row of UBS-ARC-07 extended. |
| DEC-074 | Row filters of list queries as a residual post-filter bounded by the order-by pushdown (13d); index rebuild with `index_since`; no merge base between branches without a common ancestor; `ubos-bench query` as the MET-PERF-016 suite. |
| DEC-075 | PH-1 step 14 in parts 14a–14d: `ubos-proto` in the engine layer and the kernel's in-process UBTP binding (14a); the Server's pipeline and the WebSocket and HTTP/SSE bindings on `axum` and `tokio-tungstenite` (14b); local identity with WebAuthn deferred to PH-2 (14c); SUITE-NOD-PROTO over every binding and the EXIT-1-15 report (14d). |
| DEC-072 | Release of BPA 1.0-rc3 (12c): notes finalised, owner's signed tag `bpa-v1.0.0-rc3`, frozen once tagged; the process state machine of DSN-DVM-501 in the kernel with its property; VER-INSP-9005. |
| DEC-071 | BPA 1.0-rc3 opened for step 12 (lifecycles) in parts 12a–12c: STD-CLS-004 `DAT-Lifecycle` and the genesis class `Lifecycle`, STD-ISA-017 clarified, STD-ISA-036 `read.actions`, port availability and guards, guards in the rule binding, the lifecycle check of STD-BPU-011; 12b `ubos-flow` in the kernel, 12c the process state machine and the release (DEC-072). |
| DEC-070 | PH-1 step 11 in parts 11a–11d: `ubos-rules` pure engine and three operations (11a), sheet objects and cascade (11b), validations, decisions and the claim of the 1.0-rc2 clauses (11c), NR-PERF-009 benchmark (11d). |
| DEC-069 | Release of BPA 1.0-rc2 (10e): notes finalised, owner's signed tag `bpa-v1.0.0-rc2`, frozen once tagged; standard part of EXIT-1-14 met with the tag. |
| DEC-068 | BPA 1.0-rc2 UBTP (10d): the in-process binding `ubtp`, envelope rules, handshake, mutate, query, subscriptions, flow control; 88 vectors; coverage complete. |
| DEC-067 | BPA 1.0-rc2 validations, decisions and the Core clauses of gate 1b (10c): rule properties, commit enforcement, preview, decisions before ports, port and overlay checks, textual queries, three instructions, superseded released vectors. |
| DEC-066 | BPA 1.0-rc2 governance sheets: clarifications of the eight LGS clauses of step 10b, genesis classes `GovernanceSheet` and `PropertyDecl`, five adapter operations, `RULE.PARSE_ERROR`, 157 vectors. |
| DEC-065 | Policy-engine library: the `cedar-policy` crate embedded in `ubos-authz`; entity schema generated from the class registry; row filters decided in step 13 after measurement. |
| DEC-064 | BPA 1.0-rc2 opened by the scope rule for gate 1b (Governance and Protocol profiles, 197 clauses); rc1 frozen; parts 10a–10e. |
| DEC-063 | Gate-1b step plan: steps 10–20 (standard first, then the kernel capabilities, the Server and services, the clients and scenarios over UBTP, close-out and acceptance); policy-engine library chosen in step 10 by its own decision; ENV-REF-SMALL for steps 16, 18, 19. |
| DEC-062 | Gate-1a acceptance in parts 9a–9c: acceptance record UBS-PHA-1, benchmark driver complete, finding 1 (merge preview linear in objects) fixed in 9b before the reference run; decision after the reference runs and the night log. |
| DEC-061 | Step 8 in parts 8a–8c; kernel property set VER-PROP-0100; coverage and mutation score of the eleven kernel crates measured by the suites themselves, shards on the workstation completed by nightly CI. |
| DEC-038 | PH-1 executed by the owner with AI sessions: gate-1a steps 0–9, verification requested, ENV-REF-SERVER rented for W-1 and gate 1a, GitHub-hosted CI incl. nightly and ARM64/macOS runners. |
| DEC-039 | BPA 1.0-rc1 scope by rule (0.1 plus Core, Bitemporal, Ledger, Proof clauses serving gate 1a), checked by the builder; BPA 0.1 frozen; regen builds rc1; step 1 in parts 1a–1f. |
| DEC-040 | BPA 1.0-rc1 object and class model clarified: ports and dispatch, selectors, extensions (genesis `ClassExtension`), relationships, reference integrity, blind indexes; new adapter operations. |
| DEC-041 | BPA 1.0-rc1 versioning, time and transactions clarified: branch kinds and write policy, change sets (genesis `ChangeSet`), previews, resolutions, tags and releases (genesis `Release`), revert, inheritance, snapshots and evidence, impact sets, rule binding, derivations, lineage, idempotency, outbox, bulk processes; two error codes and 20 adapter operations. |
| DEC-042 | BPA 1.0-rc1 proofs clarified: signature format and key histories, signed commit records, inclusion and consistency proofs, tenant root, archive format `ubos.archive.v1`, AEAD inputs; error code `PROOF.VERIFICATION_FAILED`; plain-Python crypto in the oracle. |
| DEC-043 | BPA 1.0-rc1 L1 expressions, queries, addressing and custom value types clarified; L1 interpreter in the oracle; adapter DIDs in registered form; step 1e split into 1e-1 and 1e-2 (logic in 1e-2 as host programs). |
| DEC-044 | BPA 1.0-rc1 execution clarified: host programs as the conformance binding (genesis `Logic`), context, gateway, profiles, metering, entropy, journal and replay, asset verification and static analysis, instructions, BPU views, automatic retry; rc1 MUST coverage 100%. |
| DEC-045 | BPA 1.0-rc1 released: hand-written notes listed in the manifest, owner-signed tag `bpa-v1.0.0-rc1`, frozen once tagged; PH-1 step 1 complete. |
| DEC-046 | Group commit and ledger sequence model (VER-FORM-0004, EXIT-1-07): two-phase flush of W-8 checked with 0 violations; DSN-DVM-006 states the two-phase flush, DSN-DVM-402 requires a transactional counter row. |
| DEC-047 | Storage kernel in parts 3a–3c: asynchronous `StoreTx`/`StoreRead` on sqlx, DVM-04 schema as migrations with transition columns until step 4, local blob and key providers until step 7, storage suite on PostgreSQL 16 locally and in CI (ENV-REF-SERVER run at step 9). |
| DEC-048 | Storage tenancy and physical form: 64 hash partitions, row-level security through role `ubos_app`, immutable ledger entries, stored form zstd + XChaCha20-Poly1305 with integrity check, chunk placement at 8 KiB, adapter reset to a fresh VAE. |
| DEC-049 | Index, sequence and root verifiers; storage suite SUITE-DVM-STORE with 20 cases passing on PostgreSQL 16; inspection record of the backend-neutral traits; EXIT-1-03 met. |
| DEC-050 | Engine at production quality in parts 4a–4d; 4a: full class registry (C3, traits, narrowing, ports, extensions), commit-time reference, relationship and extension checks; custom value types wait for the L1 evaluator (step 5). |
| DEC-051 | Versioning in the kernel: commit records with identity and HLC transaction time, branch metadata and lifetimes in settings until 4d, parent-line views with hide markers, merges with recorded resolutions and STD-VER-055 validation, previews, change sets, tags, releases, revert, explicit processes recorded. |

The full log with context and alternatives is `docs/50-system/90-decisions/000-index.md`.

Values confirmed by the owner (2026-09-29): Form 1099-NEC/MISC threshold for tax year 2026
= USD 2,000.00; Texas, City of Austin combined sales tax rate = 8.25%.

## 3. What exists

**System specification (`docs/50-system/`)** — about 250 documents, fully cross-checked:
33 personas, 220 capabilities, 67 scenarios (19 deferred), 800 FR, 106 NR, 54 CR, 239
standard clauses, 695 design items, 118 interfaces, 85 exit criteria (7 withdrawn), 77
suites, 90 verification items, 30 decisions. Traceability has 0 Must gaps in every phase.
Annex `10-requirements/10-finance-mapping.md` maps the finance requirements to the platform.

**Finance requirements (`docs/finance-requirements/`, version 1.0, frozen)** — 153 requirements (130 Must, 20
Should, 3 Could) in 14 areas; sample company Northwind Components, Inc. with a CSV dataset
and generated expected results (January 2026 net income 5,127.10; total assets 617,415.00);
15 acceptance scenarios covering every Must requirement; comparison protocol with 14
metrics and 6 standard change requests; work-item and defect templates in `templates/`;
own generator and checker in `tools/` (`sh tools/regen.sh`). Copy guide for other
repositories: `docs/reports/finance-requirements-copy-guide-zh.md`.

**Reports to the owner (Chinese):** `docs/reports/ubos-system-spec-summary-zh.md`,
`docs/reports/finance-requirements-summary-zh.md`.

**Tools:** `docs/tools/` holds `check_system.py`, `gen_phase_features.py`,
`gen_traceability.py`, `gen_indexes.py`; `docs/finance-requirements/tools/` holds
`gen_finance_expected.py`, `check_finance.py` and `regen.sh`; `docs/tools/regen_all.sh` runs
all. Expected result today: system volume `errors: 0, warnings: 0`;
finance `errors: 0`.

## 4. Next steps

Done on 2026-09-29: finance requirements frozen at 1.0 (DEC-029); monorepo decided (DEC-028);
PH-0 plan confirmed by the owner and step 0 done (DEC-030). The step plan and status are in
`docs/reports/ph0-plan-zh.md`.

Step 1 done: monorepo skeleton (`standard/`, `verify/`, `crates/` with five skeleton
crates), licence files and `LICENSING.md`, CI (`.github/workflows/ci.yml`), dependency-graph
checker `docs/tools/check_deps.py` reading UBS-ARC-07 §2.

Step 2 done: TLA+ models in `verify/models/` checked per DEC-031, no violation; report
`verify/models/reports/README.md`; finding F-1 fixed in DSN-DVM-208; OQ-004 and OQ-005 open
(owner to decide).

Step 3a done (DEC-032): release build in `standard/` (oracle, generator, builder), 390
vectors for FND, OBJ, CLS and KIND, STD-CONF-005 adapter operations; OQ-006 open.

Step 3b done: 551 vectors, MUST coverage 94% (68/72), manifest in
`standard/release/0.1/manifest.json`.

PH-0 status (details in UBS-PHR-0, `docs/50-system/50-phases/PH-0/02-findings.md`):
EXIT-0-01…04 and EXIT-0-06 met (steps 3, 4 and 6); step 5 prepared (5a–5c: tree
maintenance, storage layout, generator, benchmark driver, group commit), but EXIT-0-05 is
carried over into PH-1 because hosts cannot be rented now (DEC-035); step 7 done: PH-1
re-baselined (DEC-036, EXIT-0-07) with work items W-1…W-12, OQ-007 and OQ-008 resolved.

PH-0 accepted (DEC-037). PH-1 started (DEC-038); gate-1a steps and status in
`docs/reports/ph1-plan-zh.md` and UBS-PH-1 §11.

1. **Next AI step:** PH-1 step 1 in six parts (DEC-039): 1a done (rc1 scope rule, tooling,
   STD-OBJ-009); 1b done (object and class model, DEC-040); 1c done (versioning, time and
   transactions, DEC-041); 1d done (proofs, DEC-042); 1e split into 1e-1 and 1e-2 (DEC-043);
   1e-1 done (L1 expressions, queries, addressing, value types); 1e-2 done (execution,
   DEC-044; rc1 MUST coverage 146/146); 1f done (release, DEC-045). Owner: sign the tag
   `bpa-v1.0.0-rc1` (`docs/reports/ph1-plan-zh.md`). Step 2 done (group commit and ledger
   sequence model `verify/models/group_commit.tla`, VER-FORM-0004, EXIT-1-07, DEC-046).
   Step 3 (storage kernel, EXIT-1-03, W-7) in parts 3a–3c (DEC-047): 3a done (async
   storage traits, sqlx adapter, migrations with transition layout, two-phase group commit);
   3b done (partitions, row-level security with role `ubos_app`, physical form, chunk
   placement, `ubos-crypto`, DEC-048); 3c done (verifiers, storage suite 20/20,
   inspection record, `verify/reports/exit-1-03/`, DEC-049; EXIT-1-03 met, the ENV-REF-SERVER
   run follows at step 9). Step 4 (engine at production quality, EXIT-1-02, W-10) in parts
   4a–4d (DEC-050): 4a done (full class registry, relationships and extensions in the
   kernel; CLS, OBJ and ERR vectors pass; 1,170 of 1,227 vectors pass in memory and on
   PostgreSQL); 4b done (versioning in the kernel: commit identity and transaction time,
   branch metadata, parent-line views, merges with resolutions and validation, previews,
   change sets, tags, releases, revert, processes; VER 122/122, DEC-051); 4c done (time,
   ledger and transactions in the kernel: evaluation evidence and `follow`, range queries
   and history, derivation records and impact sets, idempotent requests, outbox and
   became-effective events, bulk processes, AEAD registry; 841 of 1,227 vectors pass in
   memory and on PostgreSQL, DEC-052); 4d-1 done (production identities of the kernel
   schema, migration 0003: commits keyed by content ID, UUID identifiers, timestamptz valid
   times, `head_index` with intervals replacing `object_state`, write sets in commit
   records, verifier rebuilt from them; DEC-053); 4d-2 done (metadata tables, effects,
   idempotency and derivation records and timers written in the flush, VAE authority and
   principal as engine scope, `parse_uri`, `resolve`, `parse_did`, `key_did`, interim signed
   runner report `verify/reports/exit-1-02/`; 869 of 1,227 vectors pass in memory and on
   PostgreSQL; DEC-054). Step 4 is complete. Step 5 (runtime, EXIT-1-10) in parts 5a–5d
   (DEC-055): 5a done (crate `ubos-expr`: L1 lexer, parser, type checker, evaluator and
   ABI 1.0 function library; custom value types through a registry hook; `check_expr`,
   `eval_expr`, `validate_value` with value types, `query`; 1,009 of 1,227 vectors pass in
   memory and on PostgreSQL); 5b done (DEC-056: ABI 1.0 catalogue embedded from the clause
   text, gateway, execution context, metering and entropy and the host-program interpreter
   in `ubos-runtime`, the store half of the instructions as the engine's `Host`, `execute`
   with automatic retry, journal and metering records; 1,127 of 1,227 vectors pass in memory
   and on PostgreSQL). 5c done (DEC-057: verification gate with
   node-signed records and a minimal Ed25519 node key in `ubos-crypto`, journals and
   metering records as chunks referenced from the process record, replay, BPU views,
   `node_did`; 1,165 of 1,227 vectors pass in memory and on PostgreSQL). 5d done (DEC-058: the L2 host in
   `ubos-runtime::l2` on a worker thread with the step-5b gateway, typed handles, kernel
   decimal and money, memory, CPU and wall limits; the escape suite of 382 cases all
   blocked, `verify/reports/exit-1-10/`, EXIT-1-10 met; W-2…W-5 and W-12 closed). Step 5 is
   complete. Step 6 (proofs, EXIT-1-12) in parts 6a–6c: 6a done (DEC-059: every commit
   record node-signed at flush through the store's node record, signature verification
   against DID documents with key histories in `ubos-crypto`, crate `ubos-proof` with the
   storage-free inclusion, commit-record and consistency checks, `rotate_node_key`,
   `did_document`, `commit_record`; 1,203 of 1,227 vectors pass in memory and on
   PostgreSQL); 6b done (`prove`, `consistency_proof`, `tenant_root`, `export_archive`,
   `import_archive` with `verify_archive` in `ubos-proof`; 1,227 of 1,227 vectors pass in
   memory and on PostgreSQL); 6c done (`ubos-verify` CLI and library in
   `verify/ubos-verify/` over `ubos-proof` only, structured reports; `ubos-roundtrip` and
   the EXIT-1-12 report `verify/reports/exit-1-12/`: 3 VAEs identical after import and
   re-import, archives verified). Step 6 is complete. Step 7 (EXIT-1-02, EXIT-1-13) in
   parts 7a–7c (DEC-060): 7a done (host boundary `ubos-dvm::host` with the CTR-002 service
   traits and the `Vae` host API; crate `ubos-node` with the PostgreSQL services: tenant
   registry, key hierarchy, node signer, blobs, secrets, journaled connectors, event log,
   jobs, telemetry; SUITE-NOD-HOST 12/12, `verify/reports/suite-nod-host/`); 7b done (Forge
   project model and `forge init`, `check` with every stage in one run, `types`, `test`
   with scenario, unit, port and property tests under the deterministic services, JSON and
   JUnit reports, signed verification records; `verify/reports/forge-7b/`); 7c done (the
   ten gate-1a scenarios as the Forge project `verify/scenarios/`, 22 of 22 tests on
   PostgreSQL and in memory, `verify/reports/exit-1-13/`; final EXIT-1-02 reports with
   1,227 of 1,227 on both backends, `verify/reports/exit-1-02/`). Step 7 is complete.
   Step 8 (DEC-061, verification close-out) in progress: 8a done (kernel property set
   VER-PROP-0100 at a million cases per property, `verify/reports/exit-1-04/`, EXIT-1-04
   met; coverage 90.1% of the kernel crates by the suites and a sampled mutation score
   89.9% on a workstation sample of 237 viable mutants (213 detected) with the vectors, scenarios and properties as test bodies,
   `verify/reports/exit-1-11/`, `verify/tools/{coverage.sh,mutants.sh}`); 8b done (the
   deterministic simulator `ubos-sim`, 1,000,000 seeds, 0 violations (54,305,916 commits, 6,274,491 aborted flushes, 3,137,413 crashes after apply, 7,453,874 clock jumps), `verify/reports/exit-1-05/`; the fault
   suite `ubos-fault` on PostgreSQL, 10,000 events (5,000 node crashes by SIGKILL of every worker, 5,000 power losses of the server), 31,732 acknowledged commits, 0 lost, 0 partial, 0 verifier findings, `verify/reports/exit-1-06/`); 8c done (`ubos-replay`: 10,000 generated processes,
   10,000 of 10,000 identical and 1,000 of 1,000 replays identical on Linux x86-64,
   `verify/replay/expected.json`, `verify/reports/exit-1-08/`; the nightly workflow
   `.github/workflows/nightly.yml` with the night log `verify/reports/exit-1-05/nights.md`
   on the branch `nightly-log`). Step 8 is complete apart from what the nights accumulate:
   30 consecutive clean nights (EXIT-1-05) and the ARM64 and macOS replay results
   (EXIT-1-08). Step 9 (gate-1a acceptance, DEC-062) in parts 9a–9c: 9a done (acceptance record
   UBS-PHA-1 `50-phases/PH-1/02-gate-1a-acceptance.md`, Chinese package and runbook in
   `docs/reports/`, `ubos-bench` completed for VER-BENCH-0100 with `branch`, `merge`,
   `asof` and `--background`; finding 1: the merge preview was linear in the objects of
   the VAE). 9b in progress: the merge-preview fix is done (`Repo::preview_states` walks
   the tree diff of the merge bases against the source head and the appended entries of
   the source commits, loads only those objects and ledger keys in bulk through the new
   `Store::objects_by_id`; workstation p50 0.37 s / p95 0.40 s at 20,000 objects, 36 s
   before, `verify/reports/exit-1-09/workstation/`). Next in 9b: the rental (W-1 first),
   the reference runs per `docs/reports/ph1-gate1a-runbook-zh.md`, the external review
   (UBS-PHR-1) and the acceptance decision. 9c done: the gate-1b step plan DEC-063 (steps 10–20 in UBS-PH-1 §11, status table in `docs/reports/ph1-plan-zh.md` §1b); Step 10 (BPA 1.0-rc2, DEC-064, parts 10a–10e): 10a done — `standard/release/1.0-rc2/` opened by rule (197 clauses, 26 added, MUST coverage 146/165), `regen_all.sh` builds rc2 and rc1 stays frozen, the policy-engine library chosen (DEC-065: `cedar-policy` in `ubos-authz`); 10b done — DEC-066 clarifies STD-LGS-001…023, the reference model `standard/tools/bpa/sheets.py` with the genesis classes `GovernanceSheet` and `PropertyDecl`, the operations `parse_sheet`, `check_sheet`, `specificity`, `cascade` and `explain_cascade`, 157 vectors, the kernel's conformance test skipping the unclaimed profiles; 10c done — DEC-067: `validations`, `invariants` and `decisions` as kernel properties with commit enforcement, the preview `validate`, decisions `decide` before command ports, port and overlay-policy checks, the textual query form, `read.explain`, `run.validate` and `search.query`, the supersession of two 1.0-rc1 vectors (`standard/vectors/superseded.txt`, STD-CONF-030), 116 vectors (MUST coverage 159 of 165); the kernel's conformance test now runs the vectors of the release it claims (1.0-rc1) and skips superseded ones; 10d done — DEC-068: the in-process binding `ubtp`, envelope, handshake, mutate, query, subscriptions, flow control (`standard/tools/bpa/ubtp.py`, 88 vectors, MUST coverage 165 of 165); 10e done — DEC-069: release notes finalised and listed in the manifest (197 clauses, 165 of 165 MUST clauses, 1,586 vectors), VER-INSP-9004, the owner signs the tag `bpa-v1.0.0-rc2` on the merge commit and the release is frozen; step 10 complete, the standard part of EXIT-1-14 met with the tag. Step 11 (DEC-070, parts 11a–11d): 11a done — `crates/ubos-rules` with the grammar, AST, registry, checks, specificity, rank and overlap analysis and the operations `parse_sheet`, `check_sheet`, `specificity` (76 vectors in the conformance test); 11b done — the genesis classes `GovernanceSheet` and `PropertyDecl`, the AST derived and the sheet set checked at commit, matching and the cascade over the state (`crates/ubos-dvm/src/sheets.rs`), `cascade`, `explain_cascade`, `read.explain` (cascade kind) and `read.effective declarations` (157 vectors of STD-LGS-001…023 in the conformance test); 11c done — commit enforcement, validations and invariants, the preview `validate`/`run.validate`, decisions before command ports with `decide` and the journaled `DecisionResult`, `read.explain` validation and decision kinds, STD-CLS-003/-031 and STD-BPU-003 (`crates/ubos-dvm/src/rules.rs`; the conformance test claims the rules and Core clauses of gate 1b, 1,475 vectors; the claimed vectors pass on PostgreSQL); 11d done — compiled sheets and the selector index (`ubos-rules::Cache`), `ubos-bench sheets` and `ubos-bench invariants`, workstation report `verify/reports/exit-1-14/workstation/` (NR-PERF-009 met: warm p95 0.41 ms per object with 10,000 blocks; a commit on a branch with sheets loads the full state, a fixed cost noted for step 13). Step 11 complete. Next: step 12 (flow: lifecycles, ports, guards and the process object; CAP-FLOW-01…03, DSN-DVM-106, -501, -730, -731) (validations, invariants, decisions, commit enforcement, Core clauses, claim of the rc2 clauses, PostgreSQL run) and 11d (NR-PERF-009 benchmark). W-1 Step 12 (DEC-071, parts 12a–12c): 12a done — BPA 1.0-rc3 opened by the scope rule (199 clauses), the lifecycle declaration STD-CLS-004 with the genesis class `Lifecycle` and the state in the version envelope, `read.actions` (STD-ISA-036) and the operation `actions`, `commit.transition` and `run.port` clarified (availability, guards, effects, field rules), the rule binding with guards, the lifecycle check of STD-BPU-011, the reference model `standard/tools/bpa/flow.py`, 56 vectors, VER-CONF-3000 superseded; 12b done — the crate `ubos-flow` (pure lifecycle engine) and the kernel's lifecycles (`crates/ubos-dvm/src/flow.rs`: genesis class `Lifecycle`, state in the version envelope, `commit.transition`, port availability and guards, `read.actions`/`actions`, guards and lifecycle versions in the rule binding, the lifecycle check of `verify_bpu`), the conformance test claiming the 12a clauses (1,531 vectors in memory, all pass on PostgreSQL but the 9 query vectors of step 13); 12c done — DEC-072: the process state machine of DSN-DVM-501 (`crates/ubos-dvm/src/process.rs`, phases and attempts in `process_info`) with the property `process_state_machine` (1,000,000 cases, 0 failures), the release notes and manifest of 1.0-rc3 (199 clauses, 166 of 166 MUST clauses, 1,642 vectors), VER-INSP-9005; the owner signs the tag `bpa-v1.0.0-rc3` after merge. Step 12 complete. Step 13 (DEC-073, parts 13a–13d): 13a done — the crate `ubos-query` (textual form, planner bounds, scan budgets, signed cursors, aggregates), declared field indexes maintained at flush (`field_index`, PostgreSQL migration 0007; `declare_index` before instances exist, unique-key fields indexed by default), the index path of `query` with `QRY.SCAN_LIMIT`, `states`/`select`/`group_by`/`aggregates`, `search.query`; the conformance test claims STD-EXPR-010 and STD-ISA-080 (1,554 vectors in memory, all pass; all Core, Governance, Bitemporal, Proof and Ledger vectors pass on PostgreSQL). 13b done — the crate `ubos-authz` (`cedar-policy` 4.13; entity model from the registry with policy-visible fields, `Policy` objects validated at commit, compiled sets cached by hash) and the kernel's enforcement (`crates/ubos-dvm/src/authz.rs`, executor `permit`): default deny once a branch holds policies, `grants` alone without, every instruction on its resource and on the branch, not-visible-means-not-found, host commit/read/query/merge with a `principal`, query row post-filter, decision log; adapter operations `authorize`, `decision_log`, `policy_schema`. Policy authors guard optional attributes with `has` (Cedar strict validation). 13c done — `crates/ubos-dvm/src/audit.rs`: `changes`, `transitions` and `audit` over the first-parent history (field changes from the versions, states left and entered, who/what/when/why/how/rules from the commit and process records; the commit record carries `on_behalf_of`, `channel`, `client`, `message`, `reason`; signed snapshot cursors; `read_history` row filter), the hash-chained security log (`security_log`, `verify_security_log`; fed by denials, policy changes, key rotation, export/import, limits, index declarations); `crates/ubos-dvm/src/views.rs`: `view` payloads (declared views with variants or generated views, field states from lifecycle field rules, sheets and permissions, masks, actions, staged-edit violations, `payload_hash`); `branch_info.overlay` counts an overlay's local commits, versions and objects. Host operations `view`, `audit`, `changes`, `transitions`, `security_log`. Noted for 13d: a merge into an overlay from a branch that shares no ancestry with it (a draft created from `main`) panics in `base_state_of` (`merge.rs`) instead of failing with a merge error. 13d done — DEC-074: the order-by pushdown (`offset + limit` index rows in key order, continued from the last key while rows are filtered, up to the budget), index rebuild with `index_since`, `Store::commit_meta` on history walks and snapshot selection, bulk version reads of query candidates, `VER.MERGE_NOT_ALLOWED` without a merge base, `ubos-bench query` (MET-PERF-016 suite) with the workstation report `verify/reports/exit-1-17/workstation/` (indexed pages p95 `eq` 7.3 ms / `key` 3.4 ms in memory and 36.4 ms / 6.4 ms on PostgreSQL at 300,000 objects, independent of the number of objects; a policy-filtered page p50 58 ms on PostgreSQL, but a page the policy denies entirely evaluates the whole key range (p95 819 ms at 300,000 objects, the scan budget at 10,000,000)); the row-filter technique recorded (residual post-filter, revisited per DEC-074). Step 13 complete. Step 14 (DEC-075, parts 14a–14d): 14a done — the crate `ubos-proto` (UBTP envelope checks, request and frame types, handshake negotiation, cursors; engine layer) and the kernel's in-process binding `crates/ubos-dvm/src/ubtp.rs` (the `ubtp` operation: sessions, handshake, mutate as a process or a raw commit with idempotency keys and `base`, the query kinds read/query/explain/history/timeline/diff, subscriptions to objects, queries and events with cursors, retention, the window of unacknowledged emits, ack, ping, cancel); the implicit process identifiers follow the reference model; the conformance test claims the Protocol clauses (1,642 vectors in memory; the Protocol vectors pass on PostgreSQL too). 14b done — the Server (`crates/ubos-node/src/server.rs`, binary `ubos-server`): a VAE actor thread per VAE, the request pipeline, `dev` credentials in the `dev` preset (local identity's kinds come with 14c), the WebSocket, HTTP and server-sent-events bindings, the REST facade with RFC 9457 problems, fan-out of emits with a bounded queue and a one-second poll, the memory preset for suites. 14c done — local identity (`crates/ubos-node/src/identity.rs`, migration 0008): principals with registered DIDs and the lifecycle of DSN-NOD-109, Argon2id passwords and TOTP with progressive delay and lockout per account and source, hashed-token sessions with timeouts and a concurrent limit, immediate revocation (re-validated by the Server every 5 s, bound connections closed at once), API keys with scopes intersected with the principal's grants, per-principal Ed25519 keys with rotation history and DID documents, service principals, the node's hash-chained security log fed by the kernel's decision and security logs, the secret store's operator interface; the Server's handshake credentials `password`/`bearer`/`api_key`, `Authorization: Bearer` on the facade, the identity API under `/api/v1/identity`, `--admin` bootstrap; the registry and identity lookups run off the runtime (the database mode of 14b panicked on them). 14d done — SUITE-NOD-PROTO (`ubos-proto-suite`): the 88 Protocol vectors pass on the in-process, HTTP and WebSocket bindings with identical frames (87; VER-CONF-5427 binding-specific, the Server refusing the bearer over the network), version skew 8 of 8 per binding, REST equivalence 4 of 4 (`verify/reports/exit-1-15/`); SUITE-NOD-AUTH (`ubos-auth-suite`) 13 of 13 on memory and PostgreSQL (`verify/reports/suite-nod-auth/`); fixed: events subscriptions under a host (`Store::outbox_released`), refused attempts counting towards the lock, revocation closing idle WebSockets; CI runs `cargo test -p ubos-node`. Step 14 complete; EXIT-1-15 NOD part met on the workstation (ENV-REF-SMALL and the SDK part with step 17). Next: step 15.
   may still change partition counts and commit-path costs by a further migration (DEC-047).
2. **Owner, when possible:** rent ENV-REF-SERVER and run `docs/reports/ph0-step5-runbook-zh.md`
   (W-1, before the storage layout of step 3 is fixed); name the external reviewer for that
   occasion.
3. **Owner, now — Actions runners:** since 2026-10-08 every job of the repository (the
   nightly workflow and the CI of the step-9a pull request, three attempts) fails with no
   runner assigned; the nights of 2026-10-06 and -07 ran and were clean on all three
   platforms. Check the Actions usage and spending limit of the account (Settings →
   Billing), then re-run the failed jobs. The nightly workflow now costs about 1.5 h of Linux minutes a night (simulation and
   properties) and, on Sundays, up to 3 h for the mutation shard and about 10 min each on
   Linux ARM64 and macOS for the replay corpus (the owner's choice of 2026-10-14); merge
   `nightly-log` into `main` from time to time.
4. **Owner, after the merges of steps 10e and 12c:** create the signed tags `bpa-v1.0.0-rc2`
   (DEC-069) and `bpa-v1.0.0-rc3` (DEC-072) on their merge commits with the manifest
   `content_id` in the tag message (procedure in `docs/reports/ph1-plan-zh.md` §3). The
   remote holds only `fin-req-v1.0`: the tags `bpa-v0.1.0` and `bpa-v1.0.0-rc1` (DEC-045)
   are still to be created and pushed as well.
5. **Java side** (owner): copy `fin-req-v1.0` into the Java repository
   (`docs/reports/finance-requirements-copy-guide-zh.md`) and start work-item records.
6. **Accounting review** (external): chart of accounts and expected results of the sample
   company (EXIT-2-16); findings become finance version 1.1.

## 5. Open items

| Item | Note |
|---|---|
| Legal review of licences | DEC-019 accepted; legal review before any public release. |
| Accounting reviewer | to be named (ASM-002, PH-3 CPA). |
| PH-0 external reviewer | database and formal-methods expert (DEC-030), to be named by the owner for the review at the carried benchmark (DEC-037). |
| ENV-REF-SERVER | rented by the owner for the carried benchmark (DEC-035), CPU with SHA extensions. |

## 6. Repositories

- `staunch0515/ubos-unit` — **current home** of the specification and the finance
  requirements (this content, with its git history), and the implementation monorepo
  (DEC-028). Continue work here.
- The owner's Java finance system lives in a separate, unrelated repository; it only holds
  an unchanged copy of a tagged finance-requirements version (DEC-029).
- `staunch0515/ubos-platform` — original repository; its `main` branch contains the same
  documents up to the migration, plus the older Java code of the research phase. Kept as an
  archive.
- The seven other research repositories (`ubos`, `ubos-system`, `ubos_core`, `ubos_web`,
  `logicorum`, `logrum-system`, `fund-ubos`) are inputs only (`docs/50-system/00-meta/06-inputs.md`).

## 7. How to resume

1. Read `CLAUDE.md`, this file, `docs/50-system/README.md` and
   `docs/50-system/00-meta/05-ai-reading-guide.md`.
2. Run `bash docs/tools/regen_all.sh` and confirm 0 errors.
3. Report to the owner in Chinese: your understanding of the state, and a recommended next
   step from §4. Wait for confirmation before changing documents.
