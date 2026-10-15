# BPA 1.0.0-rc3 — Release notes

Release candidate 3 of BPA 1.0 (PH-1 step 12, DEC-071, DEC-072). Tag `bpa-v1.0.0-rc3`,
signed by the owner (DSN-BPA-001). The manifest (`release/1.0-rc3/manifest.json`) lists
every released file with its SHA-256; its `content_id` identifies this release. BPA 1.0-rc2
stays frozen as tagged (DEC-069). The candidate was opened because the lifecycle of a class,
named by `lifecycle` since BPA 0.1, had no declaration in the standard and
`commit.transition` (STD-ISA-017) had no vectors (DEC-069 item 3, DEC-071).

## Content

| Item | Value |
|---|---|
| Profiles | Core, Bitemporal, Ledger, Proof, Governance, Protocol |
| Clauses | 199 (scope by rule, `scope.json`): the 197 clauses of BPA 1.0-rc2 plus STD-CLS-004 (lifecycle declaration) and STD-ISA-036 (`read.actions`) |
| Clarified since 1.0-rc2 (`Since 1.0-rc3` notes) | STD-ISA-017 (`commit.transition`), STD-ISA-050 (`run.port` availability and guards), STD-TIME-040 (guards and lifecycle versions in the rule binding), STD-BPU-011 (lifecycle findings), STD-CONF-005 (operation `actions`), STD-CONF-030 (supersession of a catalogue vector); error codes `FLOW.PORT_UNAVAILABLE`, `FLOW.EFFECT_FAILED`, `FLOW.FIELD_RULE_VIOLATED` |
| MUST clauses with a vector range | 166, all covered (100%, `coverage.md`) |
| Vectors | 1,642 (Core 1,160, Bitemporal 61, Ledger 55, Proof 73, Governance 205, Protocol 88); 453 tagged `prototype`; 57 added since 1.0-rc2 (56 of lifecycles, VER-CONF-0580…0599, 2664…2665, 3230…3244, 3380…3389, 3404…3412, and the catalogue vector VER-CONF-3002); the 1,585 vectors of 1.0-rc2 that are not superseded keep their identifiers and content |
| Superseded vectors | 3 (`vectors/superseded.txt`, STD-CONF-030): VER-CONF-3361 and VER-CONF-4036 of 1.0-rc1 (since 1.0-rc2) and VER-CONF-3000 of 1.0-rc1, which asserts the whole instruction catalogue of its release; they stay in the manifests of their releases and are excluded here |
| JSON Schemas | 40, generated from the `DAT-*` blocks (`DAT-Lifecycle` and `DAT-Transition` added) |
| Not in this release | STD-LGS-040 (decision tables) and STD-PROTO-021 (REST facade), whose earliest requirements are in PH-2; the Sync profile and the remaining Core and Proof clauses of PH-2 and later |

## Changes since BPA 1.0-rc2

- Part 12a (DEC-071): the lifecycle declaration `DAT-Lifecycle` with its transitions, guards,
  entry and exit effects and per-state field rules, the genesis class `Lifecycle` named by the
  class member `lifecycle`, the lifecycle state as the `state` member of the version envelope
  set at create and changed only by `commit.transition`, the extension rule for subclass
  lifecycles (STD-CLS-004); `commit.transition` clarified with its vectors (STD-ISA-017);
  `read.actions` and the adapter operation `actions` (STD-ISA-036); port availability by
  state before permission, decisions and port guards (STD-ISA-050); guards and lifecycle
  versions in the rule binding (STD-TIME-040); the lifecycle check of the BPU class
  verification (STD-BPU-011); three `FLOW.*` error codes; the reference model
  `standard/tools/bpa/flow.py`; 57 vectors; VER-CONF-3000 superseded.
- Part 12b (DEC-071): the kernel implements the release (crate `ubos-flow` and
  `ubos-dvm/src/flow.rs`); the kernel's conformance test runs the vectors of these clauses.
- Part 12c (DEC-072): the process state machine of DSN-DVM-501 in the kernel (no change to
  the standard); these notes, the manifest and the tag.

## Known limits

- The vectors added since 1.0-rc1 are verified against the reference model and, for the
  clauses of steps 11 and 12, by the kernel; the vectors of the textual query form
  (STD-EXPR-010, STD-ISA-080) and the Protocol vectors are run by the kernel when PH-1 steps
  13 and 14 implement queries and the UBTP Server (EXIT-1-14, kernel part; VER-CONF-8404 on
  PostgreSQL). The kernel keeps claiming 1.0-rc1 (STD-CONF-004) until step 14.
- The Protocol vectors exercise the in-process binding (STD-PROTO-020); the WebSocket and
  HTTP bindings replay the same message sequences with the Server of step 14 (VER-CONF-7000);
  the REST facade (STD-PROTO-021) is PH-2.
- Uniform and batch invocation (FR-FLOW-023), lifecycle evolution (FR-FLOW-014), actions that
  ask for input and the deferred commit of a `require` decision with its pending-change
  store are PH-2; decision tables (STD-LGS-040), the Sync profile and link objects of
  attributes classes (STD-ISA-015) are not in this release.
- Defects found after the tag are recorded in `errata/` and corrected in the next release
  (DEC-045 item 3).
