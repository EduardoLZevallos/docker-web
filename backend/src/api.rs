use std::collections::HashMap;

use actix_web::{web, HttpResponse, Result as ActixResult};
use serde::Serialize;

use crate::config::Config;
use crate::docker::DockerClient;

#[derive(Serialize)]
struct Edge {
    source: String,
    target: String,
}

pub async fn health() -> ActixResult<HttpResponse> {
    let health_resp = serde_json::json!({
        "status": "healthy",
        "service": "docker-web-api",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION")
    });

    Ok(HttpResponse::Ok().json(health_resp))
}

pub async fn get_containers(
    docker_client: web::Data<DockerClient>,
) -> ActixResult<HttpResponse> {
    match docker_client.list_running_containers().await {
        Ok(containers) => {
            log::info!("Successfully retrieved {} containers", containers.len());
            Ok(HttpResponse::Ok().json(containers))
        }
        Err(err) => {
            log::error!("Container operation error: {}", err);
            let error_resp = serde_json::json!({
                "error": "Container operation failed",
                "message": err.to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339()
            });
            Ok(HttpResponse::InternalServerError().json(error_resp))
        }
    }
}

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
            let error_resp = serde_json::json!({
                "error": "Network operation failed",
                "message": err.to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339()
            });
            Ok(HttpResponse::InternalServerError().json(error_resp))
        }
    }
}

pub async fn get_network_topology(
    docker_client: web::Data<DockerClient>,
) -> ActixResult<HttpResponse> {
    let (containers_result, networks_result) = tokio::join!(
        docker_client.list_running_containers(),
        docker_client.list_networks()
    );

    match (containers_result, networks_result) {
        (Ok(containers), Ok(networks)) => {
            log::info!(
                "Successfully retrieved {} containers and {} networks",
                containers.len(),
                networks.len()
            );

            let name_to_id: HashMap<&str, &str> = networks
                .iter()
                .map(|n| (n.name.as_str(), n.id.as_str()))
                .collect();

            let edges: Vec<Edge> = containers
                .iter()
                .flat_map(|c| {
                    c.networks.keys().filter_map(|net_name| {
                        name_to_id.get(net_name.as_str()).map(|net_id| Edge {
                            source: c.id.clone(),
                            target: net_id.to_string(),
                        })
                    })
                })
                .collect();

            let topology = serde_json::json!({
                "containers": containers,
                "networks": networks,
                "edges": edges,
                "timestamp": chrono::Utc::now().to_rfc3339()
            });

            Ok(HttpResponse::Ok().json(topology))
        }
        (Err(container_err), _) => {
            log::error!("Failed to retrieve containers: {}", container_err);
            let error_resp = serde_json::json!({
                "error": "Failed to retrieve containers",
                "message": container_err.to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339()
            });
            Ok(HttpResponse::InternalServerError().json(error_resp))
        }
        (_, Err(network_err)) => {
            log::error!("Failed to retrieve networks: {}", network_err);
            let error_resp = serde_json::json!({
                "error": "Failed to retrieve networks",
                "message": network_err.to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339()
            });
            Ok(HttpResponse::InternalServerError().json(error_resp))
        }
    }
}

pub async fn get_config(config: web::Data<Config>) -> ActixResult<HttpResponse> {
    let config_resp = serde_json::json!({
        "demo_mode": config.demo_mode,
        "theme": config.frontend_theme,
    });

    Ok(HttpResponse::Ok().json(config_resp))
}

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/health", web::get().to(health))
        .route("/config", web::get().to(get_config))
        .route("/containers", web::get().to(get_containers))
        .route("/networks", web::get().to(get_networks))
        .route("/topology", web::get().to(get_network_topology));
}
