//! Isolated lifecycle contract scenarios. Each is an independent test so a
//! known visibility failure cannot suppress the remaining lifecycle checks.
use super::{
    E2eHarness,
    fixtures::{self, TopologySnapshot},
};
use bollard::{
    container::{ListContainersOptions, RemoveContainerOptions, StopContainerOptions},
    models::{HealthConfig, HostConfig, RestartPolicy, RestartPolicyNameEnum},
    network::ConnectNetworkOptions,
};
use serde_json::Value;
use std::{collections::BTreeSet, time::Duration};
use tokio::time::{Instant, sleep};
const LIMIT: Duration = Duration::from_secs(30);

async fn network(h: &E2eHarness, stem: &str) -> (String, String) {
    let network =
        fixtures::create_network(h, &h.unique_name(stem), "bridge", false, "lifecycle").await;
    (network.name, network.id)
}

async fn create(
    h: &E2eHarness,
    stem: &str,
    cmd: &[&str],
    net: &str,
    restart: Option<RestartPolicy>,
    health: Option<HealthConfig>,
) -> String {
    fixtures::create_container(
        h,
        &h.unique_name(stem),
        cmd,
        HostConfig {
            network_mode: Some(net.into()),
            restart_policy: restart,
            ..Default::default()
        },
        None,
        health,
        "lifecycle",
    )
    .await
    .id
}

async fn record(h: &E2eHarness, id: &str, wanted: &str) -> Value {
    let deadline = Instant::now() + LIMIT;
    loop {
        assert_eq!(
            fixtures::state_name(&fixtures::inspect(h, id).await),
            wanted,
            "independent Docker inspect precondition"
        );
        let (status, body) = h.get("/api/containers").await;
        assert_eq!(status, reqwest::StatusCode::OK, "{body}");
        if let Some(row) = body
            .as_array()
            .and_then(|a| a.iter().find(|r| r["id"] == id))
        {
            assert_eq!(
                fixtures::state_name(&fixtures::inspect(h, id).await),
                wanted,
                "inspect state brackets HTTP read"
            );
            let s = row["status"].as_str().unwrap_or_default().to_lowercase();
            assert!(!s.is_empty(), "status must remain a nonempty string: {row}");
            return row.clone();
        }
        assert!(
            Instant::now() < deadline,
            "container {id} inspected as {wanted} but absent from /api/containers after {LIMIT:?}"
        );
        sleep(Duration::from_millis(150)).await;
    }
}
async fn restarting_observations(h: &E2eHarness, id: &str, net_id: &str) -> Value {
    let deadline = Instant::now() + LIMIT;
    let mut saw_window = false;
    let mut saw_container = false;
    let mut saw_topology = false;
    loop {
        if fixtures::state_name(&fixtures::inspect(h, id).await) == "restarting" {
            let (container_status, containers_body) = h.get("/api/containers").await;
            assert_eq!(
                container_status,
                reqwest::StatusCode::OK,
                "{containers_body}"
            );
            let (topology_status, topology_body) = h.get("/api/topology").await;
            assert_eq!(topology_status, reqwest::StatusCode::OK, "{topology_body}");
            let after = fixtures::inspect(h, id).await;
            if fixtures::state_name(&after) == "restarting" {
                saw_window = true;
                let container_row = containers_body
                    .as_array()
                    .and_then(|rows| rows.iter().find(|row| row["id"] == id));
                if let Some(row) = container_row {
                    saw_container = true;
                    let snapshot = TopologySnapshot::from_value(&topology_body);
                    snapshot.assert_unique_and_referential();
                    if snapshot.container_ids().contains(id) {
                        assert!(
                            snapshot.edges.contains(&(id.to_owned(), net_id.to_owned())),
                            "restarting container must retain its network attachment: {topology_body}"
                        );
                        saw_topology = true;
                    }
                    if saw_container && saw_topology {
                        return row.clone();
                    }
                }
            }
        }
        if Instant::now() >= deadline {
            assert!(
                saw_window,
                "restart fixture setup failed: no inspect-restarting window bracketed HTTP requests for {id}"
            );
            assert!(
                saw_container,
                "restarting window confirmed, but /api/containers omitted {id}"
            );
            panic!("restarting window confirmed, but /api/topology omitted {id} or its edge");
        }
        sleep(Duration::from_millis(100)).await;
    }
}
fn assert_record_status(row: &Value, word: &str) {
    let s = row["status"].as_str().unwrap_or_default().to_lowercase();
    assert!(
        s.contains(word),
        "expected status string containing {word}, got {s:?}"
    );
}
fn assert_health_status(row: &Value, wanted: &str) {
    let s = row["status"].as_str().unwrap_or_default().to_lowercase();
    assert!(
        s.split(|c: char| !c.is_ascii_alphanumeric())
            .any(|part| part == wanted),
        "expected exact health token {wanted} in status string {s:?}"
    );
}

