#!/bin/sh
set -eu

# Docker stop/start keeps the writable layer (including /var/run) but gives the
# next start a fresh PID namespace. A daemon interrupted during shutdown can leave
# /var/run/docker.pid behind, and its recorded PID may be reused by an unrelated
# process in the new namespace, so dockerd rejects its own startup. The official
# entrypoint performs that stale-pid cleanup plus its init/iptables housekeeping.
# Pass an explicit dockerd command so its default TCP listener is not added.
rm -f /e2e/docker.sock

# The daemon creates the socket with restrictive root ownership inside the
# container. The host test process and sibling backend share the bind-mounted
# directory, so make the socket reachable once the daemon has created it.
(
    until [ -S /e2e/docker.sock ]; do
        sleep 0.2
    done
    chmod 666 /e2e/docker.sock
    echo E2E_DOCKER_READY
) &

# Prefer a public pull-through mirror to avoid anonymous Docker Hub rate limits.
# Docker falls back to Docker Hub when the mirror is unavailable.
exec /usr/local/bin/dockerd-entrypoint.sh dockerd \
    --host=unix:///e2e/docker.sock \
    --data-root=/var/lib/docker \
    --registry-mirror=https://mirror.gcr.io
