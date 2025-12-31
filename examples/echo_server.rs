//! Echo server example for SINP.
//!
//! Run with: cargo run --example echo_server
//!
//! This example demonstrates a minimal SINP server that echoes back
//! user intents.

use sinp_core::Capability;
use sinp_server::{CapabilityRegistry, Server, ServerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    let bind_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:9000".to_string());

    println!(" Starting SINP Echo Server on {}", bind_addr);

    // Create capability registry
    let mut registry = CapabilityRegistry::new();

    // Register echo capability
    registry.register(
        Capability {
            id: "echo:v1".to_string(),
            description: "Echo back the message".to_string(),
            inputs: vec!["message".to_string()],
            privacy_level: "public".to_string(),
            cost_units: 0.1,
        },
        |req| {
            Ok(serde_json::json!({
                "echoed": req.intent,
                "words": req.intent.split_whitespace().count(),
                "timestamp": chrono::Utc::now().to_rfc3339()
            }))
        },
        0.95,
    );

    // Register reverse capability
    registry.register(
        Capability {
            id: "reverse:v1".to_string(),
            description: "Reverse the message text".to_string(),
            inputs: vec!["text".to_string()],
            privacy_level: "public".to_string(),
            cost_units: 0.2,
        },
        |req| {
            let reversed: String = req.intent.chars().rev().collect();
            Ok(serde_json::json!({
                "original": req.intent,
                "reversed": reversed
            }))
        },
        0.90,
    );

    // Register uppercase capability
    registry.register(
        Capability {
            id: "uppercase:v1".to_string(),
            description: "Convert message to uppercase".to_string(),
            inputs: vec!["text".to_string()],
            privacy_level: "public".to_string(),
            cost_units: 0.1,
        },
        |req| {
            Ok(serde_json::json!({
                "original": req.intent,
                "uppercase": req.intent.to_uppercase()
            }))
        },
        0.90,
    );

    println!(" Registered capabilities:");
    for cap in registry.capability_ids() {
        println!("   - {}", cap);
    }

    // Create and run server
    let config = ServerConfig::with_addr(bind_addr.parse()?);
    let server = Server::new(config, registry)?;

    println!("\n Server ready. Waiting for connections...\n");

    server.run().await?;
    Ok(())
}
