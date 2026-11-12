# syntax=docker/dockerfile:1.28@sha256:bb22d9815c728170f72750f4e5b0d672e06176142e1d602c7e66c050100b7e5b
FROM rust:1.99.1-bookworm@sha256:e6f2cff35d0f309acf8433b4f05878ed77092602c5065cc81801177da0bb3a9c AS build
WORKDIR /app/rust
COPY rust/Cargo.toml rust/Cargo.lock ./
COPY rust/src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo build --release --locked

FROM debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc AS runtime
LABEL org.opencontainers.image.title="Beacon MCP Server" \
      org.opencontainers.image.description="Read-only MCP gateway for MeshCore Beacon" \
      org.opencontainers.image.source="https://github.com/Bjorkan/beacon-mcp" \
      org.opencontainers.image.version="2.2.0"
RUN apt-get update && apt-get install --no-install-recommends -y ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 1000 --user-group --no-create-home --shell /usr/sbin/nologin beacon
COPY --from=build /app/rust/target/release/beacon-mcp-rust /usr/local/bin/beacon-mcp
USER beacon:beacon
ENV HOST=0.0.0.0 PORT=3000
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 CMD ["/usr/local/bin/beacon-mcp", "--healthcheck"]
STOPSIGNAL SIGTERM
CMD ["/usr/local/bin/beacon-mcp"]
