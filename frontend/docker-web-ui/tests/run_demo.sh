#!/usr/bin/env bash
set -u

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
frontend_dir="$(CDPATH= cd -- "$script_dir/.." && pwd)"
root="$(CDPATH= cd -- "$frontend_dir/../.." && pwd)"

hold_seconds="${DEMO_HOLD_SECONDS:-600}"
startup_timeout="${DEMO_STARTUP_TIMEOUT_SECONDS:-300}"
vite_timeout="${DEMO_VITE_STARTUP_TIMEOUT_SECONDS:-90}"
shutdown_timeout="${DEMO_SHUTDOWN_TIMEOUT_SECONDS:-180}"

for node_dir in \
    "$HOME/.local/bin" \
    "$HOME/.local/share/node/node-v26.13.1-linux-x64/bin"; do
    if [ -d "$node_dir" ]; then
        PATH="$node_dir:$PATH"
    fi
done
export PATH

case "$hold_seconds" in
    ''|*[!0-9]*)
        echo "error: DEMO_HOLD_SECONDS must be a non-negative integer" >&2
        exit 1
        ;;
esac

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/docker-web-demo.XXXXXX")"
ready_file="$tmp_dir/ready.json"
backend_pid=""
backend_isolated=0
vite_pid=""
vite_isolated=0

stop_process() {
    local pid=$1
    local isolated=$2
    local mode=$3
    local waited=0
    [ -n "$pid" ] || return 0
    if ! kill -0 "$pid" 2>/dev/null; then
        wait "$pid" 2>/dev/null || true
        return 0
    fi
    if [ "$mode" = "group" ] && [ "$isolated" -eq 1 ]; then
        kill -TERM -- "-$pid" 2>/dev/null || kill -TERM "$pid" 2>/dev/null || true
    else
        kill -TERM "$pid" 2>/dev/null || true
    fi
    while kill -0 "$pid" 2>/dev/null; do
        if [ "$waited" -ge "$shutdown_timeout" ]; then
            if [ "$isolated" -eq 1 ]; then
                kill -KILL -- "-$pid" 2>/dev/null || true
            else
                kill -KILL "$pid" 2>/dev/null || true
            fi
            break
        fi
        sleep 1
        waited=$((waited + 1))
    done
    wait "$pid" 2>/dev/null || true
}

cleanup() {
    local status=${1:-$?}
    trap - EXIT INT TERM
    if [ -n "$vite_pid" ]; then
        stop_process "$vite_pid" "$vite_isolated" group
        vite_pid=""
    fi
    if [ -n "$backend_pid" ]; then
        stop_process "$backend_pid" "$backend_isolated" leader
        backend_pid=""
    fi
    rm -rf "$tmp_dir"
    exit "$status"
}

trap 'cleanup 130' INT
trap 'cleanup 143' TERM
trap 'cleanup $?' EXIT

if ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1; then
    echo "error: Node 26.13.1 is required; add ~/.local/bin or" >&2
    echo "       ~/.local/share/node/node-v26.13.1-linux-x64/bin to PATH" >&2
    exit 1
fi
if [ ! -x "$frontend_dir/node_modules/.bin/vite" ]; then
    echo "error: frontend dependencies are missing; run npm ci in $frontend_dir" >&2
    exit 1
fi

read_topology_timeout=$(( $(date +%s) + startup_timeout ))

echo "==> starting isolated demo backend and fixtures"
if command -v setsid >/dev/null 2>&1; then
    setsid env \
        DEMO_HOLD_SECONDS="$hold_seconds" \
        DEMO_READY_FILE="$ready_file" \
        "$root/backend/tests/run_backend_e2e.sh" \
        demo_three_networks_with_fake_containers \
        --ignored --exact --nocapture &
    backend_isolated=1
else
    env \
        DEMO_HOLD_SECONDS="$hold_seconds" \
        DEMO_READY_FILE="$ready_file" \
        "$root/backend/tests/run_backend_e2e.sh" \
        demo_three_networks_with_fake_containers \
        --ignored --exact --nocapture &
    backend_isolated=0
