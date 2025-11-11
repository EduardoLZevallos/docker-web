use docker_web::config::Config;
use docker_web::docker::{DockerClient, DockerError};
use test_log::test;
use testcontainers::{clients::Cli, images::generic::GenericImage, Container};

#[test_log::test(tokio::test)]
async fn new_client_with_default_config_connects_successfully() -> Result<(), DockerError> {
    // GIVEN a default configuration
    let config = Config::with_defaults();
    
    // WHEN creating a new Docker client
    let client = DockerClient::new(&config)?;
    
    // AND listing running containers
    let containers = client.list_running_containers().await?;
    
    // THEN the call should succeed and return a valid container list
    assert!(containers.len() >= 0, "Should return a valid container list");
    
    Ok(())
}

#[test_log::test(tokio::test)]
async fn list_running_containers_with_testcontainer() -> Result<(), DockerError> {
    // GIVEN a running test container
    let docker = Cli::default();
    let nginx_image = GenericImage::new("nginx", "alpine")
        .with_exposed_port(80);
    let _container: Container<'_, GenericImage> = docker.run(nginx_image);
    
    // Give the container a moment to fully start
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    
    // AND a Docker client
    let config = Config::with_defaults();
    let client = DockerClient::new(&config)?;
    
    // WHEN listing running containers
    let containers = client.list_running_containers().await?;
    
    // THEN we should find at least one running container
    assert!(!containers.is_empty(), "Expected at least one running container");
    
    // AND we should find a container that is up and running
    let running_container = containers.iter()
        .find(|c| c.status.contains("Up"));
    assert!(running_container.is_some(), "Expected to find a running container");
    
    Ok(())
    // Container is automatically cleaned up when it goes out of scope
}

#[test_log::test(tokio::test)]
async fn list_running_containers_with_multiple_testcontainers() -> Result<(), DockerError> {
    // GIVEN multiple running test containers
    let docker = Cli::default();
    let nginx_image = GenericImage::new("nginx", "alpine").with_exposed_port(80);
    let alpine_image = GenericImage::new("alpine", "latest")
        .with_cmd(vec!["sleep", "30"]);
    
    let _nginx_container: Container<'_, GenericImage> = docker.run(nginx_image);
    let _alpine_container: Container<'_, GenericImage> = docker.run(alpine_image);
    
    // Give containers time to start
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    
    // AND a Docker client
    let config = Config::with_defaults();
    let client = DockerClient::new(&config)?;
    
    // WHEN listing running containers
    let containers = client.list_running_containers().await?;
    
    // THEN we should find at least our test containers
    assert!(containers.len() >= 2, "Expected at least two running containers, found {}", containers.len());
    
    // AND all containers should have valid data
    for container in &containers {
        assert!(!container.id.is_empty(), "Container ID should not be empty");
        assert!(!container.name.is_empty(), "Container name should not be empty");
        assert!(!container.status.is_empty(), "Container status should not be empty");
        assert!(container.created > 0, "Container created timestamp should be positive");
    }
    
    Ok(())
    // Containers are automatically cleaned up when they go out of scope
}

#[test_log::test(tokio::test)]
async fn new_client_with_invalid_socket_fails() {
    // GIVEN a configuration with an invalid Docker socket path
    let mut config = Config::with_defaults();
    config.docker_socket_path = String::from("/nonexistent/docker.sock");
    
    // WHEN attempting to create a new Docker client
    let result = DockerClient::new(&config);
    
    // THEN the client creation should fail
    assert!(result.is_err(), "Expected error with invalid socket path");
    
    // AND the error should be a connection error
    match result {
        Err(DockerError::ConnectionError(_)) => (),
        _ => panic!("Expected ConnectionError"),
    }
}