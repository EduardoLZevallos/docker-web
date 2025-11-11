mod api;
mod config;
mod docker;

use actix_web::{web, App, HttpServer, middleware::Logger};
use config::Config;
use env_logger::Env;
use std::path::PathBuf;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize the logger with default level 'info'
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    
    // Load configuration
    let config_path = PathBuf::from("../config/config.yaml");
    let config = Config::load(config_path)
        .unwrap_or_else(|e| {
            log::warn!("Failed to load config file: {}, using defaults", e);
            Config::with_defaults()
        });

    log::info!("Starting Docker Web API server...");
    log::info!("Configuration: discovery_interval={}, log_level={}", 
               config.discovery_interval, config.logging_level);

    // Start HTTP server
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(config.clone()))
            .wrap(Logger::default())
            .service(
                web::scope("/api")
                    .configure(api::configure_routes)
            )
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
