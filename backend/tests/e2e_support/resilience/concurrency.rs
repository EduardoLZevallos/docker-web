use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use bollard::{
    container::{ListContainersOptions, RemoveContainerOptions, StopContainerOptions},
    models::{ContainerSummary, HostConfig},
    network::{ConnectNetworkOptions, DisconnectNetworkOptions, ListNetworksOptions},
};
use serde_json::Value;
use tokio::{
    sync::{Notify, RwLock},
    time::{Instant, timeout},
};

use super::super::{
    E2eHarness,
    fixtures::{self, NetworkManifest, TopologySnapshot},
};

const LIMIT: Duration = Duration::from_secs(40);
const READER_COUNT: usize = 4;
const READ_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug)]
struct KnownContainer {
    name: String,
    image: String,
    created: i64,
}

#[derive(Default, Debug)]
struct ReaderStats {
    attempts: usize,
    ok: usize,
}

struct RemovedFixture {
    container_id: String,
    container_name: String,
    network_id: String,
    network_name: String,
}

fn run_label_filter(h: &E2eHarness) -> std::collections::HashMap<String, Vec<String>> {
    let labels = h.labels();
    let (key, value) = labels.iter().next().expect("harness run label");
    std::collections::HashMap::from([("label".to_owned(), vec![format!("{key}={value}")])])
}

fn known_metadata(summary: &ContainerSummary) -> KnownContainer {
    KnownContainer {
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
    filter: &std::collections::HashMap<String, Vec<String>>,
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
    filter: &std::collections::HashMap<String, Vec<String>>,
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
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn metadata_by_id(
    h: &E2eHarness,
    filter: &std::collections::HashMap<String, Vec<String>>,
    id: &str,
) -> KnownContainer {
    let listed = list_fixture_containers(h, filter, true).await;
    let summary = listed
        .iter()
        .find(|summary| summary.id.as_deref() == Some(id))
        .unwrap_or_else(|| {
            panic!("created container {id} is absent from the independent Docker list")
        });
    known_metadata(summary)
}

fn validate_reader_topology(body: &Value, known: &Mutex<BTreeMap<String, KnownContainer>>) {
    let snapshot = TopologySnapshot::from_value(body);
    snapshot.assert_unique_and_referential();
    for container in &snapshot.containers {
        assert!(
            !container.status.is_empty(),
            "container status must be nonempty: {body}"
        );
        let expected = known
            .lock()
            .expect("known fixture metadata lock")
            .get(&container.id)
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "unexpected container {} in the isolated daemon: {body}",
                    container.id
                )
            });
        assert_eq!(
            container.name, expected.name,
            "container {} name must remain stable",
            container.id
        );
        assert_eq!(
            container.image, expected.image,
            "container {} image must remain stable",
            container.id
        );
        assert_eq!(
            container.created, expected.created,
            "container {} created timestamp must remain stable",
            container.id
        );
    }
}

async fn wait_for_observations(
    counts: &[AtomicUsize],
    targets: &[usize],
    notify: &Notify,
    budget: Duration,
) {
    let deadline = Instant::now() + budget;
    loop {
        let notified = notify.notified();
        if counts
            .iter()
            .zip(targets)
            .all(|(count, target)| count.load(Ordering::Acquire) >= *target)
        {
            return;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "concurrent readers did not acknowledge mutations"
        );
        if timeout(remaining, notified).await.is_err() {
            panic!("concurrent readers did not acknowledge mutations within {READ_TIMEOUT:?}");
        }
    }
}

fn topology_is_converged(
    snapshot: &TopologySnapshot,
    expected_containers: &BTreeMap<String, KnownContainer>,
    expected_networks: &NetworkManifest,
    expected_edges: &BTreeSet<(String, String)>,
) -> bool {
    snapshot.container_ids() == expected_containers.keys().cloned().collect::<BTreeSet<_>>()
        && snapshot.network_manifest() == *expected_networks
        && snapshot.edges == *expected_edges
}

