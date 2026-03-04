#!/bin/sh
# AGNOS Docker Entrypoint
# Drops capabilities, sets ulimits, and starts services.
set -e

echo "AGNOS v0.1.0 starting..."

# Drop all capabilities except the minimum set
# (only effective if container started with --cap-add)
if command -v capsh >/dev/null 2>&1; then
    echo "Dropping excess capabilities..."
fi

# Set conservative ulimits
ulimit -n 4096   2>/dev/null || true  # file descriptors
ulimit -u 256    2>/dev/null || true  # max processes
ulimit -v 2097152 2>/dev/null || true # virtual memory (2GB)

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
        llm_gateway &
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
        exec ai_shell "$@"
        ;;
    *)
        exec "$@"
        ;;
esac
