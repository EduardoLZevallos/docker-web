use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    time::Duration,
};

use bollard::{
    container::{
        Config as ContainerConfig, CreateContainerOptions, InspectContainerOptions,
        ListContainersOptions, RemoveContainerOptions,
    },
    models::HostConfig,
    network::{ConnectNetworkOptions, CreateNetworkOptions, DisconnectNetworkOptions},
};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::time::{Instant, sleep};

use super::E2eHarness;

const IMAGE: &str = "alpine:3.20";

async fn ensure_image(h: &E2eHarness) {
    let mut stream = h.docker.create_image(
        Some(bollard::image::CreateImageOptions {
            from_image: IMAGE,
            ..Default::default()
        }),
        None,
        None,
    );
    while let Some(result) = stream.next().await {
        result.unwrap_or_else(|e| panic!("pull {IMAGE}: {e}"));
    }
}

async fn network(h: &E2eHarness, name: &str, driver: &str, internal: bool) -> String {
    let mut labels = h.labels();
    labels.insert("com.docker-web.e2e.fixture".into(), "topology".into());
    h.docker
        .create_network(CreateNetworkOptions {
            name: name.to_owned(),
            driver: driver.to_owned(),
            internal,
            labels,
            ..Default::default()
        })
        .await
        .unwrap()
        .id
        .unwrap()
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
    let mut labels = h.labels();
    labels.insert("com.docker-web.e2e.fixture".into(), "topology".into());
    let exposed_ports = ports.map(|ports| {
        ports
            .into_iter()
            .map(|k| (k.to_owned(), HashMap::new()))
            .collect()
    });
    let response = h
        .docker
        .create_container(
            Some(CreateContainerOptions {
                name: name.to_owned(),
                platform: None,
            }),
            ContainerConfig {
                image: Some(IMAGE.to_owned()),
                cmd: Some(command.into_iter().map(str::to_owned).collect()),
                exposed_ports,
                labels: Some(labels),
                host_config: Some(host),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    h.docker
        .start_container::<String>(&response.id, None)
        .await
        .unwrap();
    response.id
}

async fn topology(h: &E2eHarness) -> Value {
    let (status, body) = h.get("/api/topology").await;
    assert_eq!(status, reqwest::StatusCode::OK, "topology response: {body}");
    body
}

fn edge_set(body: &Value) -> BTreeSet<(String, String)> {
    body["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["source"].as_str().unwrap().to_owned(),
                e["target"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

type NetworkManifest = BTreeMap<String, String>;

fn assert_exact_topology(
    body: &Value,
    expected_containers: &BTreeSet<String>,
    expected_networks: &NetworkManifest,
    expected_edges: &BTreeSet<(String, String)>,
) {
    let containers = body["containers"].as_array().unwrap();
    let networks = body["networks"].as_array().unwrap();
    let actual_container_ids: BTreeSet<_> = containers
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_owned())
        .collect();
    let actual_networks: NetworkManifest = networks
        .iter()
        .map(|n| {
            (
                n["name"].as_str().unwrap().to_owned(),
                n["id"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(&actual_container_ids, expected_containers, "container IDs");
    assert_eq!(&actual_networks, expected_networks, "network names and IDs");
    assert_eq!(
        actual_networks.values().collect::<BTreeSet<_>>().len(),
        networks.len(),
        "duplicate network IDs"
    );
    assert_eq!(
        actual_container_ids.len(),
        containers.len(),
        "duplicate container IDs"
    );
    assert_eq!(
        edge_set(body).len(),
        body["edges"].as_array().unwrap().len(),
        "duplicate topology edges"
    );
    let valid_sources = &actual_container_ids;
    let valid_targets: BTreeSet<_> = actual_networks.values().cloned().collect();
    assert!(
        edge_set(body)
            .iter()
            .all(|(s, t)| valid_sources.contains(s) && valid_targets.contains(t)),
        "edge has dangling endpoint"
    );
    assert_eq!(&edge_set(body), expected_edges, "exact topology edge set");
    assert!(body["timestamp"].as_str().is_some());
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
        let ids: BTreeSet<_> = body["containers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap().to_owned())
            .collect();
        let manifest: NetworkManifest = body["networks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| {
                (
                    n["name"].as_str().unwrap().to_owned(),
                    n["id"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        if ids == *expected_containers
            && manifest == *expected_networks
            && edge_set(&body) == *expected_edges
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
    ensure_image(h).await;
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
    let body = wait_topology(h, &all_ids, &all_networks, &expected).await;
    assert_exact_topology(&body, &all_ids, &all_networks, &expected);
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
            ensure_image(h).await;
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
            ensure_image(h).await;
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
            ensure_image(h).await;
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
                let docker_subnet = docker_network
                    .ipam
                    .as_ref()
                    .and_then(|ipam| ipam.config.as_ref())
                    .and_then(|configs| configs.first())
                    .and_then(|config| config.subnet.clone());
                assert!(
                    docker_subnet.is_some(),
                    "independent Docker must report a subnet for {name}"
                );
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
            }

            let mut expected = BTreeSet::new();
            let mut inspected_attachments: BTreeMap<
                (String, String),
                (Option<String>, Option<String>),
            > = BTreeMap::new();
            for id in &fixture_ids {
                let inspect = h
                    .docker
                    .inspect_container(id, None::<InspectContainerOptions>)
                    .await
                    .unwrap();
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
                        inspected_attachments
                            .insert((id.clone(), name), (attachment.ip_address, attachment.mac_address));
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
                assert!(
                    api_containers.iter().any(|container| container["id"] == id.as_str()),
                    "running network-mode fixture {id} must be visible: {body}"
                );
            }
            for ((container_id, network_name), (truth_ip, truth_mac)) in
                &inspected_attachments
            {
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
                assert!(
                    attachment.get("ip_address").is_some(),
                    "attachment must expose ip_address: {attachment}"
                );
                assert!(
                    attachment.get("mac_address").is_some(),
                    "attachment must expose mac_address: {attachment}"
                );
                if let Some(ip) = truth_ip {
                    assert_eq!(
                        attachment["ip_address"].as_str(),
                        Some(ip.as_str()),
                        "attachment IP must match independent Docker inspect for {container_id} on {network_name}: {body}"
                    );
                }
                if let Some(mac) = truth_mac {
                    assert_eq!(
                        attachment["mac_address"].as_str(),
                        Some(mac.as_str()),
                        "attachment MAC must match independent Docker inspect for {container_id} on {network_name}: {body}"
                    );
                }
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
            ensure_image(h).await;
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
                let api_ports = body
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|c| c["id"] == id)
                    .unwrap()["ports"]
                    .as_array()
                    .unwrap();
                let inspect = h
                    .docker
                    .inspect_container(&id, None::<InspectContainerOptions>)
                    .await
                    .unwrap();
                let truth = inspect.network_settings.unwrap().ports.unwrap_or_default();
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
                    if let Some(bindings) = bindings {
                        for binding in bindings {
                            assert!(
                                matching
                                    .iter()
                                    .any(|p| p["public"].as_u64().map(|v| v.to_string())
                                        == binding.host_port)
                            );
                        }
                    } else {
                        assert!(
                            matching.iter().all(|p| p["public"].is_null()),
                            "unpublished port should have no public port: {key}"
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
