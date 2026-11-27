use std::time::Duration;

use bollard::container::{
    InspectContainerOptions, RemoveContainerOptions, RestartContainerOptions, StopContainerOptions,
};
use serde_json::Value;
use tokio::time::{Instant, sleep, timeout};

use super::super::{
    E2eHarness,
    fixtures::{self, FixtureContainer, FixtureNetwork, TopologySnapshot},
};

const LIMIT: Duration = Duration::from_secs(40);

async fn fixture(h: &E2eHarness, stem: &str) -> (FixtureContainer, FixtureNetwork) {
    let network = fixtures::create_network(
        h,
        &h.unique_name(&format!("{stem}-net")),
        "bridge",
        false,
        "resilience",
    )
    .await;
    let container = fixtures::create_and_start_network_container(
        h,
        &h.unique_name(stem),
        &network.name,
        &["sh", "-c", "sleep 300"],
        "resilience",
    )
    .await;
    (container, network)
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
            fixtures::assert_timestamp(&body["timestamp"]);
            return body;
        }
        assert!(
            Instant::now() < end,
            "health did not reach status={status} docker={docker}; last={body}"
        );
        sleep(Duration::from_millis(200)).await;
    }
}

// Given a running fixture, when the same DinD daemon stops and restarts, then the backend
// reports the outage, recovers without restarting, and discovers fresh marker workloads.
#[tokio::test]
async fn given_running_fixture_when_daemon_outage_and_restart_occur_then_backend_recovers_without_restarting()
 {
    E2eHarness::run(|h| {
        Box::pin(async move {
            fixtures::ensure_image(h).await;
            let (initial, initial_net) = fixture(h, "before-outage").await;
            let (health_code, health) = h.get("/api/health").await;
            assert_eq!(health_code, reqwest::StatusCode::OK, "{health}");
            assert_eq!(health["status"], "healthy");
            assert_eq!(health["docker"], true);
            fixtures::assert_timestamp(&health["timestamp"]);
            let (config_code, config) = h.get("/api/config").await;
            assert_eq!(config_code, reqwest::StatusCode::OK, "{config}");
            assert_eq!(config["demo_mode"], false);
            assert_eq!(config["theme"], "light");
            assert_eq!(config["discovery_interval"], 30);
            let (topology_code, topology) = h.get("/api/topology").await;
            assert_eq!(topology_code, reqwest::StatusCode::OK, "{topology}");
            fixtures::assert_timestamp(&topology["timestamp"]);
            assert!(topology["networks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|net| net["name"] == initial_net.name && net["id"] == initial_net.id));
            let initial_record = topology["containers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|container| container["id"] == initial.id)
                .unwrap();
            assert_eq!(initial_record["name"], initial.name);
            assert_eq!(initial_record["image"], fixtures::IMAGE);
            assert!(initial_record["created"].as_i64().is_some());
            let initial_network = topology["networks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|net| net["id"] == initial_net.id)
                .unwrap();
            assert_eq!(initial_network["driver"], "bridge");
            assert_eq!(initial_network["scope"], "local");
            assert_eq!(initial_network["internal"], false);
            assert!(initial_network["subnet"].as_str().is_some());
            assert!(fixtures::edge_set(&topology).contains(&(initial.id.clone(), initial_net.id)));

            let backend_identity = h.backend_identity().await;
            h.stop_dind().await;
            wait_health(h, "degraded", false, Duration::from_secs(15)).await;
            for path in ["/api/containers", "/api/networks", "/api/topology"] {
                let (status, body) = h.get(path).await;
                fixtures::assert_connection_error(status, &body, path);
            }
            let (config_code, config) = h.get("/api/config").await;
            assert_eq!(
                config_code,
                reqwest::StatusCode::OK,
                "config remains available during outage: {config}"
            );
            assert_eq!(config["demo_mode"], false);
            assert_eq!(config["discovery_interval"], 30);

            h.start_dind().await;
            wait_health(h, "healthy", true, Duration::from_secs(60)).await;
            assert_eq!(
                h.backend_identity().await,
                backend_identity,
                "backend container identity/start time must not change"
            );

            // Container survival is daemon/runtime dependent; if it survived, its exact
            // Docker ID remains inspectable.
            match h.docker.inspect_container(&initial.id, None).await {
                Ok(inspect) => {
                    assert_eq!(inspect.id.as_deref(), Some(initial.id.as_str()));
                    assert_eq!(
                        inspect.name.as_deref(),
                        Some(format!("/{}", initial.name).as_str())
                    );
                }
                Err(error) => eprintln!(
                    "E2E recovery: pre-outage container was not retained by dockerd restart: {error}"
                ),
            }

            let (recovered, recovered_net) = fixture(h, "after-outage").await;
            let deadline = Instant::now() + LIMIT;
            loop {
                let (status, body) = h.get("/api/topology").await;
                assert_eq!(
                    status,
                    reqwest::StatusCode::OK,
                    "recovery topology: {body}"
                );
                let snapshot = TopologySnapshot::from_value(&body);
                snapshot.assert_unique_and_referential();
                let net_id = snapshot
                    .network_manifest()
                    .get(&recovered_net.name)
                    .cloned();
                if snapshot.container_ids().contains(&recovered.id)
                    && net_id.as_deref() == Some(recovered_net.id.as_str())
                    && snapshot
                        .edges
                        .contains(&(recovered.id.clone(), recovered_net.id.clone()))
                {
                    let record = snapshot
                        .containers
                        .iter()
                        .find(|container| container.id == recovered.id)
                        .unwrap();
                    assert_eq!(record.name, recovered.name);
                    assert_eq!(record.image, fixtures::IMAGE);
                    let network = snapshot
                        .networks
                        .iter()
                        .find(|network| network.id == recovered_net.id)
                        .unwrap();
                    assert_eq!(network.driver, "bridge");
                    assert_eq!(network.scope, "local");
                    assert!(!network.internal);
                    assert!(network.subnet.is_some());
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "recovered container/network edge did not appear: {body}"
                );
                sleep(Duration::from_millis(150)).await;
            }
        })
    })
    .await;
}

// Given an unresponsive daemon socket, when health and discovery are read, then each response
// is bounded and recovery does not require restarting the backend.
#[tokio::test]
async fn given_unresponsive_daemon_when_endpoints_read_then_responses_are_bounded() {
    E2eHarness::run(|h| {
        Box::pin(async move {
            fixtures::ensure_image(h).await;
            let (initial, initial_net) = fixture(h, "unresponsive-before").await;
            wait_health(h, "healthy", true, Duration::from_secs(60)).await;
            let before = TopologySnapshot::from_value(&fixtures::topology(h).await);
            assert!(before.container_ids().contains(&initial.id));
            assert!(
                before
                    .edges
                    .contains(&(initial.id.clone(), initial_net.id.clone()))
            );

            h.pause_dind().await;

            let (status, body) = timeout(
                Duration::from_secs(8),
                h.get("/api/health"),
            )
            .await
            .expect("health response must be bounded while the daemon socket is unresponsive");
            assert_eq!(status, reqwest::StatusCode::OK, "{body}");
            assert_eq!(body["status"], "degraded", "{body}");
            assert_eq!(body["docker"], false, "{body}");
            fixtures::assert_timestamp(&body["timestamp"]);

            let (status, body) = timeout(
                Duration::from_secs(8),
                h.get("/api/topology"),
            )
            .await
            .expect("discovery response must be bounded while the daemon socket is unresponsive");
            fixtures::assert_connection_error(status, &body, "/api/topology");

            let (status, config) = timeout(
                Duration::from_secs(8),
                h.get("/api/config"),
            )
            .await
            .expect("config must stay available while the daemon socket is unresponsive");
            assert_eq!(status, reqwest::StatusCode::OK, "{config}");
            assert_eq!(config["demo_mode"], false);
            assert_eq!(config["discovery_interval"], 30);

            h.unpause_dind().await;
            wait_health(h, "healthy", true, Duration::from_secs(60)).await;

            let (recovered, recovered_net) = fixture(h, "unresponsive-after").await;
            let deadline = Instant::now() + LIMIT;
            loop {
                let body = fixtures::topology(h).await;
                let snapshot = TopologySnapshot::from_value(&body);
                if snapshot.container_ids().contains(&recovered.id)
                    && snapshot
                        .edges
                        .contains(&(recovered.id.clone(), recovered_net.id.clone()))
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "fresh marker workload did not appear after unresponsive-daemon recovery: {body}"
                );
                sleep(Duration::from_millis(150)).await;
            }
        })
    })
    .await;
}

// Given a running container, when it stops, starts, restarts, and is removed, then identity
// and network edges follow each transition while concurrent readers only see valid snapshots.
#[tokio::test]
async fn given_running_container_when_stopped_started_restarted_and_removed_then_identity_and_edges_follow()
 {
    E2eHarness::run(|h| {
        Box::pin(async move {
            fixtures::ensure_image(h).await;
            let (container, network) = fixture(h, "sequence").await;
            let initial = fixtures::inspect(h, &container.id).await;
            assert_eq!(initial.id.as_deref(), Some(container.id.as_str()));
            assert_eq!(
                initial.state.unwrap().status,
                Some(bollard::models::ContainerStateStatusEnum::RUNNING)
            );

            let topology = fixtures::topology(h).await;
            fixtures::assert_timestamp(&topology["timestamp"]);
            assert!(
                topology["networks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|net| net["name"] == network.name && net["id"] == network.id)
            );
            assert!(
                fixtures::edge_set(&topology).contains(&(container.id.clone(), network.id.clone()))
            );

            h.docker
                .stop_container(&container.id, Some(StopContainerOptions { t: 0 }))
                .await
                .unwrap();
            let stopped = fixtures::inspect(h, &container.id).await;
            assert_eq!(stopped.id.as_deref(), Some(container.id.as_str()));
            assert_eq!(
                stopped.state.unwrap().status,
                Some(bollard::models::ContainerStateStatusEnum::EXITED)
            );

            // Ordinary mutations must keep both endpoints available. Each response must be a
            // valid snapshot whose edges reference nodes in that same response.
            let http = h.http.clone();
            let url = format!("{}/api/topology", h.base_url);
            let reader = tokio::spawn(async move {
                let mut observations = 0;
                for _ in 0..5 {
                    let response = http.get(&url).send().await.unwrap();
                    assert_eq!(response.status(), reqwest::StatusCode::OK);
                    let body: Value = response.json().await.unwrap();
                    let snapshot = TopologySnapshot::from_value(&body);
                    snapshot.assert_unique_and_referential();
                    observations += 1;
                    sleep(Duration::from_millis(100)).await;
                }
                observations
            });
            fixtures::start_container(h, &container.id).await;
            let started = fixtures::inspect(h, &container.id).await;
            assert_eq!(started.id.as_deref(), Some(container.id.as_str()));
            assert_eq!(
                started.state.unwrap().status,
                Some(bollard::models::ContainerStateStatusEnum::RUNNING)
            );
            let restored = fixtures::topology(h).await;
            assert!(
                restored["containers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row["id"] == container.id && row["name"] == container.name)
            );
            assert!(
                fixtures::edge_set(&restored).contains(&(container.id.clone(), network.id.clone()))
            );

            h.docker
                .restart_container(&container.id, Some(RestartContainerOptions { t: 1 }))
                .await
                .unwrap();
            let restarted = fixtures::inspect(h, &container.id).await;
            assert_eq!(
                restarted.id.as_deref(),
                Some(container.id.as_str()),
                "restart keeps the original Docker ID"
            );
            assert_eq!(
                restarted.state.unwrap().status,
                Some(bollard::models::ContainerStateStatusEnum::RUNNING)
            );
            let after_restart = fixtures::topology(h).await;
            assert!(
                fixtures::edge_set(&after_restart).contains(&(container.id.clone(), network.id))
            );

            h.docker
                .remove_container(
                    &container.id,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await
                .unwrap();
            let removed = h
                .docker
                .inspect_container(&container.id, Some(InspectContainerOptions { size: false }))
                .await;
            assert!(
                removed.is_err(),
                "removed fixture must be absent from Docker inspect"
            );
            let after_remove = fixtures::topology(h).await;
            assert!(
                !after_remove["containers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row["id"] == container.id)
            );
            assert!(
                !fixtures::edge_set(&after_remove)
                    .iter()
                    .any(|(source, _)| source == &container.id),
                "removed container leaves no topology edges"
            );
            assert_eq!(reader.await.unwrap(), 5);
        })
    })
    .await;
}
