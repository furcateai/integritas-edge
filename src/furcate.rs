// SPDX-License-Identifier: Apache-2.0
//! Furcate trait impls — gated by the `furcate` Cargo feature.
//!
//! When this feature is enabled, [`IntegritasReceiptSink`] implements
//! `furcate-inference-core`'s [`ReceiptSink`] trait so a Furcate node can
//! drop the Integritas API in alongside `minima-attest` as a parallel
//! sink:
//!
//! ```toml
//! [receipt_sinks.minima_local]
//! type = "minima-anchor"      # local-node, via minima-attest
//!
//! [receipt_sinks.integritas]
//! type = "integritas-stamp"   # hosted API, via integritas-edge
//! ```
//!
//! Both sinks see the same [`StepReceipt`]; one writes a `txnstate`
//! transaction on a local Minima node, the other POSTs the receipt's
//! BLAKE3 hash to Integritas. They do not interact.

use async_trait::async_trait;
use furcate_inference_core::{Attestation, ReceiptSink, SinkError, SinkId, StepReceipt};

use crate::client::IntegritasClient;
use crate::error::Error;
use crate::types::StampMetadata;

/// [`ReceiptSink`] impl that stamps each receipt's BLAKE3 digest via the
/// Integritas hosted API.
///
/// `StepReceipt`s are canonicalised to JSON, hashed with BLAKE3 (the
/// Furcate-canonical receipt digest), and the resulting 32 bytes are
/// passed to Integritas's `/v2/timestamp/post` in the SHA3-256-shaped
/// `file_hash` slot. Integritas does not enforce that the input is
/// *actually* SHA3-256 — it treats the value as an opaque 32-byte
/// commitment — so reusing the slot for a BLAKE3 digest is safe.
/// Verification on the Integritas side compares stored 32-byte values,
/// not digest algorithms.
#[derive(Debug, Clone)]
pub struct IntegritasReceiptSink {
    id: SinkId,
    client: IntegritasClient,
}

impl IntegritasReceiptSink {
    /// Construct a new sink with the given [`SinkId`] and an Integritas
    /// client.
    #[must_use]
    pub fn new(id: impl Into<String>, client: IntegritasClient) -> Self {
        Self {
            id: SinkId(id.into()),
            client,
        }
    }
}

fn map_sink(e: Error) -> SinkError {
    match e {
        // Transport / HTTP failures → Transient so the Furcate agent
        // loop queues the receipt for replay via its offline buffer
        // rather than dropping it on the floor.
        Error::Transport(err) => SinkError::Transient(format!("integritas transport: {err}")),
        Error::HttpStatus { status, body } => {
            SinkError::Transient(format!("integritas http {status}: {body}"))
        }
        Error::Json(err) => SinkError::Rejected(format!("integritas json: {err}")),
        Error::Malformed { hint, body } => {
            SinkError::Rejected(format!("integritas malformed ({hint}): {body}"))
        }
        Error::Config(msg) => SinkError::Rejected(format!("integritas config: {msg}")),
        Error::InvalidHashLength(len) => {
            SinkError::Rejected(format!("integritas invalid hash length: {len}"))
        }
    }
}

#[async_trait]
impl ReceiptSink for IntegritasReceiptSink {
    fn id(&self) -> SinkId {
        self.id.clone()
    }

    async fn write(
        &self,
        receipt: &StepReceipt,
        _attestations: &[Attestation],
    ) -> Result<(), SinkError> {
        // Canonical-JSON BLAKE3 of the receipt is the commitment.
        let canon = serde_json::to_vec(receipt)
            .map_err(|e| SinkError::Rejected(format!("canonicalise receipt: {e}")))?;
        let digest = blake3::hash(&canon);
        let mut digest_bytes = [0u8; 32];
        digest_bytes.copy_from_slice(digest.as_bytes());

        // Carry the step id as the human-facing filename in the
        // Integritas report. Step ids are canonical and short — no PII.
        let metadata = StampMetadata {
            filename: Some(format!("furcate-receipt-{}", receipt.step_id)),
            filesize: Some(canon.len() as u64),
        };

        let resp = self
            .client
            .stamp(&digest_bytes, Some(metadata))
            .await
            .map_err(map_sink)?;
        if resp.status == "success" {
            Ok(())
        } else {
            // Non-success status from the API surface itself — treat as
            // transient so the loop retries (Integritas occasionally
            // returns pending/queued states).
            Err(SinkError::Transient(format!(
                "integritas non-success: status={} error={:?}",
                resp.status, resp.error
            )))
        }
    }

    async fn flush(&self) -> Result<(), SinkError> {
        // Each stamp call is synchronous from this client's perspective
        // (the HTTP POST completes before we return). Nothing to flush.
        Ok(())
    }
}
