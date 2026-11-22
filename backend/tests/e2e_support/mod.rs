use std::{
    any::Any,
    collections::HashMap,
    future::Future,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::{Path, PathBuf},
    pin::Pin,
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use bollard::{
    Docker,
    container::{ListContainersOptions, LogsOptions, RemoveContainerOptions, StopContainerOptions},
    models::ContainerStateStatusEnum,
    network::ListNetworksOptions,
};
use futures_util::{FutureExt, TryStreamExt};
use reqwest::StatusCode;
use serde_json::Value;
use tempfile::TempDir;
use testcontainers::{
    ContainerAsync, GenericImage,
    core::{Mount, RunnableImage},
    runners::AsyncRunner,
};
use tokio::{process::Command as TokioCommand, time::Instant};

pub mod lifecycle;
pub mod resilience;
pub mod topology;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(120);
const CONVERGENCE_TIMEOUT: Duration = Duration::from_secs(30);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const RECOVERY_TIMEOUT: Duration = Duration::from_secs(60);
const RUN_LABEL: &str = "com.docker-web.e2e.run";
const SOCKET_PATH: &str = "/e2e/docker.sock";
const INNER_READINESS_TIMEOUT_SECONDS: u64 = 2;
// Streaming image pulls can pause between progress chunks; a short per-connection
// timeout makes the fixture pull flaky even though the daemon is healthy.
const INNER_DOCKER_TIMEOUT_SECONDS: u64 = 120;

/// Test-only fault injection point for proving partial-startup cleanup.
///
/// Normal tests always use [`StartupFault::None`]. These variants exist only so
/// cleanup tests can fail startup deterministically without environment variables
/// or global process state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupFault {
    None,
    /// Fail after the inner DinD daemon is reachable but before the backend image build.
    AfterDindReady,
    /// Fail after the sibling backend container starts but before HTTP readiness succeeds.
    AfterBackendStarted,
}

impl StartupFault {
    fn injected_error(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::AfterDindReady => Some("injected startup fault after inner DinD readiness"),
            Self::AfterBackendStarted => {
                Some("injected startup fault after backend start before HTTP readiness")
            }
        }
    }
}

pub struct E2eHarness {
    pub docker: Docker,
    pub http: reqwest::Client,
    pub base_url: String,
    pub run_id: String,
    socket_dir: TempDir,
    _config_dir: TempDir,
    dind: Option<ContainerAsync<GenericImage>>,
    backend: Option<ContainerAsync<GenericImage>>,
    outer_docker: Docker,
    backend_image_tag: String,
    last_http: std::sync::Mutex<Option<(String, StatusCode, String)>>,
}

