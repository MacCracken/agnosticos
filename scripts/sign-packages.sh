#!/bin/bash
#
# Package Signing Script for AGNOS
# Signs packages with GPG for release integrity
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# Configuration
SIGNING_KEY="${AGNOS_SIGNING_KEY:-release@agnos.org}"
SIGNING_DIR="${PROJECT_ROOT}/signed-packages"
PACKAGES_DIR="${PROJECT_ROOT}/packages"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
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

# Check GPG is available
check_gpg() {
    if ! command -v gpg &> /dev/null; then
        log_error "GPG not found. Please install GPG."
        exit 1
    fi
    
    # Check if signing key exists
    if ! gpg --list-keys "$SIGNING_KEY" &> /dev/null; then
        log_warn "Signing key '$SIGNING_KEY' not found in keyring"
        log_info "Generating new signing key..."
        generate_signing_key
    fi
}

# Generate a new signing key
generate_signing_key() {
    log_info "Generating new GPG signing key for AGNOS releases"
    
    cat > /tmp/gpg-gen-key-script <<EOF
%echo Generating AGNOS signing key
Key-Type: RSA
Key-Length: 4096
Name-Real: AGNOS Release Signing
Name-Email: ${SIGNING_KEY}
Expire-Date: 2y
%no-protection
%commit
%echo done
EOF
    
    gpg --batch --gen-key /tmp/gpg-gen-key-script
    rm -f /tmp/gpg-gen-key-script
    
    log_info "Signing key generated successfully"
    log_info "Key fingerprint:"
    gpg --list-keys --with-colons "$SIGNING_KEY" | grep fpr | head -1 | cut -d: -f10
}

# Sign a single package
sign_package() {
    local package="$1"
    local output_dir="$2"
    local basename=$(basename "$package")
    
    log_info "Signing package: $basename"
    
    # Create detached signature
    gpg --armor --detach-sign --local-user "$SIGNING_KEY" \
        --output "${output_dir}/${basename}.asc" \
        "$package"
    
    # Create checksum file
    sha256sum "$package" > "${output_dir}/${basename}.sha256"
    
    # Create combined signature (package + signature)
    cat "$package" "${output_dir}/${basename}.asc" > "${output_dir}/${basename}.signed"
    
    log_info "Package signed: ${output_dir}/${basename}.asc"
}

# Sign all packages in a directory
sign_all_packages() {
    local input_dir="${1:-$PACKAGES_DIR}"
    local output_dir="${2:-$SIGNING_DIR}"
    
    mkdir -p "$output_dir"
    
    log_info "Signing packages from $input_dir"
    
    # Find all package files
    find "$input_dir" -type f \( -name "*.tar.gz" -o -name "*.deb" -o -name "*.rpm" -o -name "*.zip" \) | while read -r package; do
        sign_package "$package" "$output_dir"
    done
    
    # Generate signing key info
    gpg --export --armor "$SIGNING_KEY" > "${output_dir}/SIGNING-KEY.asc"
    
    log_info "All packages signed successfully"
    log_info "Signing key exported to: ${output_dir}/SIGNING-KEY.asc"
}

# Verify a signed package
verify_package() {
    local package="$1"
    local signature="${package}.asc"
    
    log_info "Verifying package: $(basename "$package")"
    
    # Check if signature exists
    if [[ ! -f "$signature" ]]; then
        log_error "Signature file not found: $signature"
        return 1
    fi
    
    # Verify GPG signature
    if gpg --verify "$signature" "$package" 2>/dev/null; then
        log_info "GPG signature verified successfully"
    else
        log_error "GPG signature verification failed"
        return 1
    fi
    
    # Verify checksum if available
    if [[ -f "${package}.sha256" ]]; then
        if sha256sum -c "${package}.sha256" &>/dev/null; then
            log_info "Checksum verified successfully"
        else
            log_error "Checksum verification failed"
            return 1
        fi
    fi
    
    return 0
}

