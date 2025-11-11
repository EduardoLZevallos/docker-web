use actix_web::{web, HttpResponse, Result};
use crate::config::Config;
use crate::docker::DockerClient;

/// Health check endpoint
pub async fn health() -> Result<HttpResponse> {
    let health_resp = serde_json::json!({
        "status": "healthy",
        "service": "docker-web-api",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION")
    });
    
    Ok(HttpResponse::Ok().json(health_resp))
}

/// Get all running containers
pub async fn get_containers(config: web::Data<Config>) -> Result<HttpResponse> {
    let client = match DockerClient::new(&config) {
        Ok(client) => client,
        Err(err) => {
            log::error!("Failed to create Docker client: {}", err);
            let error_resp = serde_json::json!({
                "error": "Docker daemon connection failed",
                "message": err.to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339()
            });
            return Ok(HttpResponse::ServiceUnavailable().json(error_resp));
        }
    };

    match client.list_running_containers().await {
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

/// Configure API routes
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg
        .route("/health", web::get().to(health))
        .route("/containers", web::get().to(get_containers));
}