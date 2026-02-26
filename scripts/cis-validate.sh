#!/bin/bash
#
# CIS Benchmark Validation Script for AGNOS
# Validates system configuration against CIS Linux Benchmark controls
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Counters
PASS=0
FAIL=0
SKIP=0
TOTAL=0

# Output format
JSON_OUTPUT=false
REPORT_FILE=""
CATEGORY=""
VERBOSE=false

# Logging functions
log_pass() {
    echo -e "${GREEN}[PASS]${NC} $1"
    ((PASS++))
    ((TOTAL++))
}

log_fail() {
    echo -e "${RED}[FAIL]${NC} $1"
    ((FAIL++))
    ((TOTAL++))
}

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
    ((SKIP++))
    ((TOTAL++))
}

# Check functions
check_cmd_exists() {
    command -v "$1" &> /dev/null
}

check_file_exists() {
    [[ -f "$1" ]]
}

check_file_permission() {
    local file="$1"
    local expected_perm="$2"
    if [[ -f "$file" ]]; then
        local actual_perm=$(stat -c "%a" "$file" 2>/dev/null || stat -f "%Lp" "$file" 2>/dev/null)
        [[ "$actual_perm" == "$expected_perm" ]]
    else
        return 1
    fi
}

check_kernel_config() {
    local config="$1"
    local expected="$2"
    if [[ -f /boot/config-$(uname -r) ]]; then
        grep -q "^${config}=${expected}" /boot/config-$(uname -r) 2>/dev/null
    elif [[ -f /proc/config.gz ]]; then
        zgrep -q "^${config}=${expected}" /proc/config.gz 2>/dev/null
    else
        return 1
    fi
}

check_sysctl() {
    local param="$1"
    local expected="$2"
    local actual=$(sysctl -n "$param" 2>/dev/null || echo "")
    [[ "$actual" == "$expected" ]]
}

# CIS 1.x - Filesystem Configuration
check_filesystem_config() {
    log_info "Checking CIS 1.x - Filesystem Configuration"
    
    # 1.1.1 - Disable unused filesystems
    if check_kernel_config "CONFIG_TMPFS_XATTR" "n"; then
        log_pass "1.1.1 - TMPFS xattr disabled"
    else
        log_fail "1.1.1 - TMPFS xattr not disabled"
    fi
    
    # 1.1.2 - /tmp noexec
    if mount | grep -q "/tmp.*noexec"; then
        log_pass "1.1.2 - /tmp mounted with noexec"
    else
        log_fail "1.1.2 - /tmp not mounted with noexec"
    fi
    
    # 1.1.3 - /var/tmp noexec
    if mount | grep -q "/var/tmp.*noexec"; then
        log_pass "1.1.3 - /var/tmp mounted with noexec"
    else
        log_fail "1.1.3 - /var/tmp not mounted with noexec"
    fi
    
    # 1.1.4 - /dev/shm noexec
    if mount | grep -q "/dev/shm.*noexec"; then
        log_pass "1.1.4 - /dev/shm mounted with noexec"
    else
        log_fail "1.1.4 - /dev/shm not mounted with noexec"
    fi
    
    # 1.1.5 - Disable automounting
    if check_kernel_config "CONFIG_AUTOMOUNT" "n"; then
        log_pass "1.1.5 - Automounting disabled"
    else
        log_fail "1.1.5 - Automounting not disabled"
    fi
}

# CIS 2.x - Services
check_services() {
    log_info "Checking CIS 2.x - Services"
    
    # 2.1.1 - Chrony configuration
    if check_cmd_exists chronyd || check_cmd_exists chrony; then
        log_pass "2.1.1 - Chrony installed"
    else
        log_warn "2.1.1 - Chrony not installed"
    fi
    
    # 2.2.x - Check disabled services
    local services=("avahi-daemon" "cups" "dhcpd" "slapd" "nfs-server" "named" "smbd" "vsftpd" "dovecot" "snmpd")
    for svc in "${services[@]}"; do
        if systemctl is-enabled "$svc" 2>/dev/null | grep -q "disabled\|masked"; then
            log_pass "2.2.x - $svc is disabled"
        elif ! systemctl list-unit-files "$svc" 2>/dev/null | grep -q "$svc"; then
            log_pass "2.2.x - $svc not installed"
        else
            log_fail "2.2.x - $svc is enabled"
        fi
    done
}

