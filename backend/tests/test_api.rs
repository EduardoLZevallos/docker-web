use actix_web::{test, App, web};
use docker_web::{api, config::Config};
use serde_json::Value;
use std::time::Duration;
use testcontainers::{core::WaitFor, runners::AsyncRunner, GenericImage};
use tokio::time::sleep;

/// Integration tests for API endpoints using testcontainers
/// Following BDD pattern: Given-When-Then

#[tokio::test]
async fn health_endpoint_returns_healthy_status() {
    // GIVEN a running API server
    let config = Config::with_defaults();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .service(
                web::scope("/api")
                    .route("/health", web::get().to(api::health))
            )
    ).await;
    
    // WHEN we call the health endpoint
    let req = test::TestRequest::get()
        .uri("/api/health")
        .to_request();
    
    let resp = test::call_service(&app, req).await;
    
    // THEN it should return 200 OK
    assert!(resp.status().is_success());
    
    // AND return valid JSON with expected fields
    let body = test::read_body(resp).await;
    let json: Value = serde_json::from_slice(&body).expect("Failed to parse JSON response");
    assert_eq!(json["service"], "docker-web-api");
    assert_eq!(json["status"], "healthy");
    assert!(json["timestamp"].is_string());
}

#[tokio::test]
async fn containers_endpoint_returns_valid_structure() {
    // GIVEN a running API server
    let config = Config::with_defaults();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .service(
                web::scope("/api")
                    .route("/containers", web::get().to(api::get_containers))
            )
    ).await;
    
    // WHEN we call the containers endpoint
    let req = test::TestRequest::get()
        .uri("/api/containers")
        .to_request();
    
    let resp = test::call_service(&app, req).await;
    
    // THEN it should return 200 OK with valid JSON structure
    assert!(resp.status().is_success());
    
    let body = test::read_body(resp).await;
    let json: Value = serde_json::from_slice(&body).expect("Failed to parse JSON response");
    assert!(json.is_array(), "Response should be a JSON array");
    
    // If containers exist, validate their structure
    if let Some(containers) = json.as_array() {
        if let Some(first_container) = containers.first() {
            assert!(first_container["id"].is_string(), "Container should have an id");
            assert!(first_container["name"].is_string(), "Container should have a name");
            assert!(first_container["image"].is_string(), "Container should have an image");
            assert!(first_container["status"].is_string(), "Container should have a status");
        }
    }
}

#[tokio::test]
async fn containers_endpoint_with_test_container() {
    // GIVEN a test container is running
    let nginx_image = GenericImage::new("nginx", "latest")
        .with_wait_for(WaitFor::seconds(3));
    
    let _container = nginx_image.start().await;
    
    // Wait a moment for container to be fully started
    sleep(Duration::from_secs(3)).await;
    
    // AND a running API server  
    let config = Config::with_defaults();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .service(
                web::scope("/api")
                    .route("/containers", web::get().to(api::get_containers))
            )
    ).await;
    
    // WHEN we call the containers endpoint
    let req = test::TestRequest::get()
        .uri("/api/containers")
        .to_request();
    
    let resp = test::call_service(&app, req).await;
    
    // THEN it should return 200 OK
    assert!(resp.status().is_success());
    
    // AND return a JSON array with at least one container
    let body = test::read_body(resp).await;
    let json: Value = serde_json::from_slice(&body).expect("Failed to parse JSON response");
    assert!(json.is_array());
    let containers = json.as_array().unwrap();
    assert!(containers.len() > 0, "Expected at least one container");
    
    // Debug: Log the first container to see the actual structure
        log::debug!("First container: {}", containers[0]);
}

