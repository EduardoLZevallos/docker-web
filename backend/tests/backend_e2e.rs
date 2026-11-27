mod e2e_support;

use std::{collections::HashMap, fs, path::PathBuf, time::Duration};

use bollard::models::{HostConfig, PortBinding};
use e2e_support::{E2eHarness, fixtures};
use serde_json::Value;

const DEMO_NETWORKS: [(&str, usize); 3] = [("frontend", 2), ("backend", 3), ("data", 4)];
const DEMO_CONTAINER_COMMAND: [&str; 3] = ["sh", "-c", "sleep 3600"];
const DEMO_HOLD_SECONDS_DEFAULT: u64 = 900;

#[tokio::test]
async fn backend_serves_http_against_an_isolated_docker_engine() {
    E2eHarness::run(|h| {
        Box::pin(async move {
            let (status, body) = h.get("/api/health").await;
            assert_eq!(status, reqwest::StatusCode::OK, "health response: {body}");
            assert_eq!(body["service"], "docker-web-api");
            assert_eq!(body["docker"], true);

            let (status, topology) = h.get("/api/topology").await;
            assert_eq!(
                status,
                reqwest::StatusCode::OK,
                "topology response: {topology}"
            );
            assert!(topology["containers"].is_array());
            assert!(topology["networks"].is_array());
            assert!(topology["edges"].is_array());

            let response_json: Value = topology;
            assert!(response_json["timestamp"].as_str().is_some());
        })
    })
    .await;
}

/// Builds the real UI with the E2E bridge, serves it with Vite preview, and runs
/// the Playwright suite against the actual backend and private DinD daemon.
///
/// Run it through the frontend runner:
/// `frontend/docker-web-ui/tests/run_frontend_e2e.sh`.
#[tokio::test]
#[ignore = "browser E2E: builds the UI and runs Playwright against the private DinD fixture"]
async fn playwright_frontend_e2e() {
    e2e_support::browser::run_playwright_frontend_e2e().await;
}

