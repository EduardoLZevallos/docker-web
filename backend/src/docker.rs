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

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
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
            state: normalize_optional_string(c.state),
            created: c.created.unwrap_or_default(),
            ports: c
                .ports
                .unwrap_or_default()
                .into_iter()
                .map(|p| PortInfo {
                    private: p.private_port,
                    public: p.public_port,
                    protocol: Self::port_protocol_to_string(p.typ).into(),
                    host_ip: normalize_optional_string(p.ip),
                })
                .collect(),
            networks: c
                .network_settings
                .and_then(|ns| ns.networks)
                .unwrap_or_default()
                .into_iter()
                .map(|(name, settings)| {
                    let ip_address = settings.ip_address;
                    (
                        name,
                        ContainerNetworkInfo {
                            network_id: normalize_optional_string(settings.network_id),
                            ipv4_address: normalize_optional_string(ip_address.clone()),
                            ipv6_address: normalize_optional_string(settings.global_ipv6_address),
                            ip_address,
                            mac_address: settings.mac_address,
                        },
                    )
                })
                .collect(),
        }
    }

    /// List all Docker networks with subnet information and attached containers.
    pub async fn list_networks(&self) -> Result<Vec<NetworkInfo>, DockerError> {
        let options = Some(ListNetworksOptions::<String> {
            ..Default::default()
        });

        let networks = self.client.list_networks(options).await?;

        Ok(networks
            .into_iter()
            .map(Self::network_info_from_response)
            .collect())
    }

    fn network_info_from_response(n: bollard::models::Network) -> NetworkInfo {
        let ipam_configs = n
            .ipam
            .as_ref()
            .and_then(|ipam| ipam.config.as_ref())
            .map(|configs| {
                configs
                    .iter()
                    .map(|config| IpamConfigInfo {
                        subnet: normalize_optional_string(config.subnet.clone()),
                        gateway: normalize_optional_string(config.gateway.clone()),
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|configs| !configs.is_empty());

        NetworkInfo {
            id: n.id.unwrap_or_default(),
            name: n.name.unwrap_or_default(),
            driver: n.driver.unwrap_or_default(),
            scope: n.scope.unwrap_or_default(),
            subnet: n.ipam.and_then(|ipam| {
                ipam.config
                    .and_then(|configs| configs.into_iter().next().and_then(|c| c.subnet))
            }),
            ipam_configs,
            internal: n.internal.unwrap_or(false),
            containers: n.containers.unwrap_or_default().into_keys().collect(),
        }
    }
}

/// A port mapping exposed by a container.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PortInfo {
    /// Container-side port number.
    pub private: u16,
    /// Host-side port number, or `null` when Docker reports no host binding.
    ///
    /// This is Docker list-endpoint metadata. Only running or paused containers
    /// should be interpreted as currently publishing the port.
    pub public: Option<u16>,
    /// Transport protocol (tcp, udp, or sctp).
    pub protocol: String,
    /// Host IP address that the container port is mapped to, if reported.
    pub host_ip: Option<String>,
}

/// Network attachment details for a container.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContainerNetworkInfo {
    /// Full Docker network ID reported by the container summary, if present.
    pub network_id: Option<String>,
    /// Docker-reported IPv4 address for this attachment, if present.
    pub ipv4_address: Option<String>,
    /// Docker-reported global IPv6 address for this attachment, if present.
    pub ipv6_address: Option<String>,
    /// Legacy IPv4/IPv6 address field retained for existing clients.
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
    /// Human-readable Docker status string.
    pub status: String,
    /// Raw Docker container state (for example `running`, `paused`, `exited`).
    ///
    /// `null` when Docker omits the state or reports only blank whitespace.
    pub state: Option<String>,
    pub created: i64,
    pub ports: Vec<PortInfo>,
    /// Map of network name to attachment details.
    pub networks: HashMap<String, ContainerNetworkInfo>,
}

