use bollard::container::ListContainersOptions;
use bollard::network::ListNetworksOptions;
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
                image: c.image.unwrap_or_default(),
                status: c.status.unwrap_or_default(),
                created: c.created.unwrap_or_default(),
                ports: c.ports
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| {
                        let private = p.private_port.to_string();
                        if let Some(public) = p.public_port {
                            format!("{}:{}", public, private)
                        } else {
                            private
                        }
                    })
                    .collect(),
                networks: c.network_settings
                    .and_then(|ns| ns.networks)
                    .unwrap_or_default()
                    .into_keys()
                    .collect(),
            })
            .collect();

        Ok(container_infos)
    }

    /// Get a specific container by ID
    pub async fn get_container_by_id(&self, container_id: &str) -> Result<Option<ContainerInfo>, DockerError> {
        let options = Some(ListContainersOptions::<String> {
            all: true,  // Include stopped containers
            filters: {
                let mut filters = std::collections::HashMap::new();
                filters.insert("id".to_string(), vec![container_id.to_string()]);
                filters
            },
            ..Default::default()
        });

        let containers = self.client.list_containers(options).await?;
        
        // Convert and return the first (should be only) match
        let container_info = containers
            .into_iter()
            .next()
            .map(|c| ContainerInfo {
                id: c.id.unwrap_or_default(),
                name: c.names
                    .unwrap_or_default()
                    .first()
                    .cloned()
                    .unwrap_or_default()
                    .trim_start_matches('/')
                    .to_string(),
                image: c.image.unwrap_or_default(),
                status: c.status.unwrap_or_default(),
                created: c.created.unwrap_or_default(),
                ports: c.ports.unwrap_or_default().into_iter().map(|p| {
                    if let Some(public_port) = p.public_port {
                        format!("{}:{}/{}", 
                               p.ip.unwrap_or_else(|| "0.0.0.0".to_string()),
                               public_port, 
                               p.private_port)
                    } else {
                        format!("{}/tcp", p.private_port)
                    }
                }).collect(),
                networks: c.network_settings
                    .and_then(|ns| ns.networks)
                    .unwrap_or_default()
                    .into_keys()
                    .collect(),
            });

        Ok(container_info)
    }

    /// List all Docker networks
    pub async fn list_networks(&self) -> Result<Vec<NetworkInfo>, DockerError> {
        let options = Some(ListNetworksOptions::<String> {
            ..Default::default()
        });

        let networks = self.client.list_networks(options).await?;
        
        let network_infos = networks
            .into_iter()
            .map(|n| NetworkInfo {
                id: n.id.unwrap_or_default(),
                name: n.name.unwrap_or_default(),
                driver: n.driver.unwrap_or_default(),
                scope: n.scope.unwrap_or_default(),
                internal: n.internal.unwrap_or(false),
                attachable: n.attachable.unwrap_or(false),
                created: n.created.unwrap_or_default(),
                containers: n.containers
                    .unwrap_or_default()
                    .into_keys()
                    .collect(),
            })
            .collect();

        Ok(network_infos)
    }

    /// Get a specific network by ID or name
    pub async fn get_network_by_id(&self, network_id: &str) -> Result<Option<NetworkInfo>, DockerError> {
        // First try to inspect the network directly
        match self.client.inspect_network(network_id, None::<bollard::network::InspectNetworkOptions<String>>).await {
            Ok(network) => {
                let network_info = NetworkInfo {
                    id: network.id.unwrap_or_default(),
                    name: network.name.unwrap_or_default(),
                    driver: network.driver.unwrap_or_default(),
                    scope: network.scope.unwrap_or_default(),
                    internal: network.internal.unwrap_or(false),
                    attachable: network.attachable.unwrap_or(false),
                    created: network.created.unwrap_or_default(),
                    containers: network
                        .containers
                        .unwrap_or_default()
                        .into_keys()
                        .collect(),
                };
                Ok(Some(network_info))
            },
            Err(bollard::errors::Error::DockerResponseServerError { status_code: 404, .. }) => {
                Ok(None)
            },
            Err(e) => {
                log::error!("Error inspecting network {}: {}", network_id, e);
                Err(DockerError::ContainerError(e.to_string()))
            }
        }
    }
}

/// Domain model for network information
#[derive(Debug, Clone, serde::Serialize)]
pub struct NetworkInfo {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
    pub internal: bool,
    pub attachable: bool,
    pub created: String,
    pub containers: Vec<String>,
}

/// Domain model for container information
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub created: i64,
    pub ports: Vec<String>,
    pub networks: Vec<String>,
}