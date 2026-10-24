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
- **Grammar / Schema:** `DAT-Connector`
- **Conformance vectors:** VER-CONF-3700…3739
- **Satisfies:** FR-LOGIC-101 … FR-LOGIC-105, FR-TXN-053
- **Notes:** In replay, `now` calls return the journaled response and make no network call (FR-LOGIC-103).

  Since 1.0-rc8 (DEC-094):
  - **Connectors.** A connector is an object of the genesis class `Connector` (`DAT-Connector`). Its endpoint host must be on the VAE's egress allow-list when it is committed and when it is called (`LOGIC.EGRESS_NOT_ALLOWED`). Its credential is a handle `secret://…`, never the secret (`MODEL.INVALID_VALUE`). An operation marked `now` declares a `compensation` operation, or `none` with `compensation_reason` (`MODEL.DECLARATION_INCOMPLETE`).
  - **Checks of a call.** In order:
    - the asset names the connector among its capabilities as `connector:<uri>` (`LOGIC.FORBIDDEN_CAPABILITY`, FR-LOGIC-102);
    - the principal holds `invoke` on `Connector`;
    - the operation exists and the request conforms to its schema (`MODEL.INVALID_VALUE`);
    - request fields whose classification the connector does not allow are refused (`LOGIC.CLASSIFICATION_EGRESS_DENIED`, `details.fields`);
    - the host is allowed;
    - a `now` call needs an operation marked `now` (`LOGIC.FORBIDDEN_CAPABILITY`).
  - **Resilience.**
    - An open breaker fails fast with `LOGIC.CONNECTOR_UNAVAILABLE` (`details.breaker`), and so does a call beyond the rate limit per window (`details.rate_limited`).
    - An unavailable endpoint is retried up to `retry.max_attempts`.
    - An error status fails with `LOGIC.CONNECTOR_ERROR` and `details.status`, and so does a response outside the operation schema (`details.invalid_response`).
    - `breaker.failures` consecutive failures open the breaker for `breaker.open_for`.
  - **Records.** Each exchange is recorded with the declared `redact` fields replaced by `"[redacted]"`. `now` calls count toward the profile's `connector_calls` and its metering.
  - **After commit.** An `after_commit` call releases an effect `connector.call` with the operation, the request and the idempotency key.
  - **Conformance.** Adapters simulate the outside world by `connector_stub`, which scripts responses `{status, body}` or `{unavailable: true}`, and set the allow-list by `egress_allow`; `exchanges` returns the records.

```yaml
DAT-Connector:
  name: { type: text, required: true, description: "name" }
  endpoint: { type: composite, required: true, description: "{protocol: https | sftp | smtp | amqp | kafka, host, path}" }
  auth: { type: composite, required: false, description: "{method, credential}; credential is a handle secret://..." }
  operations: { type: composite, required: true, description: "name -> {request, response, now, compensation, compensation_reason, redact}" }
  timeout_ms: { type: integer, required: false, description: "timeout of one attempt" }
  rate_limit: { type: composite, required: false, description: "{per: second | minute | hour | day, count}" }
  retry: { type: composite, required: false, description: "{max_attempts}" }
  breaker: { type: composite, required: false, description: "{failures, open_for}" }
  allowed_classifications: { type: list, required: false, description: "levels that may leave (default public, internal)" }
```

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
