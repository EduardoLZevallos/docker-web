use docker_web::config::Config;
use docker_web::docker::{DockerClient, DockerError};

#[test_log::test(tokio::test)]
async fn new_client_with_default_config_connects_successfully() -> Result<(), DockerError> {
    // GIVEN a default configuration
    let config = Config::with_defaults();
    
    // WHEN creating a new Docker client
    let client = DockerClient::new(&config)?;
    
    // AND listing running containers
    let containers = client.list_running_containers().await?;
    
    // THEN the call should succeed and return a valid container list
    // (Just getting here without error means the connection worked)
    let _container_count = containers.len();
    
    Ok(())
}

#[test_log::test(tokio::test)]
async fn new_client_with_invalid_socket_fails() {
    // GIVEN a configuration with an invalid Docker socket path
    let mut config = Config::with_defaults();
    config.docker_socket_path = String::from("/nonexistent/docker.sock");
    
    // WHEN attempting to create a new Docker client and use it
    let client_result = DockerClient::new(&config);
    
    // Client creation might succeed but usage should fail
    if let Ok(client) = client_result {
        let result = client.list_running_containers().await;
        
        // THEN the container listing should fail
        assert!(result.is_err(), "Expected error when using client with invalid socket path");
        
        // AND the error should be a connection error
        match result {
            Err(DockerError::ConnectionError(_)) => (),
            _ => panic!("Expected ConnectionError"),
        }
    } else {
        // If client creation fails immediately, that's also valid
        match client_result {
            Err(DockerError::ConnectionError(_)) => (),
            _ => panic!("Expected ConnectionError"),
        }
    }
}

#[test_log::test(tokio::test)]
async fn list_running_containers_with_testcontainer() -> Result<(), DockerError> {
    use testcontainers::{GenericImage, runners::AsyncRunner};
    
    // GIVEN a running test container
    let nginx_image = GenericImage::new("nginx", "alpine");
    let _container = nginx_image.start().await;
    
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
    use testcontainers::{GenericImage, runners::AsyncRunner};
    
    // GIVEN multiple running test containers
    let nginx_image = GenericImage::new("nginx", "alpine");
    let alpine_image = GenericImage::new("alpine", "latest");
    
    let _nginx_container = nginx_image.start().await;
    let _alpine_container = alpine_image.start().await;
    
    // Give containers time to start
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    
    // AND a Docker client
    let config = Config::with_defaults();
    let client = DockerClient::new(&config)?;
    
    // WHEN listing running containers
    let containers = client.list_running_containers().await?;
    
    // THEN we should find at least one running container (nginx should be running)
    assert!(containers.len() >= 1, "Expected at least one running container, found {}", containers.len());
    
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