# CIS 3.x - Network Configuration
check_network_config() {
    log_info "Checking CIS 3.x - Network Configuration"
    
    # 3.1.1 - Disable IP forwarding
    if check_sysctl "net.ipv4.ip_forward" "0"; then
        log_pass "3.1.1 - IP forwarding disabled"
    else
        log_fail "3.1.1 - IP forwarding enabled"
    fi
    
    # 3.1.2 - Disable packet redirect
    if check_sysctl "net.ipv4.conf.all.send_redirects" "0"; then
        log_pass "3.1.2 - ICMP redirects disabled"
    else
        log_fail "3.1.2 - ICMP redirects enabled"
    fi
    
    # 3.1.3 - Disable ICMP redirect acceptance
    if check_sysctl "net.ipv4.conf.all.accept_redirects" "0"; then
        log_pass "3.1.3 - ICMP redirect acceptance disabled"
    else
        log_fail "3.1.3 - ICMP redirect acceptance enabled"
    fi
    
    # 3.2.x - IPv6 settings
    if check_sysctl "net.ipv6.conf.all.accept_ra" "0"; then
        log_pass "3.2.1 - IPv6 router advertisements disabled"
    else
        log_fail "3.2.1 - IPv6 router advertisements enabled"
    fi
    
    if check_sysctl "net.ipv6.conf.all.accept_redirects" "0"; then
        log_pass "3.2.2 - IPv6 redirects disabled"
    else
        log_fail "3.2.2 - IPv6 redirects enabled"
    fi
    
    # 3.4.x - Uncommon network protocols
    local protocols=("CONFIG_MPTCP" "CONFIG_SCTP" "CONFIG_RDS" "CONFIG_TIPC")
    for proto in "${protocols[@]}"; do
        if check_kernel_config "$proto" "n"; then
            log_pass "3.4.x - $proto disabled"
        else
            log_fail "3.4.x - $proto not disabled"
        fi
    done
}

# CIS 4.x - Logging and Auditing
check_logging() {
    log_info "Checking CIS 4.x - Logging and Auditing"
    
    # 4.1.1 - auditd installed
    if check_cmd_exists auditd || check_cmd_exists auditctl; then
        log_pass "4.1.1 - auditd installed"
    else
        log_fail "4.1.1 - auditd not installed"
    fi
    
    # 4.1.2 - auditd enabled
    if systemctl is-enabled auditd 2>/dev/null | grep -q "enabled"; then
        log_pass "4.1.2 - auditd service enabled"
    else
        log_fail "4.1.2 - auditd service not enabled"
    fi
    
    # 4.1.3 - Audit flag in kernel cmdline
    if [[ -f /proc/cmdline ]] && grep -q "audit=1" /proc/cmdline; then
        log_pass "4.1.3 - Kernel auditing enabled"
    else
        log_fail "4.1.3 - Kernel auditing not enabled"
    fi
    
    # 4.2.x - rsyslog
    if check_cmd_exists rsyslogd; then
        log_pass "4.2.1 - rsyslog installed"
        if systemctl is-enabled rsyslog 2>/dev/null | grep -q "enabled"; then
            log_pass "4.2.2 - rsyslog service enabled"
        else
            log_fail "4.2.2 - rsyslog service not enabled"
        fi
    else
        log_warn "4.2.1 - rsyslog not installed"
    fi
}

