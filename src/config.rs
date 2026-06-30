//! Configuration management for rmdadm
//! Supports loading from /etc/rmdadm.conf and environment variables

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{info, warn, debug};

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Server configuration
    #[serde(default)]
    pub server: ServerConfig,
    
    /// Authentication configuration
    #[serde(default)]
    pub auth: AuthConfig,
    
    /// Rate limiting configuration
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    
    /// Monitoring configuration
    #[serde(default)]
    pub monitoring: MonitoringConfig,
    
    /// Logging configuration
    #[serde(default)]
    pub logging: LoggingConfig,
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server bind address
    #[serde(default = "default_bind_address")]
    pub bind_address: String,
    
    /// Enable TLS
    #[serde(default)]
    pub enable_tls: bool,
    
    /// TLS certificate path
    pub tls_cert: Option<PathBuf>,
    
    /// TLS key path
    pub tls_key: Option<PathBuf>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: default_bind_address(),
            enable_tls: false,
            tls_cert: None,
            tls_key: None,
        }
    }
}

fn default_bind_address() -> String {
    "0.0.0.0:8080".to_string()
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Disable authentication (not recommended for production)
    #[serde(default)]
    pub disable_auth: bool,
    
    /// JWT secret key
    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: String,
    
    /// JWT token expiry in hours
    #[serde(default = "default_token_expiry")]
    pub token_expiry_hours: u64,
    
    /// API key for simple authentication
    pub api_key: Option<String>,
    
    /// Admin username
    #[serde(default = "default_admin_user")]
    pub admin_user: String,
    
    /// Admin password (should be changed in production)
    #[serde(default = "default_admin_password")]
    pub admin_password: String,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            disable_auth: false,
            jwt_secret: default_jwt_secret(),
            token_expiry_hours: default_token_expiry(),
            api_key: None,
            admin_user: default_admin_user(),
            admin_password: default_admin_password(),
        }
    }
}

fn default_jwt_secret() -> String {
    "change-me-in-production".to_string()
}

fn default_token_expiry() -> u64 {
    24
}

fn default_admin_user() -> String {
    "admin".to_string()
}

fn default_admin_password() -> String {
    "changeme".to_string()
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Enable rate limiting
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    /// Maximum requests per window
    #[serde(default = "default_max_requests")]
    pub max_requests: u32,
    
    /// Time window in seconds
    #[serde(default = "default_window_seconds")]
    pub window_seconds: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_requests: default_max_requests(),
            window_seconds: default_window_seconds(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_max_requests() -> u32 {
    100
}

fn default_window_seconds() -> u64 {
    60
}

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Enable background monitoring
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    /// Monitoring interval in seconds
    #[serde(default = "default_monitor_interval")]
    pub interval_seconds: u64,
    
    /// Webhook URL for alerts
    pub webhook_url: Option<String>,
    
    /// Email configuration for alerts
    pub email: Option<EmailConfig>,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: default_monitor_interval(),
            webhook_url: None,
            email: None,
        }
    }
}

fn default_monitor_interval() -> u64 {
    60
}

/// Email configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    /// SMTP server
    pub smtp_server: String,
    
    /// SMTP port
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,
    
    /// SMTP username
    pub smtp_username: String,
    
    /// SMTP password
    pub smtp_password: String,
    
    /// From address
    pub from_address: String,
    
    /// To addresses
    pub to_addresses: Vec<String>,
}

