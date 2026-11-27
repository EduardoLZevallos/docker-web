# syntax=docker/dockerfile:1

FROM nginx:1.27-alpine

RUN addgroup -S -g 10001 dockerproxy

COPY deploy/docker-socket-proxy/nginx.conf /etc/nginx/nginx.conf
COPY deploy/docker-socket-proxy/entrypoint.sh /usr/local/bin/docker-socket-proxy-entrypoint
RUN chmod 0755 /usr/local/bin/docker-socket-proxy-entrypoint

ENTRYPOINT ["/usr/local/bin/docker-socket-proxy-entrypoint"]