# CIS 5.x - Access, Authentication and Authorization
check_authentication() {
    log_info "Checking CIS 5.x - Authentication and Authorization"
    
    # 5.1.x - PAM configuration
    if [[ -f /etc/security/pwquality.conf ]]; then
        log_pass "5.1.1 - pwquality.conf exists"
    else
        log_fail "5.1.1 - pwquality.conf not found"
    fi
    
    if [[ -f /etc/security/faillock.conf ]]; then
        log_pass "5.1.2 - faillock.conf exists"
    else
        log_fail "5.1.2 - faillock.conf not found"
    fi
    
    # 5.2.x - User accounts
    if [[ -f /etc/login.defs ]]; then
        log_pass "5.2.x - login.defs exists"
        
        # Check password aging
        if grep -q "^PASS_MAX_DAYS" /etc/login.defs; then
            log_pass "5.2.1 - Password expiration configured"
        else
            log_fail "5.2.1 - Password expiration not configured"
        fi
    else
        log_fail "5.2.x - login.defs not found"
    fi
}

# CIS 6.x - System Maintenance
check_system_maintenance() {
    log_info "Checking CIS 6.x - System Maintenance"
    
    # 6.1.x - File permissions
    if check_file_permission "/etc/passwd" "644"; then
        log_pass "6.1.2 - /etc/passwd permissions correct (644)"
    else
        log_fail "6.1.2 - /etc/passwd permissions incorrect"
    fi
    
    if check_file_permission "/etc/shadow" "0"; then
        log_pass "6.1.3 - /etc/shadow permissions correct (000)"
    else
        log_fail "6.1.3 - /etc/shadow permissions incorrect"
    fi
    
    if check_file_permission "/etc/group" "644"; then
        log_pass "6.1.4 - /etc/group permissions correct (644)"
    else
        log_fail "6.1.4 - /etc/group permissions incorrect"
    fi
    
    if check_file_permission "/etc/gshadow" "0"; then
        log_pass "6.1.5 - /etc/gshadow permissions correct (000)"
    else
        log_fail "6.1.5 - /etc/gshadow permissions incorrect"
    fi
    
    # 6.2.1 - Root is only UID 0
    local root_count=$(awk -F: '$3 == 0 {print}' /etc/passwd 2>/dev/null | wc -l)
    if [[ "$root_count" -eq 1 ]]; then
        log_pass "6.2.1 - Only root has UID 0"
    else
        log_fail "6.2.1 - Multiple users with UID 0"
    fi
}

# AGNOS-specific checks
check_agnos_specific() {
    log_info "Checking AGNOS-specific security controls"
    
    # Check Landlock support
    if check_kernel_config "CONFIG_SECURITY_LANDLOCK" "y"; then
        log_pass "AGNOS - Landlock enabled"
    else
        log_fail "AGNOS - Landlock not enabled"
    fi
    
    # Check Seccomp
    if check_kernel_config "CONFIG_SECCOMP" "y"; then
        log_pass "AGNOS - Seccomp enabled"
    else
        log_fail "AGNOS - Seccomp not enabled"
    fi
    
    # Check SELinux
    if check_kernel_config "CONFIG_SECURITY_SELINUX" "y"; then
        log_pass "AGNOS - SELinux enabled"
    else
        log_warn "AGNOS - SELinux not enabled"
    fi
    
    # Check namespaces
    if check_kernel_config "CONFIG_NAMESPACES" "y"; then
        log_pass "AGNOS - Namespaces enabled"
    else
        log_fail "AGNOS - Namespaces not enabled"
    fi
}

