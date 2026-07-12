use actix_cors::Cors;
use actix_web::{web, App, HttpServer, http, middleware::Logger};
use docker_web::{api, config::Config, docker::DockerClient};
use log::info;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    env_logger::init();

    info!("Starting Docker Web API server...");

    let config = Config::load("config/config.yaml").unwrap_or_else(|e| {
        log::error!("Failed to load configuration: {}", e);
        std::process::exit(1);
    });
    info!("Loaded configuration: bind address = {}", config.bind_address);

    let docker_client = DockerClient::new(&config)
        .map_err(|e| {
            log::error!("Failed to create Docker client during startup: {}", e);
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        })?;
    info!("Docker client initialized successfully");

    let bind_address = config.bind_address.clone();
    info!("Starting server on {}", bind_address);
    let server = HttpServer::new(move || {
        let cors = Cors::default()
            .allowed_origin("http://localhost:5173")
            .allowed_methods(vec!["GET", "POST"])
            .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT])
            .allowed_header(http::header::CONTENT_TYPE)
            .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(web::Data::new(docker_client.clone()))
            .app_data(web::Data::new(config.clone()))
            .wrap(Logger::default())
            .service(
                web::scope("/api")
                    .configure(api::configure_routes)
            )
    })
    .bind(&bind_address)?
    .run();

    let server_handle = server.handle();

    tokio::spawn(async move {
        let mut sigterm = tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate(),
        )
        .expect("failed to register SIGTERM handler");

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("SIGINT received, stopping server gracefully...");
            }
            _ = sigterm.recv() => {
                info!("SIGTERM received, stopping server gracefully...");
            }
        }

        server_handle.stop(true).await;
    });

    server.await
}
