# integritas-edge

**A Rust client for stamping and verifying hashes via the [Integritas](https://integritas.technology) API.**

Anchor SHA3-256 hashes on Minima via the hosted [Integritas](https://integritas.technology) service — Rust library + CLI. The smallest useful primitive for hosted on-chain attestation: hand it a 32-byte hash, get back a verifiable on-chain stamp and a PDF report.

```bash
integritas-edge stamp  --file ./evidence.bin
integritas-edge verify 0xDEAD...BEEF
integritas-edge check  0xDEAD...BEEF        # idempotency probe
```

No Minima node, no Pi, no Furcate required. Anything that can speak HTTPS and has an Integritas API key can use this crate.

---

## Where it sits

```
github.com/furcateai/
├── furcate-protocol                    wire-format specs + schemas
├── furcate-inference                   edge inference kernel
├── furcate-mesh                        LAN peer fabric for edge nodes
├── minima-attest                       Rust client for anchoring hashes on a local Minima node
├── integritas-edge   ← you are here    Rust client for the Integritas hosted stamping API
├── tenzro-edge                         runtime for participating in the Tenzro Network
├── prvnz-edge                          runtime for issuing PRVNZ Digital Product Passports
├── furcate-pi-hat                      Pi 5 HAT hardware support (GPIO, 1-Wire, OPC UA triggers)
└── furcate-pi-minima                   supervisor for running a Minima full node on a Pi
```

`integritas-edge` is the hosted-API sibling to [`minima-attest`](https://github.com/furcateai/minima-attest). Both anchor data on Minima; they differ in where you place trust and what infrastructure you have to run.

## What it does — and why

[Integritas](https://integritas.technology) ([previous URL](https://integritas.minima.global)) is a hosted timestamping service that takes SHA3-256 hashes, mints an NFT on Minima L1 against each one, and returns a verifiable proof (on-chain reference + PDF report). It's API-key authenticated and exposes three v2 endpoints — `stamp`, `verify`, `check`.

`integritas-edge` is a typed Rust binding for those three endpoints. That's it.

- **Library** (`integritas-edge = "0.1"`) — async client built on `reqwest` + `rustls`, no OpenSSL, no system TLS roots required.
- **CLI** (`cargo install integritas-edge --features cli`) — thin wrapper around the library for shell scripts and cron.
- **Optional Furcate sink** (`--features furcate`) — implements `furcate-inference-core`'s `ReceiptSink` trait so a Furcate node can wire Integritas in alongside [`minima-attest`](https://github.com/furcateai/minima-attest) as a parallel sink.

### For Integritas users

If you already use Integritas via the JS SDK, Python SDK, or MCP server and you're looking for a **Rust** binding, this is it. Direct mapping of `stampHash` / `verifyHash` / `checkHash` to typed `client.stamp()` / `client.verify()` / `client.check()` methods, async on Tokio, with proper error types instead of generic exceptions.

### For Furcate operators

Furcate nodes can wire two on-chain sinks **in parallel**:

```toml
[receipt_sinks.minima_local]
type = "minima-anchor"      # local-node attestation via minima-attest

[receipt_sinks.integritas]
type = "integritas-stamp"   # hosted-API attestation via integritas-edge
```

Each `StepReceipt` produced by the agent loop is hashed (BLAKE3 canonical-JSON) and the digest fans out to every configured sink. They do not interact. One sink writing to your local Minima node and the other stamping via Integritas gives you two independent proofs of the same receipt — useful when you want both self-sovereign and hosted attestation.

## When to pick which

| | `minima-attest` | `integritas-edge` |
|---|---|---|
| Where you trust | Your own Minima full node | The Integritas backend |
| Auth | Local `rpcpassword` | Integritas `api_key` |
| On-chain shape | Direct `txnstate` write | NFT mint (Integritas-orchestrated) |
| Hash format | Any 32 bytes | SHA3-256 |
| Verify path | Local `txpow` lookup | `POST /v2/verify/file` |
| Infrastructure | A Pi or any box running a Minima full node | An HTTPS connection |
| Cost shape | Free (you run the node) | Paid (Integritas API plan) |
| Offline-tolerant | Yes (local node) | No (requires hosted service) |
| PDF / report artefact | No | Yes |

Pick `minima-attest` when you want self-sovereign attestation and you're willing to run a node. Pick `integritas-edge` when you want zero-infra hosted stamping with PDF reports. Wire both when you want belt-and-braces.

## Quick start (library)

```rust
use integritas_edge::{IntegritasClient, IntegritasConfig};

let client = IntegritasClient::new(IntegritasConfig {
    api_key: std::env::var("INTEGRITAS_API_KEY")?,
    ..Default::default()
})?;

// Stamp a 32-byte SHA3-256 hash.
let hash = integritas_edge::sha3_256(b"hello world");
let stamp = client.stamp(&hash, None).await?;
println!("stamped: status={}", stamp.status);

// Later, verify it.
let result = client.verify(&hash, None).await?;
println!("verified: status={}", result.status);
```

## Quick start (CLI)

```bash
cargo install integritas-edge --features cli

export INTEGRITAS_API_KEY=ik_live_...

# Stamp a file (hashes locally, stamps the digest, never uploads the file)
integritas-edge stamp --file ./evidence.bin

# Stamp a pre-computed hash
integritas-edge stamp --hash 0xDEAD...BEEF

# Verify
integritas-edge verify 0xDEAD...BEEF

# Cheap idempotency probe before triggering a new stamp
integritas-edge check 0xDEAD...BEEF
```

The CLI never uploads file contents — only the SHA3-256 digest crosses the wire.

## Furcate composition

Enable the `furcate` feature to get the `ReceiptSink` impl:

```toml
[dependencies]
integritas-edge = { version = "0.1", features = ["furcate"] }
```

```rust
use integritas_edge::{IntegritasClient, IntegritasConfig, IntegritasReceiptSink};

let client = IntegritasClient::new(IntegritasConfig {
    api_key: std::env::var("INTEGRITAS_API_KEY")?,
    ..Default::default()
})?;
let sink = IntegritasReceiptSink::new(client);

// Wire `sink` into the Furcate agent loop's receipt fan-out alongside
// other ReceiptSink impls (e.g. minima-attest's). Transport errors map
// to ReceiptSinkError::Transient so the offline buffer replays them.
```

## What this is **not**

- Not a Minima node — Integritas operates that piece, you only need an HTTPS connection.
- Not coupled to Furcate — the `furcate` feature is opt-in; without it the crate has no Furcate types in its public API.
- Not a replacement for `minima-attest` — different trust model, different infra footprint, valid in different contexts.
- Not a wrapper around all of Integritas — only the three v2 endpoints documented in the public MCP tool reference.

## Status

- Version: **0.1.0**
- Targets Integritas v2 API (`stamp`, `verify`, `check`)
- Breaking API changes possible until 1.0

## Versioning

- Releases independently of the `furcate-inference` kernel.
- Pins `furcate-inference-core` to a specific major version when the `furcate` feature is enabled.

## Sibling repos

- [`minima-attest`](https://github.com/furcateai/minima-attest) — self-sovereign sibling: anchor 32-byte hashes on a local Minima full node
- [`furcate-inference`](https://github.com/furcateai/furcate-inference) — edge inference kernel (this crate implements `ReceiptSink` from `furcate-inference-core`)
- [`furcate-protocol`](https://github.com/furcateai/furcate-protocol) — wire-format specs

## License

Apache License 2.0. See [LICENSE](./LICENSE) and [NOTICE](./NOTICE).