# Generate JSON report
generate_json_report() {
    local report_file="$1"
    cat > "$report_file" << EOF
{
    "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "version": "1.0",
    "system": "$(uname -a)",
    "results": {
        "total": $TOTAL,
        "passed": $PASS,
        "failed": $FAIL,
        "skipped": $SKIP,
        "pass_rate": $(echo "scale=1; $PASS * 100 / $TOTAL" | bc -l 2>/dev/null || echo "0")
    },
    "compliance": {
        "status": "$(if [[ $FAIL -eq 0 ]]; then echo "COMPLIANT"; else echo "NON_COMPLIANT"; fi)",
        "score": $(echo "scale=1; $PASS * 100 / $TOTAL" | bc -l 2>/dev/null || echo "0")
    }
}
EOF
    log_info "JSON report saved to: $report_file"
}

# Print summary
print_summary() {
    echo ""
    echo "========================================"
    echo "    CIS Compliance Summary"
    echo "========================================"
    echo -e "Total Checks:    ${TOTAL}"
    echo -e "${GREEN}Passed:          ${PASS}${NC}"
    echo -e "${RED}Failed:          ${FAIL}${NC}"
    echo -e "${YELLOW}Skipped:         ${SKIP}${NC}"
    echo ""
    
    local rate=$(echo "scale=1; $PASS * 100 / $TOTAL" | bc -l 2>/dev/null || echo "0")
    if [[ "$rate" == "100.0" ]] || [[ "$rate" == "100" ]]; then
        echo -e "${GREEN}Compliance Rate: ${rate}%${NC}"
        echo -e "${GREEN}Status: COMPLIANT${NC}"
    elif (( $(echo "$rate >= 80" | bc -l 2>/dev/null || echo "0") )); then
        echo -e "${YELLOW}Compliance Rate: ${rate}%${NC}"
        echo -e "${YELLOW}Status: MOSTLY COMPLIANT${NC}"
    else
        echo -e "${RED}Compliance Rate: ${rate}%${NC}"
        echo -e "${RED}Status: NON-COMPLIANT${NC}"
    fi
    echo "========================================"
}

# Usage
usage() {
    cat << EOF
Usage: $0 [OPTIONS]

Validate AGNOS system against CIS Linux Benchmarks

OPTIONS:
    -c, --category CATEGORY   Check only specific category (filesystem|network|services|authentication|system|all)
    -r, --report FILE         Generate JSON report to FILE
    -j, --json                Output results in JSON format
    -v, --verbose             Enable verbose output
    -h, --help                Show this help message

EXAMPLES:
    $0                        Run all checks
    $0 --category network     Check only network configuration
    $0 --report cis.json      Generate JSON report
    $0 --category filesystem --json

EOF
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -c|--category)
            CATEGORY="$2"
            shift 2
            ;;
        -r|--report)
            REPORT_FILE="$2"
            shift 2
            ;;
        -j|--json)
            JSON_OUTPUT=true
            shift
            ;;
        -v|--verbose)
            VERBOSE=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

# Main execution
main() {
    log_info "Starting CIS Benchmark Validation for AGNOS"
    log_info "Timestamp: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    
    # Check if running as root (some checks require root)
    if [[ $EUID -ne 0 ]]; then
        log_warn "Some checks require root privileges. Run with sudo for complete validation."
    fi
    
    # Run checks based on category
    case "${CATEGORY:-all}" in
        filesystem)
            check_filesystem_config
            ;;
        services)
            check_services
            ;;
        network)
            check_network_config
            ;;
        authentication)
            check_authentication
            ;;
        system)
            check_system_maintenance
            ;;
        all|*)
            check_filesystem_config
            check_services
            check_network_config
            check_logging
            check_authentication
            check_system_maintenance
            check_agnos_specific
            ;;
    esac
    
    # Generate report if requested
    if [[ -n "$REPORT_FILE" ]]; then
        generate_json_report "$REPORT_FILE"
    fi
    
    # Print summary (unless JSON output only)
    if [[ "$JSON_OUTPUT" == "false" ]]; then
        print_summary
    else
        generate_json_report /dev/stdout
    fi
    
    # Exit with appropriate code
    if [[ $FAIL -eq 0 ]]; then
        exit 0
    else
        exit 1
    fi
}

main "$@"
