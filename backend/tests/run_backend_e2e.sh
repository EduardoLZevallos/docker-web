#!/usr/bin/env bash
set -u

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
tag="${DOCKER_WEB_E2E_SUITE_IMAGE:-docker-web-e2e:suite-$(date +%s)-$$}"
context=""
test_pid=""

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

cleanup() {
    local status=$?
    trap - EXIT INT TERM
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

interrupt() {
    local signal=$1
    local status=$2
    if [ -n "$test_pid" ] && kill -0 "$test_pid" 2>/dev/null; then
        kill -"$signal" "$test_pid" 2>/dev/null || true
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

if ! docker build \
    --file "$root/backend/tests/e2e_assets/Dockerfile" \
    --tag "$tag" \
    "$context"; then
    echo "error: backend test image build failed" >&2
    exit 1
fi

export DOCKER_WEB_E2E_SUITE_IMAGE="$tag"

cargo test \
    --manifest-path "$root/backend/Cargo.toml" \
    --test backend_e2e \
    -- --test-threads=1 "$@" &
test_pid=$!
wait "$test_pid"
