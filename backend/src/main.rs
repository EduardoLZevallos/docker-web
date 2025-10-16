mod config;

use config::{Config, ConfigError};
use std::path::PathBuf;
use env_logger::Env;

fn main() -> Result<(), ConfigError> {
    // Initialize the logger with default level 'info'
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    // Load configuration from the project root directory
    let config_path = PathBuf::from("../config/config.yaml");
    let config = Config::load(config_path)?;

    println!("Configuration loaded successfully:");
    println!("Discovery interval: {} seconds", config.discovery_interval);
    println!("Log level: {}", config.logging_level);
    println!("Theme: {}", config.frontend_theme);

    Ok(())
}
