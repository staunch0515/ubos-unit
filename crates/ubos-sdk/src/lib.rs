//! `ubos-sdk` — the Rust SDK of UBOS, version 0.2 (PH-1 step 17b, DEC-078; FR-DEV-061;
//! DSN-SDK-001…007, -010, -011, -013, -014, -016; IF-SDK-001…006).
//!
//! Layers (DSN-SDK-001), each usable without the ones above it:
//!
//! 1. **Transport** ([`transport`]): the UBTP bindings of the Server — HTTP with
//!    server-sent events, and WebSocket — over TCP or TLS.
//! 2. **Session** ([`session`]): the handshake with protocol version negotiation, the
//!    credentials (password sign-in, API keys, bearer tokens), and reconnection with
//!    back-off and re-authentication that resumes subscriptions from their last
//!    acknowledged cursors.
//! 3. **Operations** ([`ops`], [`files`], [`versioning`]): reads and queries with branch
//!    and temporal selectors, paging that follows snapshot cursors, mutations with automatic
//!    UUIDv7 idempotency keys and the registry's retry classification, subscriptions,
//!    resumable uploads and ranged downloads; branches, change sets, merges, reverts, bulk
//!    processes and introspection (BPA 1.0-rc4, step 18c).
//! 4. **Typed API** ([`typed`]): what `forge types` generates builds on — typed items,
//!    enums over a class and its subclasses, and query builders compiling to DAT-Query.
//!
//! Errors are typed by the registry of the standard ([`registry`], generated from
//! STD-ERR-002); decimals and money are exact strings, and floats are refused at the API
//! boundary ([`decimal`]). The SDK propagates the caller's W3C trace context in the UBTP
//! envelope and sends nothing to any telemetry endpoint.
//!
//! ```no_run
//! # async fn demo() -> ubos_sdk::Result<()> {
//! use ubos_sdk::{Auth, Client, Config, Selectors};
//! let c = Client::connect(Config::new("http://127.0.0.1:8080", "acme",
//!     Auth::Password { username: "admin".into(), password: "…".into() })).await?;
//! let item = c.read("ubos://acme/Customer/acme", &Selectors::default()).await?;
//! println!("{} {}", item.version(), item.body());
//! # Ok(()) }
//! ```

pub mod decimal;
pub mod error;
pub mod files;
pub mod ops;
pub mod registry;
pub mod retry;
pub mod session;
pub mod transport;
pub mod typed;
pub mod util;
pub mod versioning;

pub use decimal::{Decimal, Money};
pub use error::{Error, Result};
pub use files::{BlobRef, UploadOptions};
pub use ops::{CallOptions, Item, MutateResult, Page, Selectors, Subscription, Target};
pub use registry::{Area, Code};
pub use retry::RetryPolicy;
pub use session::{Auth, Client, Config, Update};
pub use transport::Binding;
pub use typed::{Cmp, Literal, Query, Typed};
pub use util::TraceContext;
pub use versioning::ChangeSetAction;
