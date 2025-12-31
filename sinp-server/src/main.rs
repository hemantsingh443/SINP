//! SINP Server - Semantic Intent Negotiation Protocol server implementation.

mod capability;
mod config;
mod handler;
mod state_machine;

pub use capability::CapabilityRegistry;
pub use config::{ServerConfig, TlsConfig};
pub use handler::Server;
pub use state_machine::ServerStateMachine;

use sinp_core::{Capability, Request, SinpResult};
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> SinpResult<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Parse command line args
    let bind_addr: SocketAddr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:9000".to_string())
        .parse()
        .expect("Invalid bind address");

    // Create config with lower thresholds for testing
    let config = ServerConfig::with_addr(bind_addr)
        .with_thresholds(sinp_core::Thresholds::new(0.20, 0.10, 0.10));

    // Create capability registry with example capabilities
    let mut registry = CapabilityRegistry::new();

    // Register echo capability with more keywords
    registry.register(
        Capability {
            id: "echo:v1".to_string(),
            description: "Echo back repeat say print message text hello hi".to_string(),
            inputs: vec!["message".to_string(), "text".to_string()],
            privacy_level: "public".to_string(),
            cost_units: 0.1,
        },
        |req: &Request| {
            Ok(serde_json::json!({
                "echo": req.intent,
                "timestamp": chrono::Utc::now().to_rfc3339()
            }))
        },
        0.95,
    );

    // Register help capability
    registry.register(
        Capability {
            id: "help:v1".to_string(),
            description: "Get help and list available capabilities".to_string(),
            inputs: vec![],
            privacy_level: "public".to_string(),
            cost_units: 0.1,
        },
        |_req: &Request| {
            Ok(serde_json::json!({
                "message": "Available capabilities: echo, help",
                "version": sinp_core::PROTOCOL_VERSION
            }))
        },
        0.99,
    );

    tracing::info!("Starting SINP server on {}", bind_addr);
    tracing::info!("Registered capabilities: {:?}", registry.capability_ids());

    // Create and run server
    let server = Server::new(config, registry)?;
    server.run().await
}