fn assert_converged_topology(
    body: &Value,
    expected_containers: &BTreeMap<String, KnownContainer>,
    expected_networks: &NetworkManifest,
    fixture_networks: &NetworkManifest,
    expected_edges: &BTreeSet<(String, String)>,
    removed: &RemovedFixture,
) {
    let snapshot = TopologySnapshot::from_value(body);
    snapshot.assert_unique_and_referential();
    let expected_container_ids: BTreeSet<_> = expected_containers.keys().cloned().collect();
    assert_eq!(
        snapshot.container_ids(),
        expected_container_ids,
        "exact surviving fixture container IDs: {body}"
    );
    for (id, expected) in expected_containers {
        let record = snapshot
            .containers
            .iter()
            .find(|container| container.id == *id)
            .unwrap_or_else(|| panic!("expected container {id} is absent: {body}"));
        assert_eq!(record.name, expected.name, "container {id} name: {body}");
        assert_eq!(record.image, expected.image, "container {id} image: {body}");
        assert_eq!(
            record.created, expected.created,
            "container {id} created timestamp: {body}"
        );
    }
    assert_eq!(
        snapshot.network_manifest(),
        *expected_networks,
        "exact network manifest including built-ins: {body}"
    );
    for (name, id) in fixture_networks {
        assert_eq!(
            snapshot.network_manifest().get(name),
            Some(id),
            "fixture network {name} must remain present with its exact ID: {body}"
        );
    }
    assert_eq!(
        snapshot.edges, *expected_edges,
        "exact fixture edge set: {body}"
    );
    assert!(
        !snapshot.container_ids().contains(&removed.container_id),
        "removed container {} must be absent: {body}",
        removed.container_id
    );
    assert!(
        !snapshot
            .containers
            .iter()
            .any(|container| container.name == removed.container_name),
        "removed container name {} must be absent: {body}",
        removed.container_name
    );
    assert!(
        !snapshot
            .network_manifest()
            .contains_key(&removed.network_name),
        "removed network {} must be absent: {body}",
        removed.network_name
    );
    assert!(
        !snapshot
            .network_manifest()
            .values()
            .any(|id| id == &removed.network_id),
        "removed network ID {} must be absent: {body}",
        removed.network_id
    );
    assert!(
        snapshot.edges.iter().all(|(source, target)| {
            source != &removed.container_id && target != &removed.network_id
        }),
        "removed fixture endpoints must leave no edges: {body}"
    );
}

