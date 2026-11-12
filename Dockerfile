# syntax=docker/dockerfile:1
# Multi-stage build (PRD.md §9.6). Produksi: image dibangun di CI lalu di-pull ke VPS,
# tidak di-build di VPS. Target: `api`, `worker`.

FROM rust:1.98-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p umkm-api -p umkm-worker \
 && mkdir -p /out \
 && cp target/release/umkm-api target/release/umkm-worker /out/

FROM debian:bookworm-slim AS runtime
# Sertifikat CA untuk HTTPS keluar ke provider AI (P5, ADR-0011).
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
RUN useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin umkm
COPY config /app/config
ENV UMKM_CONFIG_DIR=/app/config
WORKDIR /app
# Non-root (PRD.md §9.6, §14.3)
USER 10001:10001

FROM runtime AS api
COPY --from=build /out/umkm-api /usr/local/bin/umkm-api
EXPOSE 8080
ENTRYPOINT ["umkm-api"]
CMD ["serve"]

FROM runtime AS worker
COPY --from=build /out/umkm-worker /usr/local/bin/umkm-worker
ENTRYPOINT ["umkm-worker"]

# ---------------------------------------------------------------- frontend (ADR-0014)
# Static export Next.js; tidak ada Node di image produksi. CSP hash skrip inline dibuat saat build.
FROM node:22-bookworm-slim AS frontend-build
WORKDIR /fe
COPY frontend/package.json frontend/package-lock.json frontend/.npmrc ./
RUN npm ci --no-audit
COPY frontend/ ./
ARG PUBLIC_API_ORIGIN
ENV NEXT_PUBLIC_API_ORIGIN=${PUBLIC_API_ORIGIN} NEXT_TELEMETRY_DISABLED=1
RUN test -n "$PUBLIC_API_ORIGIN" || { echo "build-arg PUBLIC_API_ORIGIN wajib"; exit 1; }; npm run build

FROM caddy:2.10-alpine AS web
COPY --from=frontend-build /fe/out /srv/www
COPY ops/caddy/Caddyfile /etc/caddy/Caddyfile
