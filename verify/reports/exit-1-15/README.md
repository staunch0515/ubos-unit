# EXIT-1-15 (NOD part) — SUITE-NOD-PROTO over every binding (PH-1 step 14d)

VER-CONF-7000 (UBTP server conformance; FR-INT-011…-015, DSN-NOD-010…017) run by
`ubos-proto-suite` (DEC-075, part 14d) on the development workstation (Intel Xeon @ 2.80 GHz, 4 vCPUs, 15 GiB, Linux; rustc 1.94.1, debug build). The reference-environment run
(ENV-REF-SMALL) and the SDK part of EXIT-1-15 (SUITE-SDK-PROTO, SUITE-SDK-NET) come with
step 17. Report: `proto-suite.json` (signed by an ephemeral runner key).

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
