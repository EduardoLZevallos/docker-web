#!/usr/bin/env bash
set -u

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
tag="${DOCKER_WEB_E2E_SUITE_IMAGE:-docker-web-e2e:suite-$(date +%s)-$$}"
context=""
build_pid=""
test_pid=""
test_isolated=0
shutdown_timeout="${DOCKER_WEB_E2E_SHUTDOWN_TIMEOUT_SECONDS:-120}"

remove_image() {
    local attempt
    for attempt in 1 2 3 4 5; do
        if ! docker image inspect "$tag" >/dev/null 2>&1; then
            return 0
        fi
        docker image rm "$tag" >/dev/null 2>&1 || true
        sleep 1
    done
    ! docker image inspect "$tag" >/dev/null 2>&1
}

stop_build() {
    local waited=0
    [ -n "$build_pid" ] || return 0
    if ! kill -0 "$build_pid" 2>/dev/null; then
        wait "$build_pid" 2>/dev/null || true
        build_pid=""
        return 0
    fi
    kill -TERM "$build_pid" 2>/dev/null || true
    while kill -0 "$build_pid" 2>/dev/null; do
        if [ "$waited" -ge "$shutdown_timeout" ]; then
            kill -KILL "$build_pid" 2>/dev/null || true
            break
        fi
        sleep 1
        waited=$((waited + 1))
    done
    wait "$build_pid" 2>/dev/null || true
    build_pid=""
}

cleanup() {
    local status=$?
    trap - EXIT INT TERM
    stop_build
    if [ -n "$context" ]; then
        rm -rf "$context"
    fi
    if ! remove_image; then
        echo "error: owned test image $tag remains after cleanup" >&2
        if [ "$status" -eq 0 ]; then
            status=1
        fi
    fi
    exit "$status"
}

signal_test_child() {
    local signal=$1
    local child
    local signaled=0
    if [ -z "$test_pid" ] || ! kill -0 "$test_pid" 2>/dev/null; then
        return 0
    fi
    while read -r child; do
        [ -n "$child" ] || continue
        if kill -"$signal" "$child" 2>/dev/null; then
            signaled=1
        fi
    done < <(
        pgrep -P "$test_pid" 2>/dev/null \
            || ps -o pid= --ppid "$test_pid" 2>/dev/null \
            || true
    )
    if [ "$signaled" -eq 0 ]; then
        kill -"$signal" "$test_pid" 2>/dev/null || true
    fi
}

wait_for_test_shutdown() {
    local waited=0
    while [ -n "$test_pid" ] && kill -0 "$test_pid" 2>/dev/null; do
        if [ "$waited" -ge "$shutdown_timeout" ]; then
            if [ "$test_isolated" -eq 1 ]; then
                kill -KILL -- "-$test_pid" 2>/dev/null || true
            else
                kill -KILL "$test_pid" 2>/dev/null || true
            fi
            break
        fi
        sleep 1
        waited=$((waited + 1))
    done
}

interrupt() {
    local signal=$1
    local status=$2
    trap - INT TERM
    stop_build
    signal_test_child "$signal"
    wait_for_test_shutdown
    if [ -n "$test_pid" ]; then
        wait "$test_pid" 2>/dev/null || true
    fi
    exit "$status"
}

trap cleanup EXIT
trap 'interrupt INT 130' INT
trap 'interrupt TERM 143' TERM

context="$(mktemp -d)"
mkdir -p "$context/backend"
cp "$root/backend/Cargo.toml" "$root/backend/Cargo.lock" "$context/backend/"
cp -R "$root/backend/src" "$context/backend/src"

docker build \
    --file "$root/backend/tests/e2e_assets/Dockerfile" \
    --tag "$tag" \
    "$context" &
build_pid=$!
if ! wait "$build_pid"; then
    build_pid=""
    echo "error: backend test image build failed" >&2
    exit 1
fi
build_pid=""

export DOCKER_WEB_E2E_SUITE_IMAGE="$tag"

test_args=(
    --manifest-path "$root/backend/Cargo.toml"
    --test backend_e2e
    --
    --test-threads=1
)
if command -v setsid >/dev/null 2>&1; then
    setsid cargo test "${test_args[@]}" "$@" &
    test_isolated=1
else
    cargo test "${test_args[@]}" "$@" &
fi
test_pid=$!
wait "$test_pid"
