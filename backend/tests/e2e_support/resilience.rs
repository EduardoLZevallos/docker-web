use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    future::Future,
    panic::AssertUnwindSafe,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use bollard::{
    Docker,
    container::{
        Config as ContainerConfig, CreateContainerOptions, InspectContainerOptions,
        ListContainersOptions, RemoveContainerOptions,
    },
    image::ListImagesOptions,
    models::{ContainerSummary, HostConfig},
    network::{
        ConnectNetworkOptions, CreateNetworkOptions, DisconnectNetworkOptions, ListNetworksOptions,
    },
    volume::ListVolumesOptions,
};
use futures_util::{FutureExt, StreamExt};
use serde_json::Value;
use tokio::time::{Instant, sleep};

use super::{E2eHarness, StartupFault};

const IMAGE: &str = "alpine:3.20";
const DEADLINE: Duration = Duration::from_secs(40);
const OWNED_PREFIX: &str = "docker-web-e2e";

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
        result.unwrap_or_else(|error| panic!("pull {IMAGE}: {error}"));
    }
}

async fn fixture(h: &E2eHarness, stem: &str) -> (String, String, String, String) {
    let network_name = h.unique_name(&format!("{stem}-net"));
    let mut labels = h.labels();
    labels.insert("com.docker-web.e2e.fixture".into(), stem.into());
    let network_id = h
        .docker
        .create_network(CreateNetworkOptions {
            name: network_name.clone(),
            driver: "bridge".into(),
            internal: false,
            labels: labels.clone(),
            ..Default::default()
        })
        .await
        .unwrap_or_else(|error| panic!("create {network_name}: {error}"))
        .id
        .unwrap();
    let container_name = h.unique_name(stem);
    let response = h
        .docker
        .create_container(
            Some(CreateContainerOptions {
                name: container_name.clone(),
                platform: None,
            }),
            ContainerConfig {
                image: Some(IMAGE.into()),
                cmd: Some(vec!["sh".into(), "-c".into(), "sleep 300".into()]),
                labels: Some(labels),
                host_config: Some(HostConfig {
                    network_mode: Some(network_name.clone()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap_or_else(|error| panic!("create {container_name}: {error}"));
    h.docker
        .start_container::<String>(&response.id, None)
        .await
        .unwrap();
    (container_name, network_name, network_id, response.id)
}

fn edge_set(body: &Value) -> BTreeSet<(String, String)> {
    body["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|edge| {
            (
                edge["source"].as_str().unwrap().to_owned(),
                edge["target"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn assert_timestamp(value: &Value) {
    let timestamp = value.as_str().expect("timestamp must be a string");
    time::OffsetDateTime::parse(timestamp, &time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|error| panic!("timestamp must be RFC3339 ({timestamp}): {error}"));
}

async fn wait_health(h: &E2eHarness, status: &str, docker: bool, timeout: Duration) -> Value {
    let end = Instant::now() + timeout;
    loop {
        let (code, body) = h.get("/api/health").await;
        assert_eq!(
            code,
            reqwest::StatusCode::OK,
            "health endpoint remains live: {body}"
        );
        if body["status"] == status && body["docker"] == docker {
            assert_eq!(body["service"], "docker-web-api");
            assert_timestamp(&body["timestamp"]);
            return body;
        }
        assert!(
            Instant::now() < end,
            "health did not reach status={status} docker={docker}; last={body}"
        );
        sleep(Duration::from_millis(200)).await;
    }
}

fn assert_connection_error(status: reqwest::StatusCode, body: &Value, path: &str) {
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

#[derive(Clone, Debug)]
struct ExpectedContainerMetadata {
    name: String,
    image: String,
    created: i64,
}

#[derive(Default, Debug)]
struct ReaderStats {
    attempts: usize,
    ok: usize,
    unavailable: usize,
}

struct RemovedFixture {
    container_id: String,
    container_name: String,
    network_id: String,
    network_name: String,
}

fn concurrent_fixture_labels(h: &E2eHarness) -> HashMap<String, String> {
    let mut labels = h.labels();
    labels.insert(
        "com.docker-web.e2e.fixture".to_owned(),
        "concurrent-readers".to_owned(),
    );
    labels
}

fn run_label_filter(h: &E2eHarness) -> HashMap<String, Vec<String>> {
    let labels = h.labels();
    let (key, value) = labels.iter().next().expect("harness run label");
    HashMap::from([("label".to_owned(), vec![format!("{key}={value}")])])
}

async fn concurrent_network(
    h: &E2eHarness,
    name: &str,
    labels: &HashMap<String, String>,
) -> String {
    h.docker
        .create_network(CreateNetworkOptions {
            name: name.to_owned(),
            driver: "bridge".to_owned(),
            internal: false,
            labels: labels.clone(),
            ..Default::default()
        })
        .await
        .unwrap_or_else(|error| panic!("create concurrent fixture network {name}: {error}"))
        .id
        .expect("created network has an ID")
}

async fn create_concurrent_container(
    h: &E2eHarness,
    name: &str,
    network: &str,
    labels: &HashMap<String, String>,
) -> String {
    h.docker
        .create_container(
            Some(CreateContainerOptions {
                name: name.to_owned(),
                platform: None,
            }),
            ContainerConfig {
                image: Some(IMAGE.to_owned()),
                cmd: Some(vec![
                    "sh".to_owned(),
                    "-c".to_owned(),
                    "sleep 300".to_owned(),
                ]),
                labels: Some(labels.clone()),
                host_config: Some(HostConfig {
                    network_mode: Some(network.to_owned()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap_or_else(|error| panic!("create concurrent fixture container {name}: {error}"))
        .id
}

async fn start_concurrent_container(h: &E2eHarness, id: &str) {
    h.docker
        .start_container::<String>(id, None)
        .await
        .unwrap_or_else(|error| panic!("start concurrent fixture container {id}: {error}"));
}

fn expected_container_metadata(summary: &ContainerSummary) -> ExpectedContainerMetadata {
    ExpectedContainerMetadata {
        name: summary
            .names
            .as_ref()
            .and_then(|names| names.first())
            .map(|name| name.trim_start_matches('/').to_owned())
            .unwrap_or_default(),
        image: summary.image.clone().unwrap_or_default(),
        created: summary.created.unwrap_or_default(),
    }
}

async fn list_fixture_containers(
    h: &E2eHarness,
    filter: &HashMap<String, Vec<String>>,
    all: bool,
) -> Vec<ContainerSummary> {
    h.docker
        .list_containers(Some(ListContainersOptions {
            all,
            filters: filter.clone(),
            ..Default::default()
        }))
        .await
        .unwrap_or_else(|error| panic!("list concurrent fixture containers: {error}"))
}

async fn wait_for_initial_containers(
    h: &E2eHarness,
    filter: &HashMap<String, Vec<String>>,
    expected: &BTreeSet<String>,
    timeout: Duration,
) -> Vec<ContainerSummary> {
    let deadline = Instant::now() + timeout;
    loop {
        let listed = list_fixture_containers(h, filter, true).await;
        let ids: BTreeSet<_> = listed
            .iter()
            .filter_map(|summary| summary.id.clone())
            .collect();
        if &ids == expected {
            return listed;
        }
        assert!(
            Instant::now() < deadline,
            "initial concurrent fixture containers did not appear in Docker: expected={expected:?} actual={ids:?}"
        );
        sleep(Duration::from_millis(100)).await;
    }
}

async fn metadata_by_id(
    h: &E2eHarness,
    filter: &HashMap<String, Vec<String>>,
    id: &str,
) -> ExpectedContainerMetadata {
    let listed = list_fixture_containers(h, filter, true).await;
    let summary = listed
        .iter()
        .find(|summary| summary.id.as_deref() == Some(id))
        .unwrap_or_else(|| {
            panic!("created container {id} is absent from the independent Docker list")
        });
    expected_container_metadata(summary)
}

fn validate_reader_topology(body: &Value, known: &HashMap<String, ExpectedContainerMetadata>) {
    assert_timestamp(&body["timestamp"]);
    let containers = body["containers"]
        .as_array()
        .unwrap_or_else(|| panic!("topology containers must be an array: {body}"));
    let networks = body["networks"]
        .as_array()
        .unwrap_or_else(|| panic!("topology networks must be an array: {body}"));
    let edges = body["edges"]
        .as_array()
        .unwrap_or_else(|| panic!("topology edges must be an array: {body}"));

    let mut container_ids = BTreeSet::new();
    for container in containers {
        let id = container["id"]
            .as_str()
            .unwrap_or_else(|| panic!("container ID must be a string: {container}"));
        assert!(
            !id.is_empty(),
            "container ID must not be empty: {container}"
        );
        assert!(
            container_ids.insert(id.to_owned()),
            "duplicate container ID {id}: {body}"
        );
        let name = container["name"]
            .as_str()
            .unwrap_or_else(|| panic!("container name must be a string: {container}"));
        let image = container["image"]
            .as_str()
            .unwrap_or_else(|| panic!("container image must be a string: {container}"));
        let created = container["created"]
            .as_i64()
            .unwrap_or_else(|| panic!("container created must be an integer: {container}"));
        let expected = known
            .get(id)
            .unwrap_or_else(|| panic!("unexpected container {id} in the isolated daemon: {body}"));
        assert_eq!(
            name, expected.name,
            "container {id} name must remain stable"
        );
        assert_eq!(
            image, expected.image,
            "container {id} image must remain stable"
        );
        assert_eq!(
            created, expected.created,
            "container {id} created timestamp must remain stable"
        );
    }

    let mut network_ids = BTreeSet::new();
    let mut network_names = BTreeSet::new();
    for network in networks {
        let id = network["id"]
            .as_str()
            .unwrap_or_else(|| panic!("network ID must be a string: {network}"));
        let name = network["name"]
            .as_str()
            .unwrap_or_else(|| panic!("network name must be a string: {network}"));
        assert!(!id.is_empty(), "network ID must not be empty: {network}");
        assert!(
            !name.is_empty(),
            "network name must not be empty: {network}"
        );
        assert!(
            network_ids.insert(id.to_owned()),
            "duplicate network ID {id}: {body}"
        );
        assert!(
            network_names.insert(name.to_owned()),
            "duplicate network name {name}: {body}"
        );
    }

    let mut edge_tuples = BTreeSet::new();
    for edge in edges {
        let source = edge["source"]
            .as_str()
            .unwrap_or_else(|| panic!("edge source must be a string: {edge}"));
        let target = edge["target"]
            .as_str()
            .unwrap_or_else(|| panic!("edge target must be a string: {edge}"));
        assert!(
            container_ids.contains(source),
            "edge source {source} is absent from containers: {body}"
        );
        assert!(
            network_ids.contains(target),
            "edge target {target} is absent from networks: {body}"
        );
        assert!(
            edge_tuples.insert((source.to_owned(), target.to_owned())),
            "duplicate edge {source}->{target}: {body}"
        );
    }
}

fn topology_is_converged(
    body: &Value,
    expected_containers: &BTreeMap<String, ExpectedContainerMetadata>,
    expected_networks: &BTreeMap<String, String>,
    expected_edges: &BTreeSet<(String, String)>,
) -> bool {
    let Some(containers) = body["containers"].as_array() else {
        return false;
    };
    let Some(networks) = body["networks"].as_array() else {
        return false;
    };
    let Some(edges) = body["edges"].as_array() else {
        return false;
    };
    let container_ids: BTreeSet<_> = containers
        .iter()
        .filter_map(|container| container["id"].as_str().map(str::to_owned))
        .collect();
    let network_manifest: BTreeMap<_, _> = networks
        .iter()
        .filter_map(|network| {
            Some((
                network["name"].as_str()?.to_owned(),
                network["id"].as_str()?.to_owned(),
            ))
        })
        .collect();
    let edge_tuples: BTreeSet<_> = edges
        .iter()
        .filter_map(|edge| {
            Some((
                edge["source"].as_str()?.to_owned(),
                edge["target"].as_str()?.to_owned(),
            ))
        })
        .collect();
    container_ids == expected_containers.keys().cloned().collect::<BTreeSet<_>>()
        && network_manifest == *expected_networks
        && edge_tuples == *expected_edges
        && container_ids.len() == containers.len()
        && network_manifest.len() == networks.len()
        && edge_tuples.len() == edges.len()
}

fn assert_converged_topology(
    body: &Value,
    expected_containers: &BTreeMap<String, ExpectedContainerMetadata>,
    expected_networks: &BTreeMap<String, String>,
    fixture_networks: &BTreeMap<String, String>,
    expected_edges: &BTreeSet<(String, String)>,
    removed: &RemovedFixture,
) {
    assert_timestamp(&body["timestamp"]);
    let containers = body["containers"].as_array().unwrap();
    let networks = body["networks"].as_array().unwrap();
    let edges = body["edges"].as_array().unwrap();

    let expected_container_ids: BTreeSet<_> = expected_containers.keys().cloned().collect();
    let actual_container_ids: BTreeSet<_> = containers
        .iter()
        .map(|container| container["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        actual_container_ids, expected_container_ids,
        "exact surviving fixture container IDs: {body}"
    );
    assert_eq!(
        actual_container_ids.len(),
        containers.len(),
        "duplicate container IDs: {body}"
    );
    for (id, expected) in expected_containers {
        let record = containers
            .iter()
            .find(|container| container["id"] == *id)
            .unwrap_or_else(|| panic!("expected container {id} is absent: {body}"));
        assert_eq!(
            record["name"].as_str(),
            Some(expected.name.as_str()),
            "container {id} name: {body}"
        );
        assert_eq!(
            record["image"].as_str(),
            Some(expected.image.as_str()),
            "container {id} image: {body}"
        );
        assert_eq!(
            record["created"].as_i64(),
            Some(expected.created),
            "container {id} created timestamp: {body}"
        );
    }

    let actual_networks: BTreeMap<_, _> = networks
        .iter()
        .map(|network| {
            (
                network["name"].as_str().unwrap().to_owned(),
                network["id"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        actual_networks, *expected_networks,
        "exact network manifest including built-ins: {body}"
    );
    assert_eq!(
        actual_networks.len(),
        networks.len(),
        "duplicate network names: {body}"
    );
    assert_eq!(
        actual_networks.values().collect::<BTreeSet<_>>().len(),
        networks.len(),
        "duplicate network IDs: {body}"
    );
    for (name, id) in fixture_networks {
        assert_eq!(
            actual_networks.get(name),
            Some(id),
            "fixture network {name} must remain present with its exact ID: {body}"
        );
    }

    let actual_edges = edge_set(body);
    assert_eq!(
        actual_edges.len(),
        edges.len(),
        "duplicate topology edges: {body}"
    );
    assert_eq!(
        actual_edges, *expected_edges,
        "exact fixture edge set: {body}"
    );
    let valid_targets: BTreeSet<_> = actual_networks.values().cloned().collect();
    assert!(
        actual_edges.iter().all(|(source, target)| {
            actual_container_ids.contains(source) && valid_targets.contains(target)
        }),
        "topology edge has a dangling endpoint: {body}"
    );

    assert!(
        !actual_container_ids.contains(&removed.container_id),
        "removed container {} must be absent: {body}",
        removed.container_id
    );
    assert!(
        !containers
            .iter()
            .any(|container| container["name"] == removed.container_name),
        "removed container name {} must be absent: {body}",
        removed.container_name
    );
    assert!(
        !actual_networks.contains_key(&removed.network_name),
        "removed network {} must be absent: {body}",
        removed.network_name
    );
    assert!(
        !actual_networks.values().any(|id| id == &removed.network_id),
        "removed network ID {} must be absent: {body}",
        removed.network_id
    );
    assert!(
        actual_edges.iter().all(|(source, target)| {
            source != &removed.container_id && target != &removed.network_id
        }),
        "removed fixture endpoints must leave no edges: {body}"
    );
}

async fn wait_for_converged_topology(
    h: &E2eHarness,
    expected_containers: &BTreeMap<String, ExpectedContainerMetadata>,
    expected_networks: &BTreeMap<String, String>,
    fixture_networks: &BTreeMap<String, String>,
    expected_edges: &BTreeSet<(String, String)>,
    removed: &RemovedFixture,
) -> Value {
    let deadline = Instant::now() + DEADLINE;
    let mut last_ok = None;
    loop {
        let (status, body) = h.get("/api/topology").await;
        match status {
            reqwest::StatusCode::OK => {
                if topology_is_converged(
                    &body,
                    expected_containers,
                    expected_networks,
                    expected_edges,
                ) {
                    assert_converged_topology(
                        &body,
                        expected_containers,
                        expected_networks,
                        fixture_networks,
                        expected_edges,
                        removed,
                    );
                    return body;
                }
                last_ok = Some(body);
            }
            reqwest::StatusCode::SERVICE_UNAVAILABLE => {
                assert_connection_error(status, &body, "/api/topology");
            }
            _ => panic!("unexpected topology status during convergence: {status}: {body}"),
        }
        if Instant::now() >= deadline {
            match last_ok {
                Some(body) => {
                    assert_converged_topology(
                        &body,
                        expected_containers,
                        expected_networks,
                        fixture_networks,
                        expected_edges,
                        removed,
                    );
                    panic!("topology did not converge within {DEADLINE:?}; last={body}");
                }
                None => panic!("no successful topology response within {DEADLINE:?}"),
            }
        }
        sleep(Duration::from_millis(150)).await;
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct OuterResources {
    containers: BTreeSet<String>,
    images: BTreeSet<String>,
    volumes: BTreeSet<String>,
}

async fn outer_resources(outer: &Docker) -> OuterResources {
    let containers = outer
        .list_containers(Some(ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        }))
        .await
        .expect("list outer containers")
        .into_iter()
        .flat_map(|summary| summary.names.unwrap_or_default())
        .map(|name| name.trim_start_matches('/').to_owned())
        .filter(|name| name.starts_with(OWNED_PREFIX))
        .collect();
    let images = outer
        .list_images(Some(ListImagesOptions::<String>::default()))
        .await
        .expect("list outer images")
        .into_iter()
        .flat_map(|image| image.repo_tags)
        .filter(|tag| tag.starts_with(OWNED_PREFIX))
        .collect();
    let volumes = outer
        .list_volumes(Some(ListVolumesOptions::<String>::default()))
        .await
        .expect("list outer volumes")
        .volumes
        .unwrap_or_default()
        .into_iter()
        .map(|volume| volume.name)
        .collect();
    OuterResources {
        containers,
        images,
        volumes,
    }
}

async fn assert_probe_left_no_owned_resources(before: &OuterResources, outer: &Docker) {
    let after = outer_resources(outer).await;
    assert_eq!(
        after.containers, before.containers,
        "owned outer test containers must be removed by the probe: before={:?} after={:?}",
        before.containers, after.containers
    );
    assert_eq!(
        after.images, before.images,
        "owned outer test images must be removed by the probe: before={:?} after={:?}",
        before.images, after.images
    );
    let new_volumes: BTreeSet<_> = after.volumes.difference(&before.volumes).cloned().collect();
    assert!(
        new_volumes.is_empty(),
        "probe left new outer anonymous volumes behind: {new_volumes:?}"
    );
}

fn panic_instead_of_returning_future(
    _h: &E2eHarness,
) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    panic!("intentional closure invocation panic")
}

// Given a running fixture, when the same DinD daemon stops and restarts, then the backend
// reports the outage, recovers without restarting, and discovers fresh marker workloads.
#[tokio::test]
async fn given_running_fixture_when_daemon_outage_and_restart_occur_then_backend_recovers_without_restarting()
 {
    E2eHarness::run(|h| Box::pin(async move {
        ensure_image(h).await;
        let (initial_name, initial_net, initial_net_id, initial_id) = fixture(h, "before-outage").await;
        let (health_code, health) = h.get("/api/health").await;
        assert_eq!(health_code, reqwest::StatusCode::OK, "{health}");
        assert_eq!(health["status"], "healthy");
        assert_eq!(health["docker"], true);
        assert_timestamp(&health["timestamp"]);
        let (config_code, config) = h.get("/api/config").await;
        assert_eq!(config_code, reqwest::StatusCode::OK, "{config}");
        assert_eq!(config["demo_mode"], false);
        assert_eq!(config["theme"], "light");
        let (topology_code, topology) = h.get("/api/topology").await;
        assert_eq!(topology_code, reqwest::StatusCode::OK, "{topology}");
        assert_timestamp(&topology["timestamp"]);
        assert!(topology["networks"].as_array().unwrap().iter().any(|net|
            net["name"] == initial_net && net["id"] == initial_net_id));
        let initial_record = topology["containers"].as_array().unwrap().iter()
            .find(|container| container["id"] == initial_id).unwrap();
        assert_eq!(initial_record["name"], initial_name);
        assert_eq!(initial_record["image"], IMAGE);
        assert!(initial_record["created"].as_i64().is_some());
        let initial_network = topology["networks"].as_array().unwrap().iter()
            .find(|net| net["id"] == initial_net_id).unwrap();
        assert_eq!(initial_network["driver"], "bridge");
        assert_eq!(initial_network["scope"], "local");
        assert_eq!(initial_network["internal"], false);
        assert!(initial_network["subnet"].as_str().is_some());
        assert!(edge_set(&topology).contains(&(initial_id.clone(), initial_net_id)));

        let backend_identity = h.backend_identity().await;
        h.stop_dind().await;
        wait_health(h, "degraded", false, Duration::from_secs(15)).await;
        for path in ["/api/containers", "/api/networks", "/api/topology"] {
            let (status, body) = h.get(path).await;
            assert_connection_error(status, &body, path);
        }
        let (config_code, config) = h.get("/api/config").await;
        assert_eq!(config_code, reqwest::StatusCode::OK, "config remains available during outage: {config}");
        assert_eq!(config["demo_mode"], false);

        h.start_dind().await;
        wait_health(h, "healthy", true, Duration::from_secs(60)).await;
        assert_eq!(h.backend_identity().await, backend_identity, "backend container identity/start time must not change");

        // Check the prior fixture directly before creating recovery markers. Container survival is
        // daemon/runtime dependent; if it survived, its exact Docker ID remains inspectable.
        match h.docker.inspect_container(&initial_id, None).await {
            Ok(inspect) => {
                assert_eq!(inspect.id.as_deref(), Some(initial_id.as_str()));
                assert_eq!(inspect.name.as_deref(), Some(format!("/{initial_name}").as_str()));
            }
            Err(error) => eprintln!("E2E recovery: pre-outage container was not retained by dockerd restart: {error}"),
        }

        let (recovered_name, recovered_net, recovered_net_id, recovered_id) = fixture(h, "after-outage").await;
        let deadline = Instant::now() + DEADLINE;
        loop {
            let (status, body) = h.get("/api/topology").await;
            assert_eq!(status, reqwest::StatusCode::OK, "recovery topology: {body}");
            assert_timestamp(&body["timestamp"]);
            let containers = body["containers"].as_array().unwrap();
            let networks = body["networks"].as_array().unwrap();
            let net_id = networks.iter().find(|net| net["name"] == recovered_net).map(|net| net["id"].as_str().unwrap());
            let edges = edge_set(&body);
            if containers.iter().any(|container| container["id"] == recovered_id)
                && net_id.is_some_and(|id| id == recovered_net_id && edges.contains(&(recovered_id.clone(), id.to_owned())))
            {
                assert!(containers.iter().any(|container| container["name"] == recovered_name));
                let recovered_record = containers.iter().find(|container| container["id"] == recovered_id).unwrap();
                assert_eq!(recovered_record["image"], IMAGE);
                assert!(recovered_record["created"].as_i64().is_some());
                let recovered_network = networks.iter().find(|network| network["id"] == recovered_net_id).unwrap();
                assert_eq!(recovered_network["driver"], "bridge");
                assert_eq!(recovered_network["scope"], "local");
                assert_eq!(recovered_network["internal"], false);
                assert!(recovered_network["subnet"].as_str().is_some());
                assert!(edges.iter().all(|(source, target)|
                    containers.iter().any(|container| container["id"] == *source)
                    && networks.iter().any(|network| network["id"] == *target)), "no dangling topology edges: {body}");
                break;
            }
            assert!(Instant::now() < deadline, "recovered container/network edge did not appear: {body}");
            sleep(Duration::from_millis(150)).await;
        }
    })).await;
}

// Given a running container, when it stops, starts, restarts, and is removed, then identity
// and network edges follow each transition while concurrent readers only see valid snapshots.
#[tokio::test]
async fn given_running_container_when_stopped_started_restarted_and_removed_then_identity_and_edges_follow()
 {
    E2eHarness::run(|h| {
        Box::pin(async move {
            ensure_image(h).await;
            let (container_name, network_name, network_id, id) = fixture(h, "sequence").await;
            let initial = h.docker.inspect_container(&id, None).await.unwrap();
            assert_eq!(initial.id.as_deref(), Some(id.as_str()));
            assert_eq!(
                initial.state.unwrap().status,
                Some(bollard::models::ContainerStateStatusEnum::RUNNING)
            );

            let topology = h.get("/api/topology").await.1;
            assert_timestamp(&topology["timestamp"]);
            assert!(
                topology["networks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|network| network["name"] == network_name && network["id"] == network_id)
            );
            assert!(edge_set(&topology).contains(&(id.clone(), network_id.clone())));

            h.docker
                .stop_container(&id, Some(bollard::container::StopContainerOptions { t: 0 }))
                .await
                .unwrap();
            let stopped = h.docker.inspect_container(&id, None).await.unwrap();
            assert_eq!(stopped.id.as_deref(), Some(id.as_str()));
            assert_eq!(
                stopped.state.unwrap().status,
                Some(bollard::models::ContainerStateStatusEnum::EXITED)
            );

            // Read through the API while the container transitions. Each response must be valid and
            // any edge must refer to nodes present in that same response; counts may be transient.
            let http = h.http.clone();
            let url = format!("{}/api/topology", h.base_url);
            let reader = tokio::spawn(async move {
                let mut observations = 0;
                for _ in 0..5 {
                    let response = http.get(&url).send().await.unwrap();
                    let status = response.status();
                    let body: Value = response.json().await.unwrap();
                    assert!(
                        status == reqwest::StatusCode::OK
                            || status == reqwest::StatusCode::SERVICE_UNAVAILABLE,
                        "{status}: {body}"
                    );
                    if status == reqwest::StatusCode::OK {
                        let containers = body["containers"].as_array().unwrap();
                        let networks = body["networks"].as_array().unwrap();
                        for edge in body["edges"].as_array().unwrap() {
                            assert!(
                                containers
                                    .iter()
                                    .any(|container| container["id"] == edge["source"])
                            );
                            assert!(
                                networks
                                    .iter()
                                    .any(|network| network["id"] == edge["target"])
                            );
                        }
                    }
                    observations += 1;
                    sleep(Duration::from_millis(100)).await;
                }
                observations
            });
            h.docker.start_container::<String>(&id, None).await.unwrap();
            let started = h.docker.inspect_container(&id, None).await.unwrap();
            assert_eq!(started.id.as_deref(), Some(id.as_str()));
            assert_eq!(
                started.state.unwrap().status,
                Some(bollard::models::ContainerStateStatusEnum::RUNNING)
            );
            let restored = h.get("/api/topology").await.1;
            assert!(
                restored["containers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|container| container["id"] == id && container["name"] == container_name)
            );
            assert!(edge_set(&restored).contains(&(id.clone(), network_id.clone())));

            h.docker
                .restart_container(
                    &id,
                    Some(bollard::container::RestartContainerOptions { t: 1 }),
                )
                .await
                .unwrap();
            let restarted = h.docker.inspect_container(&id, None).await.unwrap();
            assert_eq!(
                restarted.id.as_deref(),
                Some(id.as_str()),
                "restart keeps the original Docker ID"
            );
            assert_eq!(
                restarted.state.unwrap().status,
                Some(bollard::models::ContainerStateStatusEnum::RUNNING)
            );
            let after_restart = h.get("/api/topology").await.1;
            assert!(edge_set(&after_restart).contains(&(id.clone(), network_id)));

            h.docker
                .remove_container(
                    &id,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await
                .unwrap();
            let removed = h
                .docker
                .inspect_container(&id, Some(InspectContainerOptions { size: false }))
                .await;
            assert!(
                removed.is_err(),
                "removed fixture must be absent from Docker inspect"
            );
            let after_remove = h.get("/api/topology").await.1;
            assert!(
                !after_remove["containers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|container| container["id"] == id)
            );
            assert!(
                !edge_set(&after_remove)
                    .iter()
                    .any(|(source, _)| source == &id),
                "removed container leaves no topology edges"
            );
            assert_eq!(reader.await.unwrap(), 5);
        })
    })
    .await;
}

#[tokio::test]
async fn given_concurrent_readers_when_bounded_topology_mutations_run_then_snapshots_are_valid_and_final_state_converges()
 {
    E2eHarness::run(|h| {
        Box::pin(async move {
            ensure_image(h).await;
            let labels = concurrent_fixture_labels(h);
            let filter = run_label_filter(h);

            let net_a_name = h.unique_name("concurrent-net-a");
            let net_b_name = h.unique_name("concurrent-net-b");
            let net_a = concurrent_network(h, &net_a_name, &labels).await;
            let net_b = concurrent_network(h, &net_b_name, &labels).await;

            let c1_name = h.unique_name("concurrent-multihomed");
            let c2_name = h.unique_name("concurrent-net-a-only");
            let c3_name = h.unique_name("concurrent-net-b-only");
            let c1 = create_concurrent_container(h, &c1_name, &net_a_name, &labels).await;
            start_concurrent_container(h, &c1).await;
            h.docker
                .connect_network(
                    &net_b,
                    ConnectNetworkOptions {
                        container: c1.clone(),
                        ..Default::default()
                    },
                )
                .await
                .unwrap_or_else(|error| panic!("connect {c1} to {net_b_name}: {error}"));
            let c2 = create_concurrent_container(h, &c2_name, &net_a_name, &labels).await;
            start_concurrent_container(h, &c2).await;
            let c3 = create_concurrent_container(h, &c3_name, &net_b_name, &labels).await;
            start_concurrent_container(h, &c3).await;

            let initial_ids = BTreeSet::from([c1.clone(), c2.clone(), c3.clone()]);
            let initial = wait_for_initial_containers(
                h,
                &filter,
                &initial_ids,
                Duration::from_secs(20),
            )
            .await;
            let known: Arc<Mutex<HashMap<String, ExpectedContainerMetadata>>> = Arc::new(
                Mutex::new(
                    initial
                        .iter()
                        .map(|summary| {
                            (
                                summary.id.clone().expect("initial fixture container ID"),
                                expected_container_metadata(summary),
                            )
                        })
                        .collect(),
                ),
            );
            eprintln!(
                "concurrent fixture: networks {net_a_name}={net_a}, {net_b_name}={net_b}; containers c1={c1} (multihomed), c2={c2}, c3={c3}"
            );

            let running = Arc::new(AtomicBool::new(true));
            let reader_count = 4;
            let mut readers = Vec::new();
            for reader_index in 0..reader_count {
                let http = h.http.clone();
                let url = format!("{}/api/topology", h.base_url);
                let running = running.clone();
                let known = known.clone();
                readers.push(tokio::spawn(async move {
                    let mut stats = ReaderStats::default();
                    while running.load(Ordering::Relaxed) {
                        let response = http.get(&url).send().await.unwrap_or_else(|error| {
                            panic!("concurrent reader {reader_index} HTTP GET failed: {error}")
                        });
                        let status = response.status();
                        let text = response.text().await.unwrap_or_else(|error| {
                            panic!("concurrent reader {reader_index} body read failed: {error}")
                        });
                        let body: Value = serde_json::from_str(&text).unwrap_or_else(|error| {
                            panic!(
                                "concurrent reader {reader_index} received non-JSON body for {status}: {error}; body={text}"
                            )
                        });
                        stats.attempts += 1;
                        match status {
                            reqwest::StatusCode::OK => {
                                validate_reader_topology(
                                    &body,
                                    &known.lock().expect("known fixture metadata lock"),
                                );
                                stats.ok += 1;
                            }
                            reqwest::StatusCode::SERVICE_UNAVAILABLE => {
                                assert_connection_error(status, &body, "/api/topology");
                                stats.unavailable += 1;
                            }
                            _ => panic!(
                                "concurrent reader {reader_index} received unexpected status {status}: {body}"
                            ),
                        }
                        sleep(Duration::from_millis(100)).await;
                    }
                    stats
                }));
            }

            sleep(Duration::from_millis(500)).await;

            eprintln!("mutation 1: connect {c2} to {net_b_name}");
            h.docker
                .connect_network(
                    &net_b,
                    ConnectNetworkOptions {
                        container: c2.clone(),
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            sleep(Duration::from_millis(800)).await;

            eprintln!("mutation 2: disconnect multihomed {c1} from {net_b_name}");
            h.docker
                .disconnect_network(
                    &net_b,
                    DisconnectNetworkOptions {
                        container: c1.clone(),
                        force: true,
                    },
                )
                .await
                .unwrap();
            sleep(Duration::from_millis(800)).await;

            let net_c_name = h.unique_name("concurrent-net-c");
            eprintln!("mutation 3: create transient network {net_c_name}");
            let net_c = concurrent_network(h, &net_c_name, &labels).await;
            sleep(Duration::from_millis(700)).await;

            let c4_name = h.unique_name("concurrent-transient");
            eprintln!("mutation 4: create transient container {c4_name} on {net_c_name}");
            let c4 = create_concurrent_container(h, &c4_name, &net_c_name, &labels).await;
            let c4_metadata = metadata_by_id(h, &filter, &c4).await;
            known
                .lock()
                .expect("known fixture metadata lock")
                .insert(c4.clone(), c4_metadata);
            start_concurrent_container(h, &c4).await;
            sleep(Duration::from_millis(800)).await;

            eprintln!("mutation 5: stop and restart {c3}");
            h.docker
                .stop_container(
                    &c3,
                    Some(bollard::container::StopContainerOptions { t: 0 }),
                )
                .await
                .unwrap();
            sleep(Duration::from_millis(700)).await;
            start_concurrent_container(h, &c3).await;
            sleep(Duration::from_millis(800)).await;

            eprintln!("mutation 6: remove transient container {c4}");
            h.docker
                .remove_container(
                    &c4,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await
                .unwrap();
            sleep(Duration::from_millis(700)).await;

            eprintln!("mutation 7: remove emptied network {net_c_name}");
            h.docker.remove_network(&net_c).await.unwrap();
            sleep(Duration::from_millis(800)).await;

            sleep(Duration::from_millis(500)).await;
            running.store(false, Ordering::Relaxed);

            let mut total_ok = 0;
            for (reader_index, reader) in readers.into_iter().enumerate() {
                let stats = reader.await.unwrap_or_else(|error| {
                    panic!("concurrent reader {reader_index} task failed: {error}")
                });
                eprintln!(
                    "concurrent reader {reader_index}: attempts={} ok={} unavailable={}",
                    stats.attempts, stats.ok, stats.unavailable
                );
                assert!(
                    stats.attempts >= 5,
                    "reader {reader_index} must make a meaningful number of requests: {stats:?}"
                );
                assert_eq!(
                    stats.ok + stats.unavailable,
                    stats.attempts,
                    "every reader response must be counted as valid: {stats:?}"
                );
                let required_successes = stats.attempts.min(20);
                assert!(
                    stats.ok >= required_successes,
                    "reader {reader_index} must observe successful 200 topology responses (required {required_successes} of {}): {stats:?}",
                    stats.attempts
                );
                total_ok += stats.ok;
            }
            assert!(
                total_ok > 0,
                "concurrent readers must obtain at least one 200 topology response"
            );

            assert!(
                h.docker.inspect_container(&c4, None).await.is_err(),
                "removed transient container {c4} must be absent from Docker inspect"
            );
            let remaining_fixture_networks = h
                .docker
                .list_networks(Some(ListNetworksOptions {
                    filters: filter.clone(),
                }))
                .await
                .unwrap();
            assert!(
                !remaining_fixture_networks.iter().any(|network| {
                    network.id.as_deref() == Some(net_c.as_str())
                        || network.name.as_deref() == Some(net_c_name.as_str())
                }),
                "removed transient network {net_c_name} must be absent from Docker"
            );

            let fixture_networks: BTreeMap<String, String> = remaining_fixture_networks
                .into_iter()
                .filter_map(|network| Some((network.name?, network.id?)))
                .collect();
            let full_networks: BTreeMap<String, String> = h
                .docker
                .list_networks::<String>(None)
                .await
                .unwrap()
                .into_iter()
                .filter_map(|network| Some((network.name?, network.id?)))
                .collect();
            let surviving = list_fixture_containers(h, &filter, false).await;
            let mut expected_containers = BTreeMap::new();
            let mut expected_edges = BTreeSet::new();
            for summary in surviving {
                let id = summary.id.clone().expect("surviving fixture container ID");
                expected_containers.insert(id.clone(), expected_container_metadata(&summary));
                let inspect = h
                    .docker
                    .inspect_container(&id, None)
                    .await
                    .unwrap_or_else(|error| panic!("inspect surviving fixture {id}: {error}"));
                let attachments = inspect
                    .network_settings
                    .and_then(|settings| settings.networks)
                    .unwrap_or_default();
                for name in attachments.keys() {
                    let network_id = full_networks.get(name).unwrap_or_else(|| {
                        panic!(
                            "independent Docker manifest has no ID for attached network {name}"
                        )
                    });
                    expected_edges.insert((id.clone(), network_id.clone()));
                }
            }
            assert_eq!(
                expected_containers
                    .keys()
                    .cloned()
                    .collect::<BTreeSet<_>>(),
                BTreeSet::from([c1.clone(), c2.clone(), c3.clone()]),
                "independent Docker must report exactly the surviving fixture containers"
            );
            assert_eq!(
                fixture_networks,
                BTreeMap::from([
                    (net_a_name.clone(), net_a.clone()),
                    (net_b_name.clone(), net_b.clone()),
                ]),
                "independent Docker must report exactly the surviving fixture networks"
            );
            assert_eq!(
                expected_edges,
                BTreeSet::from([
                    (c1.clone(), net_a.clone()),
                    (c2.clone(), net_a.clone()),
                    (c2.clone(), net_b.clone()),
                    (c3.clone(), net_b.clone()),
                ]),
                "independent Docker inspect must report exactly the surviving fixture edges"
            );

            let removed = RemovedFixture {
                container_id: c4,
                container_name: c4_name,
                network_id: net_c,
                network_name: net_c_name,
            };
            let body = wait_for_converged_topology(
                h,
                &expected_containers,
                &full_networks,
                &fixture_networks,
                &expected_edges,
                &removed,
            )
            .await;
            eprintln!(
                "converged topology: {} containers, {} networks, {} edges",
                body["containers"].as_array().unwrap().len(),
                body["networks"].as_array().unwrap().len(),
                body["edges"].as_array().unwrap().len()
            );
        })
    })
    .await;
}

async fn assert_removed(proof: (Vec<String>, String)) {
    let outer = bollard::Docker::connect_with_socket_defaults().unwrap();
    for id in proof.0 {
        assert!(
            outer.inspect_container(&id, None).await.is_err(),
            "owned outer test container {id} remains after harness cleanup"
        );
    }
    assert!(
        outer.inspect_image(&proof.1).await.is_err(),
        "owned backend image {} remains after harness cleanup",
        proof.1
    );
}

// Given a normal return and a panicking test future, when the harness runs, then owned
// outer containers, the backend image, and anonymous volumes are removed.
#[tokio::test]
async fn given_normal_return_or_body_panic_when_harness_runs_then_owned_resources_are_removed() {
    let outer = Docker::connect_with_socket_defaults().unwrap();
    let before_normal = outer_resources(&outer).await;
    let normal = Arc::new(Mutex::new(None));
    let normal_capture = normal.clone();
    E2eHarness::run(|h| {
        Box::pin(async move {
            ensure_image(h).await;
            fixture(h, "normal-cleanup").await;
            *normal_capture.lock().unwrap() = Some(h.cleanup_proof());
        })
    })
    .await;
    assert_removed(
        normal
            .lock()
            .unwrap()
            .take()
            .expect("normal cleanup proof recorded"),
    )
    .await;
    assert_probe_left_no_owned_resources(&before_normal, &outer).await;

    let before_panic = outer_resources(&outer).await;
    let panic_proof = Arc::new(Mutex::new(None));
    let panic_capture = panic_proof.clone();
    let expected_panic = AssertUnwindSafe(E2eHarness::run(|h| {
        Box::pin(async move {
            ensure_image(h).await;
            fixture(h, "panic-cleanup").await;
            *panic_capture.lock().unwrap() = Some(h.cleanup_proof());
            panic!("intentional harness cleanup proof panic");
        })
    }))
    .catch_unwind()
    .await;
    assert!(
        expected_panic.is_err(),
        "fixture body panic must remain visible after cleanup"
    );
    assert_removed(
        panic_proof
            .lock()
            .unwrap()
            .take()
            .expect("panic cleanup proof recorded"),
    )
    .await;
    assert_probe_left_no_owned_resources(&before_panic, &outer).await;
}

// Given a test closure that panics before returning its future, when the harness runs, then
// the original panic is re-raised and owned outer resources are still removed.
#[tokio::test]
async fn given_closure_invocation_panic_when_harness_runs_then_owned_resources_are_removed() {
    let outer = Docker::connect_with_socket_defaults().unwrap();
    let before = outer_resources(&outer).await;
    let result = AssertUnwindSafe(E2eHarness::run(panic_instead_of_returning_future))
        .catch_unwind()
        .await;
    assert!(
        result.is_err(),
        "closure invocation panic must be re-raised after cleanup"
    );
    assert_probe_left_no_owned_resources(&before, &outer).await;
}

async fn assert_startup_fault_cleanup(fault: StartupFault, expected_error: &str) {
    let outer = Docker::connect_with_socket_defaults().unwrap();
    let before = outer_resources(&outer).await;
    let result = tokio::time::timeout(
        Duration::from_secs(300),
        E2eHarness::run_with_startup_fault(fault, |_h| Box::pin(async {})),
    )
    .await
    .expect("startup fault probe timed out");
    let error = result.expect_err("injected startup fault must return an error");
    assert!(
        error.contains(expected_error),
        "startup fault must surface the injected error, got: {error}"
    );
    assert!(
        !error.contains("startup cleanup failed"),
        "startup cleanup must succeed, got: {error}"
    );
    assert_probe_left_no_owned_resources(&before, &outer).await;
}

// Given DinD becomes ready, when startup fails before the backend image build, then the
// injected error is returned and owned outer resources are removed.
#[tokio::test]
async fn given_startup_after_dind_ready_when_harness_fails_then_owned_resources_are_removed() {
    assert_startup_fault_cleanup(
        StartupFault::AfterDindReady,
        "injected startup fault after inner DinD readiness",
    )
    .await;
}

// Given the backend container has started, when startup fails before HTTP readiness, then the
// injected error is returned and owned outer containers, image, and volumes are removed.
#[tokio::test]
async fn given_startup_after_backend_start_when_harness_fails_then_owned_resources_are_removed() {
    assert_startup_fault_cleanup(
        StartupFault::AfterBackendStarted,
        "injected startup fault after backend start before HTTP readiness",
    )
    .await;
}
