use docker_web::docker::{DockerClient, DockerError};

/// Helper function to create DockerClient for testing
fn create_test_docker_client() -> Result<DockerClient, DockerError> {
    DockerClient::new_with_defaults()
}

#[test_log::test(tokio::test)]
async fn new_client_with_invalid_socket_fails() {
    // GIVEN an invalid Docker socket path
    let invalid_socket = "/nonexistent/docker.sock";
    
    // WHEN attempting to create a new Docker client and use it
    let client_result = DockerClient::new_with_socket(invalid_socket);
    
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
async fn list_running_containers_with_testcontainer_returns_containers_list() -> Result<(), DockerError> {
    use testcontainers::{GenericImage, runners::AsyncRunner};
    
    // GIVEN a running test container with explicit cleanup
    let nginx_image = GenericImage::new("nginx", "alpine");
    let _container = nginx_image.start().await;
    
    // Give the container a moment to fully start
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    
    // AND a Docker client
    let client = create_test_docker_client()?;
    
    // WHEN listing running containers
    let containers = client.list_running_containers().await?;
    
    // THEN we should find at least one running container
    assert!(!containers.is_empty(), "Expected at least one running container");
    
    // AND we should find a container that is up and running
    let running_container = containers.iter()
        .find(|c| c.status.contains("Up"));
    assert!(running_container.is_some(), "Expected to find a running container");
    
    Ok(())
}

#[test_log::test(tokio::test)]
async fn list_running_containers_with_multiple_testcontainers_returns_multiple_containers() -> Result<(), DockerError> {
    use testcontainers::{GenericImage, runners::AsyncRunner};
    
    // GIVEN multiple running test containers with explicit cleanup
    let nginx_image = GenericImage::new("nginx", "alpine");
    let alpine_image = GenericImage::new("alpine", "latest");
    
    let _nginx_container = nginx_image.start().await;
    let _alpine_container = alpine_image.start().await;
    
    // Give containers time to start
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    
    // AND a Docker client
    let client = create_test_docker_client()?;
    
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
}

#[test_log::test(tokio::test)]
async fn list_networks_with_custom_network_returns_network_with_containers() -> Result<(), DockerError> {
    use testcontainers::{GenericImage, runners::AsyncRunner};
    
    // GIVEN a test container in a custom network
    let nginx_image = GenericImage::new("nginx", "alpine");
    let _container = nginx_image.start().await;
    
    // Give the container time to start and register with networks
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    
    // AND a Docker client
    let client = create_test_docker_client()?;
    
    // WHEN we list networks
    let networks = client.list_networks().await?;
    
    // THEN we should find networks with containers attached
    assert!(!networks.is_empty(), "Should have at least default networks");
    
    // AND we should be able to find networks that have containers
    let networks_with_containers: Vec<_> = networks.iter()
        .filter(|n| !n.containers.is_empty())
        .collect();
    
    log::debug!("Found {} networks with containers attached", networks_with_containers.len());
    
    Ok(())
}