fn default_smtp_port() -> u16 {
    587
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub level: String,
    
    /// Log directory
    pub log_dir: Option<PathBuf>,
    
    /// Enable JSON logging
    #[serde(default)]
    pub json_format: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            log_dir: Some(PathBuf::from("/var/log/rmdadm")),
            json_format: false,
        }
    }
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Config {
    /// Load configuration from file and environment variables
    /// Environment variables take precedence over file configuration
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = PathBuf::from("/etc/rmdadm/rmdadm.conf");
        
        let mut config = if config_path.exists() {
            info!("Loading configuration from {}", config_path.display());
            Self::load_from_file(&config_path)?
        } else {
            debug!("Configuration file not found, using defaults");
            Self::default()
        };
        
        // Override with environment variables
        config.apply_env_overrides();
        
        // Validate configuration
        config.validate()?;
        
        Ok(config)
    }
    
    /// Load configuration from a specific file
    pub fn load_from_file(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }
    
    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        // Server
        if let Ok(addr) = std::env::var("RMDADM_BIND_ADDRESS") {
            self.server.bind_address = addr;
        }
        
        // Auth
        if std::env::var("RMDADM_DISABLE_AUTH").is_ok() {
            self.auth.disable_auth = true;
        }
        if let Ok(secret) = std::env::var("RMDADM_JWT_SECRET") {
            self.auth.jwt_secret = secret;
        }
        if let Ok(key) = std::env::var("RMDADM_API_KEY") {
            self.auth.api_key = Some(key);
        }
        if let Ok(user) = std::env::var("RMDADM_ADMIN_USER") {
            self.auth.admin_user = user;
        }
        if let Ok(pass) = std::env::var("RMDADM_ADMIN_PASSWORD") {
            self.auth.admin_password = pass;
        }
        
        // Rate limiting
        if std::env::var("RMDADM_DISABLE_RATE_LIMIT").is_ok() {
            self.rate_limit.enabled = false;
        }
        if let Ok(max) = std::env::var("RMDADM_RATE_LIMIT_MAX") {
            if let Ok(val) = max.parse() {
                self.rate_limit.max_requests = val;
            }
        }
        if let Ok(window) = std::env::var("RMDADM_RATE_LIMIT_WINDOW") {
            if let Ok(val) = window.parse() {
                self.rate_limit.window_seconds = val;
            }
        }
        
        // Monitoring
        if let Ok(url) = std::env::var("RMDADM_WEBHOOK_URL") {
            self.monitoring.webhook_url = Some(url);
        }
        if let Ok(interval) = std::env::var("RMDADM_MONITOR_INTERVAL") {
            if let Ok(val) = interval.parse() {
                self.monitoring.interval_seconds = val;
            }
        }
        
        // Logging
        if let Ok(level) = std::env::var("RUST_LOG") {
            self.logging.level = level;
        }
    }
    
    /// Validate configuration
    fn validate(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Warn about insecure defaults
        if self.auth.jwt_secret == "change-me-in-production" {
            warn!("⚠️  Using default JWT secret - change this in production!");
        }
        
        if self.auth.admin_password == "changeme" {
            warn!("⚠️  Using default admin password - change this in production!");
        }
        
        if self.auth.disable_auth {
            warn!("⚠️  Authentication is disabled - not recommended for production!");
        }
        
        // Validate TLS configuration
        if self.server.enable_tls {
            if self.server.tls_cert.is_none() || self.server.tls_key.is_none() {
                return Err("TLS enabled but certificate or key path not specified".into());
            }
        }
        
        Ok(())
    }
    
    /// Get rate limit duration
    pub fn rate_limit_duration(&self) -> Duration {
        Duration::from_secs(self.rate_limit.window_seconds)
    }
    
    /// Get monitoring interval duration
    pub fn monitor_interval(&self) -> Duration {
        Duration::from_secs(self.monitoring.interval_seconds)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            auth: AuthConfig::default(),
            rate_limit: RateLimitConfig::default(),
            monitoring: MonitoringConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.bind_address, "0.0.0.0:8080");
        assert!(!config.auth.disable_auth);
        assert!(config.rate_limit.enabled);
    }
    
    #[test]
    fn test_load_from_yaml() {
        let yaml = r#"
server:
  bind_address: "127.0.0.1:9090"
auth:
  jwt_secret: "test-secret"
  admin_user: "testadmin"
rate_limit:
  max_requests: 50
"#;
        
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();
        
        let config = Config::load_from_file(file.path()).unwrap();
        assert_eq!(config.server.bind_address, "127.0.0.1:9090");
        assert_eq!(config.auth.jwt_secret, "test-secret");
        assert_eq!(config.rate_limit.max_requests, 50);
    }
    
    #[test]
    fn test_env_overrides() {
        std::env::set_var("RMDADM_BIND_ADDRESS", "0.0.0.0:7070");
        std::env::set_var("RMDADM_JWT_SECRET", "env-secret");
        
        let mut config = Config::default();
        config.apply_env_overrides();
        
        assert_eq!(config.server.bind_address, "0.0.0.0:7070");
        assert_eq!(config.auth.jwt_secret, "env-secret");
        
        std::env::remove_var("RMDADM_BIND_ADDRESS");
        std::env::remove_var("RMDADM_JWT_SECRET");
    }
}
