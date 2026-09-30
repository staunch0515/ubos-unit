---
id: UBS-STD-08
title: BPA Standard — Content Addressing, Merkle Trees and Proof Formats (PROOF)
status: draft
phase: PH-0
depends_on: [UBS-STD-01, UBS-STD-05]
---

# BPA Standard — Merkle Trees, Signatures and Proof Formats (PROOF) `[Profile: Proof]`

## 1. The state tree

### STD-PROOF-001 — Prolly tree over branch state
- **Clause:** The state of a branch at a commit MUST be committed to by a prolly tree whose leaves are sorted by key. The keys are:
  - Definition and RawDocument objects: `o/<object_id>` → version hash of the head version (hide markers use the value `hidden`);
  - ledger entries: `l/<class-hash>/<ledger-key-hash>/<seq as 20-digit zero-padded decimal>` → entry hash;
  - the parent-line reference for branches with inheritance: `p/parent` → parent commit ID (live mode: the parent head at commit time).

  System-kind objects MUST NOT be included. A key is a node boundary at level ℓ if the first two bytes of `SHA-256("ubos.tree.boundary.v1" 0x00 ℓ key)`, read as a big-endian integer, are less than 1024 (target fan-out 64). Node encoding is canonical JSON `{level, entries: [[key, hash], …]}`, hashed with domain tag `ubos.tree.leaf.v1` (level 0) or `ubos.tree.node.v1` (level > 0). The root is the hash of the single top node, or of the empty node `{level: 0, entries: []}`. Precisely (DEC-032): ℓ is written as its decimal ASCII digits followed by a 0x00 byte, and the key as UTF-8; keys are sorted by their UTF-8 bytes; a boundary key closes its node (it is the node's last entry), and the entries after the last boundary form the level's last node; level ℓ+1 holds `[first key, node hash]` of every node of level ℓ, and levels are built until a level has a single node. `<class-hash>` and `<ledger-key-hash>` are the hexadecimal digests of the content identifiers (`ubos.chunk.v1`) of the class and of the canonical ledger-key map.
- **Grammar / Schema:** `DAT-TreeNode`
- **Conformance vectors:** VER-CONF-2300…2339
- **Satisfies:** FR-PROOF-031, FR-VER-045, FR-SYNC-011
- **Notes:** Boundaries depend only on keys, so equal states give equal trees whatever their history (NR-DET-003).

```yaml
DAT-TreeNode:
  level: { type: integer, required: true, description: "0 for leaves" }
  entries: { type: list, required: true, description: "sorted [key, hash] pairs; for level > 0, hash is a child node hash and key is the child's first key" }
```

### STD-PROOF-002 — History independence
- **Clause:** Two branch states with equal key-value sets MUST have equal roots.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2340…2349
- **Satisfies:** NR-DET-003
- **Notes:** —

### STD-PROOF-003 — Root in commit
- **Clause:** `DAT-Commit.root` MUST equal the root of the branch state after the commit's changes are applied.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2350…2354
- **Satisfies:** FR-PROOF-031
- **Notes:** —

## 2. Signatures

### STD-PROOF-010 — Signature format
- **Clause:** Signatures MUST use Ed25519 (RFC 8032) over the raw bytes of the signed content ID string (UTF-8), and MUST be represented as `DAT-Signature`.
- **Grammar / Schema:** `DAT-Signature`
- **Conformance vectors:** VER-CONF-2360…2374
- **Satisfies:** FR-PROOF-021, FR-PROOF-022, FR-VER-064
- **Notes:** —

```yaml
DAT-Signature:
  role: { type: enum, required: true, description: "author | node | approver | signer | notary | tagger" }
  signer: { type: text, required: true, description: "DID" }
  key_id: { type: text, required: true, description: "verification method id in the DID document" }
  alg: { type: enum, required: true, description: "ed25519" }
  sig: { type: text, required: true, description: "base64url signature" }
```

### STD-PROOF-011 — Verification with key history
- **Clause:** A signature is valid if it verifies under the key named by `key_id`, and that key was valid for the signer at the signed item's time (the commit `tx_time` or the item's timestamp), according to the signer's DID history (FR-IAM-033).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2375…2389
- **Satisfies:** FR-PROOF-023, FR-IAM-033
- **Notes:** —

## 3. Proofs

### STD-PROOF-020 — Inclusion proof
- **Clause:** An inclusion proof MUST contain the key, the value hash and, for each tree level from leaf to root, the canonical node content. A verifier checks the key's presence in the leaf, the hash chain to the root, and the root's presence in a signed commit.
- **Grammar / Schema:** `DAT-InclusionProof`
- **Conformance vectors:** VER-CONF-2390…2409
- **Satisfies:** FR-PROOF-032
- **Notes:** The proof size is O(fan-out × depth).

```yaml
DAT-InclusionProof:
  key: { type: text, required: true, description: "tree key" }
  value: { type: text, required: true, description: "version or entry hash" }
  nodes: { type: list, required: true, description: "canonical nodes from leaf to root" }
  commit: { type: composite, required: true, description: "the commit record with signatures (DAT-Commit)" }
```

### STD-PROOF-021 — Consistency proof
- **Clause:** A consistency proof from commit A to a later commit B on the same branch MUST be the list of commit records along B's first-parent history back to A. A verifier checks the parent links, the signatures and the monotonic `tx_time`. Implementations MAY provide checkpoint commits (signed tenant roots, STD-PROOF-030) to shorten proofs.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2410…2419
- **Satisfies:** FR-PROOF-033
- **Notes:** —

