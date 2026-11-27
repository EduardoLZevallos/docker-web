use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    time::Duration,
};

use bollard::{
    container::{InspectContainerOptions, ListContainersOptions, RemoveContainerOptions},
    models::HostConfig,
    network::{ConnectNetworkOptions, DisconnectNetworkOptions},
};
use serde_json::Value;
use tokio::time::{Instant, sleep};

use super::{
    E2eHarness,
    fixtures::{self, NetworkManifest, TopologySnapshot},
};

async fn network(h: &E2eHarness, name: &str, driver: &str, internal: bool) -> String {
    fixtures::create_network(h, name, driver, internal, "topology")
        .await
        .id
}

async fn container(
    h: &E2eHarness,
    name: &str,
    network: Option<&str>,
    command: Vec<&str>,
) -> String {
    let host = HostConfig {
        network_mode: network.map(str::to_owned),
        ..Default::default()
    };
    create_container(h, name, command, host, None).await
}

async fn create_container(
    h: &E2eHarness,
    name: &str,
    command: Vec<&str>,
    host: HostConfig,
    ports: Option<Vec<&str>>,
) -> String {
    let container =
        fixtures::create_container(h, name, &command, host, ports, None, "topology").await;
    fixtures::start_container(h, &container.id).await;
    container.id
}

async fn topology(h: &E2eHarness) -> Value {
    fixtures::topology(h).await
}

fn edge_set(body: &Value) -> BTreeSet<(String, String)> {
    fixtures::edge_set(body)
}

fn normalized(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn assert_exact_topology(
    body: &Value,
    expected_containers: &BTreeSet<String>,
    expected_networks: &NetworkManifest,
    expected_edges: &BTreeSet<(String, String)>,
) {
    TopologySnapshot::from_value(body).assert_exact(
        expected_containers,
        expected_networks,
        expected_edges,
    );
}

async fn wait_topology(
    h: &E2eHarness,
    expected_containers: &BTreeSet<String>,
    expected_networks: &NetworkManifest,
    expected_edges: &BTreeSet<(String, String)>,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let body = topology(h).await;
        let snapshot = TopologySnapshot::from_value(&body);
        if snapshot.container_ids() == *expected_containers
            && snapshot.network_manifest() == *expected_networks
            && snapshot.edges == *expected_edges
        {
            return body;
        }
        if Instant::now() >= deadline {
            assert_exact_topology(
                &body,
                expected_containers,
                expected_networks,
                expected_edges,
            );
        }
        sleep(Duration::from_millis(150)).await;
    }
}

async fn matrix(h: &E2eHarness, distribution: &[usize]) {
    let baseline_containers = h
        .docker
        .list_containers(Some(ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        }))
        .await
        .unwrap();
    assert!(
        baseline_containers.is_empty(),
        "clean inner daemon must have no baseline containers"
    );
    let baseline_networks = h.docker.list_networks::<String>(None).await.unwrap();
    let mut all_ids = BTreeSet::new();
    let mut all_networks: NetworkManifest = baseline_networks
        .into_iter()
        .filter_map(|n| Some((n.name?, n.id?)))
        .collect();
    let mut expected = BTreeSet::new();
    fixtures::ensure_image(h).await;
    let mut network_names = Vec::new();
    let mut net_ids = Vec::new();
    for i in 0..distribution.len() {
        let name = h.unique_name(&format!("matrix-net-{i}"));
        net_ids.push(network(h, &name, "bridge", false).await);
        network_names.push(name);
    }
    for (i, count) in distribution.iter().enumerate() {
        for j in 0..*count {
            let name = h.unique_name(&format!("matrix-c-{i}-{j}"));
            let id = container(
                h,
                &name,
                Some(&network_names[i]),
                vec!["sh", "-c", "sleep 300"],
            )
            .await;
            all_ids.insert(id.clone());
            expected.insert((id, net_ids[i].clone()));
        }
    }
    all_networks.extend(network_names.into_iter().zip(net_ids));
    wait_topology(h, &all_ids, &all_networks, &expected).await;
    fixtures::assert_cross_endpoint_consistency(h, &all_ids, &all_networks, &expected).await;
}

#[tokio::test]
async fn given_zero_networks_and_containers_when_topology_read_then_baseline_is_exact() {
    E2eHarness::run(|h| Box::pin(async move { matrix(h, &[]).await })).await;
}

#[tokio::test]
async fn given_one_empty_network_when_topology_read_then_no_edges_exist() {
    E2eHarness::run(|h| Box::pin(async move { matrix(h, &[0]).await })).await;
}

