---
id: UBS-SYS-DVM-04
title: DVM — Physical Data Design
status: draft
phase: PH-1
depends_on: [UBS-SYS-DVM-02-01, UBS-ARC-04]
---

# DVM — Physical Data Design

This document refines the logical table inventory of UBS-ARC-04 §3 into the physical
layout used by the PostgreSQL adapter. The SQLite adapter uses the same tables and columns
without partitioning and row-level security (one database file per VAE on Box, mobile and
browser). Business classes never become tables (AR-018): every business object lives in
the generic tables below.

## 1. Conventions

| Topic | Rule |
|---|---|
| tenant column | every table has `tenant_id uuid NOT NULL` as the first key column (AR-017) |
| hashes | content IDs are stored as `bytea` of 32 bytes (the `sha256:` prefix is implied) |
| object IDs | `uuid` (UUIDv7) |
| branches | `branch_id bigint`, internal surrogate per VAE; names live in `branch_ref` |
| commit ordinals | `cseq bigint`, dense per branch (DSN-DVM-004) |
| open intervals | `cseq_to = 9223372036854775807` and `valid_to = 'infinity'` mean open |
| time | `timestamptz` in UTC with microsecond precision |
| partitioning | hash partitioning by `tenant_id` into 64 partitions for large tables (DSN-DVM-010) |
| RLS | `USING (tenant_id = current_setting('ubos.tenant')::uuid)` on every table |
| categories | A–K per UBS-ARC-04 §1; category D and E rows are derived and rebuildable (AR-015) |

The logical columns `tx_from` and `tx_to` of `temporal_index` (UBS-ARC-04) are stored as
commit ordinals `cseq_from` and `cseq_to`. Transaction time is read from `commit` when
needed.

**Layout as implemented (DEC-053).** The migrations of `ubos-store` follow the tables below
with these settled points: classes are identified by name within a VAE and by the content
ID of their declaration (`class_name`, `class_version`), not by a UUID; the engine keeps an
integer handle `commit_no` beside `commit_id` (a surrogate like `branch_id`); ledger entries
are identified by their UUID `entry_id`, their content ID being `entry_hash`; `head_index`
rows carry `cseq` intervals, kind, slug and granularity, so that the engine's state of an
object at any ordinal is its head row and its `temporal_index` segments; every index row
records the valid-time granularity of its class (0 date, 1 instant) so that the canonical
text of the standard is reproduced on read; and every commit record carries the object rows
it wrote (`objects`), from which the verifier rebuilds the indexes.

## 2. Authoritative tables (categories A–C)

