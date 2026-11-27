mod e2e_support;

use e2e_support::E2eHarness;
use serde_json::Value;

#[tokio::test]
async fn backend_serves_http_against_an_isolated_docker_engine() {
    E2eHarness::run(|h| {
        Box::pin(async move {
            let (status, body) = h.get("/api/health").await;
            assert_eq!(status, reqwest::StatusCode::OK, "health response: {body}");
            assert_eq!(body["service"], "docker-web-api");
            assert_eq!(body["docker"], true);

            let (status, topology) = h.get("/api/topology").await;
            assert_eq!(
                status,
                reqwest::StatusCode::OK,
                "topology response: {topology}"
            );
            assert!(topology["containers"].is_array());
            assert!(topology["networks"].is_array());
            assert!(topology["edges"].is_array());

            let response_json: Value = topology;
            assert!(response_json["timestamp"].as_str().is_some());
        })
    })
    .await;
}