#[tokio::test]
async fn given_one_network_and_container_when_topology_read_then_one_edge_exists() {
    E2eHarness::run(|h| Box::pin(async move { matrix(h, &[1]).await })).await;
}

#[tokio::test]
async fn given_one_network_and_four_containers_when_topology_read_then_four_edges_exist() {
    E2eHarness::run(|h| Box::pin(async move { matrix(h, &[4]).await })).await;
}

#[tokio::test]
async fn given_three_empty_networks_when_topology_read_then_no_edges_exist() {
    E2eHarness::run(|h| Box::pin(async move { matrix(h, &[0, 0, 0]).await })).await;
}

#[tokio::test]
async fn given_three_networks_and_one_container_each_when_topology_read_then_three_edges_exist() {
    E2eHarness::run(|h| Box::pin(async move { matrix(h, &[1, 1, 1]).await })).await;
}

#[tokio::test]
async fn given_three_networks_and_four_containers_each_when_topology_read_then_twelve_edges_exist()
{
    E2eHarness::run(|h| Box::pin(async move { matrix(h, &[4, 4, 4]).await })).await;
}

#[tokio::test]
async fn given_uneven_distribution_when_topology_read_then_exact_edges_exist() {
    E2eHarness::run(|h| Box::pin(async move { matrix(h, &[0, 1, 4]).await })).await;
}

#[tokio::test]
async fn given_eight_networks_and_four_containers_each_when_topology_read_then_thirty_two_edges_exist()
 {
    E2eHarness::run(|h| Box::pin(async move { matrix(h, &[4, 4, 4, 4, 4, 4, 4, 4]).await })).await;
}