```sql
CREATE TABLE chunk (
  tenant_id     uuid    NOT NULL,
  content_id    bytea   NOT NULL,          -- hash of logical bytes
  placement     smallint NOT NULL,         -- 0 inline, 1 object store, 2 cold tier, 3 disposed
  size_logical  integer NOT NULL,
  body          bytea,                     -- zstd + tenant AEAD, when inline
  disposal_cert bytea,                     -- when disposed
  PRIMARY KEY (tenant_id, content_id)
) PARTITION BY HASH (tenant_id);

CREATE TABLE object_version (
  tenant_id     uuid   NOT NULL,
  version_id    bytea  NOT NULL,           -- content ID of the envelope
  object_id     uuid   NOT NULL,
  vae_id        uuid   NOT NULL,           -- VAE of the first writer
  class_name    text   NOT NULL,           -- classes are named within a VAE (DEC-053)
  class_version bytea  NOT NULL,           -- content ID of the declaration at write time
  kind          smallint NOT NULL,         -- 1 definition, 2 ledger, 3 system, 4 raw_document, 5 inherit
  valid_from    timestamptz NOT NULL,
  valid_to      timestamptz NOT NULL,      -- 'infinity' when open
  envelope      bytea  NOT NULL,           -- canonical envelope with body, zstd + AEAD
  chunk_ids     bytea[] ,                  -- for chunked bodies
  flags         integer NOT NULL,          -- ai-generated, retroactive, lens-migrated …
  PRIMARY KEY (tenant_id, version_id)
) PARTITION BY HASH (tenant_id);

CREATE TABLE ledger_entry (
  tenant_id     uuid    NOT NULL,
  vae_id        uuid    NOT NULL,
  branch_id     bigint  NOT NULL,
  ledger_class  text    NOT NULL,          -- class name (DEC-053)
  ledger_key    bytea   NOT NULL,          -- canonical key tuple hash
  seq           bigint  NOT NULL,
  entry_id      uuid    NOT NULL,          -- the entry's identifier (STD-KIND-020)
  entry_hash    bytea   NOT NULL,          -- content ID of the entry
  valid_at      timestamptz NOT NULL,
  cseq          bigint  NOT NULL,
  idx           integer NOT NULL,          -- position in the commit (append order)
  prev_hash     bytea   NOT NULL,          -- hash chain per key
  body          bytea   NOT NULL,          -- canonical entry record (corrections, posting group, sources)
  PRIMARY KEY (tenant_id, vae_id, branch_id, ledger_class, ledger_key, seq)
) PARTITION BY HASH (tenant_id);
-- UPDATE and DELETE revoked from the application role; trigger raises LEDG.IMMUTABLE_ENTRY.

CREATE TABLE ledger_sequence (
  tenant_id uuid, vae_id uuid, branch_id bigint, ledger_class uuid, ledger_key bytea,
  next_seq bigint NOT NULL, last_hash bytea NOT NULL,
  PRIMARY KEY (tenant_id, vae_id, branch_id, ledger_class, ledger_key)
);

CREATE TABLE ledger_reversal (
  tenant_id uuid, vae_id uuid, branch_id bigint, reversed_entry bytea, reversing_entry bytea,
  PRIMARY KEY (tenant_id, vae_id, branch_id, reversed_entry)       -- at most one reversal
);

CREATE TABLE commit (
  tenant_id   uuid   NOT NULL,
  commit_id   bytea  NOT NULL,             -- content ID of the canonical commit record
  vae_id      uuid   NOT NULL,
  branch_id   bigint NOT NULL,
  cseq        bigint NOT NULL,
  commit_no   bigint NOT NULL,             -- the engine's internal handle (DEC-053)
  tx_time     timestamptz,                 -- NULL for a genesis commit
  state_root  bytea  NOT NULL,
  process_id  uuid,                        -- explicit processes only (DEC-051)
  author      text,                        -- did:ubos of the initiator
  body        bytea  NOT NULL,             -- commit record with its write set (`objects`)
  PRIMARY KEY (tenant_id, vae_id, commit_id),
  UNIQUE (tenant_id, vae_id, branch_id, cseq),
  UNIQUE (tenant_id, vae_id, commit_no)
);
CREATE INDEX commit_time ON commit (tenant_id, vae_id, branch_id, tx_time);

CREATE TABLE commit_parent  (tenant_id uuid, vae_id uuid, commit_id bytea, ord smallint, parent_id bytea,
                             PRIMARY KEY (tenant_id, vae_id, commit_id, ord));
CREATE TABLE commit_skip    (tenant_id uuid, commit_id bytea, level smallint, target_id bytea,
                             PRIMARY KEY (tenant_id, commit_id, level));  -- DSN-DVM-933
CREATE TABLE commit_signature (tenant_id uuid, commit_id bytea, signer text, role text,
                             signature bytea, added_at timestamptz,
                             PRIMARY KEY (tenant_id, commit_id, signer, role));

CREATE TABLE branch_ref (
  tenant_id    uuid   NOT NULL,
  vae_id       uuid   NOT NULL,
  branch_id    bigint NOT NULL,
  name         text   NOT NULL,
  kind         smallint NOT NULL,
  state        smallint NOT NULL,
  head_no      bigint NOT NULL,            -- handle of the head commit (DEC-053)
  head_commit  bytea,                      -- content ID of the head; NULL before the first commit
  head_cseq    bigint NOT NULL,
  parent_branch bigint,                    -- read-through parent
  fork_cseq    bigint,                     -- fork or tracked upstream point
  depth        smallint NOT NULL,
  created_at   timestamptz, active_at timestamptz,     -- lifetimes (STD-VER-011, DEC-054)
  parent_line  jsonb,                      -- {branch, mode, pin} (STD-VER-080)
  created_from bytea,                      -- commit the branch was created from
  leader_node  text, leader_lease_until timestamptz,   -- optional sequencer lease
  PRIMARY KEY (tenant_id, vae_id, branch_id),
  UNIQUE (tenant_id, vae_id, name)
);

CREATE TABLE tag (tenant_id uuid, vae_id uuid, name text, commit_id bytea, branch text,
                  signer text, signature text,          -- a content ID until step 6 (DEC-051)
                  tombstoned boolean NOT NULL DEFAULT false,
                  PRIMARY KEY (tenant_id, vae_id, name));

CREATE TABLE change_set (tenant_id uuid, vae_id uuid, change_set_id uuid, target text,
                         merge_commit bytea,            -- STD-VER-020 (DEC-054)
                         PRIMARY KEY (tenant_id, vae_id, change_set_id));
```

