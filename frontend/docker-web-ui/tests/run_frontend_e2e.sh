#!/usr/bin/env bash
set -u

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
frontend_dir="$(CDPATH= cd -- "$script_dir/.." && pwd)"
root="$(CDPATH= cd -- "$frontend_dir/../.." && pwd)"

overall_timeout="${FRONTEND_E2E_TIMEOUT_SECONDS:-3600}"
build_timeout="${FRONTEND_E2E_BUILD_TIMEOUT_SECONDS:-600}"
shutdown_timeout="${FRONTEND_E2E_SHUTDOWN_TIMEOUT_SECONDS:-180}"
deadline=$(( $(date +%s) + overall_timeout ))
child_pid=""
child_isolated=0

for node_dir in \
    "$HOME/.local/bin" \
    "$HOME/.local/share/node/node-v26.13.1-linux-x64/bin"; do
    if [ -d "$node_dir" ]; then
        PATH="$node_dir:$PATH"
    fi
done
export PATH

stop_child() {
    local pid=$1
    local isolated=$2
    local waited=0
    [ -n "$pid" ] || return 0
    if ! kill -0 "$pid" 2>/dev/null; then
        wait "$pid" 2>/dev/null || true
        return 0
    fi
    if [ "$isolated" -eq 1 ]; then
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
    if [ -n "$child_pid" ]; then
        stop_child "$child_pid" "$child_isolated"
        child_pid=""
    fi
    exit "$status"
}

run_step() {
    local label=$1
    local timeout_seconds=$2
    shift 2
    local pid
    local status
    local now
    local step_deadline

    if [ "$timeout_seconds" -lt 1 ]; then
        timeout_seconds=1
    fi
    step_deadline=$(( $(date +%s) + timeout_seconds ))
    if [ "$step_deadline" -gt "$deadline" ]; then
        step_deadline=$deadline
    fi

    echo "==> $label"
    if command -v setsid >/dev/null 2>&1; then
        setsid "$@" &
        child_isolated=1
    else
        "$@" &
        child_isolated=0
    fi
    pid=$!
    child_pid=$pid

    while kill -0 "$pid" 2>/dev/null; do
        now=$(date +%s)
        if [ "$now" -ge "$step_deadline" ]; then
            echo "error: $label exceeded its deadline" >&2
            stop_child "$pid" "$child_isolated"
            child_pid=""
            return 124
        fi
        sleep 1
    done
    wait "$pid"
    status=$?
    child_pid=""
    return "$status"
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
if [ ! -f "$frontend_dir/node_modules/@playwright/test/package.json" ] \
    || [ ! -x "$frontend_dir/node_modules/.bin/playwright" ]; then
    echo "error: @playwright/test is missing; run npm ci in $frontend_dir" >&2
    exit 1
fi

if [ "$#" -gt 0 ]; then
    printf -v joined '%s\x1f' "$@"
    export E2E_PLAYWRIGHT_ARGS="$joined"
elif [ -n "${PLAYWRIGHT_ARGS:-}" ]; then
    export E2E_PLAYWRIGHT_ARGS="$PLAYWRIGHT_ARGS"
fi

export VITE_E2E=true
export E2E_PLAYWRIGHT_TIMEOUT_SECONDS="${E2E_PLAYWRIGHT_TIMEOUT_SECONDS:-$overall_timeout}"

if ! run_step "typecheck and E2E UI build" "$build_timeout" \
    npm --prefix "$frontend_dir" run build:e2e; then
    echo "error: frontend E2E build failed" >&2
    exit 1
fi
export E2E_UI_PREBUILT=true

remaining=$(( deadline - $(date +%s) ))
if [ "$remaining" -lt 1 ]; then
    echo "error: frontend E2E deadline expired before browser tests" >&2
    exit 124
fi

if ! run_step "Playwright against backend and private DinD" "$remaining" \
    "$root/backend/tests/run_backend_e2e.sh" \
    playwright_frontend_e2e --ignored --exact --nocapture; then
    echo "error: frontend browser E2E failed" >&2
    exit 1
fi
