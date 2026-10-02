//! `ubos-types` — engine layer (UBS-ARC-07 §2).
//!
//! IDs, URIs, value types, canonical JSON (RFC 8785), hashing and the error type.
//! Allowed dependencies: none (AR-031).
//!
//! PH-0 prototype scope (UBS-PH-0 §2, DEC-033): canonical JSON and NFC (STD-FND-001…003),
//! content identifiers with domain separation (STD-FND-010, -011), names and identifiers
//! (STD-FND-015, -016, STD-VER-013), value types, decimals, money, time values and
//! classification (STD-FND-020…027).

pub mod addr;
pub mod canon;
pub mod decimal;
pub mod error;
pub mod names;
pub mod time;
pub mod values;

pub use canon::{canonical, content_id, digest, nfc};
pub use error::BpaError;
