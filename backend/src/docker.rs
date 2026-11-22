use std::collections::HashMap;

use bollard::container::ListContainersOptions;
use bollard::network::ListNetworksOptions;
use bollard::models::PortTypeEnum;
use bollard::Docker;
use thiserror::Error;

use crate::config::Config;

/// Errors that can occur when interacting with the Docker daemon.
#[derive(Debug, Error)]
pub enum DockerError {
    /// The Docker daemon is unreachable or the connection was lost.
    #[error("Failed to connect to Docker daemon: {0}")]
    ConnectionError(#[from] bollard::errors::Error),

    /// An operation on a specific container or network failed.
    #[error("Container operation failed: {0}")]
    ContainerError(String),
}

/// Client for querying the local Docker daemon via its Unix socket.
#[derive(Clone)]
pub struct DockerClient {
    client: Docker,
}

impl DockerClient {
    fn port_protocol_to_string(protocol_type: Option<PortTypeEnum>) -> &'static str {
        match protocol_type {
            Some(PortTypeEnum::TCP) => "tcp",
            Some(PortTypeEnum::UDP) => "udp",
            Some(PortTypeEnum::SCTP) => "sctp",
            Some(PortTypeEnum::EMPTY) => "tcp",
            None => "tcp",
        }
    }

    /// Create a new Docker client connected to the socket path from `config`.
    pub fn new(config: &Config) -> Result<Self, DockerError> {
        let client = Docker::connect_with_socket(
            &config.docker_socket_path,
            config.docker_timeout_seconds,
            bollard::API_DEFAULT_VERSION,
        )?;
        Ok(DockerClient { client })
    }

    /// Ping the Docker daemon to verify connectivity.
    pub async fn ping(&self) -> Result<(), DockerError> {
        self.client.ping().await?;
        Ok(())
    }

    /// List all running containers with their ports and network attachments.
    pub async fn list_running_containers(&self) -> Result<Vec<ContainerInfo>, DockerError> {
        let options = Some(ListContainersOptions::<String> {
            all: false,
            ..Default::default()
        });

        let containers = self.client.list_containers(options).await?;

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
                    .map(|p| PortInfo {
                        private: p.private_port,
                        public: p.public_port,
                        protocol: Self::port_protocol_to_string(p.typ).into(),
                    })
                    .collect(),
                networks: c.network_settings
                    .and_then(|ns| ns.networks)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(name, settings)| {
                        (name, ContainerNetworkInfo {
                            ip_address: settings.ip_address,
                            mac_address: settings.mac_address,
                        })
                    })
                    .collect(),
            })
            .collect();

        Ok(container_infos)
    }

    /// List all Docker networks with subnet information and attached containers.
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
                subnet: n.ipam.and_then(|ipam| {
                    ipam.config.and_then(|configs| {
                        configs.into_iter().next().and_then(|c| c.subnet)
                    })
                }),
                internal: n.internal.unwrap_or(false),
                containers: n.containers
                    .unwrap_or_default()
                    .into_keys()
                    .collect(),
            })
            .collect();

        Ok(network_infos)
    }
}

/// A port mapping exposed by a container.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PortInfo {
    /// Container-side port number.
    pub private: u16,
    /// Host-side port number, if published.
    pub public: Option<u16>,
    /// Transport protocol (tcp, udp, or sctp).
    pub protocol: String,
}

/// Network attachment details for a container.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContainerNetworkInfo {
    /// IPv4 or IPv6 address assigned to the container on this network.
    pub ip_address: Option<String>,
    /// MAC address of the container's interface on this network.
    pub mac_address: Option<String>,
}

/// A running Docker container with its metadata.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub created: i64,
    pub ports: Vec<PortInfo>,
    /// Map of network name to attachment details.
    pub networks: HashMap<String, ContainerNetworkInfo>,
}

/// A Docker network with its configuration and member containers.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NetworkInfo {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
    /// Primary subnet CIDR, if configured.
    pub subnet: Option<String>,
    /// Whether the network is internal-only.
    pub internal: bool,
    /// IDs of containers attached to this network.
    ///
    /// Note: `list_networks()` does not populate this field because Docker's
    /// `/networks` endpoint requires `verbose=true` to return container
    /// memberships. In v0.1, the topology endpoint derives edges from
    /// container-side network data instead.
    pub containers: Vec<String>,
}
