use actix_web::{web, HttpResponse, Result as ActixResult};
use crate::docker::DockerClient;

/// Health check endpoint
pub async fn health() -> ActixResult<HttpResponse> {
    let health_resp = serde_json::json!({
        "status": "healthy",
        "service": "docker-web-api",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION")
    });
    
    Ok(HttpResponse::Ok().json(health_resp))
}

/// Get all running containers
pub async fn get_containers(docker_client: web::Data<DockerClient>) -> ActixResult<HttpResponse> {
    match docker_client.list_running_containers().await {
        Ok(containers) => {
            log::info!("Successfully retrieved {} containers", containers.len());
            Ok(HttpResponse::Ok().json(containers))
        },
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

/// Get a specific container by ID
pub async fn get_container_by_id(
    path: web::Path<String>,
    docker_client: web::Data<DockerClient>,
) -> ActixResult<HttpResponse> {
    let container_id = path.into_inner();
    
    match docker_client.get_container_by_id(&container_id).await {
        Ok(Some(container)) => {
            log::info!("Successfully retrieved container: {}", container_id);
            Ok(HttpResponse::Ok().json(container))
        },
        Ok(None) => {
            log::warn!("Container not found: {}", container_id);
            let error_resp = serde_json::json!({
                "error": "Container not found",
                "message": format!("No container found with ID: {}", container_id),
                "timestamp": chrono::Utc::now().to_rfc3339()
            });
            Ok(HttpResponse::NotFound().json(error_resp))
        },
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

/// Get all Docker networks
pub async fn get_networks(docker_client: web::Data<DockerClient>) -> ActixResult<HttpResponse> {
    match docker_client.list_networks().await {
        Ok(networks) => {
            log::info!("Successfully retrieved {} networks", networks.len());
            Ok(HttpResponse::Ok().json(networks))
        },
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

/// Get a specific network by ID or name
pub async fn get_network_by_id(
    path: web::Path<String>,
    docker_client: web::Data<DockerClient>,
) -> ActixResult<HttpResponse> {
    let network_id = path.into_inner();
    
    match docker_client.get_network_by_id(&network_id).await {
        Ok(Some(network)) => {
            log::info!("Successfully retrieved network: {}", network_id);
            Ok(HttpResponse::Ok().json(network))
        },
        Ok(None) => {
            log::warn!("Network not found: {}", network_id);
            let error_resp = serde_json::json!({
                "error": "Network not found",
                "message": format!("No network found with ID or name: {}", network_id),
                "timestamp": chrono::Utc::now().to_rfc3339()
            });
            Ok(HttpResponse::NotFound().json(error_resp))
        },
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

/// Get combined container and network data for visualization
pub async fn get_network_topology(docker_client: web::Data<DockerClient>) -> ActixResult<HttpResponse> {
    // Fetch both containers and networks concurrently
    let (containers_result, networks_result) = tokio::join!(
        docker_client.list_running_containers(),
        docker_client.list_networks()
    );

    match (containers_result, networks_result) {
        (Ok(containers), Ok(networks)) => {
            log::info!("Successfully retrieved {} containers and {} networks", 
                      containers.len(), networks.len());
            
            let topology = serde_json::json!({
                "containers": containers,
                "networks": networks,
                "timestamp": chrono::Utc::now().to_rfc3339()
            });
            
            Ok(HttpResponse::Ok().json(topology))
        },
        (Err(container_err), _) => {
            log::error!("Failed to retrieve containers: {}", container_err);
            let error_resp = serde_json::json!({
                "error": "Failed to retrieve containers",
                "message": container_err.to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339()
            });
            Ok(HttpResponse::InternalServerError().json(error_resp))
        },
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

/// Configure API routes
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg
        .route("/health", web::get().to(health))
        .route("/containers", web::get().to(get_containers))
        .route("/containers/{id}", web::get().to(get_container_by_id))
        .route("/networks", web::get().to(get_networks))
        .route("/networks/{id}", web::get().to(get_network_by_id))
        .route("/topology", web::get().to(get_network_topology));
}