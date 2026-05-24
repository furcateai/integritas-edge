// SPDX-License-Identifier: Apache-2.0
//! A Rust client for the [Integritas](https://integritas.technology)
//! timestamping API.
//!
//! Integritas is a hosted service that anchors SHA3-256 hashes on Minima
//! via NFT minting, and returns a verifiable proof (PDF + on-chain
//! reference). It is the *managed* counterpart to running your own local
//! Minima node and writing transaction state variables directly with
//! [`minima-attest`](https://github.com/furcateai/minima-attest).
//!
//! `integritas-edge` is intentionally narrow: a typed Rust binding for the
//! three v2 endpoints (`stamp`, `verify`, `check`), plus optional impls of
//! `furcate-inference-core`'s `ReceiptSink` and `Attester` traits behind
//! the `furcate` feature.
//!
//! # Quick start
//!
//! ```no_run
//! use integritas_edge::{IntegritasClient, IntegritasConfig};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let client = IntegritasClient::new(IntegritasConfig {
//!     api_key: std::env::var("INTEGRITAS_API_KEY")?,
//!     ..Default::default()
//! })?;
//!
//! // Stamp a 32-byte SHA3-256 hash.
//! let hash = integritas_edge::sha3_256(b"hello world");
//! let stamp = client.stamp(&hash, None).await?;
//! println!("stamped: {:?}", stamp);
//!
//! // Later, verify it.
//! let result = client.verify(&hash, None).await?;
//! println!("verified: {:?}", result);
//! # Ok(())
//! # }
//! ```
//!
//! # Relationship to `minima-attest`
//!
//! The two crates anchor data on Minima via **different paths**:
//!
//! | | `minima-attest` | `integritas-edge` |
//! |---|---|---|
//! | Where you trust | Your own Minima full node | The Integritas backend |
//! | Auth | Local `rpcpassword` | Integritas `api_key` |
//! | On-chain shape | Direct `txnstate` write | NFT mint (Integritas-orchestrated) |
//! | Hash format | Any 32 bytes | SHA3-256 |
//! | Verify path | Local `txpow` lookup | `POST /v2/verify/file` |
//! | Self-host | Yes (that's the point) | No (hosted-only today) |
//!
//! A Furcate node can wire **both** as parallel `ReceiptSink`s. They do
//! not collide.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod client;
mod error;
mod types;

#[cfg(feature = "furcate")]
mod furcate;

pub use client::{IntegritasClient, IntegritasConfig};
pub use error::{Error, Result};
pub use types::{CheckResponse, StampMetadata, StampResponse, VerifyResponse};

#[cfg(feature = "furcate")]
pub use furcate::IntegritasReceiptSink;

/// Compute a SHA3-256 hash. Convenience wrapper so callers don't need to
/// pull `sha3` in directly.
#[must_use]
pub fn sha3_256(input: &[u8]) -> [u8; 32] {
    use sha3::{Digest, Sha3_256};
    let mut hasher = Sha3_256::new();
    hasher.update(input);
    let out = hasher.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&out);
    bytes
}
