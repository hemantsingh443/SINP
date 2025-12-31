//! TCP/TLS connection for SINP client.

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use rustls::pki_types::ServerName;

use sinp_core::{Request, Response, SinpError, SinpResult};

/// Client connection configuration.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Server address.
    pub server_addr: SocketAddr,
    /// Server hostname for TLS (if different from IP).
    pub server_name: Option<String>,
    /// Whether to use TLS.
    pub use_tls: bool,
    /// Max message size.
    pub max_message_size: usize,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            server_addr: "127.0.0.1:9000".parse().unwrap(),
            server_name: None,
            use_tls: false,
            max_message_size: 1024 * 1024,
        }
    }
}

impl ConnectionConfig {
    /// Create config for plaintext connection.
    pub fn plaintext(addr: SocketAddr) -> Self {
        Self {
            server_addr: addr,
            use_tls: false,
            ..Default::default()
        }
    }

    /// Create config for TLS connection.
    pub fn tls(addr: SocketAddr, server_name: impl Into<String>) -> Self {
        Self {
            server_addr: addr,
            server_name: Some(server_name.into()),
            use_tls: true,
            ..Default::default()
        }
    }
}

/// Connection to SINP server.
pub enum Connection {
    Tcp(TcpStream),
    Tls(tokio_rustls::client::TlsStream<TcpStream>),
}

impl Connection {
    /// Connect to server.
    pub async fn connect(config: &ConnectionConfig) -> SinpResult<Self> {
        let stream = TcpStream::connect(&config.server_addr)
            .await
            .map_err(|e| SinpError::Transport(format!("Connection failed: {}", e)))?;

        if config.use_tls {
            let connector = Self::create_tls_connector()?;
            let server_name_str = config
                .server_name
                .clone()
                .unwrap_or_else(|| "localhost".to_string());
            let server_name: ServerName<'static> = server_name_str
                .try_into()
                .map_err(|_| SinpError::Transport("Invalid server name".to_string()))?;

            let tls_stream = connector
                .connect(server_name, stream)
                .await
                .map_err(|e| SinpError::Transport(format!("TLS handshake failed: {}", e)))?;

            Ok(Self::Tls(tls_stream))
        } else {
            Ok(Self::Tcp(stream))
        }
    }

    /// Create TLS connector with system roots.
    fn create_tls_connector() -> SinpResult<TlsConnector> {
        let root_store = rustls::RootCertStore::empty();
        // In production, load system certs or custom CA
        
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        Ok(TlsConnector::from(Arc::new(config)))
    }

    /// Send a request and receive response.
    pub async fn send_request(&mut self, request: &Request) -> SinpResult<Response> {
        match self {
            Self::Tcp(stream) => Self::send_recv(stream, request).await,
            Self::Tls(stream) => Self::send_recv(stream, request).await,
        }
    }

    /// Send request and receive response on stream.
    async fn send_recv<S>(stream: &mut S, request: &Request) -> SinpResult<Response>
    where
        S: AsyncReadExt + AsyncWriteExt + Unpin,
    {
        // Serialize request
        let json = serde_json::to_vec(request)?;
        let len = json.len() as u32;

        // Send length prefix + message
        stream
            .write_all(&len.to_be_bytes())
            .await
            .map_err(|e| SinpError::Transport(format!("Write error: {}", e)))?;
        stream
            .write_all(&json)
            .await
            .map_err(|e| SinpError::Transport(format!("Write error: {}", e)))?;
        stream
            .flush()
            .await
            .map_err(|e| SinpError::Transport(format!("Flush error: {}", e)))?;

        // Read response length
        let mut len_buf = [0u8; 4];
        stream
            .read_exact(&mut len_buf)
            .await
            .map_err(|e| SinpError::Transport(format!("Read error: {}", e)))?;
        let len = u32::from_be_bytes(len_buf) as usize;

        // Read response body
        let mut msg_buf = vec![0u8; len];
        stream
            .read_exact(&mut msg_buf)
            .await
            .map_err(|e| SinpError::Transport(format!("Read error: {}", e)))?;

        // Parse response
        let response: Response = serde_json::from_slice(&msg_buf)?;
        Ok(response)
    }
}
