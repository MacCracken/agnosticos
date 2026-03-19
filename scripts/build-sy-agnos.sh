#!/bin/bash
# build-sy-agnos.sh — Build sy-agnos OCI sandbox image for SecureYeoman
#
# Produces a hardened, minimal OCI image that SecureYeoman launches as an
# execution sandbox. The OS IS the sandbox: immutable rootfs, no shell,
# baked seccomp BPF, OS-level nftables default-deny.
#
# Usage:
#   ./scripts/build-sy-agnos.sh                                    # Build with defaults
#   ./scripts/build-sy-agnos.sh --agent-binary /path/to/sy-agent   # Include SY agent
#   ./scripts/build-sy-agnos.sh --network-policy /path/to/policy   # Custom network policy
#   ./scripts/build-sy-agnos.sh --node-binary /path/to/node        # Custom Node.js binary
#   ./scripts/build-sy-agnos.sh --clean                            # Clean build
#   ./scripts/build-sy-agnos.sh --help
#
# Outputs:
#   output/sy-agnos.tar                   OCI image tarball
#   output/sy-agnos.tar.sha256            SHA256 checksum
#   output/sy-agnos-rootfs.squashfs       Standalone squashfs rootfs (optional)
#
# Requirements:
#   squashfs-tools, coreutils, tar, sha256sum
#   Optional: nft (nftables), veritysetup (dm-verity for Phase 2)

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BUILD_DIR="$REPO_ROOT/build/sy-agnos"
OUTPUT_DIR="$REPO_ROOT/output"
RECIPES_DIR="$REPO_ROOT/recipes"

AGNOS_VERSION="$(cat "$REPO_ROOT/VERSION" 2>/dev/null || echo '2026.3.18')"

# Inputs (overridable via flags)
AGENT_BINARY=""
NETWORK_POLICY=""
NODE_BINARY=""
CLEAN_BUILD=false
VERBOSE=false
BUILD_SQUASHFS=true
HAS_VERITY=false
VERITY_ROOT_HASH=""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# ---------------------------------------------------------------------------
# Logging
# ---------------------------------------------------------------------------
log_info()  { echo -e "${GREEN}[INFO]${NC}  $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC}  $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_step()  { echo -e "${BLUE}[STEP]${NC}  $1"; }

# Convert canonical version (YYYY.M.D[-N]) to filename format (YYYYMMDDN)
version_to_filename() {
    local ver="$1"
    local base patch=""
    if [[ "$ver" == *-* ]]; then
        base="${ver%-*}"
        patch="${ver##*-}"
    else
        base="$ver"
    fi
    local y m d
    IFS='.' read -r y m d <<< "$base"
    printf "%s%02d%02d%s" "$y" "$m" "$d" "$patch"
}

# ---------------------------------------------------------------------------
# Usage
# ---------------------------------------------------------------------------
usage() {
    cat << 'EOF'
Usage: build-sy-agnos.sh [OPTIONS]

Build the sy-agnos hardened OCI sandbox image for SecureYeoman.

Options:
    -a, --agent-binary PATH     Path to SY agent binary to bake into image
    -n, --network-policy PATH   Path to network policy config file
    --node-binary PATH          Path to Node.js binary (default: auto-detect)
    -o, --output DIR            Output directory (default: output/)
    -c, --clean                 Remove previous build artifacts
    -v, --verbose               Enable verbose output
    --no-squashfs               Skip squashfs creation (OCI only)
    -h, --help                  Show this help message

Examples:
    build-sy-agnos.sh
    build-sy-agnos.sh --agent-binary dist/sy-agent --network-policy policy.conf
    build-sy-agnos.sh -c -v -a /path/to/sy-agent

Output:
    output/sy-agnos.tar           OCI image tarball (importable via docker/podman)
    output/sy-agnos.tar.sha256    SHA256 checksum
EOF
}

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            -a|--agent-binary)
                AGENT_BINARY="$2"
                shift 2
                ;;
            -n|--network-policy)
                NETWORK_POLICY="$2"
                shift 2
                ;;
            --node-binary)
                NODE_BINARY="$2"
                shift 2
                ;;
            -o|--output)
                OUTPUT_DIR="$2"
                shift 2
                ;;
            -c|--clean)
                CLEAN_BUILD=true
                shift
                ;;
            -v|--verbose)
                VERBOSE=true
                shift
                ;;
            --no-squashfs)
                BUILD_SQUASHFS=false
                shift
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                log_error "Unknown argument: $1"
                usage
                exit 1
                ;;
        esac
    done

    # Validate inputs
    if [[ -n "$AGENT_BINARY" ]] && [[ ! -f "$AGENT_BINARY" ]]; then
        log_error "Agent binary not found: $AGENT_BINARY"
        exit 1
    fi

    if [[ -n "$NETWORK_POLICY" ]] && [[ ! -f "$NETWORK_POLICY" ]]; then
        log_error "Network policy file not found: $NETWORK_POLICY"
        exit 1
    fi

    if [[ -n "$NODE_BINARY" ]] && [[ ! -f "$NODE_BINARY" ]]; then
        log_error "Node.js binary not found: $NODE_BINARY"
        exit 1
    fi
}

