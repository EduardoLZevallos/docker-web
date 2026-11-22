//! Shared test fixtures and independent topology assertions.
//!
//! Expected values in this module come from fixture definitions and direct Docker
//! inspection, never from backend responses.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    time::Duration,
};

use bollard::{
    container::{Config as ContainerConfig, CreateContainerOptions, InspectContainerOptions},
    models::{ContainerInspectResponse, HealthConfig, HostConfig},
    network::CreateNetworkOptions,
};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::time::{Instant, sleep};

use super::E2eHarness;

pub const IMAGE: &str = "alpine:3.20";

pub type NetworkManifest = BTreeMap<String, String>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixtureNetwork {
    pub name: String,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixtureContainer {
    pub name: String,
    pub id: String,
}

pub fn fixture_labels(h: &E2eHarness, category: &str) -> HashMap<String, String> {
    let mut labels = h.labels();
    labels.insert("com.docker-web.e2e.fixture".into(), category.into());
    labels
}

pub async fn ensure_image(h: &E2eHarness) {
    let mut stream = h.docker.create_image(
        Some(bollard::image::CreateImageOptions {
            from_image: IMAGE,
            ..Default::default()
        }),
        None,
        None,
    );
    while let Some(result) = stream.next().await {
        result.unwrap_or_else(|error| panic!("pull {IMAGE}: {error}"));
    }
}

pub async fn create_network(
    h: &E2eHarness,
    name: &str,
    driver: &str,
    internal: bool,
    category: &str,
) -> FixtureNetwork {
    let id = h
        .docker
        .create_network(CreateNetworkOptions {
            name: name.to_owned(),
            driver: driver.to_owned(),
            internal,
            labels: fixture_labels(h, category),
            ..Default::default()
        })
        .await
        .unwrap_or_else(|error| panic!("create fixture network {name}: {error}"))
        .id
        .expect("created network has an ID");
    FixtureNetwork {
        name: name.to_owned(),
        id,
    }
}

pub async fn create_container(
    h: &E2eHarness,
    name: &str,
    command: &[&str],
    host_config: HostConfig,
    exposed_ports: Option<Vec<&str>>,
    healthcheck: Option<HealthConfig>,
    category: &str,
) -> FixtureContainer {
    let exposed_ports = exposed_ports.map(|ports| {
        ports
            .into_iter()
            .map(|port| (port.to_owned(), HashMap::new()))
            .collect()
    });
    let id = h
        .docker
        .create_container(
            Some(CreateContainerOptions {
                name: name.to_owned(),
                platform: None,
            }),
            ContainerConfig {
                image: Some(IMAGE.to_owned()),
                cmd: Some(command.iter().map(|arg| (*arg).to_owned()).collect()),
                exposed_ports,
                labels: Some(fixture_labels(h, category)),
                healthcheck,
                host_config: Some(host_config),
                ..Default::default()
            },
        )
        .await
        .unwrap_or_else(|error| panic!("create fixture container {name}: {error}"))
        .id;
    FixtureContainer {
        name: name.to_owned(),
        id,
    }
}

pub async fn create_network_container(
    h: &E2eHarness,
    name: &str,
    network: &str,
    command: &[&str],
    category: &str,
) -> FixtureContainer {
    create_container(
        h,
        name,
        command,
        HostConfig {
            network_mode: Some(network.to_owned()),
            ..Default::default()
        },
        None,
        None,
        category,
    )
    .await
}

pub async fn create_and_start_network_container(
    h: &E2eHarness,
    name: &str,
    network: &str,
    command: &[&str],
    category: &str,
) -> FixtureContainer {
    let container = create_network_container(h, name, network, command, category).await;
    start_container(h, &container.id).await;
    container
}

pub async fn start_container(h: &E2eHarness, id: &str) {
    h.docker
        .start_container::<String>(id, None)
        .await
        .unwrap_or_else(|error| panic!("start fixture container {id}: {error}"));
}

pub async fn inspect(h: &E2eHarness, id: &str) -> ContainerInspectResponse {
    h.docker
        .inspect_container(id, None::<InspectContainerOptions>)
        .await
        .unwrap_or_else(|error| panic!("inspect fixture container {id}: {error}"))
}

pub fn state_name(inspect: &ContainerInspectResponse) -> String {
    inspect
        .state
        .as_ref()
        .and_then(|state| state.status.as_ref())
        .map(|status| format!("{status:?}").to_lowercase())
        .unwrap_or_default()
}

pub async fn wait_state(h: &E2eHarness, id: &str, wanted: &str, timeout: Duration) {
    let end = Instant::now() + timeout;
    loop {
        let got = state_name(&inspect(h, id).await);
        if got == wanted {
            return;
        }
        assert!(
            Instant::now() < end,
            "fixture setup: expected inspect state {wanted}, got {got}"
        );
        sleep(Duration::from_millis(100)).await;
    }
}

pub async fn topology(h: &E2eHarness) -> Value {
    let (status, body) = h.get("/api/topology").await;
    assert_eq!(status, reqwest::StatusCode::OK, "topology response: {body}");
    body
}

pub async fn containers(h: &E2eHarness) -> Value {
    let (status, body) = h.get("/api/containers").await;
    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "containers response: {body}"
    );
    body
}