### STD-PROOF-030 — Tenant root
- **Clause:** A tenant root at time t MUST be computed as a prolly tree over the keys `<vae>/<branch>` → head commit ID at t, for all non-archived branches of kinds `main`, `release` and `overlay` (and others if configured), and MUST be signed by the node key.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2420…2429
- **Satisfies:** FR-PROOF-034
- **Notes:** —

## 4. Anchors, evidence packs and seals

### STD-PROOF-040 — Anchor receipt
- **Clause:** An anchor receipt MUST record the tenant root, the anchoring service, the service's response (an RFC 3161 token or equivalent) and the time. It MUST be stored as a ledger entry of class `AnchorReceipt` in the tenant's system VAE.
- **Grammar / Schema:** `DAT-AnchorReceipt`
- **Conformance vectors:** VER-CONF-2430…2439
- **Satisfies:** FR-PROOF-041, FR-PROOF-042
- **Notes:** —

```yaml
DAT-AnchorReceipt:
  tenant_root: { type: text, required: true, description: "content-id" }
  root_signature: { type: composite, required: true, description: "DAT-Signature of the node" }
  service: { type: text, required: true, description: "service identifier (TSA URL or ledger id)" }
  token: { type: text, required: true, description: "base64 receipt (RFC 3161 TimeStampToken or equivalent)" }
  anchored_at: { type: instant, required: true, description: "time asserted by the service" }
```

### STD-PROOF-050 — Evidence pack format
- **Clause:** An evidence pack MUST be a ZIP archive with a `manifest.json` (`DAT-EvidenceManifest`) and directories `objects/`, `entries/`, `commits/`, `proofs/`, `anchors/`, `keys/` and `rules/`. The manifest lists every file with its content ID and is signed by the Notary. All JSON is canonical.
- **Grammar / Schema:** `DAT-EvidenceManifest`
- **Conformance vectors:** VER-CONF-2440…2469
- **Satisfies:** FR-PROOF-051, FR-PROOF-052, FR-PROOF-054, FR-PROOF-063
- **Notes:** —

```yaml
DAT-EvidenceManifest:
  pack_id: { type: text, required: true, description: "UUID" }
  tenant: { type: text, required: true, description: "tenant id" }
  scope: { type: composite, required: true, description: "{query, expansion, asof, time}" }
  evaluated_at: { type: map, required: true, description: "branch → commit id" }
  files: { type: list, required: true, description: "{path, content_id, role}" }
  redactions: { type: list, required: false, description: "{object, path, commitment}" }
  created_at: { type: instant, required: true, description: "time" }
  created_by: { type: text, required: true, description: "principal DID" }
  signatures: { type: list, required: true, description: "DAT-Signature (notary)" }
```

### STD-PROOF-051 — Provable redaction
- **Clause:** A redacted field in a pack MUST be replaced by its field commitment (STD-OBJ-010). For unencrypted fields, the pack MUST include a salted commitment computed at pack creation, together with the version's committed form, so that inclusion still verifies. The salt MUST be withheld.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2470…2479
- **Satisfies:** FR-PROOF-053
- **Notes:** —

### STD-PROOF-055 — Document seal
- **Clause:** A seal MUST be the canonical record `DAT-Seal`, hashed with domain tag `ubos.seal.v1` and signed by each signer (or by the platform on the signer's behalf, recording the authentication evidence).
- **Grammar / Schema:** `DAT-Seal`
- **Conformance vectors:** VER-CONF-2480…2494
- **Satisfies:** FR-SIGN-021, FR-SIGN-022, CR-ESIGN-004
- **Notes:** —

```yaml
DAT-Seal:
  document: { type: text, required: true, description: "canonical URI and version hash of the Live Doc" }
  render_hash: { type: text, required: true, description: "content-id of the rendered output (PDF/A)" }
  data_versions: { type: list, required: true, description: "version hashes of all bound objects" }
  intent: { type: text, required: true, description: "intent statement shown to the signer" }
  consent_ref: { type: text, required: false, description: "E-SIGN consent record id" }
  signers: { type: list, required: true, description: "{DID or external identity, auth evidence ref, signed_at}" }
```

## 5. Disposal and portable archives

### STD-PROOF-060 — Content disposal
- **Clause:** Disposal of a version or entry MUST destroy its body chunks (or the keys that encrypt them) and MUST keep the envelope hash, the entry hash and all tree nodes. Reads of disposed content MUST return `{"disposed": true}` with a disposal-certificate reference. Proofs over roots that include the item MUST continue to verify.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2500…2509
- **Satisfies:** FR-AUD-072, FR-VER-093
- **Notes:** —

### STD-PROOF-070 — Portable archive format
- **Clause:** A tenant or VAE export MUST be a directory or ZIP containing:
  - chunks by content ID, versions, entries, commits, refs and tags;
  - DID documents with key histories;
  - `manifest.json` (listing counts, head roots and format version), signed by the exporting node.

  Importing it into a conforming implementation MUST reproduce identical head roots.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2510…2529
- **Satisfies:** FR-TEN-061, FR-TEN-062, NR-PORT-003
- **Notes:** —