# ---------------------------------------------------------------------------
# Dependency checks
# ---------------------------------------------------------------------------
check_tool() {
    if ! command -v "$1" &>/dev/null; then
        log_error "Required tool not found: $1"
        log_error "Install it with your package manager and retry."
        exit 1
    fi
}

check_dependencies() {
    log_step "Checking build dependencies..."
    check_tool tar
    check_tool sha256sum

    if [[ "$BUILD_SQUASHFS" == true ]]; then
        check_tool mksquashfs
    fi

    # Optional: nft for validating nftables rules
    if ! command -v nft &>/dev/null; then
        log_warn "nft not found -- nftables rules will not be validated"
    fi

    # Optional: veritysetup for dm-verity (graceful fallback if not present)
    if ! command -v veritysetup &>/dev/null; then
        log_warn "veritysetup not found -- dm-verity will be skipped"
        log_warn "Install cryptsetup for production images with verified rootfs"
        HAS_VERITY=false
    else
        HAS_VERITY=true
    fi
}

# ---------------------------------------------------------------------------
# Setup
# ---------------------------------------------------------------------------
setup_build_dirs() {
    log_step "Setting up build directories..."

    if [[ "$CLEAN_BUILD" == true ]] && [[ -d "$BUILD_DIR" ]]; then
        log_info "Cleaning previous sy-agnos build..."
        rm -rf "$BUILD_DIR"
    fi

    mkdir -p "$BUILD_DIR"/{rootfs,staging,oci}
    mkdir -p "$OUTPUT_DIR"
}

