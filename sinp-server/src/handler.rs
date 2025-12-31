//! TCP/TLS connection handler for SINP server.

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

use sinp_core::{Request, Response, SinpError, SinpResult};

use crate::capability::CapabilityRegistry;
use crate::config::ServerConfig;
use crate::state_machine::ServerStateMachine;

/// SINP Server.
pub struct Server {
    config: ServerConfig,
    registry: Arc<CapabilityRegistry>,
    tls_acceptor: Option<TlsAcceptor>,
}

impl Server {
    /// Create a new server.
    pub fn new(config: ServerConfig, registry: CapabilityRegistry) -> SinpResult<Self> {
        let tls_acceptor = if let Some(ref tls_config) = config.tls {
            Some(Self::create_tls_acceptor(tls_config)?)
        } else {
            None
        };

        Ok(Self {
            config,
            registry: Arc::new(registry),
            tls_acceptor,
        })
    }

    /// Create TLS acceptor from config.
    fn create_tls_acceptor(tls_config: &crate::config::TlsConfig) -> SinpResult<TlsAcceptor> {
        use rustls_pemfile::{certs, private_key};
        use std::fs::File;
        use std::io::BufReader;

        let cert_file = File::open(&tls_config.cert_path)
            .map_err(|e| SinpError::Transport(format!("Failed to open cert: {}", e)))?;
        let key_file = File::open(&tls_config.key_path)
            .map_err(|e| SinpError::Transport(format!("Failed to open key: {}", e)))?;

        let certs: Vec<_> = certs(&mut BufReader::new(cert_file))
            .filter_map(|r| r.ok())
            .collect();

        let key = private_key(&mut BufReader::new(key_file))
            .map_err(|e| SinpError::Transport(format!("Failed to read key: {}", e)))?
            .ok_or_else(|| SinpError::Transport("No private key found".to_string()))?;

        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| SinpError::Transport(format!("TLS config error: {}", e)))?;

        Ok(TlsAcceptor::from(Arc::new(config)))
    }

    /// Run the server.
    pub async fn run(self) -> SinpResult<()> {
        let listener = TcpListener::bind(&self.config.bind_addr)
            .await
            .map_err(|e| SinpError::Transport(format!("Failed to bind: {}", e)))?;

        tracing::info!("SINP server listening on {}", self.config.bind_addr);

        loop {
            let (stream, addr) = listener
                .accept()
                .await
                .map_err(|e| SinpError::Transport(format!("Accept failed: {}", e)))?;

            tracing::debug!("Connection from {}", addr);

            let registry = Arc::clone(&self.registry);
            let config = self.config.clone();
            let tls_acceptor = self.tls_acceptor.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::handle_connection(stream, config, registry, tls_acceptor).await
                {
                    tracing::error!("Connection error from {}: {}", addr, e);
                }
            });
        }
    }

    /// Handle a single connection.
    async fn handle_connection(
        stream: TcpStream,
        config: ServerConfig,
        registry: Arc<CapabilityRegistry>,
        tls_acceptor: Option<TlsAcceptor>,
    ) -> SinpResult<()> {
        if let Some(acceptor) = tls_acceptor {
            let tls_stream = acceptor
                .accept(stream)
                .await
                .map_err(|e| SinpError::Transport(format!("TLS handshake failed: {}", e)))?;
            Self::handle_stream(tls_stream, config, registry).await
        } else {
            Self::handle_stream(stream, config, registry).await
        }
    }

    /// Handle message stream.
    async fn handle_stream<S>(
        mut stream: S,
        config: ServerConfig,
        registry: Arc<CapabilityRegistry>,
    ) -> SinpResult<()>
    where
        S: AsyncReadExt + AsyncWriteExt + Unpin,
    {
        let mut state_machine = ServerStateMachine::new(config.clone());
        let mut buf = vec![0u8; 4];

        loop {
            // Read length prefix (4 bytes, big-endian)
            match stream.read_exact(&mut buf).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    tracing::debug!("Client disconnected");
                    break;
                }
                Err(e) => return Err(SinpError::Transport(format!("Read error: {}", e))),
            }

            let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;

            if len > config.max_message_size {
                return Err(SinpError::Validation(format!(
                    "Message too large: {} > {}",
                    len, config.max_message_size
                )));
            }

            // Read message body
            let mut msg_buf = vec![0u8; len];
            stream
                .read_exact(&mut msg_buf)
                .await
                .map_err(|e| SinpError::Transport(format!("Read error: {}", e)))?;

            // Parse request
            let request: Request = serde_json::from_slice(&msg_buf)?;
            tracing::debug!("Received request: {:?}", request.message_id);

            // Process request
            let response = match state_machine.process_request(&request, &registry) {
                Ok(resp) => resp,
                Err(e) => {
                    tracing::error!("Processing error: {}", e);
                    // Send error response
                    let error_response = create_error_response(&request, &e);
                    send_response(&mut stream, &error_response).await?;
                    state_machine.reset();
                    continue;
                }
            };

            // Send response
            send_response(&mut stream, &response).await?;

            // Reset for next conversation if done
            if state_machine.state().is_terminal() {
                state_machine.reset();
            }
        }

        Ok(())
    }
}

/// Send a response message.
async fn send_response<S>(stream: &mut S, response: &Response) -> SinpResult<()>
where
    S: AsyncWriteExt + Unpin,
{
    let json = serde_json::to_vec(response)?;
    let len = json.len() as u32;

    // Write length prefix
    stream
        .write_all(&len.to_be_bytes())
        .await
        .map_err(|e| SinpError::Transport(format!("Write error: {}", e)))?;

    // Write message body
    stream
        .write_all(&json)
        .await
        .map_err(|e| SinpError::Transport(format!("Write error: {}", e)))?;

    stream
        .flush()
        .await
        .map_err(|e| SinpError::Transport(format!("Flush error: {}", e)))?;

    Ok(())
}

/// Create an error response.
fn create_error_response(request: &Request, error: &SinpError) -> Response {
    use sinp_core::{Action, ActionMetadata, Interpretation, RefusalCode, Responder};

    Response {
        message_id: uuid::Uuid::new_v4(),
        in_response_to: request.message_id,
        conversation_id: request.conversation_id,
        timestamp: chrono::Utc::now(),
        responder: Responder {
            id: "sinp-server".to_string(),
            capabilities: vec![],
        },
        interpretation: Interpretation {
            text: "Error processing request".to_string(),
            confidence: 0.0,
        },
        action: Action::Refuse,
        action_metadata: Some(ActionMetadata {
            reason_code: Some(RefusalCode::MalformedContext),
            reason: Some(error.to_string()),
            ..Default::default()
        }),
        alternatives: None,
        confidence: 0.0,
    }
}
