#!/bin/bash
#
# Delta Update System for AGNOS
# Creates and applies delta updates with rollback capability
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# Configuration
UPDATE_DIR="${AGNOS_UPDATE_DIR:-/var/lib/agnos/updates}"
BACKUP_DIR="${AGNOS_BACKUP_DIR:-/var/lib/agnos/backups}"
CURRENT_VERSION="${AGNOS_VERSION:-$(cat ${PROJECT_ROOT}/VERSION 2>/dev/null || echo '0.1.0')}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_step() {
    echo -e "${BLUE}[STEP]${NC} $1"
}

# Check required tools
check_requirements() {
    local missing=()
    
    if ! command -v bsdiff &> /dev/null && ! command -v xdelta3 &> /dev/null; then
        missing+=("bsdiff or xdelta3")
    fi
    
    if ! command -v zstd &> /dev/null; then
        missing+=("zstd")
    fi
    
    if ! command -v sha256sum &> /dev/null; then
        missing+=("sha256sum")
    fi
    
    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing required tools: ${missing[*]}"
        log_info "Install with: sudo apt-get install bsdiff xdelta3 zstd"
        exit 1
    fi
}

# Calculate delta between two package versions
create_delta() {
    local old_version="$1"
    local new_version="$2"
    local old_package="$3"
    local new_package="$4"
    local output_dir="${5:-${PROJECT_ROOT}/deltas}"
    
    log_step "Creating delta update from ${old_version} to ${new_version}"
    
    mkdir -p "$output_dir"
    
    local delta_file="${output_dir}/agnos-${old_version}-to-${new_version}.delta"
    local manifest_file="${delta_file}.manifest"
    
    # Check which delta tool to use
    if command -v xdelta3 &> /dev/null; then
        log_info "Using xdelta3 for delta compression"
        xdelta3 -f -s "$old_package" "$new_package" "$delta_file"
    else
        log_info "Using bsdiff for delta compression"
        bsdiff "$old_package" "$new_package" "$delta_file"
    fi
    
    # Compress delta
    log_info "Compressing delta package"
    zstd -19 -f "$delta_file" -o "${delta_file}.zst"
    rm -f "$delta_file"
    
    # Create manifest
    cat > "$manifest_file" << EOF
{
    "from_version": "${old_version}",
    "to_version": "${new_version}",
    "delta_format": "$(command -v xdelta3 &> /dev/null && echo 'xdelta3' || echo 'bsdiff')",
    "compression": "zstd",
    "delta_size": $(stat -f%z "${delta_file}.zst" 2>/dev/null || stat -c%s "${delta_file}.zst"),
    "old_package_size": $(stat -f%z "$old_package" 2>/dev/null || stat -c%s "$old_package"),
    "new_package_size": $(stat -f%z "$new_package" 2>/dev/null || stat -c%s "$new_package"),
    "old_package_hash": "$(sha256sum "$old_package" | cut -d' ' -f1)",
    "new_package_hash": "$(sha256sum "$new_package" | cut -d' ' -f1)",
    "delta_hash": "$(sha256sum "${delta_file}.zst" | cut -d' ' -f1)",
    "created_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "requires_rollback_support": true,
    "minimum_free_space_mb": 500
}
EOF
    
    # Sign the manifest
    if [[ -f "${PROJECT_ROOT}/scripts/sign-packages.sh" ]]; then
        log_info "Signing delta manifest"
        "${PROJECT_ROOT}/scripts/sign-packages.sh" sign "$manifest_file" 2>/dev/null || true
    fi
    
    log_info "Delta created: ${delta_file}.zst"
    log_info "Manifest created: $manifest_file"
    
    # Calculate savings
    local delta_size=$(stat -f%z "${delta_file}.zst" 2>/dev/null || stat -c%s "${delta_file}.zst")
    local new_size=$(stat -f%z "$new_package" 2>/dev/null || stat -c%s "$new_package")
    local savings=$((100 - (delta_size * 100 / new_size)))
    
    log_info "Delta size: $((delta_size / 1024 / 1024)) MB"
    log_info "Full package size: $((new_size / 1024 / 1024)) MB"
    log_info "Space savings: ${savings}%"
}