async fn assert_inactive_visible(
    h: &E2eHarness,
    id: &str,
    wanted: &str,
    net_id: &str,
    exit_code: Option<i64>,
) {
    assert_eq!(
        fixtures::state_name(&fixtures::inspect(h, id).await),
        wanted,
        "independent Docker inspect precondition"
    );
    let (cs, containers) = h.get("/api/containers").await;
    assert_eq!(cs, reqwest::StatusCode::OK, "{containers}");
    let row = containers
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id);
    let string_status = row.and_then(|r| r["status"].as_str());
    let (ts, t) = h.get("/api/topology").await;
    assert_eq!(ts, reqwest::StatusCode::OK, "{t}");
    let topology_row = t["containers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id);
    let edges = fixtures::edge_set(&t);
    let has_edge = edges.iter().any(|(source, _)| source == id);
    assert!(
        row.is_some(),
        "Docker inspect says {wanted}, but /api/containers omitted {id}"
    );
    assert!(
        string_status.is_some_and(|s| !s.is_empty()),
        "status must be a nonempty existing string, got {row:?}"
    );
    let record = row.unwrap();
    assert_record_status(
        record,
        if wanted == "created" {
            "created"
        } else {
            "exited"
        },
    );
    if let Some(code) = exit_code {
        let expected = format!("exited ({code})");
        assert_record_status(record, &expected);
    }
    assert!(
        topology_row.is_some(),
        "Docker inspect says {wanted}, but /api/topology omitted {id}"
    );
    assert!(
        !has_edge,
        "inactive fixture {id} has topology edges: {edges:?}; attached network {net_id}"
    );
}

