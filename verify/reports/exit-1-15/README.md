# EXIT-1-15 (NOD part) — SUITE-NOD-PROTO over every binding (PH-1 step 14d)

VER-CONF-7000 (UBTP server conformance; FR-INT-011…-015, DSN-NOD-010…017) run by
`ubos-proto-suite` (DEC-075, part 14d) on the development workstation (Intel Xeon @ 2.80 GHz, 4 vCPUs, 15 GiB, Linux; rustc 1.94.1, debug build). The reference-environment run
(ENV-REF-SMALL) comes with step 19; the SDK part of EXIT-1-15 (SUITE-SDK-PROTO, SUITE-SDK-NET)
follows below (step 17d). Report: `proto-suite.json` (signed by an ephemeral runner key).

| Item | Value |
|---|---|
| Vectors | the 88 Protocol vectors of BPA 1.0-rc3 (`standard/vectors/protocol`, STD-PROTO-001, -002, -010…-013, -020; none superseded) |
| Bindings | in-process (the kernel's `ubtp` operation), HTTP (`POST /ubtp/v1`), WebSocket (`GET /ubtp/v1`, subprotocol `ubtp.v1`) of `ubos-server` in the `dev` preset |
| Result | **88 of 88 on each binding**; identical frames across bindings for 87 vectors, 1 binding-specific (below) |
| Version skew | **8 of 8 on each binding** |
| REST facade | **4 of 4** equivalence cases (STD-PROTO-021) |
| Time | 7.4 s for the whole suite |

## Method

Each vector runs on a fresh engine state (`reset`) of a memory VAE with a fixed clock (the
vectors' `set_now`) and seeded entropy, once per binding. Steps that are not `ubtp`
messages (`commit`, `read`, `query`, …) run in-process on the same VAE; the `ubtp` steps go
over the binding under test. Over a network binding:

- the vector's in-process credentials (or none) are sent as the `dev` kind with the same
  members, which the `dev` preset turns back into exactly those in-process credentials (the
  Server refuses in-process credentials from the network);
- the Server names the session, so its identifier is mapped back to the vector's session key
  before comparison; over WebSocket each vector session is one connection, and a step reads
  as many frames as the in-process binding answered.

Everything else must match: each vector's expectations on every binding, and the frames of
every `ubtp` step byte for byte against the in-process run (FR-INT-015).

**Binding-specific vector.** VER-CONF-5427 ("bearer tokens are verified by the Server, not by
the in-process binding") asserts the in-process binding's own refusal. Over a network binding
the Server verifies the credential first; the suite checks that it refuses the malformed
bearer with `IAM.DENIED` answering the handshake (passes on HTTP and WebSocket). Its frames
differ by design and are listed apart, not counted as identical.

## Version skew (VER-CONF-7000 procedure 2)

The handshake of a client of another protocol, ABI, encoding or envelope version, or a newer
client's unknown members; the same answers on every binding.

| Case | Answer | Bindings |
|---|---|---|
| client N-1: protocol version 0.9 only | error `INT.VERSION_UNSUPPORTED` | pass on all three |
| client offering 0.9 and 1.0 | result | pass on all three |
| client N-1: ABI 0.9 only | error `INT.VERSION_UNSUPPORTED` | pass on all three |
| client N+1: ABI range 0.9 to 2.0 | result | pass on all three |
| client with an encoding the server lacks | error `INT.VERSION_UNSUPPORTED` | pass on all three |
| client N-1: envelope version 0.9 | error `INT.VERSION_UNSUPPORTED` | pass on all three |
| client N+1: unknown message type | error `INT.PROTOCOL_ERROR` | pass on all three |
| client N+1: unknown handshake members are ignored | result | pass on all three |

## REST facade equivalence (STD-PROTO-021)

| Case | Result |
|---|---|
| read: GET /api/v1/{vae}/{Class}/{slug} equals the query message's read | pass |
| query: GET /api/v1/{vae}/{Class}?q= equals the query message's textual query | pass |
| error: a problem document carries the message's error code and the status of STD-ERR-002 | pass |
| mutate: POST /api/v1/{vae}/{Class}/{slug}/{port} equals the mutate message's port call | pass |

## Defect found and fixed

Events subscriptions (STD-PROTO-012) read the commit's effects from the outbox's *pending*
list. A hosted VAE releases and acknowledges its outbox after every operation, so under the
Server an events subscription never emitted (VER-CONF-5507 failed even in-process through the
host). The binding now reads the effects the commit released, acknowledged or not
(`Store::outbox_released`; memory and PostgreSQL).

## Reproduce

```
cargo build --locked -p ubos-node --bin ubos-proto-suite
./target/debug/ubos-proto-suite --report verify/reports/exit-1-15/proto-suite.json
cargo test --locked -p ubos-node    # the same suite as a test body
```

# EXIT-1-15 (SDK part) — SUITE-SDK-PROTO, SUITE-SDK-NET and SUITE-SDK-TYPES (PH-1 step 17d)

The SDK suites (DEC-078, part 17d) run by `verify/sdk/sdk_suite.py` on the development
workstation (same machine as above; debug builds, PostgreSQL 16). Report: `sdk-suite.json`.

| Item | Value |
|---|---|
| SDKs | the Rust SDK `ubos-sdk` 0.2 and the TypeScript SDK `@ubos/sdk` 0.2 (Node.js 22), each through its runner (`crates/ubos-sdk/examples/suite.rs`, `packages/sdk-ts/suite/suite.ts`) |
| Bindings | HTTP with server-sent events and WebSocket (the PH-1 bindings of the SDKs, DEC-078 item 5) |
| Node | `ubos-server` in the `server` preset on a fresh PostgreSQL database, with the classes of `verify/sdk/fixture` and the branch `feature/sdk` holding its logic, behind the suite's TCP fault proxy |
| Result | **23 of 23 cases for each SDK on each binding** (15 SUITE-SDK-PROTO, 8 SUITE-SDK-NET; 92 runs) |
| SUITE-SDK-TYPES | **4 of 4** |
| Time | 21.5 s, 21.9 s, 22.5 s and 22.9 s per SDK and binding; 92.4 s in all |

## Method

The cases are written once (`verify/sdk/cases.json`) and implemented by each SDK's runner,
which reports every case with its checks. The driver bootstraps a node, signs the
administrator in, issues an API key, commits the fixture's port logic on `feature/sdk`, and
puts a TCP proxy between the runners and the node. The runners drive the proxy through its
control endpoint:

| Fault | What the proxy does |
|---|---|
| `drop` | closes every open connection |
| `cut_reply` | forwards the next request, then closes its connection 0.5 s later without forwarding the reply (a reply lost after the commit) |
| `cut_after` | closes the connection that carries the N-th byte sent after arming (an upload cut in the middle) |
| `latency` | delays every chunk in both directions |
| `partition` | holds every connection, new ones included, for the given time, then closes them |
| `restart` | the driver stops the node with SIGTERM and starts it again |

## Cases

| Case | What it checks | Requirements | Rust ws / http, TypeScript ws / http |
|---|---|---|---|
| P01 | handshake: protocol 1.0 negotiated, the principal named | IF-SDK-001, DSN-SDK-014 | pass / pass / pass / pass |
| P02 | credentials: password sign-in, API key, bearer token; a wrong password is a typed IAM error, not retried | DSN-SDK-010, DSN-SDK-007 | pass / pass / pass / pass |
| P03 | raw commit: the process, the commits and the new versions | IF-SDK-003 | pass / pass / pass / pass |
| P04 | reads with branch, commit, asof and time selectors and their evidence; a missing object is a typed error | DSN-SDK-006, IF-SDK-002 | pass / pass / pass / pass |
| P05 | paging: every item of a query in pages within one snapshot, unchanged by a commit between pages | DSN-SDK-005 | pass / pass / pass / pass |
| P06 | history, timeline, diff and explain | IF-SDK-002 | pass / pass / pass / pass |
| P07 | port invocation on a branch: the result and the effect | IF-SDK-003 | pass / pass / pass / pass |
| P08 | idempotency: the same key returns the first result and commits once; a new key runs again | DSN-SDK-002 | pass / pass / pass / pass |
| P09 | retries: a reply lost after the commit is retried with the same key and commits once; a stale base is TXN.HEAD_MOVED, not retried | DSN-SDK-002, DSN-SDK-003 | pass / pass / pass / pass |
| P10 | retry classification: exactly the codes the registry marks retryable (and transport failures) are retried | DSN-SDK-003, DSN-SDK-007 | pass / pass / pass / pass |
| P11 | error mapping: node errors are instances of their code's type with area, status and details | DSN-SDK-007 | pass / pass / pass / pass |
| P12 | subscriptions to an object, a query and events, with cursors and unsubscribe | IF-SDK-005, DSN-SDK-004 | pass / pass / pass / pass |
| P13 | files: an upload in chunks with progress, resumed from its location after an interruption; whole and ranged downloads verified | DSN-SDK-011, IF-SDK-006 | pass / pass / pass / pass |
| P14 | exact decimals and money: a 30-digit decimal round-trips exactly; floats are refused at the API boundary | DSN-SDK-013 | pass / pass / pass / pass |
| P15 | typed API: a polymorphic read and query narrow by class; the builder's DAT-Query matches the text query; a typed port call | DSN-SDK-012, DSN-FRG-102 | pass / pass / pass / pass |
| N01 | connections dropped between requests: the next request reconnects | DSN-SDK-004 | pass / pass / pass / pass |
| N02 | a connection dropped during a read: the read is retried | DSN-SDK-003 | pass / pass / pass / pass |
| N03 | 300 ms latency each way: operations complete | DSN-SDK-004 | pass / pass / pass / pass |
| N04 | a 3 s partition: a request made during it completes after it heals | DSN-SDK-004 | pass / pass / pass / pass |
| N05 | a partition longer than the deadline: a typed, retryable transport error within the deadline | DSN-SDK-003 | pass / pass / pass / pass |
| N06 | a subscription across dropped connections receives every update committed meanwhile | DSN-SDK-004 | pass / pass / pass / pass |
| N07 | a node restart: reconnection, re-authentication and the subscription resumed | DSN-SDK-004, DSN-SDK-010 | pass / pass / pass / pass |
| N08 | an upload interrupted by a dropped connection resumes from the node's offset and completes verified | DSN-SDK-011 | pass / pass / pass / pass |

## SUITE-SDK-TYPES (DSN-SDK-012, DSN-FRG-102)

| Check | Result |
|---|---|
| the committed TypeScript types of the fixture are current (forge types) | pass |
| the committed Rust types of the fixture are current (forge types) | pass |
| a polymorphic query narrows by class in the type checker (DSN-SDK-012) | pass |
| the same program fails to compile once the model removes a field it uses (DSN-FRG-102) | pass |

The narrowing program iterates a query of `Party` and reads `credit_limit` only where
`class === "Customer"` and `iban` only where `class === "Supplier"`; `tsc --strict` accepts
it. Against the types of the fixture without `Customer.email`, the same program fails to
compile, naming `email`.

## Defects found and fixed

- **Idempotency across sessions (STD-TXN-040).** The UBTP binding remembered idempotency
  keys per session. A client whose reply was lost reconnects in a new session and retries
  with the same key, and the mutation ran again: a stale `base` was reported, or a second
  commit was made. The binding now carries the key into the kernel's process, so the
  kernel writes the idempotency record with the commit, and it replays a recorded key of
  the same principal, with the first process and commit, before checking `base` (P08,
  P09).
- **The `commit` coordinate of a read.** A read ignored the message's `commit`. It now
  completes the URI's selectors, and the default branch is left to the commit (STD-ADDR-002;
  P04).
- **`timeline` without `from`.** Its default `0000-01-01` is not a valid date, so the
  request failed; the default is now `0001-01-01` (P06).
- **SDK reconnection.**
  - A stream that ended by itself left a subscriber that made no request stalled. This was
    the WebSocket, or the server-sent events of HTTP, after a partition. Both SDKs now
    reopen the session and resume the subscriptions from their cursors (N06).
  - A password client signed in again at every reconnection, which used up the concurrent
    sessions of the account. It now keeps its token until the node refuses it.
- **Frame order on a WebSocket.** The once-a-second poll can drain a subscription's emits
  from the binding just before a request on the same connection is handled. The request's
  reply was then written first and the older emits after it, which broke the order of
  STD-PROTO-012. This showed in CI as VER-CONF-5504 failing over WebSocket. The session now
  sends the emits already queued before a reply.
- **Tours of 17b and 17c.** They wrote updates as `set` instead of `body`. The kernel
  ignores `set`, so those renames changed nothing; they now write `body`.

## Reproduce

```
cargo build --locked -p ubos-node -p ubos-forge -p ubos-sdk --bin ubos-server --bin forge --example suite
(cd packages/sdk-ts && npm ci && npm run examples)
python3 verify/sdk/sdk_suite.py --server target/debug/ubos-server --forge target/debug/forge \
    --rust target/debug/examples/suite --ts packages/sdk-ts/build/suite/suite.js \
    --database "host=127.0.0.1 user=postgres dbname=bpa" --report verify/reports/exit-1-15/sdk-suite.json
```