# Apply a delta update
apply_delta() {
    local delta_file="$1"
    local old_package="$2"
    local output_package="$3"
    
    log_step "Applying delta update"
    
    # Verify manifest if exists
    local manifest_file="${delta_file%.zst}.manifest"
    if [[ -f "$manifest_file" ]]; then
        log_info "Verifying delta manifest"
        if ! verify_delta_manifest "$manifest_file" "$old_package"; then
            log_error "Delta manifest verification failed"
            return 1
        fi
    fi
    
    # Decompress delta
    log_info "Decompressing delta"
    local decompressed_delta="${delta_file%.zst}.decompressed"
    zstd -d -f "$delta_file" -o "$decompressed_delta"
    
    # Apply delta
    log_info "Applying delta patch"
    if command -v xdelta3 &> /dev/null; then
        xdelta3 -d -s "$old_package" "$decompressed_delta" "$output_package"
    else
        bspatch "$old_package" "$output_package" "$decompressed_delta"
    fi
    
    # Cleanup
    rm -f "$decompressed_delta"
    
    # Verify output
    if [[ -f "$manifest_file" ]]; then
        local expected_hash=$(grep '"new_package_hash"' "$manifest_file" | cut -d'"' -f4)
        local actual_hash=$(sha256sum "$output_package" | cut -d' ' -f1)
        
        if [[ "$expected_hash" != "$actual_hash" ]]; then
            log_error "Package hash verification failed!"
            log_error "Expected: $expected_hash"
            log_error "Actual: $actual_hash"
            rm -f "$output_package"
            return 1
        fi
        
        log_info "Package hash verified successfully"
    fi
    
    log_info "Delta applied successfully: $output_package"
}

# Verify delta manifest
verify_delta_manifest() {
    local manifest_file="$1"
    local old_package="$2"
    
    # Parse JSON manifest
    local old_hash=$(grep '"old_package_hash"' "$manifest_file" | cut -d'"' -f4)
    local actual_hash=$(sha256sum "$old_package" | cut -d' ' -f1)
    
    if [[ "$old_hash" != "$actual_hash" ]]; then
        log_error "Old package hash mismatch"
        return 1
    fi
    
    log_info "Delta manifest verified"
    return 0
}

