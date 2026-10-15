---
id: UBS-STD-10-03
title: BPA Standard — Instructions search.*, call.* and ask.*
status: draft
phase: PH-0
depends_on: [UBS-STD-10]
---

# Instructions `search.*`, `call.*` and `ask.*`

## 1. `search.*`

### STD-ISA-080 — `search.query`
- **Clause:** Evaluates a criteria query (STD-EXPR query form) at the process snapshot or at explicit coordinates, with authorization inside the evaluation (FR-QRY-012).

  | Aspect | Value |
  |---|---|
  | Params | `query: query-object`, `coordinates: {branch?, commit?, tag?, time?, asof?}?`, `limit`, `cursor` |
  | Result | `{items, cursor, evaluated_at}` |
  | Errors | `QRY.SCAN_LIMIT`, `QRY.INVALID_QUERY`, `IAM.DENIED` |
  | Determinism | `snapshot` |
  | Cost | 20 + 1 per row examined |
  | Authorization | `read` per row, with masking |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3600…3639
- **Satisfies:** FR-QRY-011, FR-QRY-013, FR-QRY-015, FR-QRY-071
- **Notes:** Since 1.0-rc2 (DEC-067): `query` is a `DAT-Query` or its textual form (STD-EXPR-010); `limit` and `cursor` are its members; `items` are the object identifiers in order; explicit `coordinates` follow in PH-2.

### STD-ISA-081 — `search.aggregate`
- **Clause:** Evaluates grouped aggregates (`count`, `sum`, `avg`, `min`, `max`, `percentile`) with currency-safe money semantics (STD-FND-023).

  | Aspect | Value |
  |---|---|
  | Result | `{groups, evaluated_at}` |
  | Errors | as for `search.query`, plus `MODEL.CURRENCY_MISMATCH` |
  | Determinism | `snapshot` |
  | Cost | 20 + 1 per row examined |
  | Authorization | `read` per row (only visible rows count) |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3640…3659
- **Satisfies:** FR-QRY-041, FR-QRY-042
- **Notes:** —

### STD-ISA-082 — `search.traverse`
- **Clause:** Traverses declared relationships from a start object with a direction, a depth limit (maximum 8) and class filters, authorising each step.

  | Aspect | Value |
  |---|---|
  | Determinism | `snapshot` |
  | Cost | 10 + 2 per node |
  | Authorization | `read` per node |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3660…3669
- **Satisfies:** FR-MODEL-065
- **Notes:** —

### STD-ISA-083 — `search.text` `[since ABI 1.0, available from PH-2]`
- **Clause:** Performs ranked full-text search over declared text-indexed fields. Results carry the index watermark. When the index lags the snapshot, results MAY omit the newest changes, and the response MUST say so.

  | Aspect | Value |
  |---|---|
  | Determinism | `journaled` (index state is outside the snapshot, so the result is journaled) |
  | Cost | 30 + 1 per result |
  | Authorization | `read` per result, with snippet masking |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3670…3679
- **Satisfies:** FR-QRY-021, FR-QRY-022, FR-QRY-023
- **Notes:** Journaling keeps replays deterministic even though indexes are asynchronous.

### STD-ISA-084 — `search.vector` `[since ABI 1.0, available from PH-3]`
- **Clause:** Performs similarity search (optionally hybrid with text and criteria) over declared vector indexes. The embedding model version is returned. The result is journaled.

  | Aspect | Value |
  |---|---|
  | Determinism | `journaled` |
  | Cost | 50 + 1 per result |
  | Authorization | `read` per result |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3680…3689
- **Satisfies:** FR-QRY-031, FR-QRY-032
- **Notes:** —

## 2. `call.*`

### STD-ISA-090 — `call.connector`
- **Clause:** Invokes a declared operation of a connector object. The host resolves credentials (CTR-002) and enforces the egress allow-list, classification rules, rate limits and circuit breakers. The request and response are journaled, with the connector's redaction rules applied to the stored copy. By default the call is made after commit through the outbox (`when: after_commit`). `when: now` makes a pre-commit call and requires a declared compensation, or `compensation: none` accepted by policy.

  | Aspect | Value |
  |---|---|
  | Params | `connector: uri`, `operation: text`, `request: map`, `when: now \| after_commit`, `idempotency_key: text?` |
  | Result | `now`: the response (validated against the operation schema); `after_commit`: `{effect_id}` |
  | Errors | `LOGIC.CONNECTOR_UNAVAILABLE`, `LOGIC.CLASSIFICATION_EGRESS_DENIED`, `LOGIC.EGRESS_NOT_ALLOWED`, `LOGIC.CONNECTOR_ERROR` (with the external status), `IAM.DENIED` |
  | Determinism | `journaled` (`now`) or `deferred` (`after_commit`) |
  | Cost | 100 |
  | Authorization | `invoke` on the connector operation |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3700…3739
- **Satisfies:** FR-LOGIC-101 … FR-LOGIC-105, FR-TXN-053
- **Notes:** In replay, `now` calls return the journaled response and make no network call (FR-LOGIC-103).

## 3. `ask.*`

### STD-ISA-100 — `ask.model` `[available from PH-3]`
- **Clause:** Calls the model gateway (CTR-020) on a named route with a prompt template, bound variables and an output schema. The output MUST be validated against the schema before it is returned. The call is journaled with the model and template versions.

  | Aspect | Value |
  |---|---|
  | Params | `route: text`, `template: uri`, `vars: map`, `output_schema: schema`, `max_tokens: integer?` |
  | Result | a value conforming to `output_schema`, plus `{model, template_version, usage}` |
  | Errors | `AI.OUTPUT_INVALID`, `AI.CLASSIFICATION_BLOCKED`, `AI.BUDGET_EXHAUSTED`, `AI.PROVIDER_UNAVAILABLE`, `LOGIC.LIMIT_EXCEEDED` |
  | Determinism | `journaled` |
  | Cost | 200 + 1 per 100 tokens |
  | Authorization | `ask` on the route; classification clearance of the route covers the variables |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3800…3829
- **Satisfies:** FR-AI-091, FR-AI-092, FR-AI-093, FR-AI-013
- **Notes:** Conformance uses a deterministic stub model adapter.
