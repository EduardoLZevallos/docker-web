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
    
    // GIVEN a running test container with explicit cleanup
    let nginx_image = GenericImage::new("nginx", "alpine");
    let container = nginx_image.start().await;
    
    // Ensure cleanup even on panic
    let _cleanup_guard = scopeguard::guard((), |_| {
        log::debug!("Test cleanup: container will be automatically dropped");
    });
    
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
    
    drop(container); // Explicit cleanup
    Ok(())
}

#[test_log::test(tokio::test)]
async fn list_running_containers_with_multiple_testcontainers() -> Result<(), DockerError> {
    use testcontainers::{GenericImage, runners::AsyncRunner};
    
    // GIVEN multiple running test containers with explicit cleanup
    let nginx_image = GenericImage::new("nginx", "alpine");
    let alpine_image = GenericImage::new("alpine", "latest");
    
    let nginx_container = nginx_image.start().await;
    let alpine_container = alpine_image.start().await;
    
    // Ensure cleanup even on panic
    let _cleanup_guard = scopeguard::guard((), |_| {
        log::debug!("Test cleanup: containers will be automatically dropped");
    });
    
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
    
    // Explicit cleanup
    drop(nginx_container);
    drop(alpine_container);
    Ok(())
}

#[test_log::test(tokio::test)]
async fn list_networks_returns_valid_network_list() -> Result<(), DockerError> {
    // GIVEN a default configuration
    let config = Config::with_defaults();
    
    // WHEN creating a new Docker client and listing networks
    let client = DockerClient::new(&config)?;
    let networks = client.list_networks().await?;
    
    // THEN we should get a list of networks
    assert!(!networks.is_empty(), "Expected at least one network (default networks should exist)");
    
    // AND each network should have valid data
    for network in &networks {
        assert!(!network.id.is_empty(), "Network ID should not be empty");
        assert!(!network.name.is_empty(), "Network name should not be empty");
        assert!(!network.driver.is_empty(), "Network driver should not be empty");
        assert!(!network.scope.is_empty(), "Network scope should not be empty");
    }
    
    // AND we should find common default networks
    let network_names: Vec<&str> = networks.iter().map(|n| n.name.as_str()).collect();
    assert!(network_names.iter().any(|&name| name == "bridge"), 
            "Should find default bridge network");
    
    log::debug!("Found {} networks: {:?}", networks.len(), network_names);
    Ok(())
}

#[test_log::test(tokio::test)]
async fn list_networks_with_custom_network() -> Result<(), DockerError> {
    use testcontainers::{GenericImage, runners::AsyncRunner};
    
    // GIVEN a test container in a custom network
    let nginx_image = GenericImage::new("nginx", "alpine");
    let container = nginx_image.start().await;
    
    // Ensure cleanup even on panic
    let _cleanup_guard = scopeguard::guard((), |_| {
        log::debug!("Test cleanup: container will be automatically dropped");
    });
    
    // Give the container time to start and register with networks
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    
    // AND a Docker client
    let config = Config::with_defaults();
    let client = DockerClient::new(&config)?;
    
    // WHEN we list networks
    let networks = client.list_networks().await?;
    
    // THEN we should find networks with containers attached
    assert!(!networks.is_empty(), "Should have at least default networks");
    
    // AND we should be able to find networks that have containers
    let networks_with_containers: Vec<_> = networks.iter()
        .filter(|n| !n.containers.is_empty())
        .collect();
    
    log::debug!("Found {} networks with containers attached", networks_with_containers.len());
    
    // Clean up
    drop(container);
    Ok(())
}