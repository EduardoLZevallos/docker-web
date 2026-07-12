use actix_web::{test, App, web};
use docker_web::{api, config::Config, docker::DockerClient};
use serde_json::Value;
use std::time::Duration;
use testcontainers::{core::WaitFor, runners::AsyncRunner, GenericImage};
use tokio::time::sleep;

fn create_test_docker_client() -> DockerClient {
    DockerClient::new_with_defaults()
        .expect("Failed to create DockerClient for testing - is Docker running?")
}

fn create_test_config() -> Config {
    Config::with_defaults()
}

#[tokio::test]
async fn get_health_returns_healthy_status() {
    let docker_client = create_test_docker_client();
    let config = create_test_config();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(docker_client))
            .app_data(web::Data::new(config))
            .service(
                web::scope("/api")
                    .route("/health", web::get().to(api::health))
            )
    ).await;

    let req = test::TestRequest::get()
        .uri("/api/health")
        .to_request();

    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());

    let body = test::read_body(resp).await;
    let json: Value = serde_json::from_slice(&body).expect("Failed to parse JSON response");
    assert_eq!(json["service"], "docker-web-api");
    assert_eq!(json["status"], "healthy");
    assert!(json["timestamp"].is_string());
}

#[tokio::test]
async fn get_config_returns_demo_mode_and_theme() {
    let config = create_test_config();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .service(
                web::scope("/api")
                    .route("/config", web::get().to(api::get_config))
            )
    ).await;

    let req = test::TestRequest::get()
        .uri("/api/config")
        .to_request();

    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());

    let body = test::read_body(resp).await;
    let json: Value = serde_json::from_slice(&body).expect("Failed to parse JSON response");
    assert_eq!(json["demo_mode"], false);
    assert_eq!(json["theme"], "light");
}

#[tokio::test]
async fn get_containers_with_test_container_returns_container_list() {
    let nginx_image = GenericImage::new("nginx", "latest")
        .with_wait_for(WaitFor::seconds(3));

    let _container = nginx_image.start().await;

    sleep(Duration::from_secs(3)).await;

    let docker_client = create_test_docker_client();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(docker_client))
            .service(
                web::scope("/api")
                    .route("/containers", web::get().to(api::get_containers))
            )
    ).await;

    let req = test::TestRequest::get()
        .uri("/api/containers")
        .to_request();

    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());

    let body = test::read_body(resp).await;
    let json: Value = serde_json::from_slice(&body).expect("Failed to parse JSON response");
    assert!(json.is_array());
    let containers = json.as_array().unwrap();
    assert!(!containers.is_empty(), "Expected at least one container");

    log::debug!("First container: {}", containers[0]);
}

#[tokio::test]
async fn get_networks_with_custom_network_returns_network_list() {
    use bollard::Docker;
    use bollard::network::CreateNetworkOptions;

    let docker = Docker::connect_with_socket_defaults().unwrap();

    let network_name = format!(
        "test_network_api_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );
    let create_options = CreateNetworkOptions {
        name: network_name.to_string(),
        driver: "bridge".to_string(),
        ..Default::default()
    };

    let _network_response = docker.create_network(create_options).await.unwrap();

    sleep(Duration::from_secs(1)).await;

    let docker_client = create_test_docker_client();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(docker_client))
            .service(
                web::scope("/api")
                    .configure(api::configure_routes)
            )
    ).await;

    let req = test::TestRequest::get()
        .uri("/api/networks")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body = test::read_body(resp).await;
    let networks: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let networks_array = networks.as_array().unwrap();

    let custom_network = networks_array
        .iter()
        .find(|n| n["name"].as_str().unwrap() == network_name)
        .expect("Should find custom network");

    assert_eq!(custom_network["name"], network_name);
    assert_eq!(custom_network["driver"], "bridge");
    assert_eq!(custom_network["scope"], "local");

    if let Err(e) = docker.remove_network(&network_name).await {
        log::warn!("Failed to cleanup test network: {}", e);
    }
}

#[tokio::test]
async fn get_topology_with_custom_network_and_container_returns_combined_data() {
    use testcontainers::{GenericImage, runners::AsyncRunner, core::WaitFor};
    use bollard::Docker;
    use bollard::network::CreateNetworkOptions;
    use std::time::{SystemTime, UNIX_EPOCH};

    let docker = Docker::connect_with_socket_defaults().unwrap();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let network_name = format!("test_topology_network_{}", timestamp);

    let create_network_options = CreateNetworkOptions {
        name: network_name.to_string(),
        driver: "bridge".to_string(),
        ..Default::default()
    };

    let _network_response = docker.create_network(create_network_options).await.unwrap();
    sleep(Duration::from_secs(1)).await;

    let nginx_image = GenericImage::new("nginx", "latest")
        .with_wait_for(WaitFor::seconds(3));

    let _container = nginx_image.start().await;
    sleep(Duration::from_secs(4)).await;

    let docker_client = create_test_docker_client();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(docker_client))
            .service(
                web::scope("/api")
                    .configure(api::configure_routes)
            )
    ).await;

    let req = test::TestRequest::get()
        .uri("/api/topology")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body = test::read_body(resp).await;
    let topology: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(topology.get("containers").is_some());
    assert!(topology.get("networks").is_some());
    assert!(topology.get("edges").is_some());
    assert!(topology.get("timestamp").is_some());

    let networks = topology["networks"].as_array().unwrap();
    let containers = topology["containers"].as_array().unwrap();
    let edges = topology["edges"].as_array().unwrap();

    let _custom_network = networks
        .iter()
        .find(|n| n["name"].as_str().unwrap() == network_name)
        .expect("Should find custom network in topology");

    let nginx_container = containers
        .iter()
        .find(|c| c["image"].as_str().unwrap_or("").contains("nginx"))
        .expect("Should find nginx container");

    assert!(nginx_container["name"].is_string());
    let container_status = nginx_container["status"].as_str().unwrap();
    assert!(
        container_status.to_lowercase().contains("up")
            || container_status.to_lowercase().contains("running"),
        "Container should be running, got: {}",
        container_status
    );

    let container_id = nginx_container["id"].as_str().unwrap();
    let has_edge = edges.iter().any(|e| e["source"].as_str().unwrap() == container_id);
    assert!(
        has_edge,
        "Expected at least one edge from container {} to a network",
        container_id
    );

    assert!(networks.len() >= 4, "Should have at least 4 networks");

    log::debug!(
        "Topology test - Found {} networks, {} containers, {} edges",
        networks.len(),
        containers.len(),
        edges.len()
    );

    if let Err(e) = docker.remove_network(&network_name).await {
        log::warn!("Failed to cleanup test network: {}", e);
    }
}
