// SPDX-License-Identifier: Apache-2.0
//! Request and response types for the Integritas v2 API.
//!
//! The shapes here track what the Integritas MCP tool reference documents
//! today:
//!
//! - `stamp_hash` → `POST /v2/timestamp/post`
//! - `verify_hash` → `POST /v2/verify/file`
//! - `check_hash` → `POST /v2/file/check`
//!
//! Each endpoint accepts an optional filename + filesize alongside the
//! mandatory SHA3-256 file hash. Responses carry a `status` field plus a
//! provider-specific `data` blob; we keep `data` as `serde_json::Value`
//! rather than overfitting a typed schema that may move under our feet.

use serde::{Deserialize, Serialize};

/// Optional metadata Integritas accepts when stamping or verifying a hash.
///
/// Filename and filesize do not affect the cryptographic commitment (the
/// hash is the commitment) — they're metadata the report uses for human
/// context.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct StampMetadata {
    /// Filename to record in the stamp report. Optional.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// Filesize in bytes to record in the stamp report. Optional.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filesize: Option<u64>,
}

/// Response to `POST /v2/timestamp/post`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StampResponse {
    /// Provider-reported status string (e.g. `"success"`, `"pending"`).
    pub status: String,
    /// Provider-specific payload — typically references to the on-chain
    /// transaction, the NFT, and the generated PDF report. Kept opaque so
    /// schema changes don't break callers.
    #[serde(default)]
    pub data: serde_json::Value,
    /// Error string when `status != "success"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response to `POST /v2/verify/file`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyResponse {
    /// Status string from the provider.
    pub status: String,
    /// Optional verification report links (e.g. PDF URL).
    #[serde(default)]
    pub links: serde_json::Value,
    /// Provider-specific data.
    #[serde(default)]
    pub data: serde_json::Value,
    /// Error string when `status != "success"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response to `POST /v2/file/check`.
///
/// Used to ask whether a given hash has *already* been stamped without
/// triggering a new stamp. Idempotency check, basically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResponse {
    /// Status string from the provider.
    pub status: String,
    /// Provider-specific payload describing the stamp state.
    #[serde(default)]
    pub data: serde_json::Value,
    /// Error string when `status != "success"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
