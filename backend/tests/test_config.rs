use docker_web::config::{Config, ConfigError};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tempfile::tempdir;
use validator::Validate;

// Helper function to create temporary config files for testing
fn create_test_config(content: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("config.yaml");
    let mut file = File::create(&file_path).unwrap();
    write!(file, "{}", content).unwrap();
    (dir, file_path)
}

#[test_log::test]
fn load_with_valid_config_file_returns_config_object() {
    let config_content = r#"
        discovery_interval: 30
        docker_socket_path: "/var/run/docker.sock"
        docker_timeout_seconds: 120
        bind_address: "127.0.0.1:8080"
        logging_level: "info"
        log_file_path: "/var/log/app.log"
        metrics_log_file_path: "/var/log/metrics.log"
        frontend_theme: "light"
        auth:
          username: "admin"
    "#;

    let (_dir, config_path) = create_test_config(config_content);
    let config = Config::load(config_path).unwrap();

    assert_eq!(config.discovery_interval, 30);
    assert_eq!(config.docker_socket_path, "/var/run/docker.sock");
    assert_eq!(config.logging_level, "info");
    assert_eq!(config.frontend_theme, "light");
    assert_eq!(config.auth.as_ref().unwrap().username, "admin");
    assert!(config.auth.as_ref().unwrap().password_hash.is_none());
    assert!(!config.demo_mode);
}

#[test_log::test]
fn load_with_invalid_discovery_interval_returns_validation_error() {
    let config_content = r#"
        discovery_interval: 1
        docker_socket_path: "/var/run/docker.sock"
        docker_timeout_seconds: 120
        bind_address: "127.0.0.1:8080"
        logging_level: "info"
        log_file_path: "/var/log/app.log"
        metrics_log_file_path: "/var/log/metrics.log"
        frontend_theme: "light"
        auth:
          username: "admin"
    "#;

    let (_dir, config_path) = create_test_config(config_content);
    let result = Config::load(config_path);
    assert!(matches!(result, Err(ConfigError::ValidationError(_))));
}

#[test_log::test]
fn load_with_invalid_log_level_returns_validation_error() {
    let config_content = r#"
        discovery_interval: 30
        docker_socket_path: "/var/run/docker.sock"
        docker_timeout_seconds: 120
        bind_address: "127.0.0.1:8080"
        logging_level: "invalid"
        log_file_path: "/var/log/app.log"
        metrics_log_file_path: "/var/log/metrics.log"
        frontend_theme: "light"
        auth:
          username: "admin"
    "#;

    let (_dir, config_path) = create_test_config(config_content);
    let result = Config::load(config_path);
    assert!(matches!(result, Err(ConfigError::ValidationError(_))));
}

#[test_log::test]
fn load_with_invalid_theme_returns_validation_error() {
    let config_content = r#"
        discovery_interval: 30
        docker_socket_path: "/var/run/docker.sock"
        docker_timeout_seconds: 120
        bind_address: "127.0.0.1:8080"
        logging_level: "info"
        log_file_path: "/var/log/app.log"
        metrics_log_file_path: "/var/log/metrics.log"
        frontend_theme: "blue"
        auth:
          username: "admin"
    "#;

    let (_dir, config_path) = create_test_config(config_content);
    let result = Config::load(config_path);
    assert!(matches!(result, Err(ConfigError::ValidationError(_))));
}

#[test_log::test]
fn load_with_empty_username_returns_validation_error() {
    let config_content = r#"
        discovery_interval: 30
        docker_socket_path: "/var/run/docker.sock"
        docker_timeout_seconds: 120
        bind_address: "127.0.0.1:8080"
        logging_level: "info"
        log_file_path: "/var/log/app.log"
        metrics_log_file_path: "/var/log/metrics.log"
        frontend_theme: "light"
        auth:
          username: ""
    "#;

    let (_dir, config_path) = create_test_config(config_content);
    let result = Config::load(config_path);
    assert!(matches!(result, Err(ConfigError::ValidationError(_))));
}

#[test_log::test]
fn with_defaults_when_called_returns_default_config() {
    let config = Config::with_defaults();
    assert_eq!(config.discovery_interval, 30);
    assert_eq!(config.docker_socket_path, "/var/run/docker.sock");
    assert_eq!(config.logging_level, "info");
    assert_eq!(config.frontend_theme, "light");
    assert!(config.auth.is_none());
    assert!(!config.demo_mode);
    
    // Validate that default config passes validation
    assert!(config.validate().is_ok());
}

#[test_log::test]
fn load_with_invalid_bind_address_returns_validation_error() {
    let config_content = r#"
        discovery_interval: 30
        docker_socket_path: "/var/run/docker.sock"
        docker_timeout_seconds: 120
        bind_address: "invalid_address"
        logging_level: "info"
        log_file_path: "/var/log/app.log"
        metrics_log_file_path: "/var/log/metrics.log"
        frontend_theme: "light"
        auth:
          username: "admin"
    "#;

    let (_dir, config_path) = create_test_config(config_content);
    let result = Config::load(config_path);
    assert!(matches!(result, Err(ConfigError::ValidationError(_))));
}

#[test_log::test]
fn load_with_non_existent_file_returns_file_error() {
    let result = Config::load("/nonexistent/config.yaml");
    assert!(matches!(result, Err(ConfigError::FileError(_))));
}

#[test_log::test]
fn load_with_invalid_yaml_returns_yaml_error() {
    let config_content = "invalid: yaml: content: [";
    let (_dir, config_path) = create_test_config(config_content);
    let result = Config::load(config_path);
    assert!(matches!(result, Err(ConfigError::YamlError(_))));
}