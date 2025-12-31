//! Server configuration for SINP.

use sinp_core::Thresholds;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// Server configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Address to bind to.
    pub bind_addr: SocketAddr,
    /// Decision thresholds.
    pub thresholds: Thresholds,
    /// Replay window in milliseconds.
    pub replay_window_ms: i64,
    /// TLS configuration (optional for initial dev).
    pub tls: Option<TlsConfig>,
    /// Read timeout for connections.
    pub read_timeout: Duration,
    /// Write timeout for connections.
    pub write_timeout: Duration,
    /// Max message size in bytes.
    pub max_message_size: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:9000".parse().unwrap(),
            thresholds: Thresholds::default(),
            replay_window_ms: 5000,
            tls: None,
            read_timeout: Duration::from_secs(30),
            write_timeout: Duration::from_secs(30),
            max_message_size: 1024 * 1024, // 1MB
        }
    }
}

impl ServerConfig {
    /// Create a new config with custom bind address.
    pub fn with_addr(addr: impl Into<SocketAddr>) -> Self {
        Self {
            bind_addr: addr.into(),
            ..Default::default()
        }
    }

    /// Enable TLS with certificate and key files.
    pub fn with_tls(mut self, cert_path: PathBuf, key_path: PathBuf) -> Self {
        self.tls = Some(TlsConfig {
            cert_path,
            key_path,
        });
        self
    }

    /// Set custom thresholds.
    pub fn with_thresholds(mut self, thresholds: Thresholds) -> Self {
        self.thresholds = thresholds;
        self
    }
}

/// TLS configuration.
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// Path to certificate file (PEM).
    pub cert_path: PathBuf,
    /// Path to private key file (PEM).
    pub key_path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = ServerConfig::default();
        assert_eq!(config.bind_addr.port(), 9000);
        assert!(config.tls.is_none());
    }

    #[test]
    fn custom_config() {
        let config = ServerConfig::with_addr("0.0.0.0:8080".parse::<SocketAddr>().unwrap())
            .with_thresholds(Thresholds::new(0.9, 0.6, 0.6));

        assert_eq!(config.bind_addr.port(), 8080);
        assert_eq!(config.thresholds.tau_exec, 0.9);
    }
}
