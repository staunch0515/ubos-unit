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
  | Params | `query: query-object`, `group_by: list?`, `aggregates: list`, `convert: {currency, rates}?` |
  | Result | `{groups, evaluated_at}` |
  | Errors | as for `search.query`, plus `MODEL.CURRENCY_MISMATCH` |
  | Determinism | `snapshot` |
  | Cost | 20 + 1 per row examined |
  | Authorization | `read` per row (only visible rows count) |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3640…3659
- **Satisfies:** FR-QRY-041, FR-QRY-042
- **Notes:** Since 1.0-rc8 (DEC-095):
  - **Parameters.** They are declared in 1.0-rc8; the descriptor vector of 1.0-rc3 is superseded (STD-CONF-030).
  - **Aggregates.** `aggregates` are `{fn, field, as, p}`; `count` without `field` counts rows. `group_by` names fields of the queried class. Rows are all rows of the query, subclasses included, that the principal may read (STD-LGS-051).
  - **Money.** Without `convert`, money is grouped by currency as well (`key.currency`). `convert: {currency, rates}` converts each amount by its rate, rounded half-even to the target's minor units; a currency without a rate fails with `MODEL.CURRENCY_MISMATCH`.
  - **Results.**
    - `sum` keeps the largest input scale.
    - `avg` adds four digits to it (money: minor units), rounded half-even.
    - `percentile` takes the nearest rank `ceil(p / 100 · n)`.
    - `min` and `max` compare numbers numerically and other values by canonical JSON.
  - **Groups.** They are `{key, rows, values}` in the order of the canonical JSON of `key`.
  - **Errors.** Fields unknown, masked for the principal (STD-LGS-052), or not numeric for `sum` and `avg` fail with `QRY.INVALID_QUERY`. The principal's clearances come from the adapter's directory (`principal`), default `internal`.

### STD-ISA-082 — `search.traverse`
- **Clause:** Traverses declared relationships from a start object with a direction, a depth limit (maximum 8) and class filters, authorising each step.

  | Aspect | Value |
  |---|---|
  | Params | `start: uri`, `fields: list?`, `direction: out \| in \| both?`, `depth: integer?`, `classes: list?`, `limit: integer?` |
  | Result | `{nodes, truncated, evaluated_at}` |
  | Errors | `MODEL.INVALID_VALUE`, `IAM.NOT_FOUND` |
  | Determinism | `snapshot` |
  | Cost | 10 + 2 per node |
  | Authorization | `read` per node |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3660…3669
- **Satisfies:** FR-MODEL-065
- **Notes:** Since 1.0-rc8 (DEC-095):
  - **Walk.** The walk is breadth-first from `start` over live references; `fields` restricts the reference fields followed. Within a level, sources are taken in object order and their references in `(field, target)` order; each object is visited once.
  - **Nodes.** A node is `{object_id, uri, class, depth, via: {from, field, direction}}`. `classes` filters the nodes returned, not the walk. `limit` (default 100) sets `truncated`.
  - **Authorization.** Nodes the principal may not read (STD-LGS-051) are neither returned nor expanded, and a start it may not read is not found.

### STD-ISA-083 — `search.text` `[since ABI 1.0, available from PH-2]`
- **Clause:** Performs ranked full-text search over declared text-indexed fields. Results carry the index watermark. When the index lags the snapshot, results MAY omit the newest changes, and the response MUST say so.

  | Aspect | Value |
  |---|---|
  | Params | `text: text`, `classes: list?`, `limit: integer?` |
  | Result | `{items, watermark, lagging}` |
  | Errors | `MODEL.INVALID_VALUE` |
  | Determinism | `journaled` (index state is outside the snapshot, so the result is journaled) |
  | Cost | 30 + 1 per result |
  | Authorization | `read` per result, with snippet masking |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-3670…3679
- **Satisfies:** FR-QRY-021, FR-QRY-022, FR-QRY-023
- **Notes:** Journaling keeps replays deterministic even though indexes are asynchronous.

  Since 1.0-rc8 (DEC-095):
  - **Index.** Fields declared `text_index: true` are indexed. Analysis applies NFKC, lower case and word splitting, and strips plural endings (`ies` → `y`; `es` after s, x, z, ch, sh; a final `s` except `ss`).
  - **Ranking.** Results are ranked by the number of distinct query words matched, then by their total frequency, then by object ID. Each item is `{object_id, class, score, field, snippet}`; the snippet is the first matching field with matched words in `<em>`, cut to 160 characters around the first match.
  - **Visibility.** Fields masked for the principal are not searched, and objects it may not read are not found.
  - **Watermark.** `watermark` is the cseq the index reflects; `lagging` is true when it is behind the head. The adapter operation `index_lag {branch, cseq}` freezes the index of the reference model.

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

### STD-ISA-101 — `ask.input`
- **Clause:** Requests input from the caller. When the invocation carries no answer for the request, the process MUST end with the outcome `needs_input`, committing nothing and releasing no effect, and its result MUST carry the request: its ID, fields, message and expiry. When the caller repeats the invocation with `answers` for the request ID before the expiry, the instruction MUST return the answers, validated against the fields, and the process continues. Answers after the expiry MUST fail with `FLOW.INPUT_EXPIRED`.

  | Aspect | Value |
  |---|---|
  | Params | `request: text`, `fields: list`, `message: text`, `timeout: duration?` |
  | Result | `map`: the answers by field name |
  | Errors | `FLOW.INPUT_EXPIRED`, `IAM.NOT_FOUND`, `MODEL.INVALID_VALUE` |
  | Determinism | `journaled` |
  | Cost | 10 |
  | Authorization | none |
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-2880…2899
- **Satisfies:** FR-FLOW-071
- **Notes:** Since 1.0-rc12 (DEC-119):
  - **Fields.** Each field is `{name, type, required, description}` with a value type of STD-FND-020. A field list that is not of this form fails with `MODEL.INVALID_VALUE`.
  - **Request ID.** The ID is the content ID (`ubos.chunk.v1`) of `{invocation, request}`. The invocation is the `execute` input without `process`, `answers` and `max_attempts`, so a repeated call finds the same request.
  - **Result of the process.** `execute` returns `{outcome: "needs_input", needs_input: {request_id, request, fields, message, expires_at}}` without `cseq`, and the process record ends `aborted`.
    - `expires_at` is the time of the first request plus `timeout`, by default `PT15M`.
    - Repeating the call without answers returns the same request and expiry.
    - After the expiry, a call without answers starts the request again with a new expiry.
  - **Answers.** `execute` takes `answers`, a map from request ID to `{field: value}`.
    - A required field that is missing, a value of another type or an unknown field fails with `MODEL.INVALID_VALUE`.
    - Answers for a request that was never made fail with `IAM.NOT_FOUND`.
    - When the process commits, its answered requests are consumed, and answering them again fails with `IAM.NOT_FOUND`.
  - **Several requests.** A process may ask several requests, each by its own name. The first request without an answer ends the process.
  - **Replay.** The answers are journaled as the instruction's result (STD-CTX-012).
  - **Agents.** The request is machine-readable, for agents to answer or pass to people (FR-FLOW-072, PH-3).

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
