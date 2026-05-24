// SPDX-License-Identifier: Apache-2.0
//! Error type for `integritas-edge`.

use thiserror::Error;

/// Result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur talking to the Integritas API.
#[derive(Debug, Error)]
pub enum Error {
    /// HTTP transport / TLS failure.
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),

    /// JSON serialisation or deserialisation failed.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Configuration was invalid (e.g. missing API key, malformed base URL).
    #[error("invalid configuration: {0}")]
    Config(String),

    /// Caller passed a hash that wasn't 32 bytes.
    #[error("hash must be exactly 32 bytes, got {0}")]
    InvalidHashLength(usize),

    /// Integritas returned a non-success HTTP status.
    #[error("integritas returned HTTP {status}: {body}")]
    HttpStatus {
        /// HTTP status code returned by the Integritas API.
        status: u16,
        /// Response body verbatim — useful for debugging API errors.
        body: String,
    },

    /// The response body could not be parsed into the expected shape.
    #[error("malformed response: {hint}; body was: {body}")]
    Malformed {
        /// Short description of what was wrong.
        hint: &'static str,
        /// Raw response body for debugging.
        body: String,
    },
}