# ---------------------------------------------------------------------------
# Stage 1: Build edge base rootfs
# ---------------------------------------------------------------------------
build_base_rootfs() {
    log_step "Stage 1: Building edge base rootfs..."

    local rootfs="$BUILD_DIR/rootfs"

    # Create FHS directory structure (minimal -- no /home, no /srv, no /media)
    mkdir -p "$rootfs"/{bin,sbin,lib,lib64,dev,proc,sys,tmp,run}
    mkdir -p "$rootfs"/usr/{bin,sbin,lib,lib64}
    mkdir -p "$rootfs"/var/{lib,log,cache,run,tmp}
    mkdir -p "$rootfs"/etc/{agnos,ssl/certs,nftables,seccomp,sy-agnos}
    mkdir -p "$rootfs"/run/agnos/agents
    mkdir -p "$rootfs"/var/log/{agnos,sy-agent}
    mkdir -p "$rootfs"/opt/sy-agent/{bin,lib}
    mkdir -p "$rootfs"/var/lib/sy-agent
    chmod 1777 "$rootfs"/tmp
    chmod 1777 "$rootfs"/var/tmp

    # Write version and sy-agnos marker
    echo "$AGNOS_VERSION" > "$rootfs/etc/agnos/version"

    # OS release
    cat > "$rootfs/etc/os-release" << EOF
NAME="AGNOS sy-agnos"
VERSION="$AGNOS_VERSION"
ID=agnos
ID_LIKE=agnos
VERSION_ID=$AGNOS_VERSION
PRETTY_NAME="AGNOS sy-agnos Sandbox $AGNOS_VERSION"
HOME_URL="https://github.com/maccracken/agnosticos"
VARIANT="sy-agnos"
VARIANT_ID=sy-agnos
EOF

    # Minimal /etc files
    echo "sy-agnos" > "$rootfs/etc/hostname"

    cat > "$rootfs/etc/hosts" << 'EOF'
127.0.0.1   localhost
::1         localhost
127.0.1.1   sy-agnos
EOF

    # System users -- locked accounts, no shell access
    local _r="root" _a="agnos" _s="sy-agent"
    printf '%s\n' \
        "${_r}:x:0:0:${_r}:/${_r}:/usr/sbin/nologin" \
        "nobody:x:65534:65534:nobody:/nonexistent:/usr/sbin/nologin" \
        "${_a}:x:999:999:AGNOS Runtime:/var/lib/${_a}:/usr/sbin/nologin" \
        "${_s}:x:1000:1000:SY Agent:/var/lib/${_s}:/usr/sbin/nologin" \
        > "$rootfs/etc/passwd"

    printf '%s\n' \
        "${_r}:x:0:" \
        "nogroup:x:65534:" \
        "${_a}:x:999:" \
        "${_s}:x:1000:" \
        > "$rootfs/etc/group"

    local _shadow_fields=":!:1:0:99999:7:::"
    printf '%s\n' \
        "${_r}${_shadow_fields}" \
        "nobody${_shadow_fields}" \
        "${_a}${_shadow_fields}" \
        "${_s}${_shadow_fields}" \
        > "$rootfs/etc/shadow"
    chmod 640 "$rootfs/etc/shadow"

    # Minimal fstab -- read-only rootfs with tmpfs overlays
    cat > "$rootfs/etc/fstab" << 'EOF'
# sy-agnos fstab -- immutable rootfs, tmpfs writable layers
/dev/root      /                squashfs   ro                           0      1
tmpfs          /tmp             tmpfs      nosuid,nodev,noexec,size=64M 0      0
tmpfs          /run             tmpfs      nosuid,nodev,size=32M        0      0
tmpfs          /var/log         tmpfs      nosuid,nodev,noexec,size=16M 0      0
tmpfs          /var/lib/sy-agent tmpfs     nosuid,nodev,size=128M       0      0
EOF

    # Copy system libraries if available (from host for now -- edge base recipes in production)
    if [[ -f /lib/x86_64-linux-gnu/libc.so.6 ]]; then
        cp /lib/x86_64-linux-gnu/libc.so.6 "$rootfs/lib64/" 2>/dev/null || true
        cp /lib/x86_64-linux-gnu/libpthread.so.0 "$rootfs/lib64/" 2>/dev/null || true
        cp /lib/x86_64-linux-gnu/libdl.so.2 "$rootfs/lib64/" 2>/dev/null || true
        cp /lib/x86_64-linux-gnu/libm.so.6 "$rootfs/lib64/" 2>/dev/null || true
        cp /lib64/ld-linux-x86-64.so.2 "$rootfs/lib64/" 2>/dev/null || true
    elif [[ -f /usr/lib/libc.so.6 ]]; then
        # Arch/musl-style lib layout
        cp /usr/lib/libc.so.6 "$rootfs/lib64/" 2>/dev/null || true
        cp /usr/lib/ld-linux-x86-64.so.2 "$rootfs/lib64/" 2>/dev/null || true
    fi

    # Copy CA certificates
    if [[ -d /etc/ssl/certs ]]; then
        cp -r /etc/ssl/certs/* "$rootfs/etc/ssl/certs/" 2>/dev/null || true
    fi

    log_info "  Base rootfs constructed"
}

# ---------------------------------------------------------------------------
# Stage 2: Strip -- remove shells, debug tools, SSH, package managers
# ---------------------------------------------------------------------------
strip_rootfs() {
    log_step "Stage 2: Stripping rootfs (removing shells, debug tools, SSH)..."

    local rootfs="$BUILD_DIR/rootfs"

    # Ensure NO shells exist
    for shell in sh bash ash dash zsh csh tcsh fish; do
        rm -f "$rootfs/bin/$shell" "$rootfs/usr/bin/$shell" "$rootfs/sbin/$shell"
    done

    # Remove package managers
    for pm in apt apt-get dpkg rpm apk pacman yum dnf ark pip pip3 npm yarn; do
        rm -f "$rootfs/usr/bin/$pm" "$rootfs/usr/sbin/$pm" "$rootfs/bin/$pm"
    done
    rm -rf "$rootfs/var/lib/apt" "$rootfs/var/lib/dpkg" "$rootfs/var/cache/apt"

    # Remove SSH
    for ssh_bin in ssh sshd ssh-agent ssh-keygen sftp scp dropbear dropbearkey; do
        rm -f "$rootfs/usr/bin/$ssh_bin" "$rootfs/usr/sbin/$ssh_bin"
    done
    rm -rf "$rootfs/etc/ssh" "$rootfs/etc/dropbear"

    # Remove debug/introspection tools
    for tool in gdb strace ltrace tcpdump nmap nc ncat netcat \
                curl wget vi vim nano less more \
                find grep awk sed ps top htop \
                mount umount lsof ss ip \
                python python3 perl ruby; do
        rm -f "$rootfs/usr/bin/$tool" "$rootfs/bin/$tool"
    done

    # Remove compiler toolchain
    for cc in gcc g++ cc c++ ld as ar make cmake; do
        rm -f "$rootfs/usr/bin/$cc"
    done
    rm -rf "$rootfs/usr/lib/gcc" "$rootfs/usr/include"

    # Remove documentation
    rm -rf "$rootfs/usr/share/man" "$rootfs/usr/share/doc" "$rootfs/usr/share/info"
    rm -rf "$rootfs/usr/share/locale"

    # Remove cron/at
    rm -f "$rootfs/usr/bin/crontab" "$rootfs/usr/sbin/cron" "$rootfs/usr/bin/at"

    log_info "  Rootfs stripped -- no shells, no debug tools, no SSH"
}

# ---------------------------------------------------------------------------
# Stage 3: Install Node.js runtime + SY agent binary
# ---------------------------------------------------------------------------
install_agent() {
    log_step "Stage 3: Installing Node.js runtime + SY agent..."

    local rootfs="$BUILD_DIR/rootfs"

    # Install Node.js binary
    if [[ -n "$NODE_BINARY" ]]; then
        cp "$NODE_BINARY" "$rootfs/opt/sy-agent/bin/node"
        chmod 755 "$rootfs/opt/sy-agent/bin/node"
        log_info "  Node.js binary installed from: $NODE_BINARY"
    else
        # Try to find node on the host
        local host_node
        host_node="$(command -v node 2>/dev/null || true)"
        if [[ -n "$host_node" ]] && [[ -f "$host_node" ]]; then
            cp "$host_node" "$rootfs/opt/sy-agent/bin/node"
            chmod 755 "$rootfs/opt/sy-agent/bin/node"
            log_info "  Node.js binary installed from host: $host_node"
        else
            log_warn "  Node.js binary not found -- image will need Node.js added"
            log_warn "  Use --node-binary /path/to/node to provide it"
        fi
    fi

    # Install SY agent binary
    if [[ -n "$AGENT_BINARY" ]]; then
        cp "$AGENT_BINARY" "$rootfs/opt/sy-agent/bin/sy-agent"
        chmod 755 "$rootfs/opt/sy-agent/bin/sy-agent"
        log_info "  SY agent binary installed from: $AGENT_BINARY"
    else
        # Create placeholder
        cat > "$rootfs/opt/sy-agent/bin/sy-agent" << 'PLACEHOLDER'
#!/usr/bin/env node
console.error("ERROR: sy-agent binary was not baked into this image.");
console.error("Rebuild with: ./scripts/build-sy-agnos.sh --agent-binary /path/to/sy-agent");
process.exit(1);
PLACEHOLDER
        chmod 755 "$rootfs/opt/sy-agent/bin/sy-agent"
        log_warn "  SY agent binary not provided -- placeholder installed"
        log_warn "  Use --agent-binary /path/to/sy-agent to bake it in"
    fi

    # Create default agent config
    cat > "$rootfs/etc/sy-agnos/agent.toml" << 'AGENTCONF'
# sy-agnos agent configuration
# This is the default config baked into the sandbox image.
# SY orchestrator can override via environment variables.

[agent]
# Mode: sandbox execution only
mode = "sandbox"
# Health check port
health_port = 8099

[security]
# Seccomp BPF profile (baked into image)
seccomp_profile = "/etc/seccomp/sy-agent.json"
# No new privileges after start
no_new_privileges = true
# Read-only rootfs
readonly_rootfs = true

[resources]
# Default resource limits (overridable by SY orchestrator)
max_memory_mb = 512
max_cpu_percent = 80
max_open_files = 1024
max_disk_mb = 128

[logging]
format = "json"
level = "info"
output = "/var/log/sy-agent/agent.log"
AGENTCONF

    log_info "  Agent installation complete"
}

# ---------------------------------------------------------------------------
# Stage 4: Bake seccomp BPF filter
# ---------------------------------------------------------------------------
bake_seccomp() {
    log_step "Stage 4: Baking seccomp BPF filter..."

    local rootfs="$BUILD_DIR/rootfs"

    mkdir -p "$rootfs/etc/seccomp"

    # The seccomp profile is defined in sy-agnos-rootfs.toml recipe.
    # Here we write it directly into the rootfs.
    cat > "$rootfs/etc/seccomp/sy-agent.json" << 'SECCOMP'
{
    "defaultAction": "SCMP_ACT_ERRNO",
    "defaultErrnoRet": 1,
    "architectures": ["SCMP_ARCH_X86_64", "SCMP_ARCH_AARCH64"],
    "syscalls": [
        {
            "names": [
                "read", "write", "close", "fstat", "lseek",
                "mmap", "mprotect", "munmap", "brk",
                "rt_sigaction", "rt_sigprocmask", "rt_sigreturn",
                "ioctl", "pread64", "pwrite64",
                "readv", "writev",
                "pipe", "pipe2",
                "select", "poll", "ppoll",
                "epoll_create1", "epoll_ctl", "epoll_wait", "epoll_pwait",
                "dup", "dup2", "dup3",
                "socket", "connect", "sendto", "recvfrom",
                "sendmsg", "recvmsg",
                "bind", "listen", "accept", "accept4",
                "getsockname", "getpeername", "getsockopt", "setsockopt",
                "clock_gettime", "clock_getres", "clock_nanosleep",
                "nanosleep",
                "getpid", "getppid", "getuid", "geteuid",
                "getgid", "getegid", "gettid",
                "openat", "newfstatat", "statx",
                "futex", "set_robust_list", "get_robust_list",
                "sigaltstack",
                "getrandom",
                "memfd_create",
                "exit", "exit_group",
                "tgkill",
                "fcntl",
                "getcwd",
                "getdents64",
                "sched_getaffinity", "sched_yield",
                "mremap",
                "prctl"
            ],
            "action": "SCMP_ACT_ALLOW"
        },
        {
            "_comment": "Block dangerous syscalls -- kill the process immediately",
            "names": [
                "execve", "execveat",
                "fork", "vfork", "clone3",
                "ptrace",
                "mount", "umount2", "pivot_root",
                "chroot",
                "reboot",
                "init_module", "finit_module", "delete_module",
                "kexec_load", "kexec_file_load",
                "setns", "unshare",
                "personality"
            ],
            "action": "SCMP_ACT_KILL_PROCESS"
        }
    ]
}
SECCOMP

    log_info "  Seccomp BPF profile baked into rootfs"
}

# ---------------------------------------------------------------------------
# Stage 5: Bake nftables rules + network policy
# ---------------------------------------------------------------------------
bake_nftables() {
    log_step "Stage 5: Baking nftables firewall rules..."

    local rootfs="$BUILD_DIR/rootfs"

    mkdir -p "$rootfs/etc/nftables"
    mkdir -p "$rootfs/etc/sy-agnos"

    # Write the default-deny nftables ruleset
    cat > "$rootfs/etc/nftables/sy-agnos.nft" << 'NFT'
#!/usr/sbin/nft -f
flush ruleset

table inet sy_agnos_filter {
    set dns_resolvers {
        type ipv4_addr
        flags interval
    }

    set egress_allowlist {
        type ipv4_addr . inet_service
        flags interval
    }

    set egress_hosts {
        type ipv4_addr
        flags interval
    }

    chain input {
        type filter hook input priority 0; policy drop;
        ct state established,related accept
        iif lo accept
        tcp dport 8099 accept
    }

    chain forward {
        type filter hook forward priority 0; policy drop;
    }

    chain output {
        type filter hook output priority 0; policy drop;
        oif lo accept
        ct state established,related accept
        ip daddr @dns_resolvers udp dport 53 accept
        ip daddr @dns_resolvers tcp dport 853 accept
        ip daddr . tcp dport @egress_allowlist accept
        ip daddr @egress_hosts tcp dport 443 accept
    }
}
NFT

    # Install network policy
    if [[ -n "$NETWORK_POLICY" ]]; then
        cp "$NETWORK_POLICY" "$rootfs/etc/sy-agnos/network-policy.conf"
        log_info "  Custom network policy installed from: $NETWORK_POLICY"
    else
        # Write empty default policy (default-deny all egress)
        cat > "$rootfs/etc/sy-agnos/network-policy.conf" << 'POLICY'
# sy-agnos Network Policy -- default-deny
# No DNS resolvers, no egress hosts, no outbound traffic.
# Override at build time with: --network-policy /path/to/policy.conf
DNS_RESOLVERS=""
EGRESS_HOSTS=""
EGRESS_RULES=""
POLICY
        log_info "  Default network policy installed (deny-all egress)"
    fi

    # Write the policy loader script
    cat > "$rootfs/etc/nftables/sy-agnos-policy-loader.sh" << 'LOADER'
#!/bin/sh
POLICY_FILE="/etc/sy-agnos/network-policy.conf"
[ ! -f "$POLICY_FILE" ] && exit 0
. "$POLICY_FILE"
if [ -n "$DNS_RESOLVERS" ]; then
    for resolver in $DNS_RESOLVERS; do
        nft add element inet sy_agnos_filter dns_resolvers "{ $resolver }" 2>/dev/null || true
    done
fi
if [ -n "$EGRESS_HOSTS" ]; then
    for host in $EGRESS_HOSTS; do
        nft add element inet sy_agnos_filter egress_hosts "{ $host }" 2>/dev/null || true
    done
fi
if [ -n "$EGRESS_RULES" ]; then
    for rule in $EGRESS_RULES; do
        IP="${rule%%:*}"
        PORT="${rule##*:}"
        nft add element inet sy_agnos_filter egress_allowlist "{ $IP . $PORT }" 2>/dev/null || true
    done
fi
LOADER
    chmod 755 "$rootfs/etc/nftables/sy-agnos-policy-loader.sh"

    log_info "  nftables default-deny ruleset baked"
}

# ---------------------------------------------------------------------------
# Stage 6: Write init script
# ---------------------------------------------------------------------------
write_init() {
    log_step "Stage 6: Writing init script (3-process tree)..."

    local rootfs="$BUILD_DIR/rootfs"

    # PID 1 init -- argonaut-style, 3-process tree only
    cat > "$rootfs/sbin/init" << 'INITSCRIPT'
#!/bin/sh
# sy-agnos init -- minimal PID 1
# 3-process tree: init -> sy-agent -> health-check
# No TTY, no login, no getty, no SSH.
set -e

# Mount virtual filesystems
mount -t proc     none /proc     2>/dev/null || true
mount -t sysfs    none /sys      2>/dev/null || true
mount -t devtmpfs none /dev      2>/dev/null || true
mount -t tmpfs    none /tmp      2>/dev/null || true
mount -t tmpfs    none /run      2>/dev/null || true
mount -t tmpfs    none /var/log  2>/dev/null || true
mount -t tmpfs -o size=128M none /var/lib/sy-agent 2>/dev/null || true

# Create runtime directories
mkdir -p /run/agnos/agents
mkdir -p /var/log/agnos
mkdir -p /var/log/sy-agent

# Set hostname
hostname sy-agnos

# Verify dm-verity rootfs integrity if root hash is present
VERITY_ROOT_HASH=""
if [ -f /etc/agnos/verity-root-hash ]; then
    VERITY_ROOT_HASH="$(cat /etc/agnos/verity-root-hash)"
fi
if [ -n "$VERITY_ROOT_HASH" ] && command -v veritysetup >/dev/null 2>&1; then
    echo "sy-agnos: verifying rootfs integrity (dm-verity)..."
    if [ -f /etc/agnos/rootfs.hashtree ]; then
        veritysetup verify /dev/root /etc/agnos/rootfs.hashtree "$VERITY_ROOT_HASH" 2>/dev/null
        if [ $? -ne 0 ]; then
            echo "sy-agnos: FATAL -- rootfs dm-verity verification FAILED"
            echo "sy-agnos: refusing to start agent (exit 78 EX_CONFIG)"
            exit 78
        fi
        echo "sy-agnos: rootfs integrity verified"
    fi
fi

# Load nftables firewall rules
if [ -f /etc/nftables/sy-agnos.nft ] && command -v nft >/dev/null 2>&1; then
    nft -f /etc/nftables/sy-agnos.nft
    # Load network policy into nftables sets
    if [ -x /etc/nftables/sy-agnos-policy-loader.sh ]; then
        /etc/nftables/sy-agnos-policy-loader.sh
    fi
fi

# Start health-check in background (port 8099)
/opt/sy-agent/bin/health-check --port 8099 --check-pid-of sy-agent &

# Exec the SY agent (becomes child of PID 1)
exec /opt/sy-agent/bin/sy-agent --config /etc/sy-agnos/agent.toml
INITSCRIPT
    chmod 755 "$rootfs/sbin/init"

    # Write health-check script
    cat > "$rootfs/opt/sy-agent/bin/health-check" << 'HEALTHCHECK'
#!/bin/sh
# sy-agnos health check -- responds on port 8099
PORT="8099"
CHECK_PID_OF="sy-agent"
while [ $# -gt 0 ]; do
    case "$1" in
        --port) PORT="$2"; shift 2 ;;
        --check-pid-of) CHECK_PID_OF="$2"; shift 2 ;;
        *) shift ;;
    esac
done
while true; do
    if pgrep -x "$CHECK_PID_OF" >/dev/null 2>&1; then
        STATUS="healthy"; CODE="200"
    else
        STATUS="unhealthy"; CODE="503"
    fi
    STRENGTH=80
    if [ -f /etc/agnos/verity-root-hash ]; then STRENGTH=85; fi
    BODY="{\"status\":\"$STATUS\",\"sandbox\":\"sy-agnos\",\"strength\":$STRENGTH}"
    if command -v nc >/dev/null 2>&1; then
        printf "HTTP/1.1 %s OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: %d\r\n\r\n%s" \
            "$CODE" "${#BODY}" "$BODY" | nc -l -p "$PORT" -q 1 2>/dev/null || true
    else
        sleep 10
    fi
done
HEALTHCHECK
    chmod 755 "$rootfs/opt/sy-agent/bin/health-check"

    log_info "  Init script written (3-process tree)"
}

# ---------------------------------------------------------------------------
# Stage 7: Write release metadata
# ---------------------------------------------------------------------------
write_metadata() {
    log_step "Stage 7: Writing release metadata..."

    local rootfs="$BUILD_DIR/rootfs"

    # dm-verity metadata is updated after Stage 8.5 if verity is available
    local verity_enabled="false"
    local strength=80
    local features='["immutable-rootfs", "seccomp-bpf", "nftables-deny", "no-shell", "no-ssh"]'

    if [[ "$HAS_VERITY" == true ]]; then
        verity_enabled="true"
        strength=85
        features='["immutable-rootfs", "seccomp-bpf", "nftables-deny", "no-shell", "no-ssh", "dm-verity"]'
    fi

    cat > "$rootfs/etc/sy-agnos-release" << EOF
{
    "version": "$AGNOS_VERSION",
    "hardening": "$([ "$HAS_VERITY" == true ] && echo "verified" || echo "minimal")",
    "dmverity": $verity_enabled,
    "tpm_measured": false,
    "strength": $strength,
    "features": $features,
    "build_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "agent_included": $([ -n "$AGENT_BINARY" ] && echo "true" || echo "false")
}
EOF

    log_info "  Release metadata written (strength=$strength, dmverity=$verity_enabled)"
}

# ---------------------------------------------------------------------------
# Stage 8: Create squashfs (optional)
# ---------------------------------------------------------------------------
create_squashfs() {
    if [[ "$BUILD_SQUASHFS" != true ]]; then
        log_info "Skipping squashfs (--no-squashfs)"
        return
    fi

    log_step "Stage 8: Creating squashfs rootfs..."

    local squashfs_out="$BUILD_DIR/staging/sy-agnos-rootfs.squashfs"

    mksquashfs "$BUILD_DIR/rootfs" "$squashfs_out" \
        -comp zstd -Xcompression-level 19 \
        -noappend \
        -no-xattrs \
        -all-root \
        ${VERBOSE:+-info} \
        2>/dev/null || {
        log_error "mksquashfs failed -- is squashfs-tools installed?"
        exit 1
    }

    cp "$squashfs_out" "$OUTPUT_DIR/sy-agnos-rootfs.squashfs"
    log_info "  Squashfs rootfs: $(du -h "$squashfs_out" | cut -f1)"
}

# ---------------------------------------------------------------------------
# Stage 8.5: Generate dm-verity hash tree (optional)
# ---------------------------------------------------------------------------
generate_verity() {
    if [[ "$BUILD_SQUASHFS" != true ]]; then
        return
    fi

    if [[ "$HAS_VERITY" != true ]]; then
        log_info "Skipping dm-verity (veritysetup not available)"
        return
    fi

    log_step "Stage 8.5: Generating dm-verity hash tree..."

    local squashfs_out="$BUILD_DIR/staging/sy-agnos-rootfs.squashfs"

    if [[ ! -f "$squashfs_out" ]]; then
        log_warn "Squashfs not found -- skipping dm-verity"
        HAS_VERITY=false
        return
    fi

    veritysetup format "$squashfs_out" "$squashfs_out.hashtree" \
        > "$BUILD_DIR/staging/verity-info.txt" 2>&1 || {
        log_warn "veritysetup format failed -- continuing without dm-verity"
        HAS_VERITY=false
        return
    }

    # Extract root hash
    VERITY_ROOT_HASH="$(grep 'Root hash:' "$BUILD_DIR/staging/verity-info.txt" | awk '{print $NF}')"

    if [[ -z "$VERITY_ROOT_HASH" ]]; then
        log_warn "Could not extract verity root hash -- continuing without dm-verity"
        HAS_VERITY=false
        return
    fi

    log_info "  dm-verity root hash: $VERITY_ROOT_HASH"

    # Save root hash for fleet management / verification
    echo "$VERITY_ROOT_HASH" > "$BUILD_DIR/staging/verity-root-hash.txt"

    # Also save the root hash into the rootfs so the init script can verify at boot
    mkdir -p "$BUILD_DIR/rootfs/etc/agnos"
    echo "$VERITY_ROOT_HASH" > "$BUILD_DIR/rootfs/etc/agnos/verity-root-hash"

    # Copy hash tree to output alongside squashfs
    cp "$squashfs_out.hashtree" "$OUTPUT_DIR/sy-agnos-rootfs.squashfs.hashtree"
    cp "$BUILD_DIR/staging/verity-root-hash.txt" "$OUTPUT_DIR/sy-agnos-verity-root-hash.txt"

    log_info "  Hash tree: $(du -h "$squashfs_out.hashtree" | cut -f1)"
    log_info "  dm-verity artifacts saved to $OUTPUT_DIR/"
}

# ---------------------------------------------------------------------------
# Stage 9: Package as OCI image tarball
# ---------------------------------------------------------------------------
create_oci_image() {
    log_step "Stage 9: Creating OCI image tarball..."

    local oci_dir="$BUILD_DIR/oci"
    local rootfs_tar="$oci_dir/rootfs.tar"
    local file_version
    file_version="$(version_to_filename "$AGNOS_VERSION")"

    # Create rootfs layer tarball
    (cd "$BUILD_DIR/rootfs" && tar cf "$rootfs_tar" .)

    # Include verity hash tree as a separate layer if available
    local verity_tar=""
    if [[ "$HAS_VERITY" == true ]] && [[ -f "$BUILD_DIR/staging/sy-agnos-rootfs.squashfs.hashtree" ]]; then
        verity_tar="$oci_dir/verity.tar"
        local verity_staging="$BUILD_DIR/staging/verity-layer"
        mkdir -p "$verity_staging/etc/agnos"
        cp "$BUILD_DIR/staging/sy-agnos-rootfs.squashfs.hashtree" "$verity_staging/etc/agnos/rootfs.hashtree"
        cp "$BUILD_DIR/staging/verity-root-hash.txt" "$verity_staging/etc/agnos/verity-root-hash"
        (cd "$verity_staging" && tar cf "$verity_tar" .)
    fi

    local rootfs_sha256
    rootfs_sha256="$(sha256sum "$rootfs_tar" | cut -d' ' -f1)"
    local rootfs_size
    rootfs_size="$(stat -c%s "$rootfs_tar")"

    local verity_sha256="" verity_size=""
    local verity_diff_id=""
    if [[ -n "$verity_tar" ]] && [[ -f "$verity_tar" ]]; then
        verity_sha256="$(sha256sum "$verity_tar" | cut -d' ' -f1)"
        verity_size="$(stat -c%s "$verity_tar")"
        verity_diff_id=", \"sha256:$verity_sha256\""
    fi

    local sandbox_strength=80
    local verity_label="false"
    if [[ "$HAS_VERITY" == true ]]; then
        sandbox_strength=85
        verity_label="true"
    fi

    # Create OCI image config
    cat > "$oci_dir/config.json" << EOF
{
    "created": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "architecture": "amd64",
    "os": "linux",
    "config": {
        "Env": [
            "AGNOS_EDGE_MODE=1",
            "AGNOS_READONLY_ROOTFS=1",
            "AGNOS_LOG_FORMAT=json",
            "NODE_ENV=production"
        ],
        "Cmd": ["/sbin/init"],
        "ExposedPorts": {
            "8099/tcp": {}
        },
        "Labels": {
            "org.opencontainers.image.title": "sy-agnos",
            "org.opencontainers.image.description": "AGNOS sy-agnos sandbox for SecureYeoman",
            "org.opencontainers.image.source": "https://github.com/maccracken/agnosticos",
            "org.opencontainers.image.version": "$AGNOS_VERSION",
            "org.opencontainers.image.licenses": "GPL-3.0",
            "com.secureyeoman.sandbox.strength": "$sandbox_strength",
            "com.secureyeoman.sandbox.dmverity": "$verity_label"
        }
    },
    "rootfs": {
        "type": "layers",
        "diff_ids": ["sha256:$rootfs_sha256"$verity_diff_id]
    }
}
EOF

    local config_sha256
    config_sha256="$(sha256sum "$oci_dir/config.json" | cut -d' ' -f1)"
    local config_size
    config_size="$(stat -c%s "$oci_dir/config.json")"

    # Rename files to content-addressable names
    cp "$rootfs_tar" "$oci_dir/$rootfs_sha256.tar"
    cp "$oci_dir/config.json" "$oci_dir/$config_sha256.json"

    # Build layers list for manifest
    local layers_json="\"$rootfs_sha256.tar\""
    local tar_files=("$rootfs_sha256.tar")

    if [[ -n "$verity_sha256" ]]; then
        cp "$verity_tar" "$oci_dir/$verity_sha256.tar"
        layers_json="$layers_json, \"$verity_sha256.tar\""
        tar_files+=("$verity_sha256.tar")
    fi

    # Create OCI manifest
    cat > "$oci_dir/manifest.json" << EOF
[{
    "Config": "$config_sha256.json",
    "RepoTags": ["sy-agnos:$AGNOS_VERSION", "sy-agnos:latest"],
    "Layers": [$layers_json]
}]
EOF

    # Package as Docker-compatible image tarball
    local oci_out="$OUTPUT_DIR/sy-agnos.tar"
    (cd "$oci_dir" && tar cf "$oci_out" manifest.json "$config_sha256.json" "${tar_files[@]}")

    # Checksum
    sha256sum "$oci_out" > "$oci_out.sha256"

    log_info "  OCI image created: $oci_out"
    log_info "  Size: $(du -h "$oci_out" | cut -f1)"
    log_info "  SHA256: $(cut -d' ' -f1 < "$oci_out.sha256")"
}

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
print_summary() {
    echo ""
    log_info "=========================================="
    log_info "  sy-agnos Sandbox Image Build Complete"
    log_info "=========================================="
    local strength=80
    local phase="Phase 1 — minimal"
    if [[ "$HAS_VERITY" == true ]]; then
        strength=85
        phase="Phase 2 — dm-verity"
    fi
    log_info "  Version:        $AGNOS_VERSION"
    log_info "  Strength:       $strength ($phase)"
    log_info "  dm-verity:      $HAS_VERITY"
    log_info "  Agent included: $([ -n "$AGENT_BINARY" ] && echo "yes" || echo "no (placeholder)")"
    log_info "  Network policy: $([ -n "$NETWORK_POLICY" ] && echo "$NETWORK_POLICY" || echo "default-deny")"
    log_info "  Output:         $OUTPUT_DIR/"
    echo ""
    log_info "Outputs:"
    log_info "  $OUTPUT_DIR/sy-agnos.tar               OCI image (docker load)"
    [[ "$BUILD_SQUASHFS" == true ]] && \
    log_info "  $OUTPUT_DIR/sy-agnos-rootfs.squashfs    Standalone squashfs"
    [[ "$HAS_VERITY" == true ]] && \
    log_info "  $OUTPUT_DIR/sy-agnos-rootfs.squashfs.hashtree  dm-verity hash tree" && \
    log_info "  $OUTPUT_DIR/sy-agnos-verity-root-hash.txt      Root hash"
    echo ""
    log_info "Load into Docker/Podman:"
    log_info "  docker load -i $OUTPUT_DIR/sy-agnos.tar"
    log_info "  docker run --rm -p 8099:8099 sy-agnos:$AGNOS_VERSION"
    echo ""
    log_info "Security features:"
    log_info "  - Immutable rootfs (squashfs, read-only)"
    log_info "  - No shells (/bin/sh, bash, etc. removed)"
    log_info "  - No SSH daemon"
    log_info "  - No package managers"
    log_info "  - No debug tools (gdb, strace, tcpdump, etc.)"
    log_info "  - Seccomp BPF: allowlist-only syscalls, KILL on execve/fork/ptrace"
    log_info "  - nftables: default-deny egress, health only on 8099"
    if [[ "$HAS_VERITY" == true ]]; then
    log_info "  - dm-verity: rootfs integrity verified at boot (hash: ${VERITY_ROOT_HASH:0:16}...)"
    fi
    echo ""
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
    parse_args "$@"

    log_info "sy-agnos Sandbox Image Builder"
    log_info "  Version: $AGNOS_VERSION"
    echo ""

    check_dependencies
    setup_build_dirs
    build_base_rootfs
    strip_rootfs
    install_agent
    bake_seccomp
    bake_nftables
    write_init
    create_squashfs
    generate_verity
    write_metadata
    create_oci_image

    print_summary
}

main "$@"
