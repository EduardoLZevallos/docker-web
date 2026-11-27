//! Playwright orchestration for the real frontend/backend/DinD acceptance path.
//!
//! The browser suite is launched as a separate Node process so Playwright owns
//! its Vite preview lifecycle. The Rust harness remains the owner of the outer
//! Docker resources and exposes only their exact container IDs to the browser
//! process.

use std::{
    env,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use tokio::{
    process::{Child, Command},
    time::{Instant, timeout},
};

use super::{E2eHarness, HarnessOverrides, fixtures};

const DEFAULT_PLAYWRIGHT_TIMEOUT_SECONDS: u64 = 1800;
const GROUP_SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
const PREVIEW_SHUTDOWN_GRACE: Duration = Duration::from_secs(5);
const RUN_LABEL: &str = "com.docker-web.e2e.run";
const NODE_PATH_HINTS: [&str; 2] = [
    ".local/bin",
    ".local/share/node/node-v26.13.1-linux-x64/bin",
];

/// Runs the real built UI against the harness backend and private DinD daemon.
///
/// The frontend test process receives the backend proxy origin, the shared
/// inner Docker socket, the run label, and the exact owned outer container IDs.
pub async fn run_playwright_frontend_e2e() {
    let frontend = frontend_dir();
    let npm = preflight(&frontend);
    let theme = browser_theme();
    let overrides = HarnessOverrides::new()
        .config("discovery_interval", "5")
        .config("frontend_theme", theme.clone())
        .config("demo_mode", "false");

    E2eHarness::run_with_overrides(overrides, move |h| {
        Box::pin(async move {
            fixtures::ensure_image(h).await;
            run_playwright(h, &frontend, &npm, &theme).await;
        })
    })
    .await;
}

fn frontend_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend has workspace parent")
        .join("frontend/docker-web-ui")
}

fn preview_pid_file(h: &E2eHarness) -> PathBuf {
    h.inner_socket_path()
        .with_file_name(format!("vite-preview-{}.pid", h.run_id))
}

fn browser_theme() -> String {
    let theme = env::var("E2E_FRONTEND_THEME")
        .unwrap_or_else(|_| "light".to_owned())
        .trim()
        .to_ascii_lowercase();
    match theme.as_str() {
        "light" | "dark" => theme,
        _ => panic!("E2E_FRONTEND_THEME must be 'light' or 'dark'; received {theme:?}"),
    }
}

fn playwright_args() -> Vec<String> {
    let Ok(raw) = env::var("E2E_PLAYWRIGHT_ARGS") else {
        return Vec::new();
    };
    if raw.contains('\u{1f}') {
        raw.split('\u{1f}')
            .filter(|arg| !arg.is_empty())
            .map(str::to_owned)
            .collect()
    } else {
        raw.split_whitespace().map(str::to_owned).collect()
    }
}

fn playwright_timeout() -> Duration {
    let seconds = env::var("E2E_PLAYWRIGHT_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_PLAYWRIGHT_TIMEOUT_SECONDS);
    Duration::from_secs(seconds)
}

fn tool_path() -> OsString {
    let mut directories = Vec::new();
    if let Some(home) = env::var_os("HOME") {
        for hint in NODE_PATH_HINTS {
            directories.push(PathBuf::from(&home).join(hint));
        }
    }
    if let Some(path) = env::var_os("PATH") {
        directories.extend(env::split_paths(&path));
    }
    env::join_paths(directories).unwrap_or_else(|_| env::var_os("PATH").unwrap_or_default())
}

fn find_on_path(name: &str, path: &OsStr) -> Option<PathBuf> {
    env::split_paths(path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

fn chromium_cache_available(frontend: &Path) -> bool {
    let browsers = match env::var_os("PLAYWRIGHT_BROWSERS_PATH") {
        Some(value) if value == "0" => {
            frontend.join("node_modules/playwright-core/.local-browsers")
        }
        Some(value) => PathBuf::from(value),
        None => match env::var_os("HOME") {
            Some(home) => PathBuf::from(home).join(".cache/ms-playwright"),
            None => return false,
        },
    };
    std::fs::read_dir(&browsers).ok().is_some_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_dir())
                && entry.file_name().to_string_lossy().starts_with("chromium-")
        })
    })
}

fn preflight(frontend: &Path) -> PathBuf {
    let path = tool_path();
    let npm = find_on_path("npm", &path).unwrap_or_else(|| {
        panic!(
            "Node/npm are required for browser E2E; add Node 26.13.1 to PATH \
             (~/.local/bin or ~/.local/share/node/node-v26.13.1-linux-x64/bin)"
        )
    });
    if find_on_path("node", &path).is_none() {
        panic!(
            "Node is required for browser E2E; add Node 26.13.1 to PATH \
             (~/.local/bin or ~/.local/share/node/node-v26.13.1-linux-x64/bin)"
        );
    }
    if !frontend
        .join("node_modules/@playwright/test/package.json")
        .is_file()
        || !frontend.join("node_modules/.bin/playwright").is_file()
    {
        panic!(
            "Playwright is not installed in {}; run `npm ci` there first. \
             The test never installs dependencies or browsers implicitly.",
            frontend.display()
        );
    }
    if !chromium_cache_available(frontend) {
        panic!(
            "No cached Playwright Chromium browser was found; run \
             `npx playwright install chromium` in {} first. The test never \
             downloads browsers implicitly.",
            frontend.display()
        );
    }
    npm
}

