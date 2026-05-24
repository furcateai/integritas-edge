// SPDX-License-Identifier: Apache-2.0
//! `integritas-edge` CLI.
//!
//! Three subcommands matching the three v2 endpoints. `--api-key` is also
//! readable from `INTEGRITAS_API_KEY` for shell-script + cron use.

use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use integritas_edge::{IntegritasClient, IntegritasConfig, StampMetadata};

#[derive(Parser, Debug)]
#[command(
    name = "integritas-edge",
    version,
    about = "Stamp and verify hashes via the Integritas API"
)]
struct Cli {
    /// Integritas API key. Required.
    #[arg(long, env = "INTEGRITAS_API_KEY", global = true)]
    api_key: Option<String>,

    /// Override the Integritas base URL. Defaults to
    /// https://api.integritas.technology.
    #[arg(long, env = "INTEGRITAS_BASE_URL", global = true)]
    base_url: Option<String>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Stamp a hash or file.
    Stamp {
        /// 64-char hex SHA3-256 hash. Mutually exclusive with `--file`.
        #[arg(long, conflicts_with = "file")]
        hash: Option<String>,
        /// Path to a file to hash + stamp.
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Verify a previously stamped hash.
    Verify {
        /// 64-char hex SHA3-256 hash.
        hash: String,
    },
    /// Check whether a hash has already been stamped (idempotency probe).
    Check {
        /// 64-char hex SHA3-256 hash.
        hash: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    let api_key = cli
        .api_key
        .ok_or_else(|| anyhow!("set --api-key or INTEGRITAS_API_KEY"))?;

    let client = IntegritasClient::new(IntegritasConfig {
        api_key,
        base_url: cli.base_url,
        timeout: None,
    })?;

    match cli.cmd {
        Cmd::Stamp { hash, file } => {
            let (bytes, meta) = resolve_hash_input(hash, file)?;
            let resp = client.stamp(&bytes, meta).await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
        Cmd::Verify { hash } => {
            let bytes = parse_hash(&hash)?;
            let resp = client.verify(&bytes, None).await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
        Cmd::Check { hash } => {
            let bytes = parse_hash(&hash)?;
            let resp = client.check(&bytes).await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
    }
    Ok(())
}

fn resolve_hash_input(
    hash: Option<String>,
    file: Option<PathBuf>,
) -> Result<([u8; 32], Option<StampMetadata>)> {
    match (hash, file) {
        (Some(_), Some(_)) => Err(anyhow!("pass --hash OR --file, not both")),
        (None, None) => Err(anyhow!("pass --hash <hex> or --file <path>")),
        (Some(h), None) => Ok((parse_hash(&h)?, None)),
        (None, Some(path)) => {
            let bytes = std::fs::read(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let filesize = bytes.len() as u64;
            let hash = integritas_edge::sha3_256(&bytes);
            let filename = path
                .file_name()
                .and_then(|s| s.to_str())
                .map(str::to_string);
            Ok((
                hash,
                Some(StampMetadata {
                    filename,
                    filesize: Some(filesize),
                }),
            ))
        }
    }
}

fn parse_hash(s: &str) -> Result<[u8; 32]> {
    let trimmed = s.strip_prefix("0x").unwrap_or(s);
    let raw = hex::decode(trimmed).context("hash is not valid hex")?;
    let arr: [u8; 32] = raw
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("hash must be 32 bytes (64 hex chars), got {}", raw.len()))?;
    Ok(arr)
}
