use serde::{Deserialize, Serialize};
use thiserror::Error;
use validator::{Validate, ValidationError};
use std::path::PathBuf;
use std::fs;
use std::str::FromStr;
use log::{debug, info, error};

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read configuration file: {0}")]
    FileError(#[from] std::io::Error),
    #[error("Failed to parse YAML: {0}")]
    YamlError(#[from] serde_yaml::Error),
    #[error("Invalid configuration: {0}")]
    ValidationError(String),
}

#[derive(Debug, Serialize, Deserialize, Validate, Clone)]
pub struct Auth {
    #[validate(length(min = 1, message = "Username cannot be empty"))]
    pub username: String,
    pub password_hash: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate, Clone)]
pub struct Config {
    #[validate(range(min = 5, max = 3600, message = "Discovery interval must be between 5 and 3600 seconds"))]
    pub discovery_interval: u32,

    #[validate(custom = "validate_socket_path")]
    pub docker_socket_path: String,

    #[validate(range(min = 1, max = 300, message = "Docker timeout must be between 1 and 300 seconds"))]
    pub docker_timeout_seconds: u64,

    #[validate(custom = "validate_bind_address")]
    pub bind_address: String,

    pub reverse_proxy_url: Option<String>,

    #[validate(custom = "validate_log_level")]
    pub logging_level: String,

    #[validate(custom = "validate_file_path")]
    pub log_file_path: String,

    #[validate(custom = "validate_file_path")]
    pub metrics_log_file_path: String,

    #[validate(custom = "validate_theme")]
    pub frontend_theme: String,

    #[validate]
    pub auth: Auth,
}

fn validate_socket_path(path: &str) -> Result<(), ValidationError> {
    if path.is_empty() {
        return Err(ValidationError::new("Socket path cannot be empty"));
    }
    Ok(())
}

fn validate_bind_address(address: &str) -> Result<(), ValidationError> {
    if address.is_empty() {
        return Err(ValidationError::new("Bind address cannot be empty"));
    }
    
    // Use std::net::SocketAddr for proper validation
    if std::net::SocketAddr::from_str(address).is_err() {
        return Err(ValidationError::new("Invalid socket address format (expected IP:PORT)"));
    }
    
    Ok(())
}

fn validate_log_level(level: &str) -> Result<(), ValidationError> {
    if level.is_empty() {
        return Err(ValidationError::new("Log level cannot be empty"));
    }
    match level.to_lowercase().as_str() {
        "debug" | "info" | "warn" | "error" => Ok(()),
        _ => Err(ValidationError::new("Invalid log level")),
    }
}

fn validate_file_path(path: &str) -> Result<(), ValidationError> {
    if path.is_empty() {
        return Err(ValidationError::new("File path cannot be empty"));
    }
    Ok(())
}

fn validate_theme(theme: &str) -> Result<(), ValidationError> {
    if theme.is_empty() {
        return Err(ValidationError::new("Theme cannot be empty"));
    }
    match theme.to_lowercase().as_str() {
        "light" | "dark" => Ok(()),
        _ => Err(ValidationError::new("Theme must be either 'light' or 'dark'")),
    }
}

impl Config {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, ConfigError> {
        let path = path.into();
        debug!("Attempting to load configuration from: {:?}", path);
        
        // Read the file contents
        let contents = match fs::read_to_string(&path) {
            Ok(content) => {
                info!("Successfully read configuration file");
                content
            },
            Err(e) => {
                error!("Failed to read configuration file: {}", e);
                return Err(ConfigError::FileError(e));
            }
        };
        
        // Parse YAML content
        let config: Config = match serde_yaml::from_str(&contents) {
            Ok(cfg) => {
                info!("Successfully parsed YAML configuration");
                cfg
            },
            Err(e) => {
                error!("Failed to parse YAML: {}", e);
                return Err(ConfigError::YamlError(e));
            }
        };

        // Validate configuration
        debug!("Validating configuration...");
        if let Err(errors) = config.validate() {
            let err_msg = errors.to_string();
            error!("Configuration validation failed: {}", err_msg);
            return Err(ConfigError::ValidationError(err_msg));
        }

        info!("Configuration loaded and validated successfully");
        debug!("Config details: {:?}", config);
        Ok(config)
    }

    pub fn with_defaults() -> Self {
        Self {
            discovery_interval: 30,
            docker_socket_path: std::env::var("DOCKER_SOCKET_PATH")
                .unwrap_or_else(|_| "/var/run/docker.sock".to_string()),
            docker_timeout_seconds: std::env::var("DOCKER_TIMEOUT")
                .ok()
                .and_then(|t| t.parse().ok())
                .unwrap_or(120),
            bind_address: std::env::var("BIND_ADDRESS").unwrap_or_else(|_| "127.0.0.1:8080".to_string()),
            reverse_proxy_url: None,
            logging_level: String::from("info"),
            log_file_path: String::from("/var/log/docker-web.log"),
            metrics_log_file_path: String::from("/var/log/docker-web-metrics.log"),
            frontend_theme: String::from("light"),
            auth: Auth {
                username: String::from("admin"),
                password_hash: None,
            },
        }
    }
}