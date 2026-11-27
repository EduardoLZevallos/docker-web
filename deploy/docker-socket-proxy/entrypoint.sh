#!/bin/sh
set -eu

if [ ! -S /var/run/docker.sock ]; then
    echo "docker-socket-proxy: /var/run/docker.sock is not a socket" >&2
    exit 1
fi

mkdir -p /run/docker-api
rm -f /run/docker-api/docker.sock
chown root:dockerproxy /run/docker-api
chmod 0770 /run/docker-api

umask 007
nginx -g 'daemon off;' &
nginx_pid=$!

trap 'kill -TERM "$nginx_pid" 2>/dev/null || true' TERM INT

attempts=0
while [ ! -S /run/docker-api/docker.sock ] && [ "$attempts" -lt 100 ]; do
    sleep 0.05
    attempts=$((attempts + 1))
done

if [ -S /run/docker-api/docker.sock ]; then
    chgrp dockerproxy /run/docker-api/docker.sock
    chmod 0660 /run/docker-api/docker.sock
fi

wait "$nginx_pid"
