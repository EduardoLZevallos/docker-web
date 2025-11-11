use actix_web::{web, HttpResponse, Result};
use crate::config::Config;
use crate::docker::{DockerClient, DockerError};

/// Health check endpoint
pub async fn health() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "service": "docker-web-api",
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}

/// Get all running containers
pub async fn get_containers(config: web::Data<Config>) -> Result<HttpResponse> {
    match DockerClient::new(&config) {
        Ok(client) => {
            match client.list_running_containers().await {
                Ok(containers) => {
                    log::info!("Successfully retrieved {} containers", containers.len());
                    Ok(HttpResponse::Ok().json(containers))
                },
                Err(DockerError::ConnectionError(err)) => {
                    log::error!("Docker connection error: {}", err);
                    Ok(HttpResponse::ServiceUnavailable().json(serde_json::json!({
                        "error": "Docker daemon connection failed",
                        "message": err.to_string()
                    })))
                },
                Err(DockerError::ContainerError(err)) => {
                    log::error!("Container operation error: {}", err);
                    Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": "Container operation failed",
                        "message": err
                    })))
                }
            }
        },
        Err(err) => {
            log::error!("Failed to create Docker client: {}", err);
            Ok(HttpResponse::ServiceUnavailable().json(serde_json::json!({
                "error": "Docker client initialization failed",
                "message": err.to_string()
            })))
        }
    }
}

/// Configure API routes
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg
        .route("/health", web::get().to(health))
        .route("/containers", web::get().to(get_containers));
}