impl ContainerInfo {
    /// Whether this container is eligible to appear in topology edges.
    pub(crate) fn is_edge_eligible(&self) -> bool {
        matches!(
            self.state.as_deref(),
            Some("running" | "paused" | "restarting")
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bollard::models::{
        ContainerSummaryNetworkSettings, EndpointSettings, Ipam, IpamConfig, Network, Port,
    };

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

    fn network_with_ipam(configs: Option<Vec<IpamConfig>>) -> Network {
        Network {
            id: Some("network-id".to_string()),
            name: Some("fixture-net".to_string()),
            driver: Some("bridge".to_string()),
            scope: Some("local".to_string()),
            internal: Some(false),
            ipam: Some(Ipam {
                config: configs,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn container_summary_maps_raw_state_and_edge_eligibility() {
        for state in ["running", "paused", "restarting"] {
            let info = DockerClient::container_info_from_summary(summary(Some(state)));
            assert_eq!(info.state.as_deref(), Some(state));
            assert!(
                info.is_edge_eligible(),
                "state {state:?} should be edge eligible"
            );
        }

        for state in ["exited", "created", "dead", "removing", "unknown"] {
            let info = DockerClient::container_info_from_summary(summary(Some(state)));
            assert_eq!(info.state.as_deref(), Some(state));
            assert!(
                !info.is_edge_eligible(),
                "state {state:?} should not be edge eligible"
            );
        }

        let missing_state = DockerClient::container_info_from_summary(summary(None));
        assert_eq!(missing_state.state, None);
        assert!(!missing_state.is_edge_eligible());

        let blank_state = DockerClient::container_info_from_summary(summary(Some("  ")));
        assert_eq!(blank_state.state, None);
        assert!(!blank_state.is_edge_eligible());
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
                    network_id: Some("network-id".to_string()),
                    ip_address: Some("172.30.0.2".to_string()),
                    global_ipv6_address: Some("fd00::2".to_string()),
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
        assert_eq!(info.state.as_deref(), Some("exited"));
        assert_eq!(info.created, 1_700_000_000);
        assert!(!info.is_edge_eligible());
        assert_eq!(info.ports.len(), 1);
        assert_eq!(info.ports[0].private, 8080);
        assert_eq!(info.ports[0].public, Some(18080));
        assert_eq!(info.ports[0].protocol, "tcp");
        assert_eq!(info.ports[0].host_ip.as_deref(), Some("0.0.0.0"));
        let attachment = info
            .networks
            .get("fixture-net")
            .expect("network attachment is preserved");
        assert_eq!(attachment.network_id.as_deref(), Some("network-id"));
        assert_eq!(attachment.ipv4_address.as_deref(), Some("172.30.0.2"));
        assert_eq!(attachment.ipv6_address.as_deref(), Some("fd00::2"));
        assert_eq!(attachment.ip_address.as_deref(), Some("172.30.0.2"));
        assert_eq!(attachment.mac_address.as_deref(), Some("02:42:ac:1e:00:02"));

        let json = serde_json::to_value(&info).expect("container info serializes");
        assert_eq!(json["status"], "Exited (0) 2 minutes ago");
        assert_eq!(json["state"], "exited");
        assert_eq!(json["networks"]["fixture-net"]["network_id"], "network-id");
        assert_eq!(
            json["networks"]["fixture-net"]["ipv4_address"],
            "172.30.0.2"
        );
        assert_eq!(
            json["networks"]["fixture-net"]["ipv6_address"],
            "fd00::2"
        );
        assert_eq!(json["networks"]["fixture-net"]["ip_address"], "172.30.0.2");
        assert_eq!(
            json["networks"]["fixture-net"]["mac_address"],
            "02:42:ac:1e:00:02"
        );
        assert_eq!(json["ports"][0]["host_ip"], "0.0.0.0");
        assert!(
            json.get("edge_eligible").is_none(),
            "private eligibility must not be serialized: {json}"
        );
    }

    #[test]
    fn blank_new_nullable_strings_serialize_as_null_and_legacy_ip_is_retained() {
        let mut summary = summary(Some("running"));
        summary.ports = Some(vec![Port {
            ip: Some("   ".to_string()),
            private_port: 8080,
            public_port: None,
            typ: Some(PortTypeEnum::UDP),
        }]);
        summary.network_settings = Some(ContainerSummaryNetworkSettings {
            networks: Some(HashMap::from([(
                "fixture-net".to_string(),
                EndpointSettings {
                    network_id: Some("".to_string()),
                    ip_address: Some("".to_string()),
                    global_ipv6_address: Some("   ".to_string()),
                    mac_address: Some("".to_string()),
                    ..Default::default()
                },
            )])),
        });

        let info = DockerClient::container_info_from_summary(summary);
        let attachment = &info.networks["fixture-net"];
        assert_eq!(attachment.network_id, None);
        assert_eq!(attachment.ipv4_address, None);
        assert_eq!(attachment.ipv6_address, None);
        assert_eq!(attachment.ip_address.as_deref(), Some(""));
        assert_eq!(info.ports[0].host_ip, None);

        let json = serde_json::to_value(&info).expect("container info serializes");
        assert!(json["state"].is_string());
        assert!(json["networks"]["fixture-net"]["network_id"].is_null());
        assert!(json["networks"]["fixture-net"]["ipv4_address"].is_null());
        assert!(json["networks"]["fixture-net"]["ipv6_address"].is_null());
        assert_eq!(json["networks"]["fixture-net"]["ip_address"], "");
        assert!(json["ports"][0]["host_ip"].is_null());
        assert!(json["ports"][0]["public"].is_null());
    }

    #[test]
    fn each_reported_port_binding_is_preserved() {
        let mut published = summary(Some("running"));
        published.ports = Some(vec![
            Port {
                ip: Some("127.0.0.1".to_string()),
                private_port: 8080,
                public_port: Some(18080),
                typ: Some(PortTypeEnum::TCP),
            },
            Port {
                ip: Some("::1".to_string()),
                private_port: 8080,
                public_port: Some(18081),
                typ: Some(PortTypeEnum::TCP),
            },
            Port {
                ip: None,
                private_port: 5353,
                public_port: None,
                typ: Some(PortTypeEnum::UDP),
            },
        ]);

        let info = DockerClient::container_info_from_summary(published);
        assert_eq!(info.ports.len(), 3);
        assert_eq!(
            info.ports
                .iter()
                .filter(|port| port.private == 8080)
                .count(),
            2,
            "both host bindings must remain visible"
        );
        assert_eq!(info.ports[0].host_ip.as_deref(), Some("127.0.0.1"));
        assert_eq!(info.ports[1].host_ip.as_deref(), Some("::1"));
        assert_eq!(info.ports[2].public, None);
        assert_eq!(info.ports[2].host_ip, None);
    }

    #[test]
    fn network_summary_maps_ordered_ipam_configs_and_legacy_subnet() {
        let info = DockerClient::network_info_from_response(network_with_ipam(Some(vec![
            IpamConfig {
                subnet: Some("172.30.0.0/16".to_string()),
                gateway: Some("172.30.0.1".to_string()),
                ..Default::default()
            },
            IpamConfig {
                subnet: Some("fd00::/64".to_string()),
                gateway: Some("fd00::1".to_string()),
                ..Default::default()
            },
        ])));

        assert_eq!(info.id, "network-id");
        assert_eq!(info.name, "fixture-net");
        assert_eq!(info.subnet.as_deref(), Some("172.30.0.0/16"));
        let configs = info.ipam_configs.as_ref().expect("IPAM configs present");
        assert_eq!(configs.len(), 2);
        assert_eq!(configs[0].subnet.as_deref(), Some("172.30.0.0/16"));
        assert_eq!(configs[0].gateway.as_deref(), Some("172.30.0.1"));
        assert_eq!(configs[1].subnet.as_deref(), Some("fd00::/64"));
        assert_eq!(configs[1].gateway.as_deref(), Some("fd00::1"));

        let json = serde_json::to_value(&info).expect("network info serializes");
        assert_eq!(json["subnet"], "172.30.0.0/16");
        assert_eq!(json["ipam_configs"][0]["subnet"], "172.30.0.0/16");
        assert_eq!(json["ipam_configs"][0]["gateway"], "172.30.0.1");
        assert_eq!(json["ipam_configs"][1]["subnet"], "fd00::/64");
        assert_eq!(json["ipam_configs"][1]["gateway"], "fd00::1");
    }

    #[test]
    fn absent_or_empty_network_ipam_maps_to_null_configs() {
        for network in [
            Network {
                ipam: None,
                ..network_with_ipam(None)
            },
            network_with_ipam(Some(Vec::new())),
        ] {
            let info = DockerClient::network_info_from_response(network);
            assert!(info.ipam_configs.is_none());
            let json = serde_json::to_value(&info).expect("network info serializes");
            assert!(json["ipam_configs"].is_null());
        }
    }

    #[test]
    fn blank_network_ipam_values_normalize_to_null() {
        let info = DockerClient::network_info_from_response(network_with_ipam(Some(vec![
            IpamConfig {
                subnet: Some("   ".to_string()),
                gateway: Some("".to_string()),
                ..Default::default()
            },
        ])));
        let configs = info.ipam_configs.as_ref().expect("IPAM configs present");
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].subnet, None);
        assert_eq!(configs[0].gateway, None);

        let json = serde_json::to_value(&info).expect("network info serializes");
        assert!(json["ipam_configs"][0]["subnet"].is_null());
        assert!(json["ipam_configs"][0]["gateway"].is_null());
    }
}

/// One Docker network IPAM configuration entry.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IpamConfigInfo {
    /// Subnet CIDR reported by Docker, if present.
    pub subnet: Option<String>,
    /// Gateway address reported by Docker, if present.
    pub gateway: Option<String>,
}

/// A Docker network with its configuration and member containers.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NetworkInfo {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
    /// Legacy primary subnet CIDR (the first IPAM config's subnet), if reported.
    pub subnet: Option<String>,
    /// Ordered Docker IPAM configurations, or `null` when Docker reports none.
    pub ipam_configs: Option<Vec<IpamConfigInfo>>,
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
