use std::collections::HashMap;

use bollard::Docker;
use bollard::container::ListContainersOptions;
use bollard::models::{ContainerSummary, PortTypeEnum};
use bollard::network::ListNetworksOptions;
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
        self.list_containers_with_all(false).await
    }

    /// List all containers, including inactive ones, with their ports and network attachments.
    pub async fn list_containers(&self) -> Result<Vec<ContainerInfo>, DockerError> {
        self.list_containers_with_all(true).await
    }

    async fn list_containers_with_all(&self, all: bool) -> Result<Vec<ContainerInfo>, DockerError> {
        let options = Some(ListContainersOptions::<String> {
            all,
            ..Default::default()
        });

        let containers = self.client.list_containers(options).await?;

        Ok(containers
            .into_iter()
            .map(Self::container_info_from_summary)
            .collect())
    }

    fn container_info_from_summary(c: ContainerSummary) -> ContainerInfo {
        ContainerInfo {
            id: c.id.unwrap_or_default(),
            name: c
                .names
                .unwrap_or_default()
                .first()
                .cloned()
                .unwrap_or_default()
                .trim_start_matches('/')
                .to_string(),
            image: c.image.unwrap_or_default(),
            status: c.status.unwrap_or_default(),
            created: c.created.unwrap_or_default(),
            ports: c
                .ports
                .unwrap_or_default()
                .into_iter()
                .map(|p| PortInfo {
                    private: p.private_port,
                    public: p.public_port,
                    protocol: Self::port_protocol_to_string(p.typ).into(),
                })
                .collect(),
            networks: c
                .network_settings
                .and_then(|ns| ns.networks)
                .unwrap_or_default()
                .into_iter()
                .map(|(name, settings)| {
                    (
                        name,
                        ContainerNetworkInfo {
                            ip_address: settings.ip_address,
                            mac_address: settings.mac_address,
                        },
                    )
                })
                .collect(),
            edge_eligible: matches!(
                c.state.as_deref(),
                Some("running" | "paused" | "restarting")
            ),
        }
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

/// A Docker container with its metadata.
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
    /// Whether this container is eligible to appear in topology edges.
    #[serde(skip)]
    edge_eligible: bool,
}

impl ContainerInfo {
    /// Whether this container is eligible to appear in topology edges.
    pub(crate) fn is_edge_eligible(&self) -> bool {
        self.edge_eligible
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bollard::models::{ContainerSummaryNetworkSettings, EndpointSettings, Port};

    use super::*;

    fn summary(state: Option<&str>) -> ContainerSummary {
        ContainerSummary {
            id: Some("container-id".to_string()),
            names: Some(vec!["/fixture".to_string()]),
            image: Some("nginx:latest".to_string()),
            created: Some(1_700_000_000),
            state: state.map(str::to_string),
            status: Some("Exited (0) 2 minutes ago".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn container_summary_maps_edge_eligibility_from_state() {
        for state in ["running", "paused", "restarting"] {
            let info = DockerClient::container_info_from_summary(summary(Some(state)));
            assert!(
                info.is_edge_eligible(),
                "state {state:?} should be edge eligible"
            );
        }

        for state in ["exited", "created", "dead", "removing", "unknown"] {
            let info = DockerClient::container_info_from_summary(summary(Some(state)));
            assert!(
                !info.is_edge_eligible(),
                "state {state:?} should not be edge eligible"
            );
        }

        let missing_state = DockerClient::container_info_from_summary(summary(None));
        assert!(!missing_state.is_edge_eligible());
    }

    #[test]
    fn inactive_summary_preserves_metadata_without_serializing_eligibility() {
        let mut inactive = summary(Some("exited"));
        inactive.ports = Some(vec![Port {
            ip: Some("0.0.0.0".to_string()),
            private_port: 8080,
            public_port: Some(18080),
            typ: Some(PortTypeEnum::TCP),
        }]);
        inactive.network_settings = Some(ContainerSummaryNetworkSettings {
            networks: Some(HashMap::from([(
                "fixture-net".to_string(),
                EndpointSettings {
                    ip_address: Some("172.30.0.2".to_string()),
                    mac_address: Some("02:42:ac:1e:00:02".to_string()),
                    ..Default::default()
                },
            )])),
        });

        let info = DockerClient::container_info_from_summary(inactive);

        assert_eq!(info.id, "container-id");
        assert_eq!(info.name, "fixture");
        assert_eq!(info.image, "nginx:latest");
        assert_eq!(info.status, "Exited (0) 2 minutes ago");
        assert_eq!(info.created, 1_700_000_000);
        assert!(!info.is_edge_eligible());
        assert_eq!(info.ports.len(), 1);
        assert_eq!(info.ports[0].private, 8080);
        assert_eq!(info.ports[0].public, Some(18080));
        assert_eq!(info.ports[0].protocol, "tcp");
        let attachment = info
            .networks
            .get("fixture-net")
            .expect("network attachment is preserved");
        assert_eq!(attachment.ip_address.as_deref(), Some("172.30.0.2"));
        assert_eq!(attachment.mac_address.as_deref(), Some("02:42:ac:1e:00:02"));

        let json = serde_json::to_value(&info).expect("container info serializes");
        assert_eq!(json["status"], "Exited (0) 2 minutes ago");
        assert_eq!(json["networks"]["fixture-net"]["ip_address"], "172.30.0.2");
        assert_eq!(
            json["networks"]["fixture-net"]["mac_address"],
            "02:42:ac:1e:00:02"
        );
        assert!(
            json.get("edge_eligible").is_none(),
            "private eligibility must not be serialized: {json}"
        );
    }
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
