//! Quick test client for SINP server

use sinp_client::SinpClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔌 Connecting to SINP server at 127.0.0.1:8080...");

    let mut client = SinpClient::connect("127.0.0.1:8080").await?;
    println!(" Connected!\n");

    println!(" Sending: 'echo hello world'");
    let result = client.send_intent("echo hello world", 0.90).await?;
    
    println!(" Response: {:?}", result);
    
    if let Some(value) = client.get_result() {
        println!("\n Result: {}", serde_json::to_string_pretty(&value)?);
    }

    Ok(())
}
