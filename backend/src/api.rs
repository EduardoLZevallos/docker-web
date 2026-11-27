use std::collections::{HashMap, HashSet};
use std::time::Duration;

use actix_web::{web, HttpResponse, Result as ActixResult};
use serde::Serialize;
use time::OffsetDateTime;

use crate::config::Config;
use crate::docker::{ContainerInfo, DockerClient, DockerError, NetworkInfo};

#[derive(Debug, Serialize)]
struct Edge {
    source: String,
    target: String,
}

fn timestamp_now() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// Builds container-to-network edges from Docker list data.
///
/// The attachment's full Docker network ID is preferred. A network name is
/// used only when the attachment has no ID, and only when that name maps to a
/// single listed network. An attachment with a stale or unknown ID never falls
/// back to name matching, so it cannot produce a dangling or incorrect edge.
fn topology_edges(containers: &[ContainerInfo], networks: &[NetworkInfo]) -> Vec<Edge> {
    let network_ids: HashSet<&str> = networks
        .iter()
        .map(|network| network.id.as_str())
        .collect();
    let mut unique_name_ids: HashMap<&str, Option<&str>> = HashMap::new();
    for network in networks {
        unique_name_ids
            .entry(network.name.as_str())
            .and_modify(|id| *id = None)
            .or_insert(Some(network.id.as_str()));
    }

    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for container in containers
        .iter()
        .filter(|container| container.is_edge_eligible())
    {
        for (name, attachment) in &container.networks {
            let attachment_id = attachment
                .network_id
                .as_deref()
                .filter(|network_id| !network_id.trim().is_empty());
            let target = if let Some(network_id) = attachment_id {
                network_ids
                    .contains(network_id)
                    .then_some(network_id)
            } else {
                unique_name_ids
                    .get(name.as_str())
                    .and_then(|network_id| *network_id)
            };
            let Some(target) = target else {
                continue;
            };
            let edge = (container.id.clone(), target.to_owned());
            if seen.insert(edge.clone()) {
                edges.push(Edge {
                    source: edge.0,
                    target: edge.1,
                });
            }
        }
    }
    edges
}

fn docker_error_response(err: &DockerError) -> HttpResponse {
    let status = match err {
        DockerError::ConnectionError(_) => {
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE
        }
        DockerError::ContainerError(_) => {
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        }
    };
    let error_type = if matches!(err, DockerError::ConnectionError(_)) {
        "Docker connection failed"
    } else {
        "Container operation failed"
    };
    let error_resp = serde_json::json!({
        "error": error_type,
        "message": err.to_string(),
        "timestamp": timestamp_now()
    });
    HttpResponse::build(status).json(error_resp)
}

/// Health check endpoint.
///
/// Returns service status and Docker reachability. Docker connectivity is
/// tested with a 5-second timeout to avoid blocking.
pub async fn health(docker_client: web::Data<DockerClient>) -> ActixResult<HttpResponse> {
    let docker_reachable = tokio::time::timeout(Duration::from_secs(5), docker_client.ping())
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false);

    let health_resp = serde_json::json!({
        "status": if docker_reachable { "healthy" } else { "degraded" },
        "docker": docker_reachable,
        "service": "docker-web-api",
        "timestamp": timestamp_now(),
        "version": env!("CARGO_PKG_VERSION")
    });

    Ok(HttpResponse::Ok().json(health_resp))
}

/// List all Docker containers, including inactive ones.
///
/// Inactive containers remain visible with their metadata but are not
/// eligible to contribute topology edges.
pub async fn get_containers(docker_client: web::Data<DockerClient>) -> ActixResult<HttpResponse> {
    match docker_client.list_containers().await {
        Ok(containers) => {
            log::info!("Successfully retrieved {} containers", containers.len());
            Ok(HttpResponse::Ok().json(containers))
        }
        Err(err) => {
            log::error!("Container operation error: {}", err);
            Ok(docker_error_response(&err))
        }
    }
}

/// List all Docker networks.
pub async fn get_networks(
    docker_client: web::Data<DockerClient>,
) -> ActixResult<HttpResponse> {
    match docker_client.list_networks().await {
        Ok(networks) => {
            log::info!("Successfully retrieved {} networks", networks.len());
            Ok(HttpResponse::Ok().json(networks))
        }
        Err(err) => {
            log::error!("Network operation error: {}", err);
            Ok(docker_error_response(&err))
        }
    }
}

/// Combined container and network topology with edges.
///
/// Fetches all containers and networks concurrently, then builds edges by
/// matching active containers' network memberships to network IDs. Inactive
/// containers remain as nodes without edges.
pub async fn get_network_topology(
    docker_client: web::Data<DockerClient>,
) -> ActixResult<HttpResponse> {
    let (containers_result, networks_result) = tokio::join!(
        docker_client.list_containers(),
        docker_client.list_networks()
    );

    match (containers_result, networks_result) {
        (Ok(containers), Ok(networks)) => {
            log::info!(
                "Successfully retrieved {} containers and {} networks",
                containers.len(),
                networks.len()
            );

            let edges = topology_edges(&containers, &networks);

            let topology = serde_json::json!({
                "containers": containers,
                "networks": networks,
                "edges": edges,
                "timestamp": timestamp_now()
            });

            Ok(HttpResponse::Ok().json(topology))
        }
        (Err(container_err), _) => {
            log::error!("Failed to retrieve containers: {}", container_err);
            Ok(docker_error_response(&container_err))
        }
        (_, Err(network_err)) => {
            log::error!("Failed to retrieve networks: {}", network_err);
            Ok(docker_error_response(&network_err))
        }
    }
}

