use std::{
    collections::BTreeSet,
    future::Future,
    panic::AssertUnwindSafe,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

use bollard::{
    Docker, container::ListContainersOptions, image::ListImagesOptions, volume::ListVolumesOptions,
};
use futures_util::FutureExt;

use super::super::{BACKEND_STARTUP_FAILURE_CATEGORY, CleanupProof, E2eHarness, StartupFault};

const OWNED_PREFIX: &str = "docker-web-e2e";
const SUITE_IMAGE_PREFIX: &str = "docker-web-e2e:suite-";

#[derive(Debug, Default, PartialEq, Eq)]
struct OuterResources {
    containers: BTreeSet<String>,
    images: BTreeSet<String>,
    volumes: BTreeSet<String>,
}

async fn outer_resources(outer: &Docker) -> OuterResources {
    let containers = outer
        .list_containers(Some(ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        }))
        .await
        .expect("list outer containers")
        .into_iter()
        .flat_map(|summary| summary.names.unwrap_or_default())
        .map(|name| name.trim_start_matches('/').to_owned())
        .filter(|name| name.starts_with(OWNED_PREFIX))
        .collect();
    let images = outer
        .list_images(Some(ListImagesOptions::<String>::default()))
        .await
        .expect("list outer images")
        .into_iter()
        .flat_map(|image| image.repo_tags)
        .filter(|tag| tag.starts_with(OWNED_PREFIX) && !tag.starts_with(SUITE_IMAGE_PREFIX))
        .collect();
    let volumes = outer
        .list_volumes(Some(ListVolumesOptions::<String>::default()))
        .await
        .expect("list outer volumes")
        .volumes
        .unwrap_or_default()
        .into_iter()
        .map(|volume| volume.name)
        .filter(|name| name.starts_with(OWNED_PREFIX))
        .collect();
    OuterResources {
        containers,
        images,
        volumes,
    }
}

async fn assert_probe_left_no_owned_resources(before: &OuterResources, outer: &Docker) {
    let after = outer_resources(outer).await;
    assert_eq!(
        after.containers, before.containers,
        "owned outer test containers must be removed by the probe: before={:?} after={:?}",
        before.containers, after.containers
    );
    assert_eq!(
        after.images, before.images,
        "owned outer test images must be removed by the probe: before={:?} after={:?}",
        before.images, after.images
    );
    assert_eq!(
        after.volumes, before.volumes,
        "owned DinD data volumes must be removed by the probe: before={:?} after={:?}",
        before.volumes, after.volumes
    );
}

fn panic_instead_of_returning_future(
    _h: &E2eHarness,
) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    panic!("intentional closure invocation panic")
}

fn assert_removed(proof: CleanupProof) {
    let _ = proof.image_tag;
    for id in proof.containers {
        let output = std::process::Command::new("docker")
            .args(["inspect", "--type", "container", &id])
            .output()
            .expect("run outer Docker inspect");
        assert!(
            !output.status.success(),
            "owned outer test container {id} remains after harness cleanup"
        );
    }
    let output = std::process::Command::new("docker")
        .args(["volume", "inspect", &proof.volume_name])
        .output()
        .expect("run outer Docker volume inspect");
    assert!(
        !output.status.success(),
        "owned DinD data volume {} remains after harness cleanup",
        proof.volume_name
    );
}

// Given a normal return and a panicking test future, when the harness runs, then owned
// outer containers and the labeled DinD data volume are removed.
#[tokio::test]
async fn given_normal_return_or_body_panic_when_harness_runs_then_owned_resources_are_removed() {
    let outer = Docker::connect_with_socket_defaults().unwrap();
    let before_normal = outer_resources(&outer).await;
    let normal = Arc::new(Mutex::new(None));
    let normal_capture = normal.clone();
    E2eHarness::run(|h| {
        Box::pin(async move {
            *normal_capture.lock().unwrap() = Some(h.cleanup_proof());
        })
    })
    .await;
    assert_removed(
        normal
            .lock()
            .unwrap()
            .take()
            .expect("normal cleanup proof recorded"),
    );
    assert_probe_left_no_owned_resources(&before_normal, &outer).await;

    let before_panic = outer_resources(&outer).await;
    let panic_proof = Arc::new(Mutex::new(None));
    let panic_capture = panic_proof.clone();
    let expected_panic = AssertUnwindSafe(E2eHarness::run(|h| {
        Box::pin(async move {
            *panic_capture.lock().unwrap() = Some(h.cleanup_proof());
            panic!("intentional harness cleanup proof panic");
        })
    }))
    .catch_unwind()
    .await;
    assert!(
        expected_panic.is_err(),
        "fixture body panic must remain visible after cleanup"
    );
    assert_removed(
        panic_proof
            .lock()
            .unwrap()
            .take()
            .expect("panic cleanup proof recorded"),
    );
    assert_probe_left_no_owned_resources(&before_panic, &outer).await;
}