# Create backup before update
create_backup() {
    local version="$1"
    local backup_name="agnos-backup-${version}-$(date +%Y%m%d-%H%M%S)"
    local backup_path="${BACKUP_DIR}/${backup_name}.tar.zst"
    
    log_step "Creating backup: $backup_name"
    
    mkdir -p "$BACKUP_DIR"
    
    # Backup critical system files
    local backup_files=()
    
    if [[ -d /usr/lib/agnos ]]; then
        backup_files+=(/usr/lib/agnos)
    fi
    
    if [[ -d /etc/agnos ]]; then
        backup_files+=(/etc/agnos)
    fi
    
    if [[ -d /var/lib/agnos ]]; then
        backup_files+=(/var/lib/agnos)
    fi
    
    if [[ ${#backup_files[@]} -gt 0 ]]; then
        tar -cf - "${backup_files[@]}" | zstd -19 -o "$backup_path"
        log_info "Backup created: $backup_path"
        echo "$backup_path"
    else
        log_warn "No system files to backup"
        return 1
    fi
}

# Rollback to previous version
rollback() {
    local backup_file="$1"
    
    log_step "Rolling back system from backup"
    
    if [[ ! -f "$backup_file" ]]; then
        # Try to find latest backup
        backup_file=$(find "$BACKUP_DIR" -name "agnos-backup-*.tar.zst" -type f | sort | tail -1)
        
        if [[ -z "$backup_file" ]]; then
            log_error "No backup file found for rollback"
            return 1
        fi
        
        log_info "Using latest backup: $backup_file"
    fi
    
    # Verify backup integrity
    log_info "Verifying backup integrity"
    if ! zstd -t "$backup_file" 2>/dev/null; then
        log_error "Backup file is corrupted"
        return 1
    fi
    
    # Stop services before rollback
    log_info "Stopping AGNOS services"
    systemctl stop agnos-agent-runtime 2>/dev/null || true
    systemctl stop agnos-llm-gateway 2>/dev/null || true
    
    # Restore from backup
    log_info "Restoring from backup"
    zstd -d -c "$backup_file" | tar -xf - -C /
    
    # Restart services
    log_info "Restarting AGNOS services"
    systemctl start agnos-agent-runtime 2>/dev/null || true
    systemctl start agnos-llm-gateway 2>/dev/null || true
    
    log_info "Rollback completed successfully"
}

# Check if update is available
check_update() {
    local current_version="${1:-$CURRENT_VERSION}"
    local update_url="${AGNOS_UPDATE_URL:-https://updates.agnos.org}"
    
    log_step "Checking for updates"
    
    # Fetch version manifest
    local manifest_url="${update_url}/versions.json"
    local temp_manifest=$(mktemp)
    
    if ! curl -fsSL "$manifest_url" -o "$temp_manifest" 2>/dev/null; then
        log_warn "Could not fetch version manifest"
        rm -f "$temp_manifest"
        return 1
    fi
    
    # Parse manifest and find latest version
    local latest_version=$(grep -o '"version": "[^"]*"' "$temp_manifest" | head -1 | cut -d'"' -f4)
    rm -f "$temp_manifest"
    
    if [[ -z "$latest_version" ]]; then
        log_warn "Could not determine latest version"
        return 1
    fi
    
    if [[ "$latest_version" != "$current_version" ]]; then
        log_info "Update available: ${current_version} -> ${latest_version}"
        echo "UPDATE_AVAILABLE"
        echo "current: $current_version"
        echo "latest: $latest_version"
        return 0
    else
        log_info "System is up to date (version $current_version)"
        return 1
    fi
}

# Download update
download_update() {
    local version="$1"
    local update_url="${AGNOS_UPDATE_URL:-https://updates.agnos.org}"
    local output_dir="${UPDATE_DIR}"
    
    log_step "Downloading update ${version}"
    
    mkdir -p "$output_dir"
    
    # Download delta or full package
    local delta_file="${output_dir}/agnos-${CURRENT_VERSION}-to-${version}.delta.zst"
    local full_package="${output_dir}/agnos-${version}.tar.gz"
    
    # Try delta first
    local delta_url="${update_url}/deltas/agnos-${CURRENT_VERSION}-to-${version}.delta.zst"
    if curl -fsSL --head "$delta_url" 2>/dev/null | grep -q "200 OK"; then
        log_info "Downloading delta update"
        curl -fSL --progress-bar "$delta_url" -o "$delta_file"
        echo "DELTA:$delta_file"
    else
        # Fall back to full package
        log_info "Delta not available, downloading full package"
        curl -fSL --progress-bar "${update_url}/packages/agnos-${version}.tar.gz" -o "$full_package"
        echo "FULL:$full_package"
    fi
}

# Install update with rollback support
install_update() {
    local package_file="$1"
    local is_delta="${2:-false}"
    
    log_step "Installing update"
    
    # Create backup first
    local backup_file=$(create_backup "$CURRENT_VERSION")
    
    if [[ $? -ne 0 ]] || [[ -z "$backup_file" ]]; then
        log_error "Failed to create backup, aborting update"
        return 1
    fi
    
    # Apply update
    if [[ "$is_delta" == "true" ]]; then
        local current_package="${UPDATE_DIR}/current-package.tar.gz"
        
        # Find current package or create placeholder
        if [[ ! -f "$current_package" ]]; then
            log_warn "Current package not found for delta application"
            return 1
        fi
        
        local new_package="${UPDATE_DIR}/new-package.tar.gz"
        if ! apply_delta "$package_file" "$current_package" "$new_package"; then
            log_error "Failed to apply delta update"
            return 1
        fi
        
        package_file="$new_package"
    fi
    
    # Verify package signature
    log_info "Verifying package signature"
    if [[ -f "${PROJECT_ROOT}/scripts/sign-packages.sh" ]]; then
        if ! "${PROJECT_ROOT}/scripts/sign-packages.sh" verify "$package_file" 2>/dev/null; then
            log_error "Package signature verification failed"
            rollback "$backup_file"
            return 1
        fi
    fi
    
    # Extract and install
    log_info "Extracting update package"
    local extract_dir=$(mktemp -d)
    tar -xzf "$package_file" -C "$extract_dir"
    
    # Run pre-install hooks
    if [[ -f "${extract_dir}/pre-install.sh" ]]; then
        log_info "Running pre-install script"
        bash "${extract_dir}/pre-install.sh" || {
            log_error "Pre-install script failed"
            rollback "$backup_file"
            rm -rf "$extract_dir"
            return 1
        }
    fi
    
    # Install files
    log_info "Installing update files"
    cp -r "${extract_dir}/"* / 2>/dev/null || true
    
    # Run post-install hooks
    if [[ -f "${extract_dir}/post-install.sh" ]]; then
        log_info "Running post-install script"
        bash "${extract_dir}/post-install.sh"
    fi
    
    # Cleanup
    rm -rf "$extract_dir"
    
    log_info "Update installed successfully"
    log_info "Backup available at: $backup_file"
    log_info "Run 'agnos-update rollback' if issues occur"
}

# Clean old backups and updates
cleanup() {
    local keep_count="${1:-5}"
    
    log_step "Cleaning up old backups and updates"
    
    # Keep only last N backups
    if [[ -d "$BACKUP_DIR" ]]; then
        local backup_count=$(find "$BACKUP_DIR" -name "agnos-backup-*.tar.zst" -type f | wc -l)
        if [[ $backup_count -gt $keep_count ]]; then
            log_info "Removing old backups (keeping last $keep_count)"
            find "$BACKUP_DIR" -name "agnos-backup-*.tar.zst" -type f | sort | head -n -${keep_count} | xargs rm -f
        fi
    fi
    
    # Remove old update packages
    if [[ -d "$UPDATE_DIR" ]]; then
        find "$UPDATE_DIR" -name "*.delta.zst" -type f -mtime +7 -delete
        find "$UPDATE_DIR" -name "agnos-*.tar.gz" -type f -mtime +7 -delete
        log_info "Cleaned up old update packages"
    fi
}

# Main command handler
main() {
    case "${1:-}" in
        create-delta)
            shift
            create_delta "$@"
            ;;
        apply-delta)
            shift
            apply_delta "$@"
            ;;
        check)
            shift
            check_update "$@"
            ;;
        download)
            shift
            download_update "$@"
            ;;
        install)
            shift
            install_update "$@"
            ;;
        rollback)
            shift
            rollback "$@"
            ;;
        backup)
            shift
            create_backup "$@"
            ;;
        cleanup)
            shift
            cleanup "$@"
            ;;
        auto-update)
            log_step "Running automatic update check and install"
            if check_update; then
                local latest=$(check_update 2>/dev/null | grep "latest:" | cut -d' ' -f2)
                if [[ -n "$latest" ]]; then
                    local download_result=$(download_update "$latest")
                    local download_type=$(echo "$download_result" | cut -d: -f1)
                    local download_file=$(echo "$download_result" | cut -d: -f2-)
                    
                    if [[ "$download_type" == "DELTA" ]]; then
                        install_update "$download_file" true
                    else
                        install_update "$download_file" false
                    fi
                fi
            else
                log_info "No update available"
            fi
            ;;
        help|--help|-h)
            cat << EOF