## 3. Kernel indexes (category D)

```sql
CREATE TABLE merkle_node (tenant_id uuid, node_id bytea, level smallint, body bytea,
                          PRIMARY KEY (tenant_id, node_id)) PARTITION BY HASH (tenant_id);

CREATE TABLE head_index (                 -- the head of an object per commit interval (DEC-053)
  tenant_id uuid, vae_id uuid, branch_id bigint, object_id uuid,
  cseq_from bigint, cseq_to bigint,
  version_id bytea,                        -- NULL: no version on the branch
  class_name text, kind smallint, slug text,
  granularity smallint,                    -- 0 date, 1 instant
  hidden boolean,                          -- removed on the branch
  PRIMARY KEY (tenant_id, vae_id, branch_id, object_id, cseq_from)
);
CREATE INDEX head_class ON head_index (tenant_id, vae_id, branch_id, class_name, cseq_to);

CREATE TABLE temporal_index (
  tenant_id uuid, vae_id uuid, branch_id bigint, object_id uuid,
  valid_from timestamptz, valid_to timestamptz,
  cseq_from bigint, cseq_to bigint,
  version_id bytea, class_name text, granularity smallint,
  PRIMARY KEY (tenant_id, vae_id, branch_id, object_id, valid_from, cseq_from)
) PARTITION BY HASH (tenant_id);
CREATE INDEX temporal_valid ON temporal_index USING gist
  (tenant_id, vae_id, branch_id, tstzrange(valid_from, valid_to), int8range(cseq_from, cseq_to));
CREATE INDEX temporal_cseq ON temporal_index USING gist (tenant_id, vae_id, branch_id, int8range(cseq_from, cseq_to));
CREATE INDEX temporal_class ON temporal_index (tenant_id, vae_id, branch_id, class_name, cseq_to);

CREATE TABLE relationship_index (
  tenant_id uuid, vae_id uuid, branch_id bigint,
  source_id uuid, relation uuid, target_id uuid,
  valid_from timestamptz, valid_to timestamptz, cseq_from bigint, cseq_to bigint,
  authz boolean,
  PRIMARY KEY (tenant_id, vae_id, branch_id, source_id, relation, target_id, valid_from, cseq_from)
);
CREATE INDEX rel_reverse ON relationship_index (tenant_id, vae_id, branch_id, target_id, relation);

CREATE TABLE unique_key_index (
  tenant_id uuid, vae_id uuid, branch_id bigint, key_decl uuid, key_value bytea,
  valid_from timestamptz, valid_to timestamptz, object_id uuid, cseq_from bigint, cseq_to bigint,
  EXCLUDE USING gist (tenant_id WITH =, vae_id WITH =, branch_id WITH =, key_decl WITH =,
                      key_value WITH =, tstzrange(valid_from, valid_to) WITH &&,
                      int8range(cseq_from, cseq_to) WITH &&)
);

CREATE TABLE field_index (
  tenant_id uuid, vae_id uuid, branch_id bigint, class_ord integer, field_id uuid,
  value_key bytea,                          -- order-preserving encoding per type
  object_id uuid, cseq_from bigint, cseq_to bigint,
  PRIMARY KEY (tenant_id, vae_id, branch_id, field_id, value_key, object_id, cseq_from)
) PARTITION BY HASH (tenant_id);

CREATE TABLE derivation_edge (                -- one row per input of a derivation record (DEC-054)
  tenant_id uuid, vae_id uuid, binding_id bytea,  -- the record: a chunk under its content ID
  output_version bytea, input_object uuid, input_version bytea,
  valid_at timestamptz, granularity smallint,
  PRIMARY KEY (tenant_id, vae_id, binding_id, output_version, input_object, input_version, valid_at)
);
CREATE INDEX derivation_out ON derivation_edge (tenant_id, vae_id, output_version);
CREATE INDEX derivation_in ON derivation_edge (tenant_id, vae_id, input_version);

CREATE TABLE commit_write_summary (
  tenant_id uuid, vae_id uuid, branch_id bigint, cseq bigint,
  object_ids uuid[], index_ranges bytea,   -- compact encoded ranges
  PRIMARY KEY (tenant_id, vae_id, branch_id, cseq)
);                                          -- retained ≥ maximum process age
```

