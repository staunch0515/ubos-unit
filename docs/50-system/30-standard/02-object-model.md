---
id: UBS-STD-02
title: BPA Standard — Object Model (OBJ)
status: draft
phase: PH-0
depends_on: [UBS-STD-01]
---

# BPA Standard — Object Model (OBJ)

## 1. Objects and versions

### STD-OBJ-001 — Object
- **Clause:** An object is identified by its `object_id` (STD-FND-015) and belongs to exactly one tenant and one VAE. It has an unbounded sequence of immutable versions per branch. For objects of Ledger kind, "version" means "entry" (STD-KIND-020).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0300…0304
- **Satisfies:** FR-MODEL-014, FR-TEN-011
- **Notes:** An object has no mutable state. "Current state" is the version selected by a snapshot.

### STD-OBJ-002 — Object version envelope
- **Clause:** Every stored object version MUST consist of an envelope (`DAT-ObjectVersion`) and a body. The version hash MUST be computed over the canonical envelope with `body_ref` set to the body's content identifier (domain tag `ubos.version.v1`). The body's content identifier is the chunk identifier of its canonical bytes (domain tag `ubos.chunk.v1`) when the body is a single chunk (STD-OBJ-008); ledger entry bodies use the same rule (STD-KIND-020). Envelope members listed as required MUST be present.
- **Grammar / Schema:** `DAT-ObjectVersion`
- **Conformance vectors:** VER-CONF-0305…0329
- **Satisfies:** FR-VER-011, FR-PROOF-012, FR-TIME-021
- **Notes:** Transaction time is not in the envelope. It belongs to the commit, so identical content committed twice deduplicates to the same version hash.

```yaml
DAT-ObjectVersion:
  object_id: { type: text, required: true, description: "UUID of the object" }
  class: { type: text, required: true, description: "canonical URI of the class object (no selectors)" }
  class_version: { type: text, required: true, description: "content-id of the class version used" }
  schema_version: { type: integer, required: true, description: "schema version of the class at write time" }
  kind: { type: enum, required: true, description: "definition | system | raw_document (ledger entries use DAT-LedgerEntry)" }
  slug: { type: text, required: true, description: "locator slug at this version" }
  valid: { type: period, required: true, description: "valid-time period; Definition kind only, else {from: null, to: null}" }
  state: { type: text, required: false, description: "lifecycle state, when the class has a lifecycle" }
  body_ref: { type: text, required: true, description: "content-id of the body (canonical JSON of own field values)" }
  encryption: { type: map, required: false, description: "per-field encryption descriptors (STD-OBJ-010)" }
  predecessor: { type: text, required: false, description: "content-id of the previous version on the same branch line, absent for creation" }
  tombstone: { type: boolean, required: false, description: "true when this version ends the object (ended, or hidden on a child branch)" }
  provenance_ref: { type: text, required: false, description: "content-id of the derivation record for logic-produced versions (STD-TXN-030)" }
```

### STD-OBJ-003 — Version body holds own values only
- **Clause:** The body MUST contain only the values set at this object (for fields of the class and its extensions). Inherited defaults and computed fields MUST NOT be stored unless the class declares them `materialized: true`, in which case they MUST be recomputable and verified on write.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0330…0339
- **Satisfies:** FR-MODEL-025, FR-MODEL-044
- **Notes:** Computed values are derived. Materialisation is a performance option with a correctness check.

### STD-OBJ-004 — Locator and slug changes
- **Clause:** The locator of an object is `(vae, class, slug)`. A slug change MUST produce a new version with the new slug. The platform MUST keep a redirect from the old locator to the `object_id` for at least the retention period of the object's versions.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0340…0344
- **Satisfies:** FR-MODEL-014
- **Notes:** —

### STD-OBJ-005 — Ending an object
- **Clause:** Objects of Definition kind are never deleted. They are *ended* by a version with `tombstone: true` and a valid-time start. Ended objects MUST remain readable at earlier `asof` and `time` coordinates. Referential rules (`restrict`, `nullify`, `cascade_end`) MUST be applied when an object is ended.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0345…0359
- **Satisfies:** FR-MODEL-045, FR-VER-012
- **Notes:** Physical disposal is a retention function (STD-PROOF-060), not a version operation.

