# SINP - Semantic Intent Negotiation Protocol

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A Rust implementation of the **Semantic Intent Negotiation Protocol (SINP)** — an application-layer protocol that replaces deterministic address-routing with semantic negotiation between clients and servers.

## Overview

SINP enables intelligent communication where:

- **Clients** express **intent** in natural language with a confidence score
- **Servers** interpret the intent, match it to capabilities, and respond with one of:
  - **EXECUTE** — Perform the action and return results
  - **CLARIFY** — Ask for more details
  - **PROPOSE** — Suggest alternative actions
  - **REFUSE** — Reject with a reason code

This creates a negotiation loop where both parties converge on the right action, governed by configurable confidence thresholds.

## Architecture

```
┌─────────────┐                          ┌─────────────┐
│   Client    │                          │   Server    │
│             │  ──── Intent + Φc ────►  │             │
│  State:     │                          │  State:     │
│  INIT       │                          │  RECEIVED   │
│  PENDING    │  ◄── Response + Φs ───   │  DECIDING   │
│  REFINING   │                          │  DONE       │
│  SATISFIED  │                          │             │
└─────────────┘                          └─────────────┘
```

## Quick Start

### Build

```bash
git clone https://github.com/hemantsingh443/sinp.git
cd sinp
cargo build --workspace
```

### Run Server

```bash
cargo run -p sinp-server -- 127.0.0.1:8080
```

### Run Client

```bash
cargo run -p sinp-client --bin test_client
```

## Project Structure

```
sinp/
├── sinp-core/          # Core library
│   ├── message.rs      # Message types (Request, Response)
│   ├── confidence.rs   # Φ computation & decision logic
│   ├── security.rs     # SHA256, Ed25519, replay protection
│   ├── state.rs        # State machine definitions
│   └── interpreter.rs  # Intent interpretation
├── sinp-server/        # TCP/TLS server
│   ├── config.rs       # Server configuration
│   ├── capability.rs   # Capability registry
│   ├── handler.rs      # Connection handling
│   └── state_machine.rs
├── sinp-client/        # Client SDK
│   ├── lib.rs          # High-level SinpClient API
│   ├── connection.rs   # TCP/TLS connection
│   └── state_machine.rs
└── SINP.pdf            # RFC specification
```

## Usage

### Server

```rust
use sinp_server::{Server, ServerConfig, CapabilityRegistry};
use sinp_core::Capability;

let mut registry = CapabilityRegistry::new();

registry.register(
    Capability {
        id: "greet:v1".to_string(),
        description: "Greet the user".to_string(),
        inputs: vec!["name".to_string()],
        privacy_level: "public".to_string(),
        cost_units: 0.1,
    },
    |req| Ok(serde_json::json!({"greeting": format!("Hello, {}!", req.intent)})),
    0.95,
);

let config = ServerConfig::with_addr("0.0.0.0:9000".parse()?);
let server = Server::new(config, registry)?;
server.run().await?;
```

### Client

```rust
use sinp_client::SinpClient;

let mut client = SinpClient::connect("127.0.0.1:9000").await?;

match client.send_intent("echo hello world", 0.90).await? {
    NextAction::Done(response) => {
        println!("Result: {:?}", response.action_metadata);
    }
    NextAction::Clarify { questions, .. } => {
        // Server needs more info
        client.respond_to_clarify("more details here", 0.85).await?;
    }
    NextAction::Propose { alternatives, .. } => {
        // Server suggests alternatives
        client.accept_proposal(&alternatives[0], 0.90).await?;
    }
    NextAction::Refused { reason, .. } => {
        println!("Refused: {}", reason);
    }
}
```

## Protocol Details

### Wire Format

Messages are length-prefixed JSON over TCP:

```
┌──────────────────┬─────────────────────────┐
│ Length (4 bytes) │ JSON Message (UTF-8)    │
│ Big-endian u32   │                         │
└──────────────────┴─────────────────────────┘
```

### Decision Thresholds

| Threshold | Default | Description                 |
| --------- | ------- | --------------------------- |
| τ_exec    | 0.85    | Minimum Φs to execute       |
| τ_clarify | 0.50    | Threshold for clarification |
| τ_accept  | 0.50    | Minimum Φc to proceed       |

### Confidence Computation

```
Φs = min(1, ρ × R(c) × A(res)) × P(pol)
```

Where:

- `ρ` — Raw interpretation probability
- `R(c)` — Capability reliability
- `A(res)` — Resource availability
- `P(pol)` — Policy check (0 or 1)

### Security Features

- **Semantic Hashing**: SHA256 for caching identical intents
- **Replay Protection**: 5-second timestamp window
- **Signatures**: Ed25519 with JCS canonicalization (RFC 8785)

## Tests

```bash
cargo test --workspace
```

**33 tests** covering:

- Message serialization
- Confidence computation
- State machine transitions
- Security primitives
- Capability registry

## RFC Specification

See [SINP.tex](SINP.tex) for the full protocol specification.

## License

MIT