#[tokio::test]
async fn networks_endpoint_returns_network_list() {
    // GIVEN the API server is running
    let config = Config::with_defaults();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .service(
                web::scope("/api")
                    .configure(api::configure_routes)
            )
    ).await;

    // WHEN we request the networks endpoint
    let req = test::TestRequest::get()
        .uri("/api/networks")
        .to_request();

    let resp = test::call_service(&app, req).await;

    // THEN we should get a successful response
    assert!(resp.status().is_success(), "Expected successful response, got: {}", resp.status());

    // AND the response should contain a list of networks
    let body = test::read_body(resp).await;
    let networks: serde_json::Value = serde_json::from_slice(&body)
        .expect("Response should be valid JSON");

    assert!(networks.is_array(), "Response should be an array of networks");
    
    // Should have at least the default networks (bridge, host, none)
    let network_array = networks.as_array().unwrap();
    assert!(network_array.len() >= 1, "Should have at least one network");

    // Validate network structure
    if let Some(first_network) = network_array.first() {
        assert!(first_network["id"].is_string(), "Network should have an id");
        assert!(first_network["name"].is_string(), "Network should have a name");
        assert!(first_network["driver"].is_string(), "Network should have a driver");
        assert!(first_network["scope"].is_string(), "Network should have a scope");
        assert!(first_network["containers"].is_array(), "Network should have a containers array");
    }
}

#[tokio::test]
async fn topology_endpoint_returns_combined_data() {
    // GIVEN the API server is running
    let config = Config::with_defaults();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .service(
                web::scope("/api")
                    .configure(api::configure_routes)
            )
    ).await;

    // WHEN we request the topology endpoint
    let req = test::TestRequest::get()
        .uri("/api/topology")
        .to_request();

    let resp = test::call_service(&app, req).await;

    // THEN we should get a successful response
    assert!(resp.status().is_success(), "Expected successful response, got: {}", resp.status());

    // AND the response should contain both containers and networks
    let body = test::read_body(resp).await;
    let topology: serde_json::Value = serde_json::from_slice(&body)
        .expect("Response should be valid JSON");

    assert!(topology["containers"].is_array(), "Topology should contain containers array");
    assert!(topology["networks"].is_array(), "Topology should contain networks array");
    assert!(topology["timestamp"].is_string(), "Topology should contain timestamp");

    // Validate structure
    let networks = topology["networks"].as_array().unwrap();
    assert!(networks.len() >= 1, "Should have at least one network");

    log::debug!("Topology response: containers={}, networks={}", 
              topology["containers"].as_array().unwrap().len(),
              networks.len());
}

#[tokio::test]
async fn container_by_id_endpoint_returns_specific_container() {
    use testcontainers::{GenericImage, runners::AsyncRunner};
    
    // GIVEN a test container is running
    let nginx_image = GenericImage::new("nginx", "latest")
        .with_wait_for(WaitFor::seconds(3));
    
    let _container = nginx_image.start().await;
    sleep(Duration::from_secs(3)).await;
    
    // AND we know the container ID
    let config = Config::with_defaults();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config.clone()))
            .service(
                web::scope("/api")
                    .configure(api::configure_routes)
            )
    ).await;

    // Get all containers first to find the nginx container ID
    let list_req = test::TestRequest::get()
        .uri("/api/containers")
        .to_request();

    let list_resp = test::call_service(&app, list_req).await;
    assert!(list_resp.status().is_success());

    let list_body = test::read_body(list_resp).await;
    let containers: serde_json::Value = serde_json::from_slice(&list_body).unwrap();
    let containers_array = containers.as_array().unwrap();
    
    let nginx_container = containers_array.iter()
        .find(|c| c["image"].as_str().unwrap_or("").contains("nginx"))
        .expect("Should find nginx container");
    
    let container_id = nginx_container["id"].as_str().unwrap();

    // WHEN we request the specific container by ID
    let req = test::TestRequest::get()
        .uri(&format!("/api/containers/{}", container_id))
        .to_request();

    let resp = test::call_service(&app, req).await;

    // THEN we should get the specific container
    assert!(resp.status().is_success());

    let body = test::read_body(resp).await;
    let container: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(container["id"], container_id);
    assert!(container["name"].is_string());
    assert!(container["image"].as_str().unwrap().contains("nginx"));
}