pub async fn networks(h: &E2eHarness) -> Value {
    let (status, body) = h.get("/api/networks").await;
    assert_eq!(status, reqwest::StatusCode::OK, "networks response: {body}");
    body
}

pub fn edge_set(body: &Value) -> BTreeSet<(String, String)> {
    body["edges"]
        .as_array()
        .unwrap_or_else(|| panic!("topology edges must be an array: {body}"))
        .iter()
        .map(|edge| {
            (
                edge["source"].as_str().unwrap().to_owned(),
                edge["target"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

pub fn assert_timestamp(value: &Value) {
    let timestamp = value.as_str().expect("timestamp must be a string");
    time::OffsetDateTime::parse(timestamp, &time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|error| panic!("timestamp must be RFC3339 ({timestamp}): {error}"));
}

pub fn assert_connection_error(status: reqwest::StatusCode, body: &Value, path: &str) {
    assert_eq!(
        status,
        reqwest::StatusCode::SERVICE_UNAVAILABLE,
        "GET {path}: {body}"
    );
    assert_eq!(
        body["error"], "Docker connection failed",
        "GET {path}: {body}"
    );
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty()),
        "GET {path}: {body}"
    );
    assert_timestamp(&body["timestamp"]);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologyContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub created: i64,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologyNetwork {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
    pub internal: bool,
    pub subnet: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TopologySnapshot {
    pub containers: Vec<TopologyContainer>,
    pub networks: Vec<TopologyNetwork>,
    pub edges: BTreeSet<(String, String)>,
    pub timestamp: String,
}

impl TopologySnapshot {
    pub fn from_value(body: &Value) -> Self {
        assert_timestamp(&body["timestamp"]);
        let containers = body["containers"]
            .as_array()
            .unwrap_or_else(|| panic!("topology containers must be an array: {body}"))
            .iter()
            .map(|container| TopologyContainer {
                id: container["id"]
                    .as_str()
                    .unwrap_or_else(|| panic!("container ID must be a string: {container}"))
                    .to_owned(),
                name: container["name"]
                    .as_str()
                    .unwrap_or_else(|| panic!("container name must be a string: {container}"))
                    .to_owned(),
                image: container["image"]
                    .as_str()
                    .unwrap_or_else(|| panic!("container image must be a string: {container}"))
                    .to_owned(),
                created: container["created"]
                    .as_i64()
                    .unwrap_or_else(|| panic!("container created must be an integer: {container}")),
                status: container["status"]
                    .as_str()
                    .unwrap_or_else(|| panic!("container status must be a string: {container}"))
                    .to_owned(),
            })
            .collect();
        let networks = body["networks"]
            .as_array()
            .unwrap_or_else(|| panic!("topology networks must be an array: {body}"))
            .iter()
            .map(|network| TopologyNetwork {
                id: network["id"]
                    .as_str()
                    .unwrap_or_else(|| panic!("network ID must be a string: {network}"))
                    .to_owned(),
                name: network["name"]
                    .as_str()
                    .unwrap_or_else(|| panic!("network name must be a string: {network}"))
                    .to_owned(),
                driver: network["driver"]
                    .as_str()
                    .unwrap_or_else(|| panic!("network driver must be a string: {network}"))
                    .to_owned(),
                scope: network["scope"]
                    .as_str()
                    .unwrap_or_else(|| panic!("network scope must be a string: {network}"))
                    .to_owned(),
                internal: network["internal"]
                    .as_bool()
                    .unwrap_or_else(|| panic!("network internal must be a boolean: {network}")),
                subnet: network["subnet"].as_str().map(str::to_owned),
            })
            .collect();
        let edge_values: Vec<(String, String)> = body["edges"]
            .as_array()
            .unwrap_or_else(|| panic!("topology edges must be an array: {body}"))
            .iter()
            .map(|edge| {
                (
                    edge["source"]
                        .as_str()
                        .unwrap_or_else(|| panic!("edge source must be a string: {edge}"))
                        .to_owned(),
                    edge["target"]
                        .as_str()
                        .unwrap_or_else(|| panic!("edge target must be a string: {edge}"))
                        .to_owned(),
                )
            })
            .collect();
        let edges: BTreeSet<_> = edge_values.iter().cloned().collect();
        assert_eq!(
            edges.len(),
            edge_values.len(),
            "duplicate topology edges: {body}"
        );
        Self {
            containers,
            networks,
            edges,
            timestamp: body["timestamp"].as_str().unwrap().to_owned(),
        }
    }

    pub fn container_ids(&self) -> BTreeSet<String> {
        self.containers
            .iter()
            .map(|container| container.id.clone())
            .collect()
    }

    pub fn network_manifest(&self) -> NetworkManifest {
        self.networks
            .iter()
            .map(|network| (network.name.clone(), network.id.clone()))
            .collect()
    }

    pub fn assert_unique_and_referential(&self) {
        assert_eq!(
            self.container_ids().len(),
            self.containers.len(),
            "duplicate container IDs: {self:?}"
        );
        assert_eq!(
            self.network_manifest().len(),
            self.networks.len(),
            "duplicate network names: {self:?}"
        );
        assert_eq!(
            self.networks
                .iter()
                .map(|network| network.id.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            self.networks.len(),
            "duplicate network IDs: {self:?}"
        );
        let container_ids = self.container_ids();
        let network_ids: BTreeSet<_> = self
            .networks
            .iter()
            .map(|network| network.id.as_str())
            .collect();
        assert!(
            self.edges.iter().all(|(source, target)| {
                container_ids.contains(source) && network_ids.contains(target.as_str())
            }),
            "topology edge has a dangling endpoint: {self:?}"
        );
    }

    pub fn assert_exact(
        &self,
        expected_containers: &BTreeSet<String>,
        expected_networks: &NetworkManifest,
        expected_edges: &BTreeSet<(String, String)>,
    ) {
        self.assert_unique_and_referential();
        assert_eq!(
            &self.container_ids(),
            expected_containers,
            "container IDs: {self:?}"
        );
        assert_eq!(
            &self.network_manifest(),
            expected_networks,
            "network names and IDs: {self:?}"
        );
        assert_eq!(&self.edges, expected_edges, "topology edge set: {self:?}");
    }
}

/// Reads all three stable-state endpoints and checks they describe the same
/// independently-defined fixture topology.
pub async fn assert_cross_endpoint_consistency(
    h: &E2eHarness,
    expected_containers: &BTreeSet<String>,
    expected_networks: &NetworkManifest,
    expected_edges: &BTreeSet<(String, String)>,
) -> TopologySnapshot {
    let containers_body = containers(h).await;
    let networks_body = networks(h).await;
    let topology_body = topology(h).await;
    let snapshot = TopologySnapshot::from_value(&topology_body);
    snapshot.assert_exact(expected_containers, expected_networks, expected_edges);

    let container_rows = containers_body
        .as_array()
        .unwrap_or_else(|| panic!("containers response must be an array: {containers_body}"));
    let container_ids: BTreeSet<_> = container_rows
        .iter()
        .map(|row| row["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        container_ids.len(),
        container_rows.len(),
        "duplicate container IDs from /api/containers: {containers_body}"
    );
    assert_eq!(
        &container_ids, expected_containers,
        "containers endpoint IDs: {containers_body}"
    );

    let network_rows = networks_body
        .as_array()
        .unwrap_or_else(|| panic!("networks response must be an array: {networks_body}"));
    let network_manifest: NetworkManifest = network_rows
        .iter()
        .map(|row| {
            (
                row["name"].as_str().unwrap().to_owned(),
                row["id"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        network_manifest.len(),
        network_rows.len(),
        "duplicate network names from /api/networks: {networks_body}"
    );
    assert_eq!(
        &network_manifest, expected_networks,
        "networks endpoint manifest: {networks_body}"
    );

    let topology_container_ids = snapshot.container_ids();
    assert_eq!(
        topology_container_ids, container_ids,
        "containers and topology endpoints disagree on container IDs"
    );
    assert_eq!(
        snapshot.network_manifest(),
        network_manifest,
        "networks and topology endpoints disagree on network IDs"
    );
    for container in &snapshot.containers {
        let row = container_rows
            .iter()
            .find(|row| row["id"] == container.id)
            .unwrap_or_else(|| {
                panic!(
                    "topology container {} missing from /api/containers",
                    container.id
                )
            });
        for network_name in row["networks"]
            .as_object()
            .unwrap_or_else(|| panic!("container networks must be an object: {row}"))
            .keys()
        {
            let network_id = network_manifest.get(network_name).unwrap_or_else(|| {
                panic!(
                    "container {} references unknown network {network_name}",
                    container.id
                )
            });
            assert!(
                snapshot
                    .edges
                    .contains(&(container.id.clone(), network_id.clone())),
                "container {} network {network_name} has no topology edge: {topology_body}",
                container.id
            );
        }
    }
    snapshot
}
