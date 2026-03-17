#!/bin/bash
# agnos-init.sh — AgnosticOS System Initialization Script
# Runs once at boot via agnos-init.service (before all AGNOS services).

set -e

AGNOS_ROOT="/var/lib/agnos"
LOG_FILE="/var/log/agnos/init.log"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE" 2>/dev/null || echo "$1"
}

# -----------------------------------------------------------------------
# System users and groups — must exist before any AGNOS service starts
# -----------------------------------------------------------------------
setup_users() {
    log "Creating system users..."

    # agnos — agent runtime daemon user
    getent group agnos >/dev/null 2>&1 || groupadd -r agnos
    id -u agnos >/dev/null 2>&1 || \
        useradd -r -g agnos -d /var/lib/agnos -s /usr/sbin/nologin \
        -c "AGNOS Agent Runtime" agnos

    # agnos-llm — LLM gateway daemon user
    getent group agnos-llm >/dev/null 2>&1 || groupadd -r agnos-llm
    id -u agnos-llm >/dev/null 2>&1 || \
        useradd -r -g agnos-llm -d /var/lib/agnos/models -s /usr/sbin/nologin \
        -c "AGNOS LLM Gateway" agnos-llm

    # user — default interactive user (UID 1000, password: agnos)
    if ! id -u user >/dev/null 2>&1; then
        useradd -m -G sudo,video,audio,input,render -s /bin/bash -u 1000 user 2>/dev/null || true
        echo 'user:agnos' | chpasswd 2>/dev/null || true
    fi

    # root password (default: agnos)
    echo 'root:agnos' | chpasswd 2>/dev/null || true

    log "System users ready"
}

# -----------------------------------------------------------------------
# Directories
# -----------------------------------------------------------------------
setup_directories() {
    log "Setting up AGNOS directories..."

    mkdir -p /var/lib/agnos/{agents,models,cache,audit}
    mkdir -p /var/log/agnos/audit
    mkdir -p /run/agnos
    mkdir -p /etc/agnos
    mkdir -p /run/user/1000

    chown -R agnos:agnos /var/lib/agnos/agents 2>/dev/null || true
    chown -R agnos-llm:agnos-llm /var/lib/agnos/models 2>/dev/null || true
    chown user:user /run/user/1000 2>/dev/null || true
    chmod 0700 /run/user/1000
    chmod 755 /var/lib/agnos
    chmod 750 /var/log/agnos
}

# -----------------------------------------------------------------------
# Hostname
# -----------------------------------------------------------------------
setup_hostname() {
    if [[ -f /etc/hostname ]]; then
        hostname "$(cat /etc/hostname)"
    else
        echo "agnos" > /etc/hostname
        hostname agnos
    fi
    log "Hostname: $(hostname)"
}

# -----------------------------------------------------------------------
# Timezone and locale
# -----------------------------------------------------------------------
setup_locale() {
    log "Configuring locale and timezone..."

    # Default timezone — UTC (user can change via timedatectl)
    if [[ ! -e /etc/localtime ]]; then
        ln -sf /usr/share/zoneinfo/UTC /etc/localtime 2>/dev/null || true
    fi

    # Default locale
    if [[ ! -f /etc/locale.conf ]]; then
        cat > /etc/locale.conf << 'EOF'
LANG=en_US.UTF-8
LC_COLLATE=C
EOF
    fi

    # Generate locale if localedef is available
    if command -v localedef >/dev/null 2>&1; then
        if [[ -f /usr/share/i18n/locales/en_US ]]; then
            localedef -i en_US -f UTF-8 en_US.UTF-8 2>/dev/null || true
        fi
    fi

    log "Timezone: $(readlink -f /etc/localtime 2>/dev/null || echo 'UTC')"
}

# -----------------------------------------------------------------------
# DNS resolution
# -----------------------------------------------------------------------
setup_dns() {
    # If systemd-resolved is running, it handles /etc/resolv.conf via stub
    if [[ -L /run/systemd/resolve/stub-resolv.conf ]]; then
        ln -sf /run/systemd/resolve/stub-resolv.conf /etc/resolv.conf 2>/dev/null || true
        return
    fi

    # Fallback: create a basic resolv.conf if none exists
    if [[ ! -f /etc/resolv.conf ]] || [[ ! -s /etc/resolv.conf ]]; then
        cat > /etc/resolv.conf << 'EOF'
# AgnosticOS default DNS configuration
# Override via systemd-resolved or DHCP
nameserver 1.1.1.1
nameserver 8.8.8.8
nameserver 2606:4700:4700::1111
EOF
        log "Default DNS configured (Cloudflare + Google fallback)"
    fi
}

# -----------------------------------------------------------------------
# Network — ensure basic DHCP config exists for systemd-networkd
# -----------------------------------------------------------------------
setup_network() {
    log "Setting up networking..."

    # Default DHCP for wired interfaces
    local net_conf="/etc/systemd/network/20-wired.network"
    if [[ ! -f "$net_conf" ]]; then
        mkdir -p /etc/systemd/network
        cat > "$net_conf" << 'EOF'
[Match]
Name=en* eth*

[Network]
DHCP=yes
EOF
        log "Default DHCP network config created"
    fi

    # Create agent network namespace if needed
    if command -v ip >/dev/null 2>&1; then
        if [[ ! -d /run/netns/agnos ]]; then
            ip netns add agnos 2>/dev/null || true
        fi
    fi
}

# -----------------------------------------------------------------------
# Sysctl hardening (CIS compliance)
# -----------------------------------------------------------------------
setup_sysctl() {
    log "Applying sysctl hardening..."

    if [[ -f /etc/sysctl.d/99-agnos-hardening.conf ]]; then
        sysctl -p /etc/sysctl.d/99-agnos-hardening.conf 2>/dev/null || true
    fi

    chmod 1777 /tmp 2>/dev/null || true
}

# -----------------------------------------------------------------------
# Security
# -----------------------------------------------------------------------
setup_security() {
    log "Initializing security..."

    setup_sysctl

    if [[ -x /usr/bin/agnos-setup-landlock ]]; then
        /usr/bin/agnos-setup-landlock 2>/dev/null || true
    fi
}

# -----------------------------------------------------------------------
# Kernel modules
# -----------------------------------------------------------------------
check_modules() {
    log "Checking kernel modules..."

    local required_modules=("agnos_security" "agnos_agent" "agnos_llm" "agnos_audit")

    for mod in "${required_modules[@]}"; do
        if lsmod 2>/dev/null | grep -q "^${mod}"; then
            log "Module $mod: OK"
        fi
    done
}

# -----------------------------------------------------------------------
# Audit
# -----------------------------------------------------------------------
setup_audit() {
    log "Initializing audit system..."

    mkdir -p /var/log/agnos/audit
    chmod 750 /var/log/agnos/audit
}

# -----------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------
main() {
    mkdir -p /var/log/agnos
    log "AgnosticOS System Initialization starting..."

    setup_users
    setup_directories
    setup_hostname
    setup_locale
    setup_dns
    setup_network
    setup_security
    check_modules
    setup_audit

    log "AgnosticOS System Initialization complete"
}

main "$@"
