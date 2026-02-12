#!/bin/bash
# agnos-load-modules.sh - Load AGNOS kernel modules

set -e

MODULES_DIR="/lib/modules/$(uname -r)/kernel/agnos"
LOG_FILE="/var/log/agnos/modules.log"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

# Load modules in correct order
load_modules() {
    log "Loading AGNOS kernel modules..."
    
    # Order matters: security -> audit -> agent -> llm
    local modules=("agnos_security" "agnos_audit" "agnos_agent" "agnos_llm")
    
    for mod in "${modules[@]}"; do
        if modprobe "$mod" 2>/dev/null; then
            log "Loaded: $mod"
        else
            log "Warning: Failed to load $mod (may not be available)"
        fi
    done
    
    # Verify
    log "Loaded modules:"
    lsmod | grep "agnos_" | while read line; do
        log "  $line"
    done
}

# Check if modules are available
check_availability() {
    if [ ! -d "$MODULES_DIR" ]; then
        log "Warning: AGNOS modules directory not found: $MODULES_DIR"
        log "Kernel modules may need to be built or installed"
        return 1
    fi
    
    return 0
}

main() {
    log "AGNOS Module Loader starting..."
    
    if check_availability; then
        load_modules
    fi
    
    log "AGNOS Module Loader complete"
}

main "$@"