AGNOS Delta Update System

Usage: $0 <command> [options]

Commands:
    create-delta <old_v> <new_v> <old_pkg> <new_pkg> [out_dir]
                              Create delta update between versions
    apply-delta <delta> <old> <output>
                              Apply delta to reconstruct new package
    check [current_version]    Check if update is available
    download <version>         Download update package
    install <package> [is_delta]
                              Install update with rollback support
    rollback [backup_file]     Rollback to previous version
    backup <version>           Create system backup
    cleanup [keep_count]       Clean old backups (default: keep 5)
    auto-update                Check, download, and install updates automatically
    help                       Show this help message

Environment Variables:
    AGNOS_VERSION              Current system version
    AGNOS_UPDATE_URL           Update server URL
    AGNOS_UPDATE_DIR           Directory for update files
    AGNOS_BACKUP_DIR           Directory for backups

Examples:
    # Create delta update
    $0 create-delta 0.1.0 0.2.0 agnos-0.1.0.tar.gz agnos-0.2.0.tar.gz

    # Check for updates
    $0 check

    # Install update
    $0 install agnos-0.2.0.tar.gz

    # Rollback if needed
    $0 rollback

EOF
            ;;
        *)
            log_error "Unknown command: ${1:-}"
            echo "Use '$0 help' for usage information"
            exit 1
            ;;
    esac
}

main "$@"