#[tokio::test]
async fn container_by_id_endpoint_returns_404_for_nonexistent() {
    // GIVEN the API server is running
    let config = Config::with_defaults();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .service(
                web::scope("/api")
                    .configure(api::configure_routes)
            )
    ).await;

    // WHEN we request a non-existent container ID
    let req = test::TestRequest::get()
        .uri("/api/containers/nonexistent-container-id-12345")
        .to_request();

    let resp = test::call_service(&app, req).await;

    // THEN we should get a 404 response
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);

    let body = test::read_body(resp).await;
    let error: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(error["error"], "Container not found");
    assert!(error["message"].as_str().unwrap().contains("nonexistent-container-id-12345"));
}

#[tokio::test]
async fn network_by_id_endpoint_returns_specific_network() {
    // GIVEN the API server is running
    let config = Config::with_defaults();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .service(
                web::scope("/api")
                    .configure(api::configure_routes)
            )
    ).await;

    // Get all networks first to find a valid network ID
    let list_req = test::TestRequest::get()
        .uri("/api/networks")
        .to_request();

    let list_resp = test::call_service(&app, list_req).await;
    assert!(list_resp.status().is_success());

    let list_body = test::read_body(list_resp).await;
    let networks: serde_json::Value = serde_json::from_slice(&list_body).unwrap();
    let networks_array = networks.as_array().unwrap();
    assert!(!networks_array.is_empty(), "Should have at least one network");
    
    let first_network = &networks_array[0];
    let network_id = first_network["id"].as_str().unwrap();

    // WHEN we request the specific network by ID
    let req = test::TestRequest::get()
        .uri(&format!("/api/networks/{}", network_id))
        .to_request();

    let resp = test::call_service(&app, req).await;

    // THEN we should get the specific network
    assert!(resp.status().is_success());

    let body = test::read_body(resp).await;
    let network: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(network["id"], network_id);
    assert!(network["name"].is_string());
    assert!(network["driver"].is_string());
}

#[tokio::test]
async fn network_by_id_endpoint_returns_404_for_nonexistent() {
    // GIVEN the API server is running
    let config = Config::with_defaults();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .service(
                web::scope("/api")
                    .configure(api::configure_routes)
            )
    ).await;

    // WHEN we request a non-existent network ID
    let req = test::TestRequest::get()
        .uri("/api/networks/nonexistent-network-id-12345")
        .to_request();

    let resp = test::call_service(&app, req).await;

    // THEN we should get a 404 response
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);

    let body = test::read_body(resp).await;
    let error: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(error["error"], "Network not found");
    assert!(error["message"].as_str().unwrap().contains("nonexistent-network-id-12345"));
}