async fn run_playwright(h: &E2eHarness, frontend: &Path, npm: &Path, theme: &str) {
    let preview_pid_file = preview_pid_file(h);
    let _ = std::fs::remove_file(&preview_pid_file);

    let mut command = Command::new(npm);
    command
        .current_dir(frontend)
        .args(["run", "test:e2e", "--"])
        .args(playwright_args())
        .env("PATH", tool_path())
        .env("BACKEND_URL", &h.base_url)
        .env("E2E_BACKEND_URL", &h.base_url)
        .env("E2E_DOCKER_SOCKET", h.inner_socket_path())
        .env("E2E_RUN_ID", &h.run_id)
        .env("E2E_DIND_ID", h.dind_container_id())
        .env("E2E_BACKEND_ID", h.backend_container_id())
        .env("E2E_RUN_LABEL", RUN_LABEL)
        .env("E2E_FRONTEND_THEME", theme)
        .env("E2E_PREVIEW_PID_FILE", &preview_pid_file)
        .env("VITE_E2E", "true")
        .env("PLAYWRIGHT_HTML_OPEN", "never")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    if let Some(host) = h.outer_docker_host() {
        command.env("E2E_OUTER_DOCKER_HOST", host);
    } else {
        command.env_remove("E2E_OUTER_DOCKER_HOST");
    }
    #[cfg(unix)]
    command.process_group(0);

    eprintln!(
        "Playwright E2E: run_id={} backend={} dind={} theme={}",
        h.run_id,
        h.backend_container_id(),
        h.dind_container_id(),
        theme
    );
    let mut child = command.spawn().unwrap_or_else(|error| {
        panic!(
            "launch Playwright from {} using {}: {error}",
            frontend.display(),
            npm.display()
        )
    });
    #[cfg(unix)]
    let process_group = child.id().unwrap_or(0);
    #[cfg(not(unix))]
    let process_group = 0;

    let outcome = wait_for_playwright(&mut child, process_group).await;
    #[cfg(unix)]
    if process_group != 0 {
        terminate_remaining_group(process_group).await;
    }
    #[cfg(unix)]
    stop_preview(&preview_pid_file).await;

    match outcome {
        Ok(status) if status.success() => {
            eprintln!("Playwright E2E passed for run_id={}", h.run_id);
        }
        Ok(status) => panic!(
            "Playwright E2E failed for run_id={} with exit status {status}",
            h.run_id
        ),
        Err(error) => panic!("Playwright E2E failed for run_id={}: {error}", h.run_id),
    }
}

#[cfg(unix)]
async fn preview_process_group_alive(process_group: u32) -> bool {
    Command::new("kill")
        .args(["-0", "--", &format!("-{process_group}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .is_ok_and(|status| status.success())
}

#[cfg(unix)]
async fn stop_preview(pid_file: &Path) {
    let Ok(contents) = std::fs::read_to_string(pid_file) else {
        return;
    };
    let pid = match contents.trim().parse::<u32>() {
        Ok(pid) if pid > 1 => pid,
        _ => {
            let _ = std::fs::remove_file(pid_file);
            return;
        }
    };
    if !preview_process_group_alive(pid).await {
        let _ = std::fs::remove_file(pid_file);
        return;
    }

    send_group_signal(pid, "-TERM").await;
    let deadline = Instant::now() + PREVIEW_SHUTDOWN_GRACE;
    while preview_process_group_alive(pid).await {
        if Instant::now() >= deadline {
            send_group_signal(pid, "-KILL").await;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let _ = std::fs::remove_file(pid_file);
}

async fn wait_for_playwright(
    child: &mut Child,
    process_group: u32,
) -> Result<std::process::ExitStatus, String> {
    let limit = playwright_timeout();
    let deadline = Instant::now() + limit;

    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut sigint = signal(SignalKind::interrupt())
            .map_err(|error| format!("install SIGINT handler: {error}"))?;
        let mut sigterm = signal(SignalKind::terminate())
            .map_err(|error| format!("install SIGTERM handler: {error}"))?;

        tokio::select! {
            status = child.wait() => status.map_err(|error| format!("wait for Playwright: {error}")),
            _ = sigint.recv() => {
                stop_child(child, process_group).await;
                Err("interrupted by SIGINT before Playwright completed".to_owned())
            }
            _ = sigterm.recv() => {
                stop_child(child, process_group).await;
                Err("interrupted by SIGTERM before Playwright completed".to_owned())
            }
            _ = tokio::time::sleep_until(deadline) => {
                stop_child(child, process_group).await;
                Err(format!("Playwright exceeded its {}s deadline", limit.as_secs()))
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = process_group;
        tokio::select! {
            status = child.wait() => status.map_err(|error| format!("wait for Playwright: {error}")),
            _ = tokio::time::sleep_until(deadline) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                Err(format!("Playwright exceeded its {}s deadline", limit.as_secs()))
            }
        }
    }
}

#[cfg(unix)]
async fn stop_child(child: &mut Child, process_group: u32) {
    send_group_signal(process_group, "-TERM").await;
    if timeout(GROUP_SHUTDOWN_GRACE, child.wait()).await.is_err() {
        send_group_signal(process_group, "-KILL").await;
        let _ = child.wait().await;
    }
}

#[cfg(unix)]
async fn terminate_remaining_group(process_group: u32) {
    send_group_signal(process_group, "-TERM").await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    send_group_signal(process_group, "-KILL").await;
}

#[cfg(unix)]
async fn send_group_signal(process_group: u32, signal: &str) {
    if process_group == 0 {
        return;
    }
    let _ = Command::new("kill")
        .args([signal, "--", &format!("-{process_group}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
}
