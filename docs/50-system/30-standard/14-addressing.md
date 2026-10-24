---
id: UBS-STD-14
title: BPA Standard — Addressing, URIs, VFS Aliases and DIDs (ADDR)
status: draft
phase: PH-0
depends_on: [UBS-STD-01]
---

# BPA Standard — Addressing (ADDR)

### STD-ADDR-001 — Canonical URI
- **Clause:** Objects MUST be addressable by URIs of the form below. Stored references MUST use the canonical form: authority present, no selectors, the class name as it appears in the object's class, and the current slug. Readers MUST follow slug redirects (STD-OBJ-004).
- **Grammar / Schema:**
  ```
  ubos-uri    = "ubos://" authority "/" class-name "/" slug [ "?" selectors ] [ "@v" 1*DIGIT ]
  authority   = tenant [ "." vae ]
  tenant      = slug
  vae         = slug
  selectors   = selector *( "&" selector )
  selector    = ( "branch=" branch-name ) / ( "commit=" content-id ) / ( "tag=" slug )
              / ( "time=" instant ) / ( "asof=" ( date / instant ) ) / ( "key=" slug )
              / ( "view=" mode )
  ```

  In detail (DEC-043): a malformed URI is rejected with `MODEL.INVALID_VALUE`. `@vN` counts the distinct versions of the object along the first-parent history of the resolved snapshot. Slug redirects follow the slugs of the object's earlier versions; the object that holds a slug now takes precedence. The `view` selector does not change a read; `key` is not supported in BPA 1.0-rc1 (`QRY.INVALID_QUERY`). An authority other than the reader's tenant is not found (`IAM.NOT_FOUND`).
- **Conformance vectors:** VER-CONF-5200…5239
- **Satisfies:** FR-VER-104, FR-TEN-013, FR-MODEL-064
- **Notes:** `@vN` addresses the N-th version on the resolved branch line (1-based). It is a convenience for humans and is never stored.

### STD-ADDR-002 — Selector precedence
- **Clause:** At most one of `commit`, `tag` and `time` MAY be given (`ADDR.CONFLICTING_SELECTORS` otherwise). `branch` defaults to `main`. `asof` combines with any of them. `commit` and `tag` imply their branch. A `branch` selector that disagrees with them MUST be rejected. A selector given twice also conflicts. The branch of a commit is the branch named in its commit record (DEC-043).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5240…5254
- **Satisfies:** FR-VER-101
- **Notes:** —

### STD-ADDR-003 — Short forms
- **Clause:** Implementations MAY accept short forms as input only: `Class/slug` relative to the context VAE, and `/data/…` VFS paths. They MUST be normalised to canonical URIs before storage or comparison.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5255…5264
- **Satisfies:** FR-FILE-041
- **Notes:** Since 1.0-rc8 (DEC-094):
  - **Where.** `resolve` and `resolve_path {branch, path}` accept `Class/slug` (with URI selectors) and VFS paths, and `resolve_path` returns `{uri, object_id}`.
  - **Changes.** A reference value of a change stays canonical-only (`MODEL.INVALID_VALUE`, released behaviour).
  - **Errors.** Text that is no short form fails with `MODEL.INVALID_VALUE`, and a form that names no object with `IAM.NOT_FOUND`.

### STD-ADDR-004 — VFS alias mapping
- **Clause:** VFS paths MUST map to canonical URIs as follows:

  | Path prefix | Maps to |
  |---|---|
  | `/data/<namespace>/<Class>/<slug>` | a DataObject instance |
  | `/usr/logic/<namespace>/<name>` | a LogicAsset |
  | `/etc/<namespace>/<name>` | a ConfigAsset instance (sheets, policies, configuration) |
  | `/view/<namespace>/<name>` | a ViewAsset |
  | `/lib/<namespace>/<name>` | a shared library LogicAsset or a Buk-provided reusable object |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-5265…5279
- **Satisfies:** FR-FILE-041, EXT-TRI
- **Notes:** Since 1.0-rc8 (DEC-094):
  - **Data paths.** A `/data` path needs a class of that namespace below `DataObject`.
  - **Asset paths.** The other roots name the live object below `LogicAsset` (`/usr/logic`, `/lib`), `ConfigAsset` (`/etc`) or `ViewAsset` (`/view`) whose `name` is `<namespace>.<name>`, the first in object order.
  - **Errors.** Other roots and shapes fail with `MODEL.INVALID_VALUE`, and paths that name nothing with `IAM.NOT_FOUND`.

### STD-ADDR-010 — The `did:ubos` method
- **Clause:** Principal identifiers MUST use the DID method `ubos` in one of two forms:
  - **key form** `did:ubos:k:<multibase-base58btc-ed25519-public-key>`, whose DID document is derived from the key (for devices and federation bootstrap);
  - **registered form** `did:ubos:<authority>:<uuid>`, whose DID document (verification methods with validity periods, service endpoints) is served by the owning node and, in federation, by the registry.

  Key rotation MUST append verification methods with validity periods and MUST NOT delete history. The key form encodes the multicodec prefix `0xed 0x01` followed by the 32-byte key; its derived document has one key with ID `<did>#<multibase key>`. Malformed identifiers are rejected with `MODEL.INVALID_VALUE` (DEC-043).
- **Grammar / Schema:**
  ```
  ubos-did   = "did:ubos:" ( key-form / reg-form )
  key-form   = "k:" "z" 1*BASE58
  reg-form   = authority ":" uuid
  ```
- **Conformance vectors:** VER-CONF-5280…5309
- **Satisfies:** FR-IAM-032, FR-IAM-033, FR-SYNC-051
- **Notes:** EXT-RFC used `did:ubos:<ed25519-public-key>`. That corresponds to the key form.