## 4. Projections (category E)

```sql
CREATE TABLE balance_delta (
  tenant_id uuid, vae_id uuid, branch_id bigint, projection uuid, key bytea, currency char(3),
  valid_at timestamptz, cseq bigint, delta numeric(38, 18),
  PRIMARY KEY (tenant_id, vae_id, branch_id, projection, key, currency, valid_at, cseq)
) PARTITION BY HASH (tenant_id);

CREATE TABLE balance_checkpoint (
  tenant_id uuid, vae_id uuid, branch_id bigint, projection uuid, key bytea, currency char(3),
  valid_at timestamptz, cseq bigint, balance numeric(38, 18), delta_count bigint,
  PRIMARY KEY (tenant_id, vae_id, branch_id, projection, key, currency, cseq, valid_at)
);

CREATE TABLE projection_agg (
  tenant_id uuid, vae_id uuid, branch_id bigint, projection uuid, group_key bytea,
  cseq_from bigint, cseq_to bigint, value bytea,
  PRIMARY KEY (tenant_id, vae_id, branch_id, projection, group_key, cseq_from)
);

CREATE TABLE membership_closure (
  tenant_id uuid, principal text, group_id text, cseq_from bigint, cseq_to bigint,
  PRIMARY KEY (tenant_id, principal, group_id, cseq_from)
);
```

`numeric(38, 18)` holds decimal amounts in the database only for aggregation. The canonical
value in the entry body remains the decimal string (STD-FND-022).

## 5. Operational tables (categories G and H)

```sql
CREATE TABLE process_record (                 -- STD-TXN-031 (DEC-054)
  tenant_id uuid, process_id uuid, vae_id uuid, seq bigint GENERATED ALWAYS AS IDENTITY,
  kind text, state text, initiator text, trigger_kind text, trigger_ref text,
  parent_process uuid, bulk uuid, idempotency_key text,
  effects integer, commits jsonb,          -- [[branch, cseq], …]
  bulk_state jsonb,                        -- STD-TXN-070: mode, staging, children, attempts
  journal_id bytea, metering bytea, error bytea,     -- step 5
  started_at timestamptz, ended_at timestamptz,
  PRIMARY KEY (tenant_id, vae_id, process_id)
);

CREATE TABLE idempotency_record (             -- scope = '<vae>:<initiator>' (STD-TXN-040)
  tenant_id uuid, scope text, key text, request_hash bytea, outcome bytea, commit_id bytea,
  expires_at timestamptz,
  PRIMARY KEY (tenant_id, scope, key)
);

CREATE TABLE outbox (
  tenant_id uuid, id bigint GENERATED ALWAYS AS IDENTITY, vae_id uuid, branch_id bigint,
  cseq bigint, idx integer, kind smallint, aggregate_key bytea, payload bytea,
  effect_id text,                          -- stable identifier (STD-TXN-050)
  claimed_by text, claim_until timestamptz, attempts integer DEFAULT 0, released_at timestamptz,
  PRIMARY KEY (tenant_id, id),
  UNIQUE (tenant_id, vae_id, effect_id)
);
CREATE INDEX outbox_ready ON outbox (claim_until) WHERE released_at IS NULL;

CREATE TABLE timer (
  tenant_id uuid, vae_id uuid, id uuid, due_at timestamptz, target uuid, step jsonb, attempt integer,
  claimed_by text, claim_until timestamptz, fired_at timestamptz,
  PRIMARY KEY (tenant_id, id)
);
CREATE INDEX timer_due ON timer (tenant_id, vae_id, due_at) WHERE fired_at IS NULL;

CREATE TABLE decision_log (
  tenant_id uuid, at timestamptz, principal text, action text, resource text,
  outcome smallint, policy_ids text[], process_id uuid
) PARTITION BY RANGE (at);
```