# Export public key for distribution
export_public_key() {
    local output_file="${1:-${PROJECT_ROOT}/SIGNING-KEY.asc}"
    
    log_info "Exporting public signing key to $output_file"
    gpg --export --armor "$SIGNING_KEY" > "$output_file"
    
    # Also export to GitHub format if available
    if command -v gh &> /dev/null; then
        local key_fingerprint=$(gpg --list-keys --with-colons "$SIGNING_KEY" | grep fpr | head -1 | cut -d: -f10)
        log_info "Key fingerprint: $key_fingerprint"
    fi
}

# Import public key from file
import_public_key() {
    local key_file="$1"
    
    if [[ ! -f "$key_file" ]]; then
        log_error "Key file not found: $key_file"
        return 1
    fi
    
    log_info "Importing public key from $key_file"
    gpg --import "$key_file"
    log_info "Key imported successfully"
}

# Revoke a signing key
revoke_key() {
    local reason="${1:-key-compromised}"
    
    log_warn "WARNING: This will revoke the signing key!"
    log_warn "This action cannot be undone!"
    read -p "Are you sure? (yes/no): " confirm
    
    if [[ "$confirm" != "yes" ]]; then
        log_info "Key revocation cancelled"
        return 0
    fi
    
    log_info "Generating revocation certificate..."
    gpg --gen-revoke "$SIGNING_KEY"
    
    log_warn "Key revoked. You must generate a new signing key for future releases."
}

# Create InRelease file (apt-style signed release)
create_inrelease() {
    local repo_dir="$1"
    local release_file="${repo_dir}/Release"
    
    log_info "Creating signed InRelease file for repository"
    
    # Create Release file if it doesn't exist
    if [[ ! -f "$release_file" ]]; then
        log_warn "Release file not found, creating..."
        cat > "$release_file" <<EOF
Origin: AGNOS
Label: AGNOS Repository
Suite: stable
Codename: agnos
Version: 1.0
Date: $(date -Ru)
Architectures: amd64 arm64
Components: main
EOF
    fi
    
    # Create InRelease (inline signed)
    gpg --clearsign --local-user "$SIGNING_KEY" \
        --output "${repo_dir}/InRelease" \
        "$release_file"
    
    # Create Release.gpg (detached signature)
    gpg --armor --detach-sign --local-user "$SIGNING_KEY" \
        --output "${repo_dir}/Release.gpg" \
        "$release_file"
    
    log_info "Signed release files created"
}

# Main command handler
main() {
    case "${1:-}" in
        sign)
            check_gpg
            shift
            if [[ -f "$1" ]]; then
                sign_package "$1" "${SIGNING_DIR}"
            else
                sign_all_packages "$@"
            fi
            ;;
        verify)
            shift
            verify_package "$1"
            ;;
        export-key)
            shift
            export_public_key "${1:-${PROJECT_ROOT}/SIGNING-KEY.asc}"
            ;;
        import-key)
            shift
            import_public_key "$1"
            ;;
        generate-key)
            generate_signing_key
            ;;
        revoke)
            shift
            revoke_key "$@"
            ;;
        inrelease)
            shift
            create_inrelease "$1"
            ;;
        help|--help|-h)
            cat << EOF
AGNOS Package Signing Tool

Usage: $0 <command> [options]

Commands:
    sign [package|directory]     Sign a package or all packages in directory
    verify <package>             Verify a signed package
    export-key [file]            Export public signing key
    import-key <file>            Import a public key
    generate-key                 Generate new signing key
    revoke [reason]              Revoke current signing key
    inrelease <repo-dir>         Create signed InRelease file
    help                         Show this help message

Environment Variables:
    AGNOS_SIGNING_KEY            GPG key ID or email (default: release@agnos.org)
    AGNOS_SIGNING_PASSPHRASE     Key passphrase (if protected)

Examples:
    $0 sign packages/              Sign all packages
    $0 sign my-package.tar.gz     Sign single package
    $0 verify my-package.tar.gz   Verify package
    $0 export-key                 Export signing key

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
