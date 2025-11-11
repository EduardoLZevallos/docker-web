use bollard::container::ListContainersOptions;
use bollard::Docker;
use thiserror::Error;

use crate::config::Config;

#[derive(Debug, Error)]
pub enum DockerError {
    #[error("Failed to connect to Docker daemon: {0}")]
    ConnectionError(#[from] bollard::errors::Error),
    
    #[error("Container operation failed: {0}")]
    ContainerError(String),
}

/// Client for interacting with the Docker daemon
pub struct DockerClient {
    client: Docker,
}

impl DockerClient {
    /// Create a new Docker client using the configuration
    pub fn new(config: &Config) -> Result<Self, DockerError> {
        let client = Docker::connect_with_socket(&config.docker_socket_path, 120, bollard::API_DEFAULT_VERSION)?;
        Ok(DockerClient { client })
    }

    /// List all running containers
    pub async fn list_running_containers(&self) -> Result<Vec<ContainerInfo>, DockerError> {
        let options = Some(ListContainersOptions::<String> {
            all: false,  // Only running containers
            ..Default::default()
        });

        let containers = self.client.list_containers(options).await?;
        
        // Convert to our domain model
        let container_infos = containers
            .into_iter()
            .map(|c| ContainerInfo {
                id: c.id.unwrap_or_default(),
                name: c.names
                    .unwrap_or_default()
                    .first()
                    .cloned()
                    .unwrap_or_default()
                    .trim_start_matches('/')
                    .to_string(),
                status: c.status.unwrap_or_default(),
                created: c.created.unwrap_or_default(),
            })
            .collect();

        Ok(container_infos)
    }
}

/// Domain model for container information
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created: i64,
}