use actix_web::{web, App, HttpServer, middleware::Logger};
use docker_web::{api, config::Config};
use log::info;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load environment variables from .env file (if it exists)
    dotenv::dotenv().ok();
    
    // Initialize logging
    env_logger::init();

    // Load configuration
    let config = Config::with_defaults();
    info!("Loaded configuration with Docker socket: {}, discovery interval: {}s, logging level: {}",
               config.docker_socket_path, config.discovery_interval, config.logging_level);

    // Start HTTP server
    let bind_address = config.bind_address.clone();
    info!("Starting server on {}", bind_address);
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(config.clone()))
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
