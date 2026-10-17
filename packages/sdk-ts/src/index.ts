// @ubos/sdk — the TypeScript SDK of UBOS, version 0.2, for browsers and Node.js (PH-1 step
// 17c, DEC-078; FR-DEV-061; DSN-SDK-001…007, -010, -011, -013, -014, -016, -017;
// IF-SDK-001…006).
//
// Layers (DSN-SDK-001), each usable without the ones above it:
//
// 1. Transport (Link): the UBTP bindings of the Server — HTTP with server-sent events, and
//    WebSocket — over the platform's fetch and WebSocket.
// 2. Session (Session): the handshake with protocol version negotiation, the credentials
//    (password sign-in, API keys, bearer tokens), and reconnection with back-off and
//    re-authentication that resumes subscriptions from their last cursors.
// 3. Operations (Client, Files, Branches, ChangeSets, Inspect): reads and queries with
//    branch and temporal selectors, paging that follows snapshot cursors, mutations with
//    automatic UUIDv7 idempotency keys and the registry's retry classification,
//    subscriptions, resumable uploads and ranged downloads; since step 18c branches, change
//    sets, merges, reverts, bulk processes and introspection (BPA 1.0-rc4).
// 4. Typed API (Typed, Query): what `forge types` generates builds on (step 17d).
//
// Errors are instances of their registered code's class (codes.ts, generated from
// STD-ERR-002); decimals and money are exact strings, and floats are refused at the API
// boundary. The SDK propagates the caller's W3C trace context in the UBTP envelope and
// sends nothing to any telemetry endpoint. No runtime dependencies.

export { connect, Client, Item, MutateResult, Page, Subscription } from "./client.js";
export type { CallOptions, Selectors, Target } from "./client.js";
export { Session, VERSIONS } from "./session.js";
export type { Auth, ConnectOptions, Update } from "./session.js";
export { Link, baseUrl } from "./transport.js";
export type { Binding, Frame } from "./transport.js";
export { Files } from "./files.js";
export { Branches, ChangeSets, Inspect } from "./versioning.js";
export type { ChangeSetAction } from "./versioning.js";
export type { BlobRef, FileLink, UploadData, UploadOptions } from "./files.js";
export { UbosError, TransportError, TRANSPORT } from "./error.js";
export { errorOf, fromBody } from "./errors.js";
export * from "./codes.js";
export { REGISTRY } from "./registry.js";
export type { Area, Code } from "./registry.js";
export { Decimal, Money, rejectFloats } from "./decimal.js";
export { DEFAULT_RETRY, NO_RETRY, backoff, withRetry } from "./retry.js";
export type { RetryPolicy } from "./retry.js";
export { Sha256 } from "./sha256.js";
export { TraceContext, uuid7 } from "./util.js";
export { Query, getTyped, literal } from "./typed.js";
export type { Comparison, DatQuery, Literal, Typed } from "./typed.js";
