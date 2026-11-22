//! Isolated lifecycle contract scenarios. Each is an independent test so a
//! known visibility failure cannot suppress the remaining lifecycle checks.
use super::E2eHarness;
use bollard::{
    container::{
        Config as ContainerConfig, CreateContainerOptions, InspectContainerOptions,
        ListContainersOptions, RemoveContainerOptions, StopContainerOptions,
    },
    models::{HealthConfig, HostConfig, RestartPolicy, RestartPolicyNameEnum},
    network::{ConnectNetworkOptions, CreateNetworkOptions},
};
use futures_util::StreamExt;
use serde_json::Value;
use std::{collections::BTreeSet, time::Duration};
use tokio::time::{Instant, sleep};
const IMAGE: &str = "alpine:3.20";
const LIMIT: Duration = Duration::from_secs(30);

async fn image(h: &E2eHarness) {
    let mut stream = h.docker.create_image(
        Some(bollard::image::CreateImageOptions {
            from_image: IMAGE,
            ..Default::default()
        }),
        None,
        None,
    );
    while let Some(item) = stream.next().await {
        item.unwrap_or_else(|e| panic!("pull {IMAGE}: {e}"));
    }
}
async fn network(h: &E2eHarness, stem: &str) -> (String, String) {
    let name = h.unique_name(stem);
    let mut labels = h.labels();
    labels.insert("com.docker-web.e2e.fixture".into(), "lifecycle".into());
    let id = h
        .docker
        .create_network(CreateNetworkOptions {
            name: name.clone(),
            driver: "bridge".into(),
            labels,
            ..Default::default()
        })
        .await
        .unwrap()
        .id
        .unwrap();
    (name, id)
}
async fn create(
    h: &E2eHarness,
    stem: &str,
    cmd: &[&str],
    net: &str,
    restart: Option<RestartPolicy>,
    health: Option<HealthConfig>,
) -> String {
    let mut labels = h.labels();
    labels.insert("com.docker-web.e2e.fixture".into(), "lifecycle".into());
    h.docker
        .create_container(
            Some(CreateContainerOptions {
                name: h.unique_name(stem),
                platform: None,
            }),
            ContainerConfig {
                image: Some(IMAGE.into()),
                cmd: Some(cmd.iter().map(|s| (*s).into()).collect()),
                labels: Some(labels),
                healthcheck: health,
                host_config: Some(HostConfig {
                    network_mode: Some(net.into()),
                    restart_policy: restart,
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .id
}
async fn inspect(h: &E2eHarness, id: &str) -> bollard::models::ContainerInspectResponse {
    h.docker
        .inspect_container(id, None::<InspectContainerOptions>)
        .await
        .unwrap()
}
fn state_name(i: &bollard::models::ContainerInspectResponse) -> String {
    i.state
        .as_ref()
        .and_then(|s| s.status.as_ref())
        .map(|s| format!("{s:?}").to_lowercase())
        .unwrap_or_default()
}
async fn wait_state(h: &E2eHarness, id: &str, wanted: &str, timeout: Duration) {
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
async fn record(h: &E2eHarness, id: &str, wanted: &str) -> Value {
    let deadline = Instant::now() + LIMIT;
    loop {
        assert_eq!(
            state_name(&inspect(h, id).await),
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
                state_name(&inspect(h, id).await),
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
async fn restarting_record(h: &E2eHarness, id: &str) -> Value {
    let deadline = Instant::now() + LIMIT;
    let mut saw_window = false;
    loop {
        if state_name(&inspect(h, id).await) == "restarting" {
            let (status, body) = h.get("/api/containers").await;
            assert_eq!(status, reqwest::StatusCode::OK, "{body}");
            let after = inspect(h, id).await;
            if state_name(&after) == "restarting" {
                saw_window = true;
                if let Some(row) = body
                    .as_array()
                    .and_then(|a| a.iter().find(|r| r["id"] == id))
                {
                    return row.clone();
                }
            }
        }
        if Instant::now() >= deadline {
            assert!(
                saw_window,
                "restart fixture setup failed: no inspect-restarting window bracketed an HTTP request for {id}"
            );
            panic!(
                "Docker inspect repeatedly confirmed restarting around HTTP reads, but /api/containers omitted {id}"
            );
        }
        sleep(Duration::from_millis(100)).await;
    }
}
async fn topology(h: &E2eHarness) -> Value {
    let (s, b) = h.get("/api/topology").await;
    assert_eq!(s, reqwest::StatusCode::OK, "{b}");
    b
}
fn edge_set(body: &Value) -> BTreeSet<(String, String)> {
    body["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["source"].as_str().unwrap().into(),
                e["target"].as_str().unwrap().into(),
            )
        })
        .collect()
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
        state_name(&inspect(h, id).await),
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
    let edges = edge_set(&t);
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
    image(h).await;
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
    wait_state(
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
        assert_eq!(inspect(h, &id).await.state.unwrap().exit_code, Some(code));
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
            image(h).await;
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
            assert_eq!(inspect(h, &paused).await.state.unwrap().paused, Some(true));

            let created =
                create(h, "mixed-created", &["sh", "-c", "sleep 300"], &a, None, None).await;
            let exited = create(h, "mixed-exited", &["sh", "-c", "exit 0"], &b, None, None).await;
            h.docker
                .start_container::<String>(&exited, None)
                .await
                .unwrap();
            wait_state(h, &exited, "exited", LIMIT).await;
            assert_eq!(state_name(&inspect(h, &created).await), "created");

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
            let fixture_edges: BTreeSet<_> = edge_set(&t)
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
                edge_set(&t)
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
            image(h).await;
            let (net, nid) = network(h, "running-net").await;
            let id = create(h, "running", &["sh", "-c", "sleep 300"], &net, None, None).await;
            h.docker.start_container::<String>(&id, None).await.unwrap();
            wait_state(h, &id, "running", LIMIT).await;
            let row = record(h, &id, "running").await;
            assert_record_status(&row, "up");
            let t = topology(h).await;
            assert!(
                t["containers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["id"] == id)
            );
            assert!(edge_set(&t).contains(&(id, nid)));
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
            image(h).await;
            let (net, nid) = network(h, "pause-net").await;
            let id = create(h, "paused", &["sh", "-c", "sleep 300"], &net, None, None).await;
            h.docker.start_container::<String>(&id, None).await.unwrap();
            h.docker.pause_container(&id).await.unwrap();
            assert_eq!(inspect(h, &id).await.state.unwrap().paused, Some(true));
            assert_eq!(state_name(&inspect(h, &id).await), "paused");
            let row = record(h, &id, "paused").await;
            assert_record_status(&row, "paused");
            let t = topology(h).await;
            assert!(
                t["containers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["id"] == id)
            );
            assert!(edge_set(&t).contains(&(id, nid)));
        })
    })
    .await
}
// Given an unpaused container, when topology is read, then its running node and network edge remain.
#[tokio::test]
async fn given_unpaused_container_when_topology_read_then_it_is_visible_with_its_network_edge() {
    E2eHarness::run(|h| {
        Box::pin(async move {
            image(h).await;
            let (net, nid) = network(h, "unpause-net").await;
            let id = create(h, "unpaused", &["sh", "-c", "sleep 300"], &net, None, None).await;
            h.docker.start_container::<String>(&id, None).await.unwrap();
            h.docker.pause_container(&id).await.unwrap();
            h.docker.unpause_container(&id).await.unwrap();
            wait_state(h, &id, "running", LIMIT).await;
            assert_record_status(&record(h, &id, "running").await, "up");
            assert!(edge_set(&topology(h).await).contains(&(id, nid)));
        })
    })
    .await
}
// Given a running container, when Docker restarts it, then the ID and edge are preserved.
#[tokio::test]
async fn given_running_container_when_docker_restarts_it_then_id_and_edge_are_preserved() {
    E2eHarness::run(|h| {
        Box::pin(async move {
            image(h).await;
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
            let old = inspect(h, &id).await.id.unwrap();
            h.docker
                .restart_container(
                    &id,
                    Some(bollard::container::RestartContainerOptions { t: 1 }),
                )
                .await
                .unwrap();
            wait_state(h, &id, "running", LIMIT).await;
            assert_eq!(inspect(h, &id).await.id.as_deref(), Some(old.as_str()));
            assert_record_status(&record(h, &id, "running").await, "up");
            let t = topology(h).await;
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
                edge_set(&t).contains(&(id, net_id)),
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
            image(h).await;
            let (net, _) = network(h, "restarting-net").await;
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
            wait_state(h, &id, "restarting", Duration::from_secs(90)).await;
            assert_eq!(inspect(h, &id).await.id.as_deref(), Some(id.as_str()));
            assert_record_status(&restarting_record(h, &id).await, "restarting");
        })
    })
    .await
}
// Given a removed container, when both APIs are read, then neither contains its ID or stale edges.
#[tokio::test]
async fn given_removed_container_when_both_apis_read_then_it_is_absent_without_stale_edges() {
    E2eHarness::run(|h| {
        Box::pin(async move {
            image(h).await;
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
            let t = topology(h).await;
            assert!(
                !t["containers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["id"] == id)
            );
            assert!(
                !edge_set(&t).iter().any(|(source, _)| source == &id),
                "removed ID leaves no stale edge"
            );
        })
    })
    .await
}
async fn health_fixture(h: &E2eHarness, healthy_after: bool) {
    image(h).await;
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
        let got = inspect(h, id)
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
