use std::collections::HashMap;

use bollard::container::ListContainersOptions;
use bollard::network::ListNetworksOptions;
use bollard::models::PortTypeEnum;
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

    pub fn new(config: &Config) -> Result<Self, DockerError> {
        let client = Docker::connect_with_socket(
            &config.docker_socket_path,
            config.docker_timeout_seconds,
            bollard::API_DEFAULT_VERSION,
        )?;
        Ok(DockerClient { client })
    }

    pub async fn ping(&self) -> Result<(), DockerError> {
        self.client.ping().await?;
        Ok(())
    }

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

#[derive(Debug, Clone, serde::Serialize)]
pub struct PortInfo {
    pub private: u16,
    pub public: Option<u16>,
    pub protocol: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ContainerNetworkInfo {
    pub ip_address: Option<String>,
    pub mac_address: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub created: i64,
    pub ports: Vec<PortInfo>,
    pub networks: HashMap<String, ContainerNetworkInfo>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NetworkInfo {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
    pub subnet: Option<String>,
    pub internal: bool,
    pub containers: Vec<String>,
}
