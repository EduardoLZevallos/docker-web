use actix_web::{web, App, HttpServer, middleware::Logger};
use docker_web::{api, config::Config, docker::DockerClient};
use log::info;
use std::sync::Arc;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    env_logger::init();

    info!("Starting Docker Web API server...");

    let config = Arc::new(Config::load("config/config.yaml").unwrap_or_else(|e| {
        log::error!("Failed to load configuration: {}", e);
        std::process::exit(1);
    }));
    info!("Loaded configuration: bind address = {}", config.bind_address);

    let docker_client = DockerClient::new(&config)
        .map_err(|e| {
            log::error!("Failed to create Docker client during startup: {}", e);
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        })?;
    info!("Docker client initialized successfully");

    let bind_address = config.bind_address.clone();
    info!("Starting server on {}", bind_address);
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(docker_client.clone()))
            .app_data(web::Data::from(config.clone()))
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