#[tokio::test]
async fn networks_endpoint_with_custom_network() {
    use bollard::Docker;
    use bollard::network::CreateNetworkOptions;
    
    // GIVEN we create a custom Docker network
    let docker = Docker::connect_with_socket_defaults().unwrap();
    
    let network_name = "test_network_api_endpoint";
    let create_options = CreateNetworkOptions {
        name: network_name.to_string(),
        driver: "bridge".to_string(),
        ..Default::default()
    };
    
    let network_response = docker.create_network(create_options).await.unwrap();
    let custom_network_id = network_response.id.as_ref().unwrap();
    
    // Give the network a moment to be created
    sleep(Duration::from_secs(1)).await;
    
    // AND the API server is running
    let config = Config::with_defaults();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .service(
                web::scope("/api")
                    .configure(api::configure_routes)
            )
    ).await;

    // WHEN we request the networks list
    let req = test::TestRequest::get()
        .uri("/api/networks")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body = test::read_body(resp).await;
    let networks: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let networks_array = networks.as_array().unwrap();
    
    // THEN we should find our custom network in the list
    let custom_network = networks_array.iter()
        .find(|n| n["name"].as_str().unwrap() == network_name)
        .expect("Should find custom network");
    
    assert_eq!(custom_network["name"], network_name);
    assert_eq!(custom_network["driver"], "bridge");
    assert_eq!(custom_network["scope"], "local");
    
    // AND we should be able to get the specific network by ID
    let network_by_id_req = test::TestRequest::get()
        .uri(&format!("/api/networks/{}", custom_network_id))
        .to_request();

    let network_by_id_resp = test::call_service(&app, network_by_id_req).await;
    assert!(network_by_id_resp.status().is_success());

    let network_by_id_body = test::read_body(network_by_id_resp).await;
    let specific_network: serde_json::Value = serde_json::from_slice(&network_by_id_body).unwrap();
    
    assert_eq!(specific_network["id"].as_str().unwrap(), custom_network_id);
    assert_eq!(specific_network["name"], network_name);
    assert_eq!(specific_network["driver"], "bridge");
    
    // AND we should be able to get it by name as well
    let network_by_name_req = test::TestRequest::get()
        .uri(&format!("/api/networks/{}", network_name))
        .to_request();

    let network_by_name_resp = test::call_service(&app, network_by_name_req).await;
    assert!(network_by_name_resp.status().is_success());

    let network_by_name_body = test::read_body(network_by_name_resp).await;
    let specific_network_by_name: serde_json::Value = serde_json::from_slice(&network_by_name_body).unwrap();
    
    assert_eq!(specific_network_by_name["name"], network_name);
    assert_eq!(specific_network_by_name["driver"], "bridge");
    
    // Cleanup: remove the custom network
    let _ = docker.remove_network(network_name).await;
}

#[tokio::test]
async fn topology_endpoint_with_custom_network_and_container() {
    use testcontainers::{GenericImage, runners::AsyncRunner, core::WaitFor};
    use bollard::Docker;
    use bollard::network::CreateNetworkOptions;
    use std::time::{SystemTime, UNIX_EPOCH};
    
    // GIVEN we create a custom Docker network with unique name
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
    
    // AND we start a testcontainer
    let nginx_image = GenericImage::new("nginx", "latest")
        .with_wait_for(WaitFor::seconds(3));
    
    let _container = nginx_image.start().await;
    sleep(Duration::from_secs(4)).await;
    
    // AND the API server is running
    let config = Config::with_defaults();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .service(
                web::scope("/api")
                    .configure(api::configure_routes)
            )
    ).await;

    // WHEN we request the topology data
    let req = test::TestRequest::get()
        .uri("/api/topology")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body = test::read_body(resp).await;
    let topology: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    // THEN the topology should contain both containers and networks
    assert!(topology.get("containers").is_some());
    assert!(topology.get("networks").is_some());
    assert!(topology.get("timestamp").is_some());
    
    let networks = topology["networks"].as_array().unwrap();
    let containers = topology["containers"].as_array().unwrap();
    
    // AND we should find our custom network
    let custom_network = networks.iter()
        .find(|n| n["name"].as_str().unwrap() == network_name)
        .expect("Should find custom network in topology");
    
    assert_eq!(custom_network["name"], network_name);
    assert_eq!(custom_network["driver"], "bridge");
    
    // AND we should find our container
    let nginx_container = containers.iter()
        .find(|c| c["image"].as_str().unwrap_or("").contains("nginx"))
        .expect("Should find nginx container");
    
    assert!(nginx_container["name"].is_string());
    let container_status = nginx_container["status"].as_str().unwrap();
    log::debug!("Container status: {}", container_status);
    assert!(container_status.to_lowercase().contains("up") || 
            container_status.to_lowercase().contains("running"),
            "Container should be running, got: {}", container_status);
    
    // AND we should have multiple networks (including our custom one and default docker networks)
    assert!(networks.len() >= 4, "Should have at least 4 networks (bridge, host, none, and our custom network)");
    
    log::debug!("Topology test - Found {} networks and {} containers", 
              networks.len(), containers.len());
    log::debug!("Custom network: {} found in topology", network_name);
    
    // Cleanup: remove the custom network (container will be cleaned up by testcontainers)
    let _ = docker.remove_network(&network_name).await;
}