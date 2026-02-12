#!/bin/bash
# agnos-init.sh - AGNOS System Initialization Script

set -e

AGNOS_ROOT="/var/lib/agnos"
LOG_FILE="/var/log/agnos/init.log"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

# Create necessary directories
setup_directories() {
    log "Setting up AGNOS directories..."
    
    mkdir -p /var/lib/agnos/{agents,models,cache,audit}
    mkdir -p /var/log/agnos
    mkdir -p /run/agnos
    mkdir -p /etc/agnos
    
    # Set permissions
    chown -R agnos:agnos /var/lib/agnos/agents
    chown -R agnos-llm:agnos-llm /var/lib/agnos/models
    chmod 755 /var/lib/agnos
    chmod 750 /var/log/agnos
}

# Initialize security policies
setup_security() {
    log "Initializing AGNOS security..."
    
    # Load SELinux policies if available
    if [ -f /etc/selinux/agnos/config ]; then
        log "Loading SELinux policies..."
        # semodule -i /usr/share/selinux/agnos/agnos.pp
    fi
    
    # Set up Landlock rules
    if [ -x /usr/bin/agnos-setup-landlock ]; then
        log "Setting up Landlock..."
        /usr/bin/agnos-setup-landlock
    fi
}

# Check kernel modules
check_modules() {
    log "Checking AGNOS kernel modules..."
    
    local required_modules=("agnos_security" "agnos_agent" "agnos_llm" "agnos_audit")
    
    for mod in "${required_modules[@]}"; do
        if ! lsmod | grep -q "^${mod}"; then
            log "Warning: Module $mod not loaded"
        else
            log "Module $mod: OK"
        fi
    done
}

# Setup networking for agents
setup_network() {
    log "Setting up agent networking..."
    
    # Create agent network namespace if needed
    if [ ! -d /run/netns/agnos ]; then
        ip netns add agnos 2>/dev/null || true
    fi
}

# Initialize audit system
setup_audit() {
    log "Initializing audit system..."
    
    # Create audit log directory with proper permissions
    mkdir -p /var/log/agnos/audit
    chmod 750 /var/log/agnos/audit
    
    # Set immutable attribute on audit directory (optional)
    # chattr +a /var/log/agnos/audit 2>/dev/null || true
}

# Main initialization
main() {
    log "AGNOS System Initialization starting..."
    
    setup_directories
    setup_security
    check_modules
    setup_network
    setup_audit
    
    log "AGNOS System Initialization complete"
}

main "$@"