Journals are stored as chunks. `process_record.journal_id` points to the journal's root
chunk, and `process_record.metering` to the metering record's chunk (migration
`0005_journal.sql`, DEC-057). The verification record of a logic asset (STD-CTX-023) is
kept under its record ID and referenced by the asset's `verification` member.

## 6. Row structures of derived tables

### DAT-TemporalIndexRow
- **Purpose:** One timeline segment of one object, visible over a commit-ordinal interval.
- **Kind:** Projection
- **Stored in:** `temporal_index`
- **Schema:**
```yaml
DAT-TemporalIndexRow:
  object_id:  { type: uuid, required: true, description: object identity }
  valid_from: { type: instant, required: true, description: start of validity (inclusive) }
  valid_to:   { type: instant, required: true, description: end of validity (exclusive) }
  cseq_from:  { type: integer, required: true, description: first commit ordinal where the row is visible }
  cseq_to:    { type: integer, required: true, description: first commit ordinal where it is no longer visible }
  version_id: { type: content_id, required: true, description: the version valid in this segment }
  class_name: { type: string, required: true, description: class of the object (DEC-053) }
  granularity: { type: integer, required: true, description: valid-time granularity of the class, 0 date or 1 instant }
```
- **Invariants:**
  1. For one `(object, cseq)`, segments do not overlap in valid time.
  2. The set of rows equals the rebuild from the write sets of the commit records (DSN-DVM-092, DEC-053).
- **Satisfies:** STD-TIME-011, STD-TIME-012, AR-016

### DAT-BalanceDelta
- **Purpose:** One change to one balance, with bitemporal coordinates.
- **Kind:** Projection
- **Stored in:** `balance_delta`
- **Schema:**
```yaml
DAT-BalanceDelta:
  projection: { type: uuid, required: true, description: projection declaration }
  key:        { type: bytes, required: true, description: canonical group key }
  currency:   { type: string, required: true, description: ISO 4217 code }
  valid_at:   { type: instant, required: true, description: business date of the entry }
  cseq:       { type: integer, required: true, description: commit ordinal of the entry }
  delta:      { type: decimal, required: true, description: signed amount }
```
- **Invariants:**
  1. Balance at `(asof, s)` = checkpoint + Σ delta where `valid_at ≤ asof ∧ cseq ≤ s` after the checkpoint coordinates.
- **Satisfies:** FR-LEDG-031, FR-LEDG-032

### DAT-DerivationEdge
- **Purpose:** One input dependency of one derived version.
- **Kind:** Projection
- **Stored in:** `derivation_edge`
- **Schema:**
```yaml
DAT-DerivationEdge:
  output_version: { type: content_id, required: true, description: derived version }
  input_version:  { type: content_id, required: true, description: version read as input }
  binding_id:     { type: content_id, required: true, description: the derivation record (DAT-Derivation), stored as a chunk under this ID }
  input_object:   { type: uuid, required: true, description: object or entry the input belongs to }
  valid_at:       { type: instant, required: true, description: evaluation valid time (asof), with its granularity }
```
- **Invariants:**
  1. Every edge corresponds to a derivation record in the commit (STD-TXN-030); the edges are written in the commit's flush (DSN-DVM-533, DEC-054).
