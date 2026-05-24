// SPDX-License-Identifier: Apache-2.0
//! HTTP client for the Integritas v2 API.
//!
//! All three endpoints share the same request shape (an `api_key` plus a
//! SHA3-256 `file_hash` plus optional filename/filesize metadata). We keep
//! the request body assembly in one place rather than three near-identical
//! methods so the JSON shape moves as one unit if the upstream schema
//! shifts.

use std::time::Duration;

use reqwest::Client;
use serde::Serialize;
use tracing::{debug, warn};

use crate::error::{Error, Result};
use crate::types::{CheckResponse, StampMetadata, StampResponse, VerifyResponse};

/// Default Integritas API base URL. Overridable via [`IntegritasConfig`]
/// for staging, on-prem, or test environments.
pub const DEFAULT_BASE_URL: &str = "https://api.integritas.technology";

/// Default per-request timeout. Stamping involves an on-chain mint;
/// Integritas's docs don't quote latency, so we set a generous default
/// and let callers tighten it.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Configuration for [`IntegritasClient`].
#[derive(Debug, Default, Clone)]
pub struct IntegritasConfig {
    /// Integritas-issued API key. Required.
    pub api_key: String,
    /// Override the API base URL. Defaults to
    /// [`DEFAULT_BASE_URL`](https://api.integritas.technology).
    pub base_url: Option<String>,
    /// Per-request timeout. Defaults to 30s.
    pub timeout: Option<Duration>,
}

/// Client for the Integritas v2 API.
///
/// Cheap to clone — the underlying `reqwest::Client` is reference-counted
/// and shares a connection pool. Construct once per process, clone freely.
#[derive(Debug, Clone)]
pub struct IntegritasClient {
    http: Client,
    base_url: String,
    api_key: String,
}

impl IntegritasClient {
    /// Build a new client.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] if the API key is empty or the base URL
    /// is malformed, and [`Error::Transport`] if the underlying HTTP
    /// client can't be constructed.
    pub fn new(config: IntegritasConfig) -> Result<Self> {
        if config.api_key.trim().is_empty() {
            return Err(Error::Config("api_key must not be empty".into()));
        }
        let base_url = config
            .base_url
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string();

        let http = Client::builder()
            .timeout(config.timeout.unwrap_or(DEFAULT_TIMEOUT))
            .user_agent(concat!(
                "integritas-edge/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()?;

        Ok(Self {
            http,
            base_url,
            api_key: config.api_key,
        })
    }

    /// Stamp a SHA3-256 hash. Wraps `POST /v2/timestamp/post`.
    ///
    /// The metadata block (filename/filesize) is optional — they shape the
    /// human-readable report but do not affect the cryptographic
    /// commitment.
    ///
    /// # Errors
    ///
    /// See [`Error`] variants for transport, HTTP, and parse failure modes.
    pub async fn stamp(
        &self,
        file_hash: &[u8; 32],
        metadata: Option<StampMetadata>,
    ) -> Result<StampResponse> {
        let body = build_request(self.api_key.as_str(), file_hash, metadata.as_ref());
        let resp: StampResponse = self.post("/v2/timestamp/post", &body).await?;
        if resp.status != "success" {
            warn!(status = %resp.status, error = ?resp.error, "integritas stamp non-success");
        }
        Ok(resp)
    }

    /// Verify a previously stamped hash. Wraps `POST /v2/verify/file`.
    ///
    /// # Errors
    ///
    /// See [`Error`].
    pub async fn verify(
        &self,
        file_hash: &[u8; 32],
        metadata: Option<StampMetadata>,
    ) -> Result<VerifyResponse> {
        let body = build_request(self.api_key.as_str(), file_hash, metadata.as_ref());
        self.post("/v2/verify/file", &body).await
    }

    /// Check whether a hash has already been stamped. Wraps `POST
    /// /v2/file/check` — useful as a cheap idempotency probe before
    /// triggering a new stamp.
    ///
    /// # Errors
    ///
    /// See [`Error`].
    pub async fn check(&self, file_hash: &[u8; 32]) -> Result<CheckResponse> {
        let body = build_request(self.api_key.as_str(), file_hash, None);
        self.post("/v2/file/check", &body).await
    }

    async fn post<Body, Out>(&self, path: &str, body: &Body) -> Result<Out>
    where
        // `Sync` is required so the returned future is `Send` — the
        // `ReceiptSink` trait downstream (and any reasonable Tokio
        // multi-thread executor) expects `Send` futures.
        Body: Serialize + Sync + ?Sized,
        Out: for<'de> serde::Deserialize<'de>,
    {
        let url = format!("{}{}", self.base_url, path);
        debug!(%url, "integritas POST");
        let response = self.http.post(&url).json(body).send().await?;
        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
            return Err(Error::HttpStatus {
                status: status.as_u16(),
                body: truncate(&text, 1024),
            });
        }
        // Empty body on success is unusual but not impossible — surface
        // it as a Malformed error rather than letting serde produce a
        // less helpful one.
        if text.is_empty() {
            return Err(Error::Malformed {
                hint: "empty response body",
                body: String::new(),
            });
        }
        serde_json::from_str::<Out>(&text).map_err(|_| Error::Malformed {
            hint: "response was not the expected JSON shape",
            body: truncate(&text, 1024),
        })
    }
}

/// The shared request shape used by all three endpoints.
#[derive(Debug, Serialize)]
struct StampRequest<'a> {
    api_key: &'a str,
    file_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filesize: Option<u64>,
}

fn build_request<'a>(
    api_key: &'a str,
    file_hash: &[u8; 32],
    metadata: Option<&'a StampMetadata>,
) -> StampRequest<'a> {
    StampRequest {
        api_key,
        file_hash: hex::encode(file_hash),
        filename: metadata.and_then(|m| m.filename.as_deref()),
        filesize: metadata.and_then(|m| m.filesize),
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_string()
    } else {
        let mut out = text[..max].to_string();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_hex_encodes_hash() {
        let hash = [0xab; 32];
        let req = build_request("k", &hash, None);
        assert_eq!(req.file_hash, "ab".repeat(32));
        assert_eq!(req.api_key, "k");
        assert!(req.filename.is_none());
        assert!(req.filesize.is_none());
    }

    #[test]
    fn build_request_passes_metadata() {
        let hash = [0u8; 32];
        let meta = StampMetadata {
            filename: Some("evidence.bin".into()),
            filesize: Some(123),
        };
        let req = build_request("k", &hash, Some(&meta));
        assert_eq!(req.filename, Some("evidence.bin"));
        assert_eq!(req.filesize, Some(123));
    }

    #[test]
    fn client_rejects_empty_api_key() {
        let err = IntegritasClient::new(IntegritasConfig::default()).unwrap_err();
        assert!(matches!(err, Error::Config(_)));
    }

    #[test]
    fn truncate_respects_max() {
        assert_eq!(truncate("hi", 10), "hi");
        let big = "a".repeat(100);
        let t = truncate(&big, 10);
        assert!(t.starts_with(&"a".repeat(10)));
        assert!(t.ends_with('…'));
    }
}