/// Seeds three networks with 2, 3, and 4 containers, prints the live API URL,
/// and holds the isolated topology open for visual inspection.
///
/// This is an interactive demo, not a contract test. Run it through the suite
/// runner with `backend/tests/run_backend_e2e.sh demo --ignored --nocapture`.
/// Set `DEMO_HOLD_SECONDS` to bound the hold; Ctrl+C stops early and cleans up.
#[tokio::test]
#[ignore = "interactive demo: seeds a fake topology and holds it for inspection"]
async fn demo_three_networks_with_fake_containers() {
    E2eHarness::run(|h| {
        Box::pin(async move {
            let hold_seconds = std::env::var("DEMO_HOLD_SECONDS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(DEMO_HOLD_SECONDS_DEFAULT);

            #[cfg(unix)]
            let mut interrupt =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                    .expect("register demo SIGINT handler");
            #[cfg(unix)]
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("register demo SIGTERM handler");

            let setup = async {
                fixtures::ensure_image(h).await;

                let mut summary = Vec::new();
                let mut demo_network_manifest = Vec::new();
                let mut demo_container_manifest = Vec::new();
                let mut created_networks = Vec::new();
                for (stem, count) in DEMO_NETWORKS {
                    let network = fixtures::create_network(
                        h,
                        &h.unique_name(&format!("demo-{stem}")),
                        "bridge",
                        false,
                        "demo",
                    )
                    .await;
                    let mut names = Vec::new();
                    for index in 1..=count {
                        let name = h.unique_name(&format!("demo-{stem}-{index}"));
                        let container = fixtures::create_and_start_network_container(
                            h,
                            &name,
                            &network.name,
                            &DEMO_CONTAINER_COMMAND,
                            "demo",
                        )
                        .await;
                        demo_container_manifest.push(serde_json::json!({
                            "name": container.name,
                            "id": container.id,
                            "networks": [network.name.clone()],
                        }));
                        names.push(name);
                    }
                    summary.push(format!("  {}: {}", network.name, names.join(", ")));
                    demo_network_manifest.push(serde_json::json!({
                        "name": network.name,
                        "id": network.id,
                    }));
                    created_networks.push(network);
                }

                let frontend_name = created_networks
                    .first()
                    .expect("demo frontend network exists")
                    .name
                    .clone();
                let backend_name = created_networks
                    .get(1)
                    .expect("demo backend network exists")
                    .name
                    .clone();
                let multi_name = h.unique_name("demo-shared");
                let multi_homed = fixtures::create_container(
                    h,
                    &multi_name,
                    &DEMO_CONTAINER_COMMAND,
                    HostConfig {
                        network_mode: Some(frontend_name.clone()),
                        port_bindings: Some(HashMap::from([
                            (
                                "8080/tcp".to_string(),
                                Some(vec![PortBinding {
                                    host_ip: Some("127.0.0.1".to_string()),
                                    host_port: Some("0".to_string()),
                                }]),
                            ),
                            (
                                "8081/tcp".to_string(),
                                Some(vec![PortBinding {
                                    host_ip: Some("0.0.0.0".to_string()),
                                    host_port: Some("0".to_string()),
                                }]),
                            ),
                        ])),
                        ..Default::default()
                    },
                    Some(vec!["8080/tcp", "8081/tcp"]),
                    None,
                    "demo",
                )
                .await;
                fixtures::start_container(h, &multi_homed.id).await;
                fixtures::connect_container_to_network(h, &multi_homed.id, &backend_name).await;
                demo_container_manifest.push(serde_json::json!({
                    "name": multi_homed.name,
                    "id": multi_homed.id,
                    "networks": [frontend_name.clone(), backend_name.clone()],
                    "host_bindings": [
                        "127.0.0.1:0 -> 8080/tcp",
                        "0.0.0.0:0 -> 8081/tcp"
                    ],
                }));
                summary.push(format!(
                    "  {}: {} + {} (loopback 8080/tcp, wildcard 8081/tcp)",
                    multi_homed.name, frontend_name, backend_name
                ));

                write_demo_ready_file(
                    h,
                    hold_seconds,
                    &demo_network_manifest,
                    &demo_container_manifest,
                );

                eprintln!("\nDEMO READY (token {})", h.run_id);
                eprintln!("  topology:   {}/api/topology", h.base_url);
                eprintln!("  containers: {}/api/containers", h.base_url);
                eprintln!("  networks:   {}/api/networks", h.base_url);
                eprintln!("  fixtures:");
                for line in &summary {
                    eprintln!("{line}");
                }
                eprintln!(
                    "Holding for up to {hold_seconds}s; press Ctrl+C to stop and clean up.\n"
                );
            };

            #[cfg(unix)]
            tokio::select! {
                _ = setup => {}
                _ = interrupt.recv() => {
                    eprintln!("SIGINT received during startup; cleaning up...");
                    return;
                }
                _ = terminate.recv() => {
                    eprintln!("SIGTERM received during startup; cleaning up...");
                    return;
                }
            }
            #[cfg(not(unix))]
            setup.await;

            #[cfg(unix)]
            tokio::select! {
                _ = interrupt.recv() => eprintln!("SIGINT received; cleaning up..."),
                _ = terminate.recv() => eprintln!("SIGTERM received; cleaning up..."),
                _ = tokio::time::sleep(Duration::from_secs(hold_seconds)) => {
                    eprintln!("Demo hold elapsed; cleaning up...");
                }
            }
            #[cfg(not(unix))]
            tokio::select! {
                _ = tokio::signal::ctrl_c() => eprintln!("Ctrl+C received; cleaning up..."),
                _ = tokio::time::sleep(Duration::from_secs(hold_seconds)) => {
                    eprintln!("Demo hold elapsed; cleaning up...");
                }
            }
        })
    })
    .await;
}

fn write_demo_ready_file(
    h: &E2eHarness,
    hold_seconds: u64,
    networks: &[Value],
    containers: &[Value],
) {
    let Some(path) = std::env::var_os("DEMO_READY_FILE").map(PathBuf::from) else {
        return;
    };
    let payload = serde_json::json!({
        "base_url": h.base_url,
        "run_id": h.run_id,
        "token": h.run_id,
        "dind_id": h.dind_container_id(),
        "backend_id": h.backend_container_id(),
        "hold_seconds": hold_seconds,
        "topology_url": format!("{}/api/topology", h.base_url),
        "networks": networks,
        "containers": containers,
    });
    let temporary = path.with_extension(format!("tmp-{}-{}", std::process::id(), hold_seconds));
    let rendered = serde_json::to_vec_pretty(&payload).expect("serialize demo readiness");
    fs::write(&temporary, rendered)
        .unwrap_or_else(|error| panic!("write demo readiness {}: {error}", temporary.display()));
    fs::rename(&temporary, &path).unwrap_or_else(|error| {
        let _ = fs::remove_file(&temporary);
        panic!("publish demo readiness {}: {error}", path.display());
    });
}