## 2. Bodies, chunks and blobs

### STD-OBJ-008 — Chunking
- **Clause:** Bodies and blobs larger than 64 KiB MUST be split into content-defined chunks with a target size of 64 KiB (minimum 16 KiB, maximum 256 KiB), using the chunking function specified in `DAT-ChunkingParams`. The body or blob is then represented by a manifest of chunk IDs. Smaller content is a single chunk.
- **Grammar / Schema:** `DAT-ChunkingParams`
- **Conformance vectors:** VER-CONF-0360…0374
- **Satisfies:** FR-PROOF-013, NR-COST-001
- **Notes:** A standardised chunking function makes deduplication and sync interoperable between implementations.

```yaml
DAT-ChunkingParams:
  algorithm: { type: enum, required: true, description: "fastcdc-v2020" }
  min_size: { type: integer, required: true, description: "16384" }
  avg_size: { type: integer, required: true, description: "65536" }
  max_size: { type: integer, required: true, description: "262144" }
  manifest: { type: composite, required: true, description: "{chunks: [content-id], total_size: integer, media_type: text}" }
```

### STD-OBJ-009 — Size limits
- **Clause:** Implementations MUST support bodies up to 16 MiB and blobs up to 5 GiB. They MAY support more. A body larger than the configured limit MUST be rejected with `MODEL.OBJECT_TOO_LARGE`. The size of a body is the length in bytes of its canonical JSON (STD-FND-001) after validation, measured on the complete body that a create or an update produces.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0375…0379
- **Satisfies:** FR-FILE-012
- **Notes:** —

## 3. Field encryption

### STD-OBJ-010 — Encrypted fields and field commitments
- **Clause:** A field whose effective classification is `personal` or `sensitive_personal` MUST be stored encrypted with AEAD (AES-256-GCM or XChaCha20-Poly1305) under the subject key named by the class's `subject_key` declaration. The body MUST hold, in place of the plaintext:
  - the ciphertext;
  - a key reference `(subject_id, key_version)`;
  - a field commitment `H(ubos.field-commit.v1 || salt || canonical(plaintext))` with a random 16-byte salt stored inside the ciphertext.

  Hashes and trees cover this stored form only (AR-020).
- **Grammar / Schema:** `DAT-EncryptedField`
- **Conformance vectors:** VER-CONF-0380…0399
- **Satisfies:** FR-PROOF-071, FR-PROOF-072, FR-PROOF-053
- **Notes:** The commitment lets evidence packs prove a disclosed value without revealing undisclosed ones. Destroying the key leaves every hash valid.

```yaml
DAT-EncryptedField:
  enc: { type: enum, required: true, description: "aes-256-gcm | xchacha20-poly1305" }
  key: { type: composite, required: true, description: "{subject_id: text, key_version: integer}" }
  nonce: { type: text, required: true, description: "base64url nonce" }
  ct: { type: text, required: true, description: "base64url ciphertext of canonical({salt, value})" }
  commit: { type: text, required: true, description: "content-id of the field commitment" }
```

### STD-OBJ-011 — Reading erased fields
- **Clause:** When the key of an encrypted field has been destroyed, reads MUST return the marker `{"erased": true, "commit": <commitment>}` for that field. Operations that require the value MUST fail with `PROOF.VALUE_ERASED`.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0400…0404
- **Satisfies:** FR-PROOF-073, SCN-016
- **Notes:** —

### STD-OBJ-012 — Equality lookups on encrypted fields
- **Clause:** A field declared `lookup: equality` MAY additionally store a keyed blind index `HMAC(tenant_lookup_key, normalised value)` for uniqueness and equality queries. Blind indexes MUST use a per-tenant key and MUST be destroyed together with the subject key when the field is erased. The index MUST be `hmac-sha256:` followed by the lowercase hexadecimal HMAC-SHA-256, keyed with the 32-byte tenant lookup key, over the UTF-8 canonical JSON (STD-FND-001) of the value after validation and normalisation (STD-FND-020…027).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-0405…0409
- **Satisfies:** AR-021, FR-MODEL-013
- **Notes:** This replaces deterministic encryption for most lookups.