- **Satisfies:** FR-TXN-042, FR-TIME-042

### DAT-OutboxRow
- **Purpose:** One effect awaiting release after commit.
- **Kind:** System
- **Stored in:** `outbox`
- **Schema:**
```yaml
DAT-OutboxRow:
  id:            { type: integer, required: true, description: monotonic row ID }
  cseq:          { type: integer, required: true, description: commit ordinal }
  idx:           { type: integer, required: true, description: order within the commit }
  kind:          { type: enum, required: true, description: event | notify | connector | compensation }
  aggregate_key: { type: bytes, required: true, description: ordering key }
  payload:       { type: bytes, required: true, description: canonical payload, masked per recipient at delivery }
  attempts:      { type: integer, required: true, description: release attempts }
```
- **Invariants:**
  1. A row exists if and only if its commit exists.
  2. Rows of one aggregate key are released in `(cseq, idx)` order.
  3. `effect_id` is `<process_id>/<n>` for a process effect and `became-effective/<branch>/<object_id>/<start>` for a BecameEffective event; acknowledgement sets `released_at` (DEC-054).
- **Satisfies:** FR-TXN-051, FR-TXN-052, STD-TXN-050

### DAT-IdempotencyRecord
- **Purpose:** The recorded outcome of a keyed request.
- **Kind:** System
- **Stored in:** `idempotency_record`
- **Schema:**
```yaml
DAT-IdempotencyRecord:
  scope:        { type: string, required: true, description: API or handler scope }
  key:          { type: string, required: true, description: caller-provided or derived key }
  request_hash: { type: content_id, required: true, description: hash of the canonical request }
  outcome:      { type: bytes, required: true, description: recorded response or error }
  commit_id:    { type: content_id, required: false, description: commit produced, if any }
  expires_at:   { type: instant, required: true, description: retention end }
```
- **Invariants:**
  1. Written in the same transaction as the commit it records.
- **Satisfies:** FR-TXN-031, FR-TXN-032, STD-TXN-040

### DAT-CommitWriteSummary
- **Purpose:** The compact write set of one commit for read-set validation.
- **Kind:** Projection
- **Stored in:** `commit_write_summary`
- **Schema:**
```yaml
DAT-CommitWriteSummary:
  cseq:         { type: integer, required: true, description: commit ordinal }
  object_ids:   { type: list<uuid>, required: true, description: objects written }
  index_ranges: { type: bytes, required: true, description: encoded index key ranges touched }
```
- **Invariants:**
  1. Covers every object and index row changed by the commit.
- **Satisfies:** STD-TXN-022

## 7. Size estimates (per 1,000,000 Definition objects, 10 versions each)

| Table | Rows | Approx. size |
|---|---|---|
| `object_version` | 10,000,000 | 12 GB (1.2 KB compressed envelope) |
| `temporal_index` | 10,000,000 | 1.6 GB plus 1.2 GB indexes |
| `head_index` | 1,000,000 | 120 MB |
| `merkle_node` | ≈ 350,000 live, plus history | 0.4 GB per 100 commits of churn |
| `commit` | 1,000,000 (10 changes each) | 0.6 GB |

These estimates feed NR-COST-001 and the capacity model in UBS-SYS-DVM-07.

## 8. Rebuild procedures

| Table | Rebuilt from | Procedure |
|---|---|---|
| `head_index`, `temporal_index`, `relationship_index`, `unique_key_index`, `field_index` | commits and versions | `rebuild index <name>` (IF-DVM-038) replays the write sets of the commit records in `cseq` order (DEC-053) |
| `merkle_node` | head state per commit | tree rebuild; roots must equal stored roots |
| `derivation_edge` | derivation records in commits | replay commit metadata |
| `balance_delta`, `balance_checkpoint`, `projection_agg` | ledger entries and versions | `rebuild projection <name>` with shadow swap |
| `membership_closure` | group relations | recompute closure |
| `commit_write_summary` | commits | recompute for the retention window |
