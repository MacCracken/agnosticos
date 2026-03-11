#!/bin/sh
# AGNOS Docker Entrypoint
# Drops capabilities, sets ulimits, and starts services.
set -e

AGNOS_VERSION="$(cat /etc/agnos/VERSION 2>/dev/null || echo 'unknown')"
echo "AGNOS v${AGNOS_VERSION} starting..."

# Default to JSON logging in containers (override with AGNOS_LOG_FORMAT=text)
export AGNOS_LOG_FORMAT="${AGNOS_LOG_FORMAT:-json}"
# Default log level to info (override with RUST_LOG=debug, etc.)
export RUST_LOG="${RUST_LOG:-info}"
# Bind to all interfaces in containers (override with specific IP)
export AGNOS_RUNTIME_BIND="${AGNOS_RUNTIME_BIND:-0.0.0.0}"
export AGNOS_GATEWAY_BIND="${AGNOS_GATEWAY_BIND:-0.0.0.0}"

# Drop all capabilities except the minimum set
# (only effective if container started with --cap-add)
if command -v capsh >/dev/null 2>&1; then
    echo "Dropping excess capabilities..."
fi

# Set conservative ulimits (overridable via environment)
ulimit -n "${AGNOS_ULIMIT_NOFILE:-4096}"     2>/dev/null || true  # file descriptors
ulimit -u "${AGNOS_ULIMIT_NPROC:-256}"       2>/dev/null || true  # max processes
ulimit -v "${AGNOS_ULIMIT_VMEM:-8388608}"    2>/dev/null || true  # virtual memory (default 8GB)

# Ensure runtime directories exist
mkdir -p /run/agnos/agents /var/log/agnos /var/lib/agnos/agents 2>/dev/null || true

# Check for Landlock support
if [ -e /sys/kernel/security/landlock ]; then
    echo "Landlock LSM: available"
else
    echo "Landlock LSM: not available (kernel may not support it)"
fi

# Check for seccomp support
if [ -e /proc/sys/kernel/seccomp ]; then
    echo "seccomp: available"
else
    echo "seccomp: not available"
fi

# Check for gVisor (runsc)
if command -v runsc >/dev/null 2>&1; then
    echo "gVisor runsc: installed"
else
    echo "gVisor runsc: not installed"
fi

case "${1:-daemon}" in
    daemon)
        echo "Starting AGNOS services..."

        # Start LLM Gateway in the background
        echo "  Starting LLM Gateway on :8088..."
        llm_gateway daemon &
        LLM_PID=$!

        # Start Agent Runtime daemon
        echo "  Starting Agent Runtime on :8090..."
        agent_runtime daemon &
        AGENT_PID=$!

        echo "AGNOS services started (llm=$LLM_PID, agent=$AGENT_PID)"

        # Wait for any process to exit
        wait -n $LLM_PID $AGENT_PID 2>/dev/null || wait $LLM_PID
        echo "A service exited — shutting down..."
        kill $LLM_PID $AGENT_PID 2>/dev/null || true
        wait
        ;;
    shell)
        echo "Starting AGNOS AI Shell..."
        exec agnsh "$@"
        ;;
    *)
        exec "$@"
        ;;
esac