#[tokio::test]
async fn given_multihomed_fixtures_when_membership_changes_then_topology_tracks_lifecycle() {
    E2eHarness::run(|h| {
        Box::pin(async move {
            let baseline_containers = h
                .docker
                .list_containers(Some(ListContainersOptions::<String> {
                    all: true,
                    ..Default::default()
                }))
                .await
                .unwrap();
            assert!(baseline_containers.is_empty());
            let mut all_containers = BTreeSet::new();
            let mut all_networks: NetworkManifest = h
                .docker
                .list_networks::<String>(None)
                .await
                .unwrap()
                .into_iter()
                .filter_map(|n| Some((n.name?, n.id?)))
                .collect();
            let baseline_edges = BTreeSet::new();
            fixtures::ensure_image(h).await;
            let a = h.unique_name("multi-a");
            let b = h.unique_name("multi-b");
            let c = h.unique_name("multi-c");
            let aid = network(h, &a, "bridge", false).await;
            let bid = network(h, &b, "bridge", false).await;
            let cid = network(h, &c, "bridge", false).await;
            let c1 = container(
                h,
                &h.unique_name("multi-container"),
                Some(&a),
                vec!["sh", "-c", "sleep 300"],
            )
            .await;
            h.docker
                .connect_network(
                    &bid,
                    ConnectNetworkOptions {
                        container: c1.clone(),
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            let c2 = container(
                h,
                &h.unique_name("isolated-container"),
                Some(&c),
                vec!["sh", "-c", "sleep 300"],
            )
            .await;
            all_containers.extend([c1.clone(), c2.clone()]);
            all_networks.extend([
                (a.clone(), aid.clone()),
                (b.clone(), bid.clone()),
                (c.clone(), cid.clone()),
            ]);
            let mut edges = baseline_edges.clone();
            edges.extend([
                (c1.clone(), aid.clone()),
                (c1.clone(), bid.clone()),
                (c2.clone(), cid.clone()),
            ]);
            let body = wait_topology(h, &all_containers, &all_networks, &edges).await;
            assert_exact_topology(&body, &all_containers, &all_networks, &edges);
            h.docker
                .disconnect_network(
                    &bid,
                    DisconnectNetworkOptions {
                        container: c1.clone(),
                        force: true,
                    },
                )
                .await
                .unwrap();
            let mut after = baseline_edges.clone();
            after.extend([(c1.clone(), aid.clone()), (c2.clone(), cid.clone())]);
            assert_exact_topology(
                &wait_topology(h, &all_containers, &all_networks, &after).await,
                &all_containers,
                &all_networks,
                &after,
            );
            h.docker.remove_network(&bid).await.unwrap();
            all_networks.remove(&b);
            assert_exact_topology(
                &wait_topology(h, &all_containers, &all_networks, &after).await,
                &all_containers,
                &all_networks,
                &after,
            );
            let recreated_id = network(h, &b, "bridge", false).await;
            assert_ne!(recreated_id, bid);
            all_networks.insert(b, recreated_id.clone());
            assert_exact_topology(
                &wait_topology(h, &all_containers, &all_networks, &after).await,
                &all_containers,
                &all_networks,
                &after,
            );
            let container_name = h.unique_name("same-container-name");
            let old_id =
                container(h, &container_name, Some(&a), vec!["sh", "-c", "sleep 300"]).await;
            let _ = h
                .docker
                .remove_container(
                    &old_id,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await;
            all_containers.insert(
                container(h, &container_name, Some(&a), vec!["sh", "-c", "sleep 300"]).await,
            );
            all_containers.remove(&old_id);
            let mut with_recreated = after;
            with_recreated.insert((
                all_containers
                    .iter()
                    .find(|id| **id != c1 && **id != c2)
                    .unwrap()
                    .clone(),
                aid.clone(),
            ));
            assert_exact_topology(
                &wait_topology(h, &all_containers, &all_networks, &with_recreated).await,
                &all_containers,
                &all_networks,
                &with_recreated,
            );
            for id in [c1, c2] {
                let _ = h
                    .docker
                    .remove_container(
                        &id,
                        Some(RemoveContainerOptions {
                            force: true,
                            ..Default::default()
                        }),
                    )
                    .await;
            }
        })
    })
    .await;
}

#[tokio::test]
async fn given_attached_container_when_topology_is_read_repeatedly_then_membership_matches_inspect()
{
    E2eHarness::run(|h| {
        Box::pin(async move {
            let baseline_networks = h.docker.list_networks::<String>(None).await.unwrap();
            let mut names: NetworkManifest = baseline_networks
                .into_iter()
                .filter_map(|n| Some((n.name?, n.id?)))
                .collect();
            fixtures::ensure_image(h).await;
            let n = h.unique_name("repeat-net");
            let nid = network(h, &n, "bridge", false).await;
            let c = container(
                h,
                &h.unique_name("repeat-c"),
                Some(&n),
                vec!["sh", "-c", "sleep 300"],
            )
            .await;
            let expected = BTreeSet::from([(c.clone(), nid.clone())]);
            let containers = BTreeSet::from([c.clone()]);
            names.insert(n, nid.clone());
            for _ in 0..3 {
                let body = wait_topology(h, &containers, &names, &expected).await;
                assert_exact_topology(&body, &containers, &names, &expected);
                let inspect = h
                    .docker
                    .inspect_container(&c, None::<InspectContainerOptions>)
                    .await
                    .unwrap();
                let memberships: BTreeMap<_, _> = inspect
                    .network_settings
                    .unwrap()
                    .networks
                    .unwrap()
                    .into_iter()
                    .map(|(name, _)| (name, true))
                    .collect();
                assert_eq!(memberships.len(), 1);
            }
        })
    })
    .await;
}

// Given bridge, internal, host, none, and shared-namespace fixtures, when topology is read,
// then network metadata and edge/attachment memberships match independent Docker data.
#[tokio::test]
async fn given_network_modes_when_topology_read_then_metadata_and_memberships_match_docker() {
    E2eHarness::run(|h| {
        Box::pin(async move {
            fixtures::ensure_image(h).await;
            let bridge = h.unique_name("mode-bridge");
            let internal = h.unique_name("mode-internal");
            let bridge_id = network(h, &bridge, "bridge", false).await;
            let internal_id = network(h, &internal, "bridge", true).await;
            let bridge_c = container(
                h,
                &h.unique_name("mode-bridge-c"),
                Some(&bridge),
                vec!["sh", "-c", "sleep 300"],
            )
            .await;
            let internal_c = container(
                h,
                &h.unique_name("mode-internal-c"),
                Some(&internal),
                vec!["sh", "-c", "sleep 300"],
            )
            .await;
            let host_c = container(
                h,
                &h.unique_name("mode-host-c"),
                Some("host"),
                vec!["sh", "-c", "sleep 300"],
            )
            .await;
            let none_c = container(
                h,
                &h.unique_name("mode-none-c"),
                Some("none"),
                vec!["sh", "-c", "sleep 300"],
            )
            .await;
            let shared_name = h.unique_name("mode-shared-c");
            let shared_c = create_container(
                h,
                &shared_name,
                vec!["sh", "-c", "sleep 300"],
                HostConfig {
                    network_mode: Some(format!("container:{bridge_c}")),
                    ..Default::default()
                },
                None,
            )
            .await;
            let fixture_ids = BTreeSet::from([
                bridge_c.clone(),
                internal_c.clone(),
                host_c.clone(),
                none_c.clone(),
                shared_c.clone(),
            ]);

            let docker_networks = h.docker.list_networks::<String>(None).await.unwrap();
            let network_manifest: NetworkManifest = docker_networks
                .iter()
                .filter_map(|network| Some((network.name.clone()?, network.id.clone()?)))
                .collect();
            assert_eq!(network_manifest.get(&bridge), Some(&bridge_id));
            assert_eq!(network_manifest.get(&internal), Some(&internal_id));

            let body = topology(h).await;
            let api_networks: BTreeMap<_, _> = body["networks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|network| {
                    (
                        network["name"].as_str().unwrap().to_owned(),
                        network,
                    )
                })
                .collect();

            for (name, id, internal_only) in [
                (&bridge, &bridge_id, false),
                (&internal, &internal_id, true),
            ] {
                let docker_network = docker_networks
                    .iter()
                    .find(|network| network.id.as_deref() == Some(id.as_str()))
                    .unwrap_or_else(|| panic!("independent Docker list omitted {name}"));
                let docker_configs = docker_network
                    .ipam
                    .as_ref()
                    .and_then(|ipam| ipam.config.as_ref());
                let docker_subnet = docker_configs
                    .and_then(|configs| configs.first())
                    .and_then(|config| config.subnet.clone());
                assert!(
                    docker_subnet.is_some(),
                    "independent Docker must report a subnet for {name}"
                );
                let docker_ipam: Option<Vec<_>> = docker_configs
                    .map(|configs| {
                        configs
                            .iter()
                            .map(|config| {
                                (
                                    normalized(config.subnet.as_deref()).map(str::to_owned),
                                    normalized(config.gateway.as_deref()).map(str::to_owned),
                                )
                            })
                            .collect()
                    })
                    .filter(|configs: &Vec<_>| !configs.is_empty());
                assert_eq!(docker_network.name.as_deref(), Some(name.as_str()));
                assert_eq!(docker_network.driver.as_deref(), Some("bridge"));
                assert_eq!(docker_network.scope.as_deref(), Some("local"));
                assert_eq!(docker_network.internal, Some(internal_only));

                let api_network = api_networks
                    .get(name.as_str())
                    .unwrap_or_else(|| panic!("API omitted fixture network {name}: {body}"));
                assert_eq!(api_network["id"].as_str(), Some(id.as_str()));
                assert_eq!(
                    api_network["driver"].as_str(),
                    docker_network.driver.as_deref()
                );
                assert_eq!(
                    api_network["scope"].as_str(),
                    docker_network.scope.as_deref()
                );
                assert_eq!(api_network["internal"].as_bool(), docker_network.internal);
                assert_eq!(api_network["subnet"].as_str(), docker_subnet.as_deref());
                let api_ipam: Option<Vec<_>> = match &api_network["ipam_configs"] {
                    Value::Null => None,
                    Value::Array(configs) => Some(
                        configs
                            .iter()
                            .map(|config| {
                                (
                                    normalized(config["subnet"].as_str()).map(str::to_owned),
                                    normalized(config["gateway"].as_str()).map(str::to_owned),
                                )
                            })
                            .collect(),
                    ),
                    other => panic!("network ipam_configs must be an array or null: {other}"),
                };
                assert_eq!(
                    api_ipam, docker_ipam,
                    "network IPAM configs must match independent Docker list data for {name}: {body}"
                );
            }

            let mut expected = BTreeSet::new();
            let mut inspected_attachments: BTreeMap<
                (String, String),
                bollard::models::EndpointSettings,
            > = BTreeMap::new();
            let mut inspected_states: BTreeMap<String, String> = BTreeMap::new();
            for id in &fixture_ids {
                let inspect = h
                    .docker
                    .inspect_container(id, None::<InspectContainerOptions>)
                    .await
                    .unwrap();
                inspected_states.insert(id.clone(), fixtures::state_name(&inspect));
                let attachments = inspect
                    .network_settings
                    .unwrap_or_default()
                    .networks
                    .unwrap_or_default();
                for (name, attachment) in attachments {
                    if let Some(network_id) = network_manifest.get(&name) {
                        expected.insert((id.clone(), network_id.clone()));
                    }
                    if name == bridge || name == internal {
                        inspected_attachments.insert((id.clone(), name), attachment);
                    }
                }
            }

            let fixture_edge_records: Vec<_> = body["edges"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|edge| {
                    fixture_ids.contains(edge["source"].as_str().unwrap())
                })
                .collect();
            let actual_fixture_edges: BTreeSet<_> = edge_set(&body)
                .into_iter()
                .filter(|(source, _)| fixture_ids.contains(source))
                .collect();
            assert_eq!(
                actual_fixture_edges.len(),
                fixture_edge_records.len(),
                "duplicate fixture topology edges: {body}"
            );
            assert_eq!(
                actual_fixture_edges, expected,
                "exact fixture edges derived from independent Docker inspect: {body}"
            );
            assert!(expected.contains(&(bridge_c.clone(), bridge_id.clone())));
            assert!(expected.contains(&(internal_c.clone(), internal_id.clone())));
            assert!(
                !actual_fixture_edges.iter().any(|(source, target)| {
                    (source == &host_c || source == &none_c)
                        && (target == &bridge_id || target == &internal_id)
                }),
                "host and none modes must not attach to custom fixture networks: {body}"
            );
            assert!(
                !expected.is_empty(),
                "network-mode fixtures must exercise at least one membership"
            );

            let all_container_ids: BTreeSet<_> = body["containers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|container| container["id"].as_str().unwrap().to_owned())
                .collect();
            let all_network_ids: BTreeSet<_> =
                api_networks.values().map(|network| network["id"].as_str().unwrap().to_owned()).collect();
            assert!(
                edge_set(&body)
                    .iter()
                    .all(|(source, target)| all_container_ids.contains(source)
                        && all_network_ids.contains(target)),
                "topology edge has a dangling endpoint: {body}"
            );
            let api_containers = body["containers"].as_array().unwrap();
            for id in &fixture_ids {
                let container = api_containers
                    .iter()
                    .find(|container| container["id"] == id.as_str())
                    .unwrap_or_else(|| {
                        panic!("running network-mode fixture {id} must be visible: {body}")
                    });
                assert_eq!(
                    container["state"].as_str(),
                    Some(
                        inspected_states
                            .get(id)
                            .expect("independent Docker inspect state")
                            .as_str()
                    ),
                    "container state must match independent Docker inspect for {id}: {body}"
                );
            }
            for ((container_id, network_name), truth) in &inspected_attachments {
                let container = api_containers
                    .iter()
                    .find(|container| container["id"] == container_id.as_str())
                    .unwrap_or_else(|| panic!("API omitted container {container_id}: {body}"));
                let attachment = container["networks"]
                    .as_object()
                    .and_then(|networks| networks.get(network_name))
                    .unwrap_or_else(|| {
                        panic!(
                            "API omitted {network_name} attachment for {container_id}: {body}"
                        )
                    });
                let truth_network_id = network_manifest.get(network_name).unwrap_or_else(|| {
                    panic!("independent Docker list omitted network {network_name}")
                });
                assert_eq!(
                    truth.network_id.as_deref(),
                    Some(truth_network_id.as_str()),
                    "independent Docker inspect and list disagree on {network_name}"
                );
                assert_eq!(
                    attachment["network_id"].as_str(),
                    Some(truth_network_id.as_str()),
                    "attachment network_id must match independent Docker inspect for {container_id} on {network_name}: {body}"
                );
                assert_eq!(
                    attachment["ipv4_address"].as_str(),
                    normalized(truth.ip_address.as_deref()),
                    "attachment ipv4_address must match independent Docker inspect for {container_id} on {network_name}: {body}"
                );
                assert_eq!(
                    attachment["ipv6_address"].as_str(),
                    normalized(truth.global_ipv6_address.as_deref()),
                    "attachment ipv6_address must match independent Docker inspect for {container_id} on {network_name}: {body}"
                );
                assert_eq!(
                    attachment["ip_address"].as_str(),
                    truth.ip_address.as_deref(),
                    "legacy attachment IP must match independent Docker inspect for {container_id} on {network_name}: {body}"
                );
                assert_eq!(
                    attachment["mac_address"].as_str(),
                    truth.mac_address.as_deref(),
                    "attachment MAC must match independent Docker inspect for {container_id} on {network_name}: {body}"
                );
            }
        })
    })
    .await;
}

// Given containers with absent, unpublished, TCP, and UDP port metadata, when the API is read,
// then each port record agrees with Docker's independent inspect response.
#[tokio::test]
async fn given_port_variants_when_containers_read_then_metadata_matches_docker() {
    E2eHarness::run(|h| {
        Box::pin(async move {
            fixtures::ensure_image(h).await;
            use bollard::models::PortBinding;
            let mut ids = Vec::new();
            ids.push(
                container(
                    h,
                    &h.unique_name("ports-none"),
                    None,
                    vec!["sh", "-c", "sleep 300"],
                )
                .await,
            );
            ids.push(
                create_container(
                    h,
                    &h.unique_name("ports-unpublished"),
                    vec!["sh", "-c", "sleep 300"],
                    HostConfig::default(),
                    Some(vec!["8080/tcp", "8081/udp"]),
                )
                .await,
            );
            let bindings = HashMap::from([
                (
                    "8080/tcp".to_owned(),
                    Some(vec![PortBinding {
                        host_ip: Some("0.0.0.0".into()),
                        host_port: Some("0".into()),
                    }]),
                ),
                (
                    "8081/udp".to_owned(),
                    Some(vec![PortBinding {
                        host_ip: Some("0.0.0.0".into()),
                        host_port: Some("0".into()),
                    }]),
                ),
            ]);
            ids.push(
                create_container(
                    h,
                    &h.unique_name("ports-published"),
                    vec!["sh", "-c", "sleep 300"],
                    HostConfig {
                        port_bindings: Some(bindings),
                        ..Default::default()
                    },
                    Some(vec!["8080/tcp", "8081/udp"]),
                )
                .await,
            );
            let deadline = Instant::now() + Duration::from_secs(20);
            let body = loop {
                let (status, body) = h.get("/api/containers").await;
                assert_eq!(status, reqwest::StatusCode::OK);
                let got: BTreeSet<_> = body
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| c["id"].as_str().unwrap().to_owned())
                    .filter(|id| ids.contains(id))
                    .collect();
                if got.len() == ids.len() {
                    break body;
                }
                assert!(
                    Instant::now() < deadline,
                    "container list did not converge: {body}"
                );
                sleep(Duration::from_millis(100)).await;
            };
            for id in ids {
                let row = body
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|c| c["id"] == id)
                    .unwrap();
                let api_ports = row["ports"].as_array().unwrap();
                let inspect = h
                    .docker
                    .inspect_container(&id, None::<InspectContainerOptions>)
                    .await
                    .unwrap();
                assert_eq!(
                    row["state"].as_str(),
                    Some(fixtures::state_name(&inspect).as_str()),
                    "container state must match independent Docker inspect for {id}: {row}"
                );
                let truth = inspect
                    .network_settings
                    .unwrap()
                    .ports
                    .unwrap_or_default();
                let exposed = inspect.config.unwrap().exposed_ports.unwrap_or_default();
                for key in exposed.keys() {
                    let (private, protocol) = key.split_once('/').unwrap();
                    let matching: Vec<_> = api_ports
                        .iter()
                        .filter(|p| {
                            p["private"].as_u64() == private.parse::<u64>().ok()
                                && p["protocol"] == protocol
                        })
                        .collect();
                    let bindings = truth.get(key).and_then(Option::as_ref);
                    assert_eq!(matching.len(), bindings.map_or(1, Vec::len));
                    for port in &matching {
                        assert!(
                            port.get("host_ip").is_some(),
                            "port must expose host_ip: {port}"
                        );
                        assert_eq!(port["private"].as_u64(), private.parse::<u64>().ok());
                        assert_eq!(port["protocol"], protocol);
                    }
                    if let Some(bindings) = bindings {
                        for binding in bindings {
                            assert!(
                                matching.iter().any(|p| {
                                    p["public"].as_u64().map(|v| v.to_string())
                                        == binding.host_port
                                        && normalized(p["host_ip"].as_str())
                                            == normalized(binding.host_ip.as_deref())
                                }),
                                "port binding must preserve public host port and host IP: {key} {binding:?} {matching:?}"
                            );
                        }
                    } else {
                        assert!(
                            matching.iter().all(|p| {
                                p["public"].is_null() && p["host_ip"].is_null()
                            }),
                            "unpublished port should have no public port or host IP: {key}"
                        );
                    }
                }
                let expected_rows: usize = exposed
                    .keys()
                    .map(|key| truth.get(key).and_then(Option::as_ref).map_or(1, Vec::len))
                    .sum();
                assert_eq!(
                    api_ports.len(),
                    expected_rows,
                    "API port row count matches inspect metadata"
                );
            }
        })
    })
    .await;
}
