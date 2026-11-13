# syntax=docker/dockerfile:1.28@sha256:bb22d9815c728170f72750f4e5b0d672e06176142e1d602c7e66c050100b7e5b
FROM ghcr.io/astral-sh/uv:0.11.29@sha256:eb2843a1e56fd9e30c7276ce1a52cba86e64c7b385f5e3279a0e08e02dd058fc AS uv
FROM python:3.13.14-slim-bookworm@sha256:67a1e1f215ccda113cfc024e8639049257e88f273898f595b61476d128d387e8 AS build
COPY --from=uv /uv /usr/local/bin/uv
WORKDIR /opt/beacon
COPY pyproject.toml uv.lock .python-version ./
COPY src ./src
RUN --mount=type=cache,target=/root/.cache/uv,sharing=locked \
    uv sync --locked --no-dev --no-editable --no-managed-python --python /usr/local/bin/python

FROM python:3.13.14-slim-bookworm@sha256:67a1e1f215ccda113cfc024e8639049257e88f273898f595b61476d128d387e8 AS runtime
LABEL org.opencontainers.image.title="Beacon MCP Server" \
      org.opencontainers.image.description="Read-only MCP gateway for MeshCore Beacon" \
      org.opencontainers.image.source="https://github.com/Bjorkan/beacon-mcp" \
      org.opencontainers.image.version="2.2.0"
RUN useradd --uid 1000 --user-group --no-create-home --shell /usr/sbin/nologin beacon \
    && rm -rf /usr/local/lib/python3.13/site-packages/pip* /usr/local/bin/pip* \
    && ln -s /opt/beacon/.venv/bin/beacon-mcp /usr/local/bin/beacon-mcp
COPY --from=build /opt/beacon/.venv /opt/beacon/.venv
ENV HOST=0.0.0.0 PORT=3000 PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1
USER 1000:1000
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 CMD ["/usr/local/bin/beacon-mcp", "--healthcheck"]
STOPSIGNAL SIGTERM
CMD ["/usr/local/bin/beacon-mcp"]