fi
backend_pid=$!

while :; do
    if [ -s "$ready_file" ]; then
        break
    fi
    if ! kill -0 "$backend_pid" 2>/dev/null; then
        wait "$backend_pid" 2>/dev/null || true
        backend_pid=""
        echo "error: demo backend exited before signaling readiness" >&2
        exit 1
    fi
    if [ "$(date +%s)" -ge "$read_topology_timeout" ]; then
        echo "error: demo backend did not signal readiness within ${startup_timeout}s" >&2
        exit 1
    fi
    sleep 0.2
done

ready_fields="$(node -e '
const fs = require("node:fs");
const payload = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
const parsed = new URL(payload.base_url);
if (
  (parsed.protocol !== "http:" && parsed.protocol !== "https:") ||
  parsed.pathname !== "/" ||
  parsed.search ||
  parsed.hash
) {
  throw new Error("base_url is not an http(s) origin");
}
if (typeof payload.run_id !== "string" || payload.run_id.length === 0) {
  throw new Error("run_id is missing");
}
process.stdout.write(parsed.origin + "\n" + payload.run_id + "\n");
' "$ready_file")" || {
    echo "error: demo readiness file is invalid" >&2
    exit 1
}
mapfile -t ready_lines <<< "$ready_fields"
backend_url="${ready_lines[0]:-}"
run_id="${ready_lines[1]:-}"
if [ -z "$backend_url" ] || [ -z "$run_id" ]; then
    echo "error: demo readiness file is incomplete" >&2
    exit 1
fi

echo "==> starting Vite dev UI"
if command -v setsid >/dev/null 2>&1; then
    (
        cd "$frontend_dir" || exit 1
        exec setsid env BACKEND_URL="$backend_url" npm run dev
    ) &
    vite_isolated=1
else
    (
        cd "$frontend_dir" || exit 1
        exec env BACKEND_URL="$backend_url" npm run dev
    ) &
    vite_isolated=0
fi
vite_pid=$!

vite_deadline=$(( $(date +%s) + vite_timeout ))
while :; do
    if node -e '
fetch(process.argv[1], { signal: AbortSignal.timeout(2000) })
  .then((response) => process.exit(response.ok ? 0 : 1))
  .catch(() => process.exit(1));
' "http://localhost:5173/" >/dev/null 2>&1; then
        break
    fi
    if ! kill -0 "$vite_pid" 2>/dev/null; then
        wait "$vite_pid" 2>/dev/null || true
        vite_pid=""
        echo "error: Vite exited before the demo UI became ready" >&2
        exit 1
    fi
    if [ "$(date +%s)" -ge "$vite_deadline" ]; then
        echo "error: Vite did not become ready within ${vite_timeout}s" >&2
        exit 1
    fi
    sleep 0.25
done

echo
echo "Demo UI:    http://localhost:5173"
echo "Backend API: $backend_url"
echo "Run ID:     $run_id"
echo "Hold:       ${hold_seconds}s (Ctrl+C stops early and cleans up)"
echo

backend_status=0
while :; do
    if ! kill -0 "$vite_pid" 2>/dev/null; then
        wait "$vite_pid" 2>/dev/null || true
        vite_pid=""
        echo "error: Vite exited before the demo hold ended" >&2
        exit 1
    fi
    if ! kill -0 "$backend_pid" 2>/dev/null; then
        wait "$backend_pid"
        backend_status=$?
        backend_pid=""
        break
    fi
    sleep 1
done

if [ -n "$vite_pid" ]; then
    stop_process "$vite_pid" "$vite_isolated" group
    vite_pid=""
fi

if [ "$backend_status" -ne 0 ]; then
    echo "error: demo backend failed with status $backend_status" >&2
    exit "$backend_status"
fi

echo "Demo hold ended; UI stopped and backend harness cleanup completed."
