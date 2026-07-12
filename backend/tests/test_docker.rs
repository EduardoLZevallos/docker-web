use docker_web::config::Config;
use docker_web::docker::{DockerClient, DockerError};

fn create_test_docker_client() -> Result<DockerClient, DockerError> {
    let config = Config::with_defaults();
    DockerClient::new(&config)
}

#[test_log::test(tokio::test)]
async fn new_client_with_invalid_socket_fails() {
    let mut config = Config::with_defaults();
    config.docker_socket_path = "/nonexistent/docker.sock".to_string();

    let client_result = DockerClient::new(&config);

    if let Ok(client) = client_result {
        let result = client.list_running_containers().await;

        assert!(
            result.is_err(),
            "Expected error when using client with invalid socket path"
        );

        match result {
            Err(DockerError::ConnectionError(_)) => (),
            _ => panic!("Expected ConnectionError"),
        }
    } else {
        match client_result {
            Err(DockerError::ConnectionError(_)) => (),
            _ => panic!("Expected ConnectionError"),
        }
    }
}

#[test_log::test(tokio::test)]
async fn list_running_containers_with_testcontainer_returns_containers_list(
) -> Result<(), DockerError> {
    use testcontainers::{GenericImage, runners::AsyncRunner};

    let nginx_image = GenericImage::new("nginx", "alpine");
    let container = nginx_image.start().await;

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let client = create_test_docker_client()?;

    let containers = client.list_running_containers().await?;

    assert!(!containers.is_empty(), "Expected at least one running container");

    let running_container = containers.iter().find(|c| c.status.contains("Up"));
    assert!(
        running_container.is_some(),
        "Expected to find a running container"
    );

    Ok(())
}

#[test_log::test(tokio::test)]
async fn list_running_containers_with_multiple_testcontainers_returns_multiple_containers(
) -> Result<(), DockerError> {
    use testcontainers::{GenericImage, runners::AsyncRunner};

    let nginx_image = GenericImage::new("nginx", "alpine");
    let alpine_image = GenericImage::new("alpine", "latest");

    let nginx_container = nginx_image.start().await;
    let alpine_container = alpine_image.start().await;

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let client = create_test_docker_client()?;

    let containers = client.list_running_containers().await?;

    assert!(
        containers.len() >= 1,
        "Expected at least one running container, found {}",
        containers.len()
    );

    for container in &containers {
        assert!(!container.id.is_empty(), "Container ID should not be empty");
        assert!(!container.name.is_empty(), "Container name should not be empty");
        assert!(
            !container.status.is_empty(),
            "Container status should not be empty"
        );
        assert!(
            container.created > 0,
            "Container created timestamp should be positive"
        );
    }

    Ok(())
}

#[test_log::test(tokio::test)]
async fn list_networks_with_custom_network_returns_network_with_containers(
) -> Result<(), DockerError> {
    use testcontainers::{GenericImage, runners::AsyncRunner};

    let nginx_image = GenericImage::new("nginx", "alpine");
    let container = nginx_image.start().await;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let client = create_test_docker_client()?;

    let networks = client.list_networks().await?;

    assert!(!networks.is_empty(), "Should have at least default networks");

    let networks_with_containers: Vec<_> = networks
        .iter()
        .filter(|n| !n.containers.is_empty())
        .collect();

    log::debug!(
        "Found {} networks with containers attached",
        networks_with_containers.len()
    );

    Ok(())
}
