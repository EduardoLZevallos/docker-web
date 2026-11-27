# syntax=docker/dockerfile:1

FROM node:22-alpine AS builder

WORKDIR /app
ENV PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
ENV PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=1

COPY frontend/docker-web-ui/package.json frontend/docker-web-ui/package-lock.json ./
RUN npm ci --no-audit --no-fund

COPY frontend/docker-web-ui/index.html frontend/docker-web-ui/tsconfig.json frontend/docker-web-ui/vite.config.ts ./
COPY frontend/docker-web-ui/public ./public
COPY frontend/docker-web-ui/src ./src
RUN npm run build

FROM nginx:1.27-alpine

COPY deploy/web/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/dist /usr/share/nginx/html

EXPOSE 8080
