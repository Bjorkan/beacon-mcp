# syntax=docker/dockerfile:1.7
FROM --platform=$BUILDPLATFORM node:24-bookworm-slim AS development-dependencies
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --ignore-scripts

FROM development-dependencies AS build
COPY tsconfig.json tsconfig.build.json eslint.config.js vitest.config.ts .prettierignore ./
COPY src ./src
COPY scripts ./scripts
COPY tests ./tests
COPY vendor ./vendor
RUN npm run openapi:generate \
    && npm run format:check \
    && npm run typecheck \
    && npm run lint \
    && npm run test \
    && npm run build

FROM node:24-bookworm-slim AS production-dependencies
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --omit=dev --ignore-scripts \
    && npm cache clean --force

FROM node:24-bookworm-slim AS runtime
LABEL org.opencontainers.image.title="beacon-mcp" \
      org.opencontainers.image.description="Read-only MCP gateway for MeshCore Beacon" \
      org.opencontainers.image.source="https://github.com/Bjorkan/beacon-mcp" \
      org.opencontainers.image.version="2.0.2"
ENV NODE_ENV=production \
    HOST=0.0.0.0 \
    PORT=3000
WORKDIR /app
COPY --from=production-dependencies --chown=node:node /app/node_modules ./node_modules
COPY --from=build --chown=node:node /app/dist ./dist
COPY --chown=node:node package.json ./package.json
COPY --chown=node:node scripts/healthcheck.mjs ./scripts/healthcheck.mjs
USER node:node
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 CMD ["node", "scripts/healthcheck.mjs"]
CMD ["node", "dist/index.js"]
