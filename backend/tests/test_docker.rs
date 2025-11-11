use std::time::Duration;
use docker_web::config::Config;
use docker_web::docker::{DockerClient, DockerError};
use test_log::test;
use tokio::time::sleep;

#[test_log::test(tokio::test)]
async fn list_running_containers_with_no_containers() -> Result<(), DockerError> {
    let config = Config::with_defaults();
    let client = DockerClient::new(&config)?;
    
    // List containers when none are running
    let containers = client.list_running_containers().await?;
    assert!(containers.is_empty(), "Expected no running containers");
    
    Ok(())
}

#[test_log::test(tokio::test)]
async fn list_running_containers_with_one_container() -> Result<(), DockerError> {
    let config = Config::with_defaults();
    let client = DockerClient::new(&config)?;
    
    // Start a test container using docker command
    let _ = std::process::Command::new("docker")
        .args(["run", "-d", "--rm", "--name", "test-container", "hello-world"])
        .output()
        .expect("Failed to start test container");
    
    // Give it a moment to start
    sleep(Duration::from_secs(1)).await;
    
    // List containers
    let containers = client.list_running_containers().await?;
    
    // Cleanup test container
    let _ = std::process::Command::new("docker")
        .args(["stop", "test-container"])
        .output()
        .expect("Failed to stop test container");
    
    assert_eq!(containers.len(), 1, "Expected one running container");
    assert_eq!(containers[0].name, "test-container");
    
    Ok(())
}

#[test_log::test(tokio::test)]
async fn new_client_with_invalid_socket() {
    let mut config = Config::with_defaults();
    config.docker_socket_path = String::from("/nonexistent/docker.sock");
    
    let result = DockerClient::new(&config);
    assert!(result.is_err(), "Expected error with invalid socket path");
    
    match result {
        Err(DockerError::ConnectionError(_)) => (),
        _ => panic!("Expected ConnectionError"),
    }
}