/// Runtime frontend configuration.
///
/// Returns `demo_mode`, `theme`, and `discovery_interval` so the frontend can
/// decide which color scheme to apply and when to poll for fresh topology.
pub async fn get_config(config: web::Data<Config>) -> ActixResult<HttpResponse> {
    let config_resp = serde_json::json!({
        "demo_mode": config.demo_mode,
        "theme": config.frontend_theme,
        "discovery_interval": config.discovery_interval,
    });

    Ok(HttpResponse::Ok().json(config_resp))
}

/// Register all API routes under the given service config scope.
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/health", web::get().to(health))
        .route("/config", web::get().to(get_config))
        .route("/containers", web::get().to(get_containers))
        .route("/networks", web::get().to(get_networks))
        .route("/topology", web::get().to(get_network_topology));
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::docker::{ContainerNetworkInfo, IpamConfigInfo, PortInfo};

    fn network(id: &str, name: &str) -> NetworkInfo {
        NetworkInfo {
            id: id.to_string(),
            name: name.to_string(),
            driver: "bridge".to_string(),
            scope: "local".to_string(),
            subnet: None,
            ipam_configs: None::<Vec<IpamConfigInfo>>,
            internal: false,
            containers: Vec::new(),
        }
    }

    fn container(
        id: &str,
        state: Option<&str>,
        attachments: &[(&str, Option<&str>)],
    ) -> ContainerInfo {
        ContainerInfo {
            id: id.to_string(),
            name: id.to_string(),
            image: "alpine:3.20".to_string(),
            status: "Up 1 minute".to_string(),
            state: state.map(str::to_string),
            created: 1_700_000_000,
            ports: Vec::<PortInfo>::new(),
            networks: attachments
                .iter()
                .map(|(name, network_id)| {
                    (
                        (*name).to_string(),
                        ContainerNetworkInfo {
                            network_id: network_id.map(str::to_string),
                            ipv4_address: None,
                            ipv6_address: None,
                            ip_address: None,
                            mac_address: None,
                        },
                    )
                })
                .collect::<HashMap<_, _>>(),
        }
    }

    fn edge_set(edges: Vec<Edge>) -> HashSet<(String, String)> {
        edges
            .into_iter()
            .map(|edge| (edge.source, edge.target))
            .collect()
    }

    #[test]
    fn attachment_full_id_is_preferred_over_network_name() {
        let networks = vec![network("network-a-id", "net-a")];
        let containers = vec![container(
            "container-id",
            Some("running"),
            &[("net-a", Some("network-a-id"))],
        )];

        assert_eq!(
            edge_set(topology_edges(&containers, &networks)),
            HashSet::from([("container-id".to_string(), "network-a-id".to_string())])
        );
    }

    #[test]
    fn stale_attachment_id_does_not_fall_back_to_network_name() {
        let networks = vec![network("current-network-id", "net-a")];
        let containers = vec![container(
            "container-id",
            Some("running"),
            &[("net-a", Some("stale-network-id"))],
        )];

        assert!(topology_edges(&containers, &networks).is_empty());
    }

    #[test]
    fn missing_attachment_id_uses_unique_network_name() {
        let networks = vec![
            network("network-a-id", "net-a"),
            network("network-b-id", "net-b"),
        ];
        let containers = vec![container(
            "container-id",
            Some("paused"),
            &[("net-b", None), ("net-a", Some(""))],
        )];

        assert_eq!(
            edge_set(topology_edges(&containers, &networks)),
            HashSet::from([
                ("container-id".to_string(), "network-a-id".to_string()),
                ("container-id".to_string(), "network-b-id".to_string()),
            ])
        );
    }

    #[test]
    fn ambiguous_network_name_without_attachment_id_produces_no_edge() {
        let networks = vec![
            network("network-a-id", "duplicate"),
            network("network-b-id", "duplicate"),
        ];
        let containers = vec![container(
            "container-id",
            Some("restarting"),
            &[("duplicate", None)],
        )];

        assert!(topology_edges(&containers, &networks).is_empty());
    }

    #[test]
    fn inactive_or_unknown_states_do_not_produce_edges() {
        let networks = vec![network("network-a-id", "net-a")];
        for state in [Some("created"), Some("exited"), Some("dead"), None] {
            let containers = vec![container(
                "container-id",
                state,
                &[("net-a", Some("network-a-id"))],
            )];
            assert!(
                topology_edges(&containers, &networks).is_empty(),
                "state {state:?} must not produce an edge"
            );
        }
    }

    #[test]
    fn edges_are_deduplicated() {
        let networks = vec![network("network-a-id", "net-a")];
        let containers = vec![container(
            "container-id",
            Some("running"),
            &[
                ("alias-a", Some("network-a-id")),
                ("alias-b", Some("network-a-id")),
            ],
        )];

        let edges = topology_edges(&containers, &networks);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source, "container-id");
        assert_eq!(edges[0].target, "network-a-id");
    }
}