// Given a test closure that panics before returning its future, when the harness runs, then
// the original panic is re-raised and owned outer resources are still removed.
#[tokio::test]
async fn given_closure_invocation_panic_when_harness_runs_then_owned_resources_are_removed() {
    let outer = Docker::connect_with_socket_defaults().unwrap();
    let before = outer_resources(&outer).await;
    let result = AssertUnwindSafe(E2eHarness::run(panic_instead_of_returning_future))
        .catch_unwind()
        .await;
    assert!(
        result.is_err(),
        "closure invocation panic must be re-raised after cleanup"
    );
    assert_probe_left_no_owned_resources(&before, &outer).await;
}

async fn assert_startup_fault_cleanup(fault: StartupFault, expected_error: &str) {
    let outer = Docker::connect_with_socket_defaults().unwrap();
    let before = outer_resources(&outer).await;
    let result = tokio::time::timeout(
        Duration::from_secs(300),
        E2eHarness::run_with_startup_fault(fault, |_h| Box::pin(async {})),
    )
    .await
    .expect("startup fault probe timed out");
    let error = result.expect_err("injected startup fault must return an error");
    assert!(
        error.contains(expected_error),
        "startup fault must surface the expected error, got: {error}"
    );
    assert!(
        !error.contains("startup cleanup failed"),
        "startup cleanup must succeed, got: {error}"
    );
    assert_probe_left_no_owned_resources(&before, &outer).await;
}

// Given DinD becomes ready, when startup fails before the backend image build, then the
// injected error is returned and owned outer resources are removed.
#[tokio::test]
async fn given_startup_after_dind_ready_when_harness_fails_then_owned_resources_are_removed() {
    assert_startup_fault_cleanup(
        StartupFault::AfterDindReady,
        "injected startup fault after inner DinD readiness",
    )
    .await;
}

// Given the backend container has started, when startup fails before HTTP readiness, then the
// injected error is returned and owned outer containers and volume are removed.
#[tokio::test]
async fn given_startup_after_backend_start_when_harness_fails_then_owned_resources_are_removed() {
    assert_startup_fault_cleanup(
        StartupFault::AfterBackendStarted,
        "injected startup fault after backend start before HTTP readiness",
    )
    .await;
}

// Given a broken build context, when the backend image build fails, then the build error is
// returned and no owned outer resources remain.
#[tokio::test]
async fn given_image_build_failure_when_startup_runs_then_owned_resources_are_removed() {
    assert_startup_fault_cleanup(
        StartupFault::ImageBuildFailure,
        "backend test image build failed",
    )
    .await;
}

// Given an invalid backend config, when the real backend exits during startup, then startup
// returns the stable backend-startup failure category and removes the backend, DinD, and
// labeled data volume.
#[tokio::test]
async fn given_invalid_backend_config_when_startup_runs_then_owned_resources_are_removed() {
    assert_startup_fault_cleanup(
        StartupFault::InvalidBackendConfig,
        BACKEND_STARTUP_FAILURE_CATEGORY,
    )
    .await;
}

// Given a missing backend image, when Testcontainers panics during backend creation, then the
// startup resource guard removes every owned container and volume before reporting the panic.
#[tokio::test]
async fn given_missing_backend_image_when_startup_panics_then_owned_resources_are_removed() {
    assert_startup_fault_cleanup(StartupFault::MissingBackendImage, "E2E startup panicked").await;
}
