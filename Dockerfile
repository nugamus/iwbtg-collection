# syntax=docker/dockerfile:1
# SPDX-FileCopyrightText: 2026 Jonas Dahl
# SPDX-License-Identifier: MIT
#
# Multi-stage build for iwbtg-server (docs/server.md, Deployment). The builder uses the
# Rust version pinned in rust-toolchain.toml; the runtime carries only the binary, the CA
# roots the healthchecks.io ping (and later OAuth/SMTP) need, and curl for the /healthz
# healthcheck.

FROM rust:1.99.1-bookworm AS build
WORKDIR /src
# BuildKit cache mounts keep the registry and the target directory between builds, so a
# redeploy recompiles only the crates whose sources changed. The release binary is copied
# out of the cache mount into the ordinary layer.
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target,sharing=locked \
    cargo build --release --locked -p iwbtg-server \
    && install -D -m 0755 target/release/iwbtg-server /out/iwbtg-server

FROM debian:trixie-slim AS runtime
# curl is the healthcheck client (Dockerfile HEALTHCHECK below and compose.yaml). The CA
# roots are not optional: the healthchecks.io ping uses ureq with native-tls, which verifies
# against the OS trust store (src/health.rs), and OAuth/SMTP calls will use it too.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# Non-root runtime user (security rule 4). uid/gid 10001 is above the system range so it
# is unlikely to collide with anything on the host; /srv/iwbtg/data is the one writable
# path, and its ownership is what seeds the named volume on first mount.
RUN groupadd --system --gid 10001 iwbtg \
    && useradd --system --uid 10001 --gid iwbtg --home-dir /srv/iwbtg \
       --no-create-home --shell /usr/sbin/nologin iwbtg \
    && install -d -o iwbtg -g iwbtg -m 0750 /srv/iwbtg/data

COPY --from=build /out/iwbtg-server /usr/local/bin/iwbtg-server

# Defaults match the compose service; an operator can still override them.
ENV IWBTG_BIND=0.0.0.0:8080 \
    IWBTG_DB=/srv/iwbtg/data/iwbtg.db
EXPOSE 8080

USER 10001:10001
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["curl", "--fail", "--silent", "--show-error", "http://127.0.0.1:8080/healthz"]

ENTRYPOINT ["/usr/local/bin/iwbtg-server"]
