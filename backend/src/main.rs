use actix_web::{web, App, HttpServer, middleware::Logger};
use docker_web::{api, config::Config, docker::DockerClient};
use log::info;
use std::sync::Arc;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load environment variables from .env file (if it exists)
    dotenvy::dotenv().ok();
    
    // Initialize logging
    env_logger::init();
    
    info!("Starting Docker Web API server...");
    
    // Load configuration for bind address (with proper validation) and wrap in Arc
    let config = Arc::new(Config::with_defaults());
    info!("Loaded configuration: bind address = {}", config.bind_address);

    // Create DockerClient once during startup using the config
    let docker_client = DockerClient::new(&config)
        .map_err(|e| {
            log::error!("Failed to create Docker client during startup: {}", e);
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        })?;
    info!("Docker client initialized successfully");

    // Start HTTP server
    let bind_address = config.bind_address.clone();
    info!("Starting server on {}", bind_address);
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(docker_client.clone()))
            .app_data(web::Data::from(config.clone())) // Share config without cloning for each worker
            .wrap(Logger::default())
            .service(
                web::scope("/api")
                    .configure(api::configure_routes)
            )
    })
    .bind(&bind_address)?
    .run()
    .await
}