impl E2eHarness {
    /// Runs a test body with panic-safe fixture cleanup and container log reporting.
    pub async fn run<F>(body: F)
    where
        F: for<'a> FnOnce(&'a E2eHarness) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>,
    {
        Self::run_with_startup_fault(StartupFault::None, body)
            .await
            .unwrap_or_else(|error| panic!("E2E harness startup failed: {error}"));
    }

    /// Test-only variant of [`Self::run`] used by cleanup tests to fail startup at a
    /// deterministic partial-startup point. Returns the startup error instead of
    /// panicking; body panics are still re-raised after cleanup.
    pub async fn run_with_startup_fault<F>(fault: StartupFault, body: F) -> Result<(), String>
    where
        F: for<'a> FnOnce(&'a E2eHarness) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>,
    {
        let mut harness = Self::start(fault).await?;
        let result = {
            let invocation = catch_unwind(AssertUnwindSafe(|| body(&harness)));
            match invocation {
                Ok(future) => AssertUnwindSafe(future).catch_unwind().await,
                Err(payload) => Err(payload),
            }
        };
        if result.is_err() {
            if let Err(payload) = AssertUnwindSafe(harness.print_diagnostics())
                .catch_unwind()
                .await
            {
                eprintln!(
                    "E2E diagnostics panicked while reporting a test failure: {}",
                    panic_message(payload.as_ref())
                );
            }
        }
        let cleanup = harness.cleanup_inner().await;
        drop(harness);
        if let Err(payload) = result {
            if let Err(error) = cleanup {
                eprintln!("E2E cleanup failed after the test body panicked: {error}");
            }
            resume_unwind(payload);
        }
        if let Err(error) = cleanup {
            panic!("E2E fixture cleanup failed: {error}");
        }
        Ok(())
    }

    async fn start(fault: StartupFault) -> Result<Self, String> {
        let run_id = unique_run_id();
        let root = workspace_root();
        let backend_image_tag = format!("docker-web-e2e:{run_id}");
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| format!("create HTTP client: {error}"))?;
        let socket_dir =
            tempfile::tempdir().map_err(|e| format!("create socket directory: {e}"))?;
        let config_dir =
            tempfile::tempdir().map_err(|e| format!("create config directory: {e}"))?;
        std::fs::copy(
            root.join("backend/tests/e2e_assets/config.yaml"),
            config_dir.path().join("config.yaml"),
        )
        .map_err(|e| format!("copy backend config: {e}"))?;

        let outer_docker = Docker::connect_with_socket_defaults()
            .map_err(|e| format!("connect to outer Docker: {e}"))?;
        outer_docker
            .ping()
            .await
            .map_err(|e| format!("outer Docker unavailable: {e}"))?;

        let dind_script = root.join("backend/tests/e2e_assets/dind-entrypoint.sh");
        let dind_image = GenericImage::new("docker", "26.1.4-dind")
            .with_entrypoint("/bin/sh")
            .with_mount(Mount::bind_mount(
                dind_script.to_string_lossy().into_owned(),
                "/e2e-entrypoint.sh",
            ));
        let dind = RunnableImage::from(dind_image)
            .with_container_name(format!("docker-web-e2e-dind-{run_id}"))
            .with_privileged(true)
            .with_mount(Mount::bind_mount(
                socket_dir.path().to_string_lossy().into_owned(),
                "/e2e",
            ))
            .with_args(vec!["/e2e-entrypoint.sh".to_owned()])
            .start()
            .await;

        let socket_path = socket_dir.path().join("docker.sock");
        let docker =
            match wait_for_inner_docker(&socket_path, STARTUP_TIMEOUT, &outer_docker, dind.id())
                .await
            {
                Ok(docker) => docker,
                Err(error) => {
                    eprintln!("E2E startup failed before backend launch: {error}");
                    print_logs(&outer_docker, dind.id(), "DinD startup failure").await;
                    if let Err(payload) = AssertUnwindSafe(dind.rm()).catch_unwind().await {
                        eprintln!(
                            "E2E startup cleanup: DinD removal panicked: {}",
                            panic_message(payload.as_ref())
                        );
                    }
                    return Err(error);
                }
            };

        let mut harness = Self {
            docker,
            http,
            base_url: String::new(),
            run_id,
            socket_dir,
            _config_dir: config_dir,
            dind: Some(dind),
            backend: None,
            outer_docker,
            backend_image_tag,
            last_http: std::sync::Mutex::new(None),
        };

        if fault == StartupFault::AfterDindReady {
            return Self::fail_startup(
                harness,
                StartupFault::AfterDindReady
                    .injected_error()
                    .expect("fault has a message")
                    .to_owned(),
            )
            .await;
        }
        if let Err(error) = build_backend_image(&root, &harness.backend_image_tag).await {
            return Self::fail_startup(harness, error).await;
        }
        let backend_image = GenericImage::new("docker-web-e2e", harness.run_id.as_str())
            .with_exposed_port(3000)
            .with_env_var("DOCKER_SOCKET_PATH", SOCKET_PATH)
            .with_env_var("BIND_ADDRESS", "0.0.0.0:3000")
            .with_env_var("RUST_LOG", "info");
        let backend = RunnableImage::from(backend_image)
            .with_mount(Mount::bind_mount(
                harness.socket_dir.path().to_string_lossy().into_owned(),
                "/e2e",
            ))
            .with_mount(Mount::bind_mount(
                harness._config_dir.path().to_string_lossy().into_owned(),
                "/app/config",
            ))
            .with_container_name(format!("docker-web-e2e-backend-{}", harness.run_id))
            .start()
            .await;

        let port = backend.get_host_port_ipv4(3000).await;
        let host = backend.get_host().await;
        harness.base_url = format!("http://{host}:{port}");
        harness.backend = Some(backend);

        if fault == StartupFault::AfterBackendStarted {
            return Self::fail_startup(
                harness,
                StartupFault::AfterBackendStarted
                    .injected_error()
                    .expect("fault has a message")
                    .to_owned(),
            )
            .await;
        }
        if let Err(error) = harness.wait_for_http_health(CONVERGENCE_TIMEOUT).await {
            return Self::fail_startup(harness, error).await;
        }
        Ok(harness)
    }

    /// Reuses the normal diagnostics and cleanup paths when startup fails.
    async fn fail_startup(mut harness: Self, error: String) -> Result<Self, String> {
        if let Err(payload) = AssertUnwindSafe(harness.print_diagnostics())
            .catch_unwind()
            .await
        {
            eprintln!(
                "E2E diagnostics panicked during startup failure: {}",
                panic_message(payload.as_ref())
            );
        }
        let cleanup = harness.cleanup_inner().await;
        drop(harness);
        Err(match cleanup {
            Ok(()) => error,
            Err(cleanup_error) => format!("{error}; startup cleanup failed: {cleanup_error}"),
        })
    }

    /// Performs a real HTTP GET and decodes the JSON body on every status code.
    pub async fn get(&self, path: &str) -> (StatusCode, Value) {
        let response = self
            .http
            .get(format!("{}{}", self.base_url, path))
            .send()
            .await
            .expect("backend HTTP GET failed");
        let status = response.status();
        let text = response
            .text()
            .await
            .expect("backend response body read failed");
        *self.last_http.lock().expect("last HTTP response lock") =
            Some((path.to_owned(), status, text.clone()));
        let body = serde_json::from_str(&text).unwrap_or_else(|error| {
            panic!("backend response was not JSON for {path} ({status}): {error}; body={text}")
        });
        (status, body)
    }

    /// Labels to apply to all inner fixture networks and containers.
    pub fn labels(&self) -> HashMap<String, String> {
        HashMap::from([(RUN_LABEL.to_owned(), self.run_id.clone())])
    }

    pub fn unique_name(&self, stem: &str) -> String {
        format!("e2e-{}-{stem}", self.run_id)
    }

    pub fn inner_socket_path(&self) -> PathBuf {
        self.socket_dir.path().join("docker.sock")
    }

    pub async fn stop_dind(&self) {
        let dind = self.dind.as_ref().expect("DinD fixture exists");
        if let Err(error) = self
            .outer_docker
            .stop_container(dind.id(), Some(StopContainerOptions { t: 30 }))
            .await
        {
            self.print_dind_failure("stop failed", dind.id()).await;
            panic!("stop the same DinD container for the outage: {error}");
        }
        match self.outer_docker.inspect_container(dind.id(), None).await {
            Ok(info) => {
                let state = info.state.unwrap_or_default();
                assert!(
                    state.running != Some(true)
                        && state.status != Some(ContainerStateStatusEnum::RUNNING),
                    "DinD container must be stopped for the outage: status={:?} running={:?}",
                    state.status,
                    state.running
                );
            }
            Err(error) => {
                self.print_dind_failure("inspect after stop failed", dind.id())
                    .await;
                panic!("inspect stopped DinD container: {error}");
            }
        }
    }

    pub async fn start_dind(&self) {
        let dind = self.dind.as_ref().expect("DinD fixture exists");
        if let Err(error) = self
            .outer_docker
            .start_container::<String>(dind.id(), None)
            .await
        {
            self.print_dind_failure("start request failed", dind.id())
                .await;
            panic!("start the same DinD container after the outage: {error}");
        }
        let deadline = Instant::now() + RECOVERY_TIMEOUT;
        loop {
            match self.outer_docker.inspect_container(dind.id(), None).await {
                Ok(info) => {
                    let state = info.state.unwrap_or_default();
                    if matches!(
                        state.status,
                        Some(ContainerStateStatusEnum::EXITED | ContainerStateStatusEnum::DEAD)
                    ) {
                        self.print_dind_failure("exited after restart", dind.id())
                            .await;
                        panic!(
                            "DinD container exited after restart: status={:?} exit_code={:?} error={:?}",
                            state.status, state.exit_code, state.error
                        );
                    }
                    if state.running == Some(true) && self.docker.ping().await.is_ok() {
                        return;
                    }
                }
                Err(error) => {
                    self.print_dind_failure("inspect after restart failed", dind.id())
                        .await;
                    panic!("inspect restarted DinD container: {error}");
                }
            }
            if Instant::now() >= deadline {
                self.print_dind_failure("did not recover inner Docker", dind.id())
                    .await;
                panic!(
                    "inner Docker daemon did not recover within {RECOVERY_TIMEOUT:?} after DinD restart"
                );
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    async fn print_dind_failure(&self, context: &str, id: &str) {
        eprintln!("E2E DinD {context} (container {id}):");
        match self.outer_docker.inspect_container(id, None).await {
            Ok(info) => {
                let state = info.state.unwrap_or_default();
                eprintln!(
                    "  state: status={:?} running={:?} restarting={:?} exit_code={:?} error={:?} oom_killed={:?} started_at={:?} finished_at={:?}",
                    state.status,
                    state.running,
                    state.restarting,
                    state.exit_code,
                    state.error,
                    state.oom_killed,
                    state.started_at,
                    state.finished_at
                );
            }
            Err(error) => eprintln!("  inspect failed: {error}"),
        }
        print_logs(&self.outer_docker, id, "DinD").await;
    }

    /// Outer Docker identity used to prove daemon recovery did not restart the backend.
    pub async fn backend_identity(&self) -> (String, String) {
        let backend = self.backend.as_ref().expect("backend fixture exists");
        let inspect = self
            .outer_docker
            .inspect_container(backend.id(), None)
            .await
            .expect("inspect backend fixture identity");
        let started = inspect
            .state
            .and_then(|state| state.started_at)
            .expect("backend container has a start time");
        (backend.id().to_owned(), started)
    }

    pub fn cleanup_proof(&self) -> (Vec<String>, String) {
        let ids = self
            .backend
            .iter()
            .chain(self.dind.iter())
            .map(|container| container.id().to_owned())
            .collect();
        (ids, self.backend_image_tag.clone())
    }

    async fn wait_for_http_health(&self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(response) = self
                .http
                .get(format!("{}/api/health", self.base_url))
                .send()
                .await
            {
                if response.status().is_success() {
                    if let Ok(body) = response.json::<Value>().await {
                        if body["status"] == "healthy" && body["docker"] == true {
                            return Ok(());
                        }
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "backend did not become HTTP-ready within {timeout:?}"
                ));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    async fn cleanup_inner(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        if self.docker.ping().await.is_err() {
            eprintln!(
                "E2E cleanup: inner Docker is unavailable; removing the containing DinD fixture also removes its private engine state"
            );
        } else {
            let filters = HashMap::from([(
                String::from("label"),
                vec![format!("{RUN_LABEL}={}", self.run_id)],
            )]);
            match self
                .docker
                .list_containers(Some(ListContainersOptions {
                    all: true,
                    filters: filters.clone(),
                    ..Default::default()
                }))
                .await
            {
                Ok(containers) => {
                    for container in containers {
                        if let Some(id) = container.id {
                            if let Err(error) = self
                                .docker
                                .remove_container(
                                    &id,
                                    Some(RemoveContainerOptions {
                                        force: true,
                                        ..Default::default()
                                    }),
                                )
                                .await
                            {
                                errors.push(format!("remove fixture container {id}: {error}"));
                            }
                        }
                    }
                }
                Err(error) => errors.push(format!("list fixture containers: {error}")),
            }
            match self
                .docker
                .list_networks(Some(ListNetworksOptions { filters }))
                .await
            {
                Ok(networks) => {
                    for network in networks {
                        if let Some(id) = network.id {
                            if let Err(error) = self.docker.remove_network(&id).await {
                                errors.push(format!("remove fixture network {id}: {error}"));
                            }
                        }
                    }
                }
                Err(error) => errors.push(format!("list fixture networks: {error}")),
            }
            match self
                .docker
                .list_containers(Some(ListContainersOptions {
                    all: true,
                    filters: HashMap::from([(
                        String::from("label"),
                        vec![format!("{RUN_LABEL}={}", self.run_id)],
                    )]),
                    ..Default::default()
                }))
                .await
            {
                Ok(remaining) if !remaining.is_empty() => errors.push(format!(
                    "{} labeled inner fixture container(s) remain",
                    remaining.len()
                )),
                Err(error) => {
                    errors.push(format!("verify inner fixture container cleanup: {error}"))
                }
                _ => {}
            }
            match self
                .docker
                .list_networks(Some(ListNetworksOptions {
                    filters: HashMap::from([(
                        String::from("label"),
                        vec![format!("{RUN_LABEL}={}", self.run_id)],
                    )]),
                }))
                .await
            {
                Ok(remaining) if !remaining.is_empty() => errors.push(format!(
                    "{} labeled inner fixture network(s) remain",
                    remaining.len()
                )),
                Err(error) => errors.push(format!("verify inner fixture network cleanup: {error}")),
                _ => {}
            }
        }
        let backend_id = self
            .backend
            .as_ref()
            .map(|container| container.id().to_owned());
        let dind_id = self
            .dind
            .as_ref()
            .map(|container| container.id().to_owned());
        if let Some(backend) = self.backend.take() {
            if let Err(payload) = AssertUnwindSafe(backend.rm()).catch_unwind().await {
                errors.push(format!(
                    "remove backend fixture container: panicked: {}",
                    panic_message(payload.as_ref())
                ));
            }
        }
        if let Some(dind) = self.dind.take() {
            if let Err(payload) = AssertUnwindSafe(dind.rm()).catch_unwind().await {
                errors.push(format!(
                    "remove DinD fixture container: panicked: {}",
                    panic_message(payload.as_ref())
                ));
            }
        }
        for (label, id) in [("backend", backend_id), ("DinD", dind_id)] {
            if let Some(id) = id {
                if self.outer_docker.inspect_container(&id, None).await.is_ok() {
                    errors.push(format!("{label} container {id} remains after rm"));
                }
            }
        }
        if let Err(error) = std::fs::remove_file(self.inner_socket_path()) {
            if error.kind() != std::io::ErrorKind::NotFound {
                errors.push(format!("remove shared socket: {error}"));
            }
        }
        let image_exists = match self
            .outer_docker
            .inspect_image(&self.backend_image_tag)
            .await
        {
            Ok(_) => true,
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => false,
            Err(error) => {
                errors.push(format!(
                    "inspect test image {} before removal: {error}",
                    self.backend_image_tag
                ));
                false
            }
        };
        if image_exists {
            match Command::new("docker")
                .args(["image", "rm", &self.backend_image_tag])
                .output()
            {
                Ok(output) if output.status.success() => {}
                Ok(output) => errors.push(format!(
                    "remove test image {}: {}",
                    self.backend_image_tag,
                    String::from_utf8_lossy(&output.stderr)
                )),
                Err(error) => errors.push(format!(
                    "remove test image {}: {error}",
                    self.backend_image_tag
                )),
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    async fn print_diagnostics(&self) {
        eprintln!("E2E fixture {} diagnostics:", self.run_id);
        eprintln!("  API: {}", self.base_url);
        eprintln!("  inner socket: {}", self.inner_socket_path().display());
        if let Some((path, status, body)) = self
            .last_http
            .lock()
            .expect("last HTTP response lock")
            .as_ref()
        {
            eprintln!("  last HTTP response: GET {path} -> {status}: {body}");
        }
        if let Ok(version) = self.outer_docker.version().await {
            eprintln!(
                "  outer Docker: version={:?} api={:?}",
                version.version, version.api_version
            );
        }
        if let Ok(version) = self.docker.version().await {
            eprintln!(
                "  inner Docker: version={:?} api={:?}",
                version.version, version.api_version
            );
        }
        if let Ok(image) = self
            .outer_docker
            .inspect_image(&self.backend_image_tag)
            .await
        {
            eprintln!(
                "  backend image: id={:?} digests={:?}",
                image.id, image.repo_digests
            );
        }
        for (label, container) in [
            ("DinD", self.dind.as_ref()),
            ("backend", self.backend.as_ref()),
        ] {
            if let Some(container) = container {
                match self
                    .outer_docker
                    .inspect_container(container.id(), None)
                    .await
                {
                    Ok(info) => eprintln!(
                        "  {label} state: id={} status={:?} started_at={:?} image={:?}",
                        container.id(),
                        info.state.as_ref().and_then(|s| s.status.clone()),
                        info.state.as_ref().and_then(|s| s.started_at.clone()),
                        info.image
                    ),
                    Err(error) => eprintln!("  {label} inspect failed: {error}"),
                }
            }
        }
        if let Some(dind) = &self.dind {
            print_logs(&self.outer_docker, dind.id(), "DinD").await;
        }
        if let Some(backend) = &self.backend {
            print_logs(&self.outer_docker, backend.id(), "backend").await;
        }
        let filter = HashMap::from([(
            String::from("label"),
            vec![format!("{RUN_LABEL}={}", self.run_id)],
        )]);
        if let Ok(containers) = self
            .docker
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters: filter,
                ..Default::default()
            }))
            .await
        {
            for container in containers {
                if let Some(id) = container.id {
                    match self.docker.inspect_container(&id, None).await {
                        Ok(info) => eprintln!(
                            "  inner fixture inspect: id={id} name={:?} status={:?} image={:?}",
                            info.name,
                            info.state.as_ref().and_then(|state| state.status),
                            info.config.as_ref().and_then(|config| config.image.clone())
                        ),
                        Err(error) => eprintln!("  inner fixture inspect failed for {id}: {error}"),
                    }
                    print_logs(&self.docker, &id, "inner fixture").await;
                }
            }
        }
    }
}

async fn wait_for_inner_docker(
    socket_path: &Path,
    timeout: Duration,
    outer_docker: &Docker,
    container_id: &str,
) -> Result<Docker, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if socket_path.exists() {
            let socket = socket_path.to_string_lossy();
            if let Ok(probe) = Docker::connect_with_socket(
                socket.as_ref(),
                INNER_READINESS_TIMEOUT_SECONDS,
                bollard::API_DEFAULT_VERSION,
            ) {
                if probe.ping().await.is_ok() {
                    return Docker::connect_with_socket(
                        socket.as_ref(),
                        INNER_DOCKER_TIMEOUT_SECONDS,
                        bollard::API_DEFAULT_VERSION,
                    )
                    .map_err(|error| {
                        format!("connect to ready inner Docker with long timeout: {error}")
                    });
                }
            }
        }
        match outer_docker.inspect_container(container_id, None).await {
            Ok(info) => {
                let state = info.state.unwrap_or_default();
                if matches!(
                    state.status,
                    Some(ContainerStateStatusEnum::EXITED | ContainerStateStatusEnum::DEAD)
                ) {
                    return Err(format!(
                        "DinD container exited during startup: status={:?} exit_code={:?} error={:?}",
                        state.status, state.exit_code, state.error
                    ));
                }
            }
            Err(error) => return Err(format!("inspect DinD container during startup: {error}")),
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "inner Docker socket did not become ready within {timeout:?}"
            ));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn build_backend_image(root: &Path, tag: &str) -> Result<(), String> {
    let context = tempfile::tempdir().map_err(|e| format!("create backend build context: {e}"))?;
    let backend_dir = context.path().join("backend");
    std::fs::create_dir(&backend_dir)
        .map_err(|e| format!("create context backend directory: {e}"))?;
    std::fs::copy(
        root.join("backend/Cargo.toml"),
        backend_dir.join("Cargo.toml"),
    )
    .map_err(|e| format!("copy backend manifest into build context: {e}"))?;
    std::fs::copy(
        root.join("backend/Cargo.lock"),
        backend_dir.join("Cargo.lock"),
    )
    .map_err(|e| format!("copy backend lockfile into build context: {e}"))?;
    copy_tree(&root.join("backend/src"), &backend_dir.join("src"))?;
    let dockerfile = root.join("backend/tests/e2e_assets/Dockerfile");
    let output = tokio::time::timeout(Duration::from_secs(300), async {
        TokioCommand::new("docker")
            .kill_on_drop(true)
            .args(["build", "--file"])
            .arg(dockerfile)
            .args(["--tag", tag])
            .arg(context.path())
            .output()
            .await
    })
    .await
    .map_err(|_| "backend test image build exceeded 300 seconds".to_owned())?
    .map_err(|e| format!("run outer Docker CLI build: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "backend test image build failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    std::fs::create_dir_all(destination)
        .map_err(|e| format!("create build context source directory: {e}"))?;
    for entry in
        std::fs::read_dir(source).map_err(|e| format!("read backend source directory: {e}"))?
    {
        let entry = entry.map_err(|e| format!("read backend source entry: {e}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else {
            std::fs::copy(&source_path, &destination_path)
                .map_err(|e| format!("copy backend source file: {e}"))?;
        }
    }
    Ok(())
}

async fn print_logs(docker: &Docker, id: &str, label: &str) {
    let options = LogsOptions::<String> {
        stdout: true,
        stderr: true,
        follow: false,
        ..Default::default()
    };
    match docker.logs(id, Some(options)).try_collect::<Vec<_>>().await {
        Ok(logs) => {
            eprintln!("--- {label} ({id}) logs ---");
            for log in logs {
                eprintln!("{log}");
            }
        }
        Err(error) => eprintln!("--- unable to read {label} logs ({id}): {error} ---"),
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend has workspace parent")
        .to_path_buf()
}

fn unique_run_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}-{nanos:x}", std::process::id())
}