async fn inactive_fixture(
    h: &E2eHarness,
    stem: &str,
    cmd: &[&str],
    action: &str,
    code: Option<i64>,
) {
    fixtures::ensure_image(h).await;
    let (net, nid) = network(h, &format!("{stem}-net")).await;
    let id = create(h, stem, cmd, &net, None, None).await;
    if action != "created" {
        h.docker.start_container::<String>(&id, None).await.unwrap();
    }
    match action {
        "stopped" => h
            .docker
            .stop_container(&id, Some(StopContainerOptions { t: 1 }))
            .await
            .unwrap(),
        "killed" => h
            .docker
            .kill_container(
                &id,
                Some(bollard::container::KillContainerOptions { signal: "SIGKILL" }),
            )
            .await
            .unwrap(),
        _ => (),
    }
    fixtures::wait_state(
        h,
        &id,
        if action == "created" {
            "created"
        } else {
            "exited"
        },
        LIMIT,
    )
    .await;
    if let Some(code) = code {
        assert_eq!(
            fixtures::inspect(h, &id).await.state.unwrap().exit_code,
            Some(code)
        );
    }
    assert_inactive_visible(
        h,
        &id,
        if action == "created" {
            "created"
        } else {
            "exited"
        },
        &nid,
        code,
    )
    .await;
}
// Given mixed states sharing two networks, when topology is read, then exact fixture nodes and
// active edges match the independent fixture manifest.
#[tokio::test]
async fn given_mixed_lifecycle_states_when_topology_read_then_only_active_membership_edges_exist() {
    E2eHarness::run(|h| {
        Box::pin(async move {
            fixtures::ensure_image(h).await;
            let baseline = h
                .docker
                .list_containers(Some(ListContainersOptions::<String> {
                    all: true,
                    ..Default::default()
                }))
                .await
                .unwrap();
            assert!(baseline.is_empty(), "clean inner daemon baseline");
            let (a, aid) = network(h, "mixed-a").await;
            let (b, bid) = network(h, "mixed-b").await;

            let running =
                create(h, "mixed-running", &["sh", "-c", "sleep 300"], &a, None, None).await;
            h.docker
                .start_container::<String>(&running, None)
                .await
                .unwrap();
            h.docker
                .connect_network(
                    &bid,
                    ConnectNetworkOptions {
                        container: running.clone(),
                        ..Default::default()
                    },
                )
                .await
                .unwrap();

            let paused = create(h, "mixed-paused", &["sh", "-c", "sleep 300"], &b, None, None).await;
            h.docker
                .start_container::<String>(&paused, None)
                .await
                .unwrap();
            h.docker.pause_container(&paused).await.unwrap();
            assert_eq!(fixtures::inspect(h, &paused).await.state.unwrap().paused, Some(true));

            let created =
                create(h, "mixed-created", &["sh", "-c", "sleep 300"], &a, None, None).await;
            let exited = create(h, "mixed-exited", &["sh", "-c", "exit 0"], &b, None, None).await;
            h.docker
                .start_container::<String>(&exited, None)
                .await
                .unwrap();
            fixtures::wait_state(h, &exited, "exited", LIMIT).await;
            assert_eq!(fixtures::state_name(&fixtures::inspect(h, &created).await), "created");

            let expected_nodes =
                BTreeSet::from([running.clone(), paused.clone(), created.clone(), exited.clone()]);
            let (cs, c) = h.get("/api/containers").await;
            assert_eq!(cs, reqwest::StatusCode::OK, "{c}");
            let container_rows = c.as_array().unwrap();
            let container_ids: BTreeSet<_> = container_rows
                .iter()
                .filter_map(|r| r["id"].as_str().map(str::to_owned))
                .collect();
            assert_eq!(
                container_ids.len(),
                container_rows.len(),
                "duplicate container IDs from /api/containers: {c}"
            );
            assert_eq!(
                container_ids, expected_nodes,
                "exact lifecycle nodes visible from /containers: {c}"
            );

            let (ts, t) = h.get("/api/topology").await;
            assert_eq!(ts, reqwest::StatusCode::OK, "{t}");
            let topology_rows = t["containers"].as_array().unwrap();
            let topology_ids: BTreeSet<_> = topology_rows
                .iter()
                .map(|r| r["id"].as_str().unwrap().to_owned())
                .collect();
            assert_eq!(
                topology_ids.len(),
                topology_rows.len(),
                "duplicate container IDs from /api/topology: {t}"
            );
            assert_eq!(
                topology_ids, expected_nodes,
                "exact lifecycle nodes visible from /topology: {t}"
            );

            let expected_edges = BTreeSet::from([
                (running.clone(), aid.clone()),
                (running.clone(), bid.clone()),
                (paused.clone(), bid.clone()),
            ]);
            let fixture_edge_records: Vec<_> = t["edges"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|edge| expected_nodes.contains(edge["source"].as_str().unwrap()))
                .collect();
            let fixture_edges: BTreeSet<_> = fixtures::edge_set(&t)
                .into_iter()
                .filter(|(source, _)| expected_nodes.contains(source))
                .collect();
            assert_eq!(
                fixture_edges.len(),
                fixture_edge_records.len(),
                "duplicate fixture topology edges: {t}"
            );
            assert_eq!(
                fixture_edges, expected_edges,
                "created/exited nodes have no edges; paused retains its edge; multi-homed running node has both"
            );
            let network_ids: BTreeSet<_> = t["networks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n["id"].as_str().unwrap().to_owned())
                .collect();
            assert!(
                fixtures::edge_set(&t)
                    .iter()
                    .all(|(source, target)| topology_ids.contains(source)
                        && network_ids.contains(target)),
                "topology edge has a dangling endpoint: {t}"
            );
        })
    })
    .await
}
// Given a created container on a network, when both APIs are read, then it is visible and has no edge.
#[tokio::test]
async fn given_created_container_when_both_apis_read_then_it_is_visible_without_topology_edges() {
    E2eHarness::run(|h| {
        Box::pin(inactive_fixture(
            h,
            "created",
            &["sh", "-c", "sleep 300"],
            "created",
            None,
        ))
    })
    .await
}
// Given a running container on a network, when both APIs are read, then it is visible with its edge.
#[tokio::test]
async fn given_running_container_when_both_apis_read_then_it_is_visible_with_its_network_edge() {
    E2eHarness::run(|h| {
        Box::pin(async move {
            fixtures::ensure_image(h).await;
            let (net, nid) = network(h, "running-net").await;
            let id = create(h, "running", &["sh", "-c", "sleep 300"], &net, None, None).await;
            h.docker.start_container::<String>(&id, None).await.unwrap();
            fixtures::wait_state(h, &id, "running", LIMIT).await;
            let row = record(h, &id, "running").await;
            assert_record_status(&row, "up");
            let t = fixtures::topology(h).await;
            assert!(
                t["containers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["id"] == id)
            );
            assert!(fixtures::edge_set(&t).contains(&(id, nid)));
        })
    })
    .await
}
// Given a stopped container, when both APIs are read, then it remains visible and has no edge.
#[tokio::test]
async fn given_stopped_container_when_both_apis_read_then_it_remains_visible_without_topology_edges()
 {
    E2eHarness::run(|h| {
        Box::pin(inactive_fixture(
            h,
            "stopped",
            &["sh", "-c", "sleep 300"],
            "stopped",
            None,
        ))
    })
    .await
}
// Given a command exits zero, when both APIs are read, then the exited node is visible without an edge.
#[tokio::test]
async fn given_exit_zero_container_when_both_apis_read_then_it_is_visible_without_topology_edges() {
    E2eHarness::run(|h| {
        Box::pin(inactive_fixture(
            h,
            "exit-zero",
            &["sh", "-c", "exit 0"],
            "exit",
            Some(0),
        ))
    })
    .await
}
// Given a command exits nonzero, when both APIs are read, then the exited node is visible without an edge.
#[tokio::test]
async fn given_exit_nonzero_container_when_both_apis_read_then_it_is_visible_without_topology_edges()
 {
    E2eHarness::run(|h| {
        Box::pin(inactive_fixture(
            h,
            "exit-nonzero",
            &["sh", "-c", "exit 7"],
            "exit",
            Some(7),
        ))
    })
    .await
}
// Given a killed container, when both APIs are read, then the exited node is visible without an edge.
#[tokio::test]
async fn given_killed_container_when_both_apis_read_then_it_is_visible_without_topology_edges() {
    E2eHarness::run(|h| {
        Box::pin(inactive_fixture(
            h,
            "killed",
            &["sh", "-c", "sleep 300"],
            "killed",
            Some(137),
        ))
    })
    .await
}

// Given a paused container, when topology is read, then the paused node and its network edge remain.
#[tokio::test]
async fn given_paused_container_when_topology_read_then_it_keeps_its_network_edge() {
    E2eHarness::run(|h| {
        Box::pin(async move {
            fixtures::ensure_image(h).await;
            let (net, nid) = network(h, "pause-net").await;
            let id = create(h, "paused", &["sh", "-c", "sleep 300"], &net, None, None).await;
            h.docker.start_container::<String>(&id, None).await.unwrap();
            h.docker.pause_container(&id).await.unwrap();
            assert_eq!(
                fixtures::inspect(h, &id).await.state.unwrap().paused,
                Some(true)
            );
            assert_eq!(
                fixtures::state_name(&fixtures::inspect(h, &id).await),
                "paused"
            );
            let row = record(h, &id, "paused").await;
            assert_record_status(&row, "paused");
            let t = fixtures::topology(h).await;
            assert!(
                t["containers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["id"] == id)
            );
            assert!(fixtures::edge_set(&t).contains(&(id, nid)));
        })
    })
    .await
}
// Given an unpaused container, when topology is read, then its running node and network edge remain.
#[tokio::test]
async fn given_unpaused_container_when_topology_read_then_it_is_visible_with_its_network_edge() {
    E2eHarness::run(|h| {
        Box::pin(async move {
            fixtures::ensure_image(h).await;
            let (net, nid) = network(h, "unpause-net").await;
            let id = create(h, "unpaused", &["sh", "-c", "sleep 300"], &net, None, None).await;
            h.docker.start_container::<String>(&id, None).await.unwrap();
            h.docker.pause_container(&id).await.unwrap();
            h.docker.unpause_container(&id).await.unwrap();
            fixtures::wait_state(h, &id, "running", LIMIT).await;
            assert_record_status(&record(h, &id, "running").await, "up");
            assert!(fixtures::edge_set(&fixtures::topology(h).await).contains(&(id, nid)));
        })
    })
    .await
}
// Given a running container, when Docker restarts it, then the ID and edge are preserved.
#[tokio::test]
async fn given_running_container_when_docker_restarts_it_then_id_and_edge_are_preserved() {
    E2eHarness::run(|h| {
        Box::pin(async move {
            fixtures::ensure_image(h).await;
            let (net, _) = network(h, "restart-net").await;
            let id = create(
                h,
                "restart-manual",
                &["sh", "-c", "sleep 300"],
                &net,
                None,
                None,
            )
            .await;
            h.docker.start_container::<String>(&id, None).await.unwrap();
            let old = fixtures::inspect(h, &id).await.id.unwrap();
            h.docker
                .restart_container(
                    &id,
                    Some(bollard::container::RestartContainerOptions { t: 1 }),
                )
                .await
                .unwrap();
            fixtures::wait_state(h, &id, "running", LIMIT).await;
            assert_eq!(
                fixtures::inspect(h, &id).await.id.as_deref(),
                Some(old.as_str())
            );
            assert_record_status(&record(h, &id, "running").await, "up");
            let t = fixtures::topology(h).await;
            let net_id = t["networks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["name"] == net)
                .unwrap()["id"]
                .as_str()
                .unwrap()
                .to_owned();
            assert!(
                fixtures::edge_set(&t).contains(&(id, net_id)),
                "manual restart retains container network edge"
            );
        })
    })
    .await
}
// Given an Always restart loop, when inspect brackets HTTP during restarting, then the node is visible under its stable ID.
#[tokio::test]
async fn given_restarting_container_when_inspect_brackets_http_then_it_is_visible_with_stable_id() {
    E2eHarness::run(|h| {
        Box::pin(async move {
            fixtures::ensure_image(h).await;
            let (net, net_id) = network(h, "restarting-net").await;
            let cmd = "if [ ! -e /tmp/e2e-first ]; then touch /tmp/e2e-first; sleep 11; fi; exit 9";
            let id = create(
                h,
                "restart-loop",
                &["sh", "-c", cmd],
                &net,
                Some(RestartPolicy {
                    name: Some(RestartPolicyNameEnum::ALWAYS),
                    maximum_retry_count: None,
                }),
                None,
            )
            .await;
            h.docker.start_container::<String>(&id, None).await.unwrap();
            fixtures::wait_state(h, &id, "restarting", Duration::from_secs(90)).await;
            assert_eq!(
                fixtures::inspect(h, &id).await.id.as_deref(),
                Some(id.as_str())
            );
            assert_record_status(
                &restarting_observations(h, &id, &net_id).await,
                "restarting",
            );
        })
    })
    .await
}
// Given a removed container, when both APIs are read, then neither contains its ID or stale edges.
#[tokio::test]
async fn given_removed_container_when_both_apis_read_then_it_is_absent_without_stale_edges() {
    E2eHarness::run(|h| {
        Box::pin(async move {
            fixtures::ensure_image(h).await;
            let (net, _) = network(h, "removed-net").await;
            let id = create(h, "removed", &["sh", "-c", "sleep 300"], &net, None, None).await;
            h.docker.start_container::<String>(&id, None).await.unwrap();
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
            let (s, b) = h.get("/api/containers").await;
            assert_eq!(s, reqwest::StatusCode::OK, "{b}");
            assert!(!b.as_array().unwrap().iter().any(|r| r["id"] == id));
            let t = fixtures::topology(h).await;
            assert!(
                !t["containers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["id"] == id)
            );
            assert!(
                !fixtures::edge_set(&t)
                    .iter()
                    .any(|(source, _)| source == &id),
                "removed ID leaves no stale edge"
            );
        })
    })
    .await
}
async fn health_fixture(h: &E2eHarness, healthy_after: bool) {
    fixtures::ensure_image(h).await;
    let (net, _) = network(
        h,
        if healthy_after {
            "health-recovery-net"
        } else {
            "health-unhealthy-net"
        },
    )
    .await;
    let health = HealthConfig {
        test: Some(vec!["CMD-SHELL".into(), "test -f /tmp/healthy".into()]),
        interval: Some(1_000_000_000),
        timeout: Some(1_000_000_000),
        retries: Some(2),
        start_period: Some(0),
        start_interval: None,
    };
    let id = create(
        h,
        "health",
        &["sh", "-c", "sleep 300"],
        &net,
        None,
        Some(health),
    )
    .await;
    h.docker.start_container::<String>(&id, None).await.unwrap();
    wait_health(h, &id, "unhealthy").await;
    if healthy_after {
        let exec = h
            .docker
            .create_exec(
                &id,
                bollard::exec::CreateExecOptions {
                    cmd: Some(vec!["sh", "-c", "touch /tmp/healthy"]),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        h.docker.start_exec(&exec.id, None).await.unwrap();
        wait_health(h, &id, "healthy").await;
    }
    let wanted = if healthy_after {
        "healthy"
    } else {
        "unhealthy"
    };
    assert_health_status(&record(h, &id, "running").await, wanted);
}
// Given an unhealthy container, when the API is read, then its exact health marker is visible.
#[tokio::test]
async fn given_unhealthy_container_when_containers_read_then_health_marker_is_visible() {
    E2eHarness::run(|h| Box::pin(health_fixture(h, false))).await
}
// Given an unhealthy container recovers, when the API is read, then its exact healthy marker is visible.
#[tokio::test]
async fn given_recovered_healthy_container_when_containers_read_then_health_marker_is_visible() {
    E2eHarness::run(|h| Box::pin(health_fixture(h, true))).await
}

async fn wait_health(h: &E2eHarness, id: &str, wanted: &str) {
    let end = Instant::now() + LIMIT;
    loop {
        let got = fixtures::inspect(h, id)
            .await
            .state
            .and_then(|s| s.health)
            .and_then(|s| s.status)
            .map(|s| format!("{s:?}").to_lowercase())
            .unwrap_or_default();
        if got == wanted {
            return;
        }
        assert!(
            Instant::now() < end,
            "health fixture setup: expected {wanted}, got {got}"
        );
        sleep(Duration::from_millis(200)).await;
    }
}