async fn wait_for_converged_topology(
    h: &E2eHarness,
    expected_containers: &BTreeMap<String, KnownContainer>,
    expected_networks: &NetworkManifest,
    fixture_networks: &NetworkManifest,
    expected_edges: &BTreeSet<(String, String)>,
    removed: &RemovedFixture,
) {
    let deadline = Instant::now() + LIMIT;
    loop {
        let (status, body) = h.get("/api/topology").await;
        assert_eq!(
            status,
            reqwest::StatusCode::OK,
            "convergence topology: {body}"
        );
        let snapshot = TopologySnapshot::from_value(&body);
        if topology_is_converged(
            &snapshot,
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
            return;
        }
        assert!(
            Instant::now() < deadline,
            "topology did not converge within {LIMIT:?}; last={body}"
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

// Given running fixtures, when concurrent readers overlap bounded topology mutations, then every
// ordinary response is a valid snapshot and the final state converges exactly.
#[tokio::test]
async fn given_concurrent_readers_when_bounded_topology_mutations_run_then_snapshots_are_valid_and_final_state_converges()
 {
    E2eHarness::run(|h| {
        Box::pin(async move {
            fixtures::ensure_image(h).await;
            let filter = run_label_filter(h);

            let net_a = fixtures::create_network(
                h,
                &h.unique_name("concurrent-net-a"),
                "bridge",
                false,
                "concurrent-readers",
            )
            .await;
            let net_b = fixtures::create_network(
                h,
                &h.unique_name("concurrent-net-b"),
                "bridge",
                false,
                "concurrent-readers",
            )
            .await;

            let c1 = fixtures::create_and_start_network_container(
                h,
                &h.unique_name("concurrent-multihomed"),
                &net_a.name,
                &["sh", "-c", "sleep 300"],
                "concurrent-readers",
            )
            .await;
            h.docker
                .connect_network(
                    &net_b.id,
                    ConnectNetworkOptions {
                        container: c1.id.clone(),
                        ..Default::default()
                    },
                )
                .await
                .unwrap_or_else(|error| panic!("connect multihomed fixture: {error}"));
            let c2 = fixtures::create_and_start_network_container(
                h,
                &h.unique_name("concurrent-net-a-only"),
                &net_a.name,
                &["sh", "-c", "sleep 300"],
                "concurrent-readers",
            )
            .await;
            let c3 = fixtures::create_and_start_network_container(
                h,
                &h.unique_name("concurrent-net-b-only"),
                &net_b.name,
                &["sh", "-c", "sleep 300"],
                "concurrent-readers",
            )
            .await;

            let initial_ids = BTreeSet::from([c1.id.clone(), c2.id.clone(), c3.id.clone()]);
            let initial =
                wait_for_initial_containers(h, &filter, &initial_ids, Duration::from_secs(20))
                    .await;
            let known: Arc<Mutex<BTreeMap<String, KnownContainer>>> = Arc::new(Mutex::new(
                initial
                    .iter()
                    .map(|summary| {
                        (
                            summary.id.clone().expect("initial fixture container ID"),
                            known_metadata(summary),
                        )
                    })
                    .collect(),
            ));

            let gate = Arc::new(RwLock::new(()));
            let running = Arc::new(AtomicBool::new(true));
            let counts: Arc<Vec<AtomicUsize>> =
                Arc::new((0..READER_COUNT).map(|_| AtomicUsize::new(0)).collect());
            let notify = Arc::new(Notify::new());
            let mut readers = Vec::new();
            for reader_index in 0..READER_COUNT {
                let http = h.http.clone();
                let url = format!("{}/api/topology", h.base_url);
                let known = known.clone();
                let gate = gate.clone();
                let running = running.clone();
                let counts = counts.clone();
                let notify = notify.clone();
                readers.push(tokio::spawn(async move {
                    let mut stats = ReaderStats::default();
                    while running.load(Ordering::Acquire) {
                        let read_guard = gate.read().await;
                        let response = http.get(&url).send().await.unwrap_or_else(|error| {
                            panic!("concurrent reader {reader_index} HTTP GET failed: {error}")
                        });
                        assert_eq!(
                            response.status(),
                            reqwest::StatusCode::OK,
                            "ordinary mutations must keep discovery available"
                        );
                        let body: Value = response.json().await.unwrap_or_else(|error| {
                            panic!(
                                "concurrent reader {reader_index} received invalid JSON: {error}"
                            )
                        });
                        validate_reader_topology(&body, &known);
                        drop(read_guard);
                        stats.attempts += 1;
                        stats.ok += 1;
                        counts[reader_index].fetch_add(1, Ordering::Release);
                        notify.notify_waiters();
                    }
                    stats
                }));
            }

            let targets = vec![1; READER_COUNT];
            wait_for_observations(&counts, &targets, &notify, READ_TIMEOUT).await;

            h.docker
                .connect_network(
                    &net_b.id,
                    ConnectNetworkOptions {
                        container: c2.id.clone(),
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            let targets = targets.iter().map(|target| target + 1).collect::<Vec<_>>();
            wait_for_observations(&counts, &targets, &notify, READ_TIMEOUT).await;

            h.docker
                .disconnect_network(
                    &net_b.id,
                    DisconnectNetworkOptions {
                        container: c1.id.clone(),
                        force: true,
                    },
                )
                .await
                .unwrap();
            let targets = targets.iter().map(|target| target + 1).collect::<Vec<_>>();
            wait_for_observations(&counts, &targets, &notify, READ_TIMEOUT).await;

            let net_c = fixtures::create_network(
                h,
                &h.unique_name("concurrent-net-c"),
                "bridge",
                false,
                "concurrent-readers",
            )
            .await;
            let targets = targets.iter().map(|target| target + 1).collect::<Vec<_>>();
            wait_for_observations(&counts, &targets, &notify, READ_TIMEOUT).await;

            // A future all-state listing could expose the transient container immediately after
            // creation. Register its identity under the write locks so no reader can observe it
            // as an unexpected node.
            let c4 = {
                let _write_guard = gate.write().await;
                let c4 = fixtures::create_container(
                    h,
                    &h.unique_name("concurrent-transient"),
                    &["sh", "-c", "sleep 300"],
                    HostConfig {
                        network_mode: Some(net_c.name.clone()),
                        ..Default::default()
                    },
                    None,
                    None,
                    "concurrent-readers",
                )
                .await;
                let metadata = metadata_by_id(h, &filter, &c4.id).await;
                known
                    .lock()
                    .expect("known fixture metadata lock")
                    .insert(c4.id.clone(), metadata);
                fixtures::start_container(h, &c4.id).await;
                c4
            };
            let targets = targets.iter().map(|target| target + 1).collect::<Vec<_>>();
            wait_for_observations(&counts, &targets, &notify, READ_TIMEOUT).await;

            h.docker
                .stop_container(&c3.id, Some(StopContainerOptions { t: 0 }))
                .await
                .unwrap();
            let targets = targets.iter().map(|target| target + 1).collect::<Vec<_>>();
            wait_for_observations(&counts, &targets, &notify, READ_TIMEOUT).await;
            fixtures::start_container(h, &c3.id).await;
            let targets = targets.iter().map(|target| target + 1).collect::<Vec<_>>();
            wait_for_observations(&counts, &targets, &notify, READ_TIMEOUT).await;

            h.docker
                .remove_container(
                    &c4.id,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await
                .unwrap();
            let targets = targets.iter().map(|target| target + 1).collect::<Vec<_>>();
            wait_for_observations(&counts, &targets, &notify, READ_TIMEOUT).await;

            h.docker.remove_network(&net_c.id).await.unwrap();
            let targets = targets.iter().map(|target| target + 1).collect::<Vec<_>>();
            wait_for_observations(&counts, &targets, &notify, READ_TIMEOUT).await;

            running.store(false, Ordering::Release);
            for (reader_index, reader) in readers.into_iter().enumerate() {
                let stats = reader.await.unwrap_or_else(|error| {
                    panic!("concurrent reader {reader_index} task failed: {error}")
                });
                assert!(
                    stats.attempts >= 5,
                    "reader {reader_index} must make a meaningful number of requests: {stats:?}"
                );
                assert_eq!(
                    stats.ok, stats.attempts,
                    "reader {reader_index} must observe only successful 200 responses: {stats:?}"
                );
            }

            assert!(
                h.docker.inspect_container(&c4.id, None).await.is_err(),
                "removed transient container {} must be absent from Docker inspect",
                c4.id
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
                    network.id.as_deref() == Some(net_c.id.as_str())
                        || network.name.as_deref() == Some(net_c.name.as_str())
                }),
                "removed transient network {} must be absent from Docker",
                net_c.name
            );

            let fixture_networks: NetworkManifest = remaining_fixture_networks
                .into_iter()
                .filter_map(|network| Some((network.name?, network.id?)))
                .collect();
            let full_networks: NetworkManifest = h
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
                expected_containers.insert(id.clone(), known_metadata(&summary));
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
                        panic!("independent Docker manifest has no ID for attached network {name}")
                    });
                    expected_edges.insert((id.clone(), network_id.clone()));
                }
            }
            assert_eq!(
                expected_containers.keys().cloned().collect::<BTreeSet<_>>(),
                BTreeSet::from([c1.id.clone(), c2.id.clone(), c3.id.clone()]),
                "independent Docker must report exactly the surviving fixture containers"
            );
            assert_eq!(
                fixture_networks,
                BTreeMap::from([
                    (net_a.name.clone(), net_a.id.clone()),
                    (net_b.name.clone(), net_b.id.clone()),
                ]),
                "independent Docker must report exactly the surviving fixture networks"
            );
            assert_eq!(
                expected_edges,
                BTreeSet::from([
                    (c1.id.clone(), net_a.id.clone()),
                    (c2.id.clone(), net_a.id.clone()),
                    (c2.id.clone(), net_b.id.clone()),
                    (c3.id.clone(), net_b.id.clone()),
                ]),
                "independent Docker inspect must report exactly the surviving fixture edges"
            );

            let removed = RemovedFixture {
                container_id: c4.id,
                container_name: c4.name,
                network_id: net_c.id,
                network_name: net_c.name,
            };
            wait_for_converged_topology(
                h,
                &expected_containers,
                &full_networks,
                &fixture_networks,
                &expected_edges,
                &removed,
            )
            .await;
        })
    })
    .await;
}
