# AGNOS — Hardened Multi-stage Docker Image
# https://github.com/agnos/agnos
#
# Build:   docker build -t agnos:latest .
# gVisor:  docker build --build-arg GVISOR=1 -t agnos:gvisor .
# Run:     docker run --rm -p 8088:8088 -p 8090:8090 agnos:latest

# ---------------------------------------------------------------------------
# Stage 1: Builder
# ---------------------------------------------------------------------------
FROM rust:1.85-bookworm AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace manifests first for layer caching
COPY userland/Cargo.toml userland/Cargo.toml
COPY userland/Cargo.lock userland/Cargo.lock
COPY userland/agnos-common/Cargo.toml userland/agnos-common/Cargo.toml
COPY userland/agnos-sys/Cargo.toml userland/agnos-sys/Cargo.toml
COPY userland/agent-runtime/Cargo.toml userland/agent-runtime/Cargo.toml
COPY userland/ai-shell/Cargo.toml userland/ai-shell/Cargo.toml
COPY userland/llm-gateway/Cargo.toml userland/llm-gateway/Cargo.toml
COPY userland/desktop-environment/Cargo.toml userland/desktop-environment/Cargo.toml
COPY userland/examples/Cargo.toml userland/examples/Cargo.toml

# Copy full source
COPY userland/ userland/

# Build release binaries
RUN cd userland && cargo build --release \
    --bin agent_runtime \
    --bin llm_gateway \
    --bin agnsh \
    2>&1

# ---------------------------------------------------------------------------
# Stage 2: Runtime
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# OCI annotations
LABEL org.opencontainers.image.title="AGNOS"
LABEL org.opencontainers.image.description="AI-Native General Operating System"
LABEL org.opencontainers.image.source="https://github.com/maccracken/agnosticos"
LABEL org.opencontainers.image.licenses="GPL-3.0"
ARG AGNOS_VERSION=dev
LABEL org.opencontainers.image.version="${AGNOS_VERSION}"

# Install minimal runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    tini \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -g 1000 agnos && \
    useradd -u 1000 -g agnos -m -s /bin/sh agnos

# Create required directories
RUN mkdir -p \
    /run/agnos/agents \
    /var/lib/agnos/secrets \
    /var/lib/agnos/agents \
    /var/log/agnos \
    /etc/agnos \
    && chown -R agnos:agnos /run/agnos /var/lib/agnos /var/log/agnos /etc/agnos

# Copy binaries from builder
COPY --from=builder /build/userland/target/release/agent_runtime /usr/local/bin/
COPY --from=builder /build/userland/target/release/llm_gateway   /usr/local/bin/
COPY --from=builder /build/userland/target/release/agnsh          /usr/local/bin/

# Copy VERSION file and entrypoint
COPY VERSION /etc/agnos/VERSION
COPY docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

# ---------------------------------------------------------------------------
# Stage 3 (optional): gVisor
# ---------------------------------------------------------------------------
ARG GVISOR=0

RUN if [ "$GVISOR" = "1" ]; then \
    set -e; \
    apt-get update && apt-get install -y --no-install-recommends curl; \
    ARCH=$(dpkg --print-architecture); \
    curl -fsSL "https://storage.googleapis.com/gvisor/releases/release/latest/${ARCH}/runsc" \
        -o /usr/local/bin/runsc; \
    curl -fsSL "https://storage.googleapis.com/gvisor/releases/release/latest/${ARCH}/runsc.sha512" \
        -o /tmp/runsc.sha512; \
    cd /usr/local/bin && sha512sum -c /tmp/runsc.sha512; \
    chmod +x /usr/local/bin/runsc; \
    rm -f /tmp/runsc.sha512; \
    apt-get purge -y curl && apt-get autoremove -y && rm -rf /var/lib/apt/lists/*; \
    fi

COPY docker/gvisor-config.toml /etc/agnos/gvisor-config.toml

# ---------------------------------------------------------------------------
# Runtime configuration
# ---------------------------------------------------------------------------

# Health check via LLM gateway
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -sf http://localhost:8088/v1/health || exit 1

# Expose ports
EXPOSE 8088 8090

# Use tini as PID 1
ENTRYPOINT ["tini", "--", "/usr/local/bin/entrypoint.sh"]

# Switch to non-root user
USER agnos

CMD ["daemon"]
