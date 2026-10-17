//! Typed errors (DSN-SDK-007): every error of the node is an [`Error`] with its registered
//! [`Code`], area, details and the registry's retryable and catchable flags; failures of
//! the transport are [`Error::transport`], retryable.

use crate::registry::{Area, Code, REGISTRY};
use serde_json::Value;
use std::fmt;

/// An error of the SDK or of the node.
#[derive(Debug, Clone)]
pub struct Error {
    /// The registered code, when the code is in the registry.
    pub code: Option<Code>,
    /// The code as text (`AREA.NAME`, or `SDK.TRANSPORT` for transport failures).
    pub code_text: String,
    /// The message.
    pub message: String,
    /// Code-specific details (the node's `details`).
    pub details: Value,
    /// The HTTP status of the code, when it has one.
    pub status: Option<u16>,
}

/// The kind of a transport failure, not an error of the node.
pub const TRANSPORT: &str = "SDK.TRANSPORT";

impl Error {
    /// An error of a code with a message and details.
    pub fn new(code: &str, message: impl Into<String>, details: Value) -> Error {
        let entry = REGISTRY.iter().find(|r| r.1 == code);
        Error {
            code: entry.map(|r| r.0),
            code_text: code.to_string(),
            message: message.into(),
            details,
            status: entry.map(|r| r.5),
        }
    }

    /// The error of an `error` frame body or an RFC 9457 problem (`code`, `message` or
    /// `detail`, `details` or `explanation`).
    pub fn from_body(b: &Value) -> Error {
        let code = b["code"].as_str().unwrap_or("INT.PROTOCOL_ERROR");
        let message = b["message"]
            .as_str()
            .or_else(|| b["detail"].as_str())
            .unwrap_or_default();
        let details = if b["details"].is_null() {
            b["explanation"].clone()
        } else {
            b["details"].clone()
        };
        Error::new(code, message, details)
    }

    /// A failure of the transport (connection refused, reset, timed out).
    pub fn transport(e: impl fmt::Display) -> Error {
        Error {
            code: None,
            code_text: TRANSPORT.into(),
            message: e.to_string(),
            details: Value::Null,
            status: None,
        }
    }

    /// The area of the code.
    pub fn area(&self) -> Option<Area> {
        REGISTRY
            .iter()
            .find(|r| Some(r.0) == self.code)
            .map(|r| r.2)
    }

    /// Whether the registry classifies the code retryable (transport failures are).
    pub fn retryable(&self) -> bool {
        if self.code_text == TRANSPORT {
            return true;
        }
        REGISTRY
            .iter()
            .find(|r| Some(r.0) == self.code)
            .is_some_and(|r| r.3)
    }

    /// Whether logic may catch the code (from the registry).
    pub fn catchable(&self) -> bool {
        REGISTRY
            .iter()
            .find(|r| Some(r.0) == self.code)
            .is_some_and(|r| r.4)
    }

    /// Whether this is a transport failure.
    pub fn is_transport(&self) -> bool {
        self.code_text == TRANSPORT
    }

    /// Whether the code is `code`.
    pub fn is(&self, code: Code) -> bool {
        self.code == Some(code)
    }

    /// The `reason` member of the details, when there is one.
    pub fn reason(&self) -> Option<&str> {
        self.details["reason"].as_str()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.message.is_empty() {
            write!(f, "{}", self.code_text)
        } else {
            write!(f, "{}: {}", self.code_text, self.message)
        }
    }
}

impl std::error::Error for Error {}

/// The result of the SDK.
pub type Result<T> = std::result::Result<T, Error>;
