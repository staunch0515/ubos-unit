//! The error of an operation, carrying a registered code (STD-ERR-001, STD-ERR-002).

use serde_json::Value;
use std::fmt;

/// An operation failed with a registered error code and optional structured details.
#[derive(Debug, Clone, PartialEq)]
pub struct BpaError {
    /// Registered code, `<AREA>.<UPPER_SNAKE>`.
    pub code: &'static str,
    /// Code-specific structured data (`DAT-Error.details`).
    pub details: Option<Value>,
    /// Human-readable detail for diagnostics; never compared.
    pub message: String,
}

impl BpaError {
    /// An error with a code and a diagnostic message.
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        BpaError {
            code,
            details: None,
            message: message.into(),
        }
    }

    /// An error with structured details.
    pub fn with_details(code: &'static str, details: Value) -> Self {
        BpaError {
            code,
            details: Some(details),
            message: String::new(),
        }
    }
}

impl fmt::Display for BpaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for BpaError {}

/// Shorthand for `MODEL.INVALID_VALUE`.
pub fn invalid(message: impl Into<String>) -> BpaError {
    BpaError::new("MODEL.INVALID_VALUE", message)
}

/// Shorthand for `MODEL.CONSTRAINT_VIOLATED`.
pub fn violated(message: impl Into<String>) -> BpaError {
    BpaError::new("MODEL.CONSTRAINT_VIOLATED", message)
}
