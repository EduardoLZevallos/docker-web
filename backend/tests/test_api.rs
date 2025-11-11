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
async fn containers_endpoint_returns_container_list() {
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
    
    // THEN it should return 200 OK
    assert!(resp.status().is_success());
    
    // AND return a JSON array (might be empty or contain existing containers)
    let body = test::read_body(resp).await;
    let json: Value = serde_json::from_slice(&body).expect("Failed to parse JSON response");
    assert!(json.is_array(), "Response should be a JSON array");
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
    
    // AND the container should have expected fields
    let container = &containers[0];
    assert!(container["id"].is_string(), "id field should be string, got: {}", container["id"]);
    assert!(container["name"].is_string(), "name field should be string, got: {}", container["name"]);
    assert!(container["image"].is_string(), "image field should be string, got: {}", container["image"]);
    assert!(container["status"].is_string(), "status field should be string, got: {}", container["status"]);
    
    // AND we should find the nginx container
    let nginx_found = containers.iter().any(|c| {
        c["image"].as_str().map(|img| img.contains("nginx")).unwrap_or(false)
    });
    assert!(nginx_found, "Expected to find nginx container");
}