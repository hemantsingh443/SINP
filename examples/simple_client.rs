//! Simple client example for SINP.
//!
//! Run with: cargo run --example simple_client
//!
//! Make sure the server is running first:
//!   cargo run --example echo_server

use sinp_client::{NextAction, SinpClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    let server_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:9000".to_string());

    println!(" Connecting to SINP server at {}...", server_addr);

    let mut client = SinpClient::connect(&server_addr).await?;
    println!(" Connected!\n");

    // Example 1: Simple echo
    println!(" Sending: 'Hello, SINP server!'");
    let result = client.send_intent("Hello, SINP server!", 0.90).await?;
    handle_result(&result);

    // Reset for new conversation
    client.reset();

    // Example 2: Reverse text
    println!("\n Sending: 'reverse this text please'");
    let result = client.send_intent("reverse this text please", 0.85).await?;
    handle_result(&result);

    // Reset for new conversation
    client.reset();

    // Example 3: Uppercase
    println!("\n Sending: 'make uppercase hello world'");
    let result = client.send_intent("make uppercase hello world", 0.85).await?;
    handle_result(&result);

    // Example 4: Unclear intent (should trigger CLARIFY)
    client.reset();
    println!("\n Sending: 'do something' (vague intent)");
    let result = client.send_intent("do something", 0.50).await?;
    
    match &result {
        NextAction::Clarify { questions, .. } => {
            println!(" Server needs clarification:");
            for q in questions {
                println!("   - {}", q);
            }
            
            // Respond with clarification
            println!("\n Responding with clarification: 'echo my message'");
            let result = client.respond_to_clarify("echo my message please", 0.90).await?;
            handle_result(&result);
        }
        _ => handle_result(&result),
    }

    println!("\n Examples complete!");
    Ok(())
}

fn handle_result(result: &NextAction) {
    match result {
        NextAction::Done(response) => {
            println!("   Intent satisfied!");
            println!("   Interpretation: {}", response.interpretation.text);
            println!("   Confidence: {:.2}", response.confidence);
            if let Some(ref metadata) = response.action_metadata {
                if let Some(ref result) = metadata.result {
                    println!("   Result: {}", serde_json::to_string_pretty(result).unwrap());
                }
            }
        }
        NextAction::Clarify { questions, response } => {
            println!(" Server needs clarification (confidence: {:.2}):", response.confidence);
            for q in questions {
                println!("   - {}", q);
            }
        }
        NextAction::Propose { alternatives, response } => {
            println!(" Server proposes alternatives (confidence: {:.2}):", response.confidence);
            for alt in alternatives {
                println!("   - {} (conf: {:.2})", alt.interpretation, alt.confidence);
            }
        }
        NextAction::Refused { reason, response } => {
            println!(" Request refused (confidence: {:.2}):", response.confidence);
            println!("   Reason: {}", reason);
        }
    }
}
