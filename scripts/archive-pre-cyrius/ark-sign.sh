#!/usr/bin/env bash
# ark-sign.sh — Sign .ark packages with Ed25519 (sigil-compatible)
#
# Usage:
#   ./scripts/ark-sign.sh dist/ark/redis7-7.4.2-x86_64.ark
#   ./scripts/ark-sign.sh dist/ark/                              # sign all .ark files
#   ./scripts/ark-sign.sh --generate-key                         # create signing keypair
#   ./scripts/ark-sign.sh --verify dist/ark/redis7-7.4.2-x86_64.ark
#
# Environment variables:
#   ARK_SIGNING_KEY    — path to Ed25519 signing key (default: ~/.config/agnos/signing.key)
#   ARK_PUBLIC_KEY     — path to Ed25519 public key (default: ~/.config/agnos/signing.pub)
#   ARK_KEYS_DIR       — directory for key storage (default: ~/.config/agnos)
#
# Key format:
#   signing.key  — 64 hex chars (32-byte Ed25519 seed)
#   signing.pub  — 64 hex chars (32-byte Ed25519 public key) + key_id on line 2
#
# Signature format:
#   .ark.sig     — 128 hex chars (64-byte Ed25519 signature) + metadata
#
# This tool produces signatures compatible with:
#   - sigil.rs (SigilVerifier.verify_artifact / verify_package_install)
#   - marketplace/trust.rs (verify_signature with PublisherKeyring)
#   - marketplace/local_registry.rs (.sig sidecar verification)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KEYS_DIR="${ARK_KEYS_DIR:-${HOME}/.config/agnos}"
SIGNING_KEY="${ARK_SIGNING_KEY:-${KEYS_DIR}/signing.key}"
PUBLIC_KEY="${ARK_PUBLIC_KEY:-${KEYS_DIR}/signing.pub}"

# Colors (if terminal)
if [ -t 1 ]; then
    BLUE='\033[36m'; GREEN='\033[32m'; YELLOW='\033[33m'; RED='\033[31m'; NC='\033[0m'
else
    BLUE=''; GREEN=''; YELLOW=''; RED=''; NC=''
fi

log()  { echo -e "${BLUE}[sigil]${NC} $*"; }
ok()   { echo -e "${GREEN}[sigil]${NC} $*"; }
warn() { echo -e "${YELLOW}[sigil]${NC} $*"; }
err()  { echo -e "${RED}[sigil]${NC} $*" >&2; }

# -----------------------------------------------------------------------
# Check for openssl (used for Ed25519 operations)
# -----------------------------------------------------------------------
check_openssl() {
    if ! command -v openssl &>/dev/null; then
        err "openssl not found — required for Ed25519 signing"
        exit 1
    fi
    # Check Ed25519 support
    if ! openssl genpkey -algorithm ed25519 -out /dev/null 2>/dev/null; then
        err "openssl does not support Ed25519 — upgrade to OpenSSL 1.1.1+"
        exit 1
    fi
}

# -----------------------------------------------------------------------
# Generate Ed25519 keypair
# -----------------------------------------------------------------------
generate_keypair() {
    check_openssl
    mkdir -p "$KEYS_DIR"

    if [ -f "$SIGNING_KEY" ]; then
        err "Signing key already exists: ${SIGNING_KEY}"
        err "Remove it first or set ARK_SIGNING_KEY to a different path"
        exit 1
    fi

    log "Generating Ed25519 keypair..."

    # Generate PEM private key
    local pem_key="${KEYS_DIR}/signing.pem"
    openssl genpkey -algorithm ed25519 -out "$pem_key" 2>/dev/null

    # Extract raw 32-byte seed (Ed25519 private key seed)
    # OpenSSL PEM → DER → last 32 bytes are the seed
    local raw_seed
    raw_seed=$(openssl pkey -in "$pem_key" -outform DER 2>/dev/null | tail -c 32 | xxd -p -c 64)
    echo "$raw_seed" > "$SIGNING_KEY"
    chmod 600 "$SIGNING_KEY"

    # Extract raw 32-byte public key
    local raw_pub
    raw_pub=$(openssl pkey -in "$pem_key" -pubout -outform DER 2>/dev/null | tail -c 32 | xxd -p -c 64)

    # Key ID = first 8 bytes of public key (16 hex chars), matching sigil convention
    local key_id="${raw_pub:0:16}"

    # Write public key file: line 1 = pubkey hex, line 2 = key_id
    {
        echo "$raw_pub"
        echo "$key_id"
    } > "$PUBLIC_KEY"

    # Keep PEM for openssl sign/verify operations
    # Generate PEM public key for verification
    openssl pkey -in "$pem_key" -pubout -out "${KEYS_DIR}/signing.pub.pem" 2>/dev/null

    ok "Keypair generated:"
    ok "  Signing key:  ${SIGNING_KEY}"
    ok "  Public key:   ${PUBLIC_KEY}"
    ok "  PEM key:      ${pem_key}"
    ok "  Key ID:       ${key_id}"
    echo ""
    ok "To distribute the public key for verification:"
    ok "  cp ${PUBLIC_KEY} /var/lib/agnos/marketplace/keys/"
    ok "  cp ${KEYS_DIR}/signing.pub.pem /var/lib/agnos/marketplace/keys/"
}

# -----------------------------------------------------------------------
# Sign a single .ark file
# -----------------------------------------------------------------------
sign_file() {
    local ark_file="$1"

    if [ ! -f "$ark_file" ]; then
        err "File not found: ${ark_file}"
        return 1
    fi

    if [ ! -f "$SIGNING_KEY" ] || [ ! -f "${KEYS_DIR}/signing.pem" ]; then
        err "Signing key not found: ${SIGNING_KEY}"
        err "Run: ark-sign.sh --generate-key"
        return 1
    fi

    local basename
    basename=$(basename "$ark_file")
    local sig_file="${ark_file}.sig"
    local pem_key="${KEYS_DIR}/signing.pem"

    # Compute SHA-256 of the .ark file
    local content_hash
    content_hash=$(sha256sum "$ark_file" | cut -d' ' -f1)

    # Sign with Ed25519 via openssl
    local raw_sig
    raw_sig=$(openssl pkeyutl -sign -inkey "$pem_key" \
        -rawin -in <(sha256sum "$ark_file" | cut -d' ' -f1 | xxd -r -p) \
        2>/dev/null | xxd -p -c 128)

    # Read key_id from public key file
    local key_id
    key_id=$(sed -n '2p' "$PUBLIC_KEY")

    local pub_hex
    pub_hex=$(sed -n '1p' "$PUBLIC_KEY")

    # Write signature file (sigil-compatible format)
    # Line 1: signature (128 hex chars = 64 bytes)
    # Line 2: key_id (16 hex chars)
    # Line 3: content_hash (SHA-256 of .ark)
    # Line 4: public_key_hex (for standalone verification)
    # Line 5: timestamp (ISO 8601)
    # Line 6: artifact_type
    {
        echo "$raw_sig"
        echo "$key_id"
        echo "$content_hash"
        echo "$pub_hex"
        echo "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        echo "package"
    } > "$sig_file"

    ok "Signed: ${basename} → ${basename}.sig (key_id: ${key_id})"
}

# -----------------------------------------------------------------------
# Verify a signed .ark file
# -----------------------------------------------------------------------
verify_file() {
    local ark_file="$1"
    local sig_file="${ark_file}.sig"

    if [ ! -f "$ark_file" ]; then
        err "File not found: ${ark_file}"
        return 1
    fi

    if [ ! -f "$sig_file" ]; then
        err "Signature not found: ${sig_file}"
        return 1
    fi

    # Read signature metadata
    local sig_hex key_id stored_hash pub_hex timestamp artifact_type
    sig_hex=$(sed -n '1p' "$sig_file")
    key_id=$(sed -n '2p' "$sig_file")
    stored_hash=$(sed -n '3p' "$sig_file")
    pub_hex=$(sed -n '4p' "$sig_file")
    timestamp=$(sed -n '5p' "$sig_file")
    artifact_type=$(sed -n '6p' "$sig_file")

    # Verify content hash matches
    local actual_hash
    actual_hash=$(sha256sum "$ark_file" | cut -d' ' -f1)

    if [ "$actual_hash" != "$stored_hash" ]; then
        err "Content hash mismatch!"
        err "  Expected: ${stored_hash}"
        err "  Actual:   ${actual_hash}"
        return 1
    fi

    # Verify Ed25519 signature using openssl
    # We need to reconstruct the PEM public key from raw bytes
    local tmp_dir
    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT

    # Build DER-encoded Ed25519 public key
    # ASN.1 header for Ed25519: 302a300506032b6570032100
    local der_header="302a300506032b6570032100"
    echo "${der_header}${pub_hex}" | xxd -r -p > "${tmp_dir}/pub.der"
    openssl pkey -pubin -inform DER -in "${tmp_dir}/pub.der" -out "${tmp_dir}/pub.pem" 2>/dev/null

    # Write signature bytes to file
    echo "$sig_hex" | xxd -r -p > "${tmp_dir}/sig.bin"

    # Write hash bytes to file (we signed the raw SHA-256 hash)
    echo "$actual_hash" | xxd -r -p > "${tmp_dir}/hash.bin"

    # Verify
    if openssl pkeyutl -verify -pubin -inkey "${tmp_dir}/pub.pem" \
        -rawin -in "${tmp_dir}/hash.bin" -sigfile "${tmp_dir}/sig.bin" 2>/dev/null; then
        ok "Verified: $(basename "$ark_file")"
        ok "  Key ID:    ${key_id}"
        ok "  SHA-256:   ${actual_hash}"
        ok "  Signed at: ${timestamp}"
        ok "  Type:      ${artifact_type}"
        return 0
    else
        err "Signature verification FAILED for $(basename "$ark_file")"
        err "  Key ID: ${key_id}"
        return 1
    fi
}

# -----------------------------------------------------------------------
# Export public key as sigil-compatible JSON (for PublisherKeyring)
# -----------------------------------------------------------------------
export_keyring_json() {
    local output="${1:-${KEYS_DIR}/publisher.json}"

    if [ ! -f "$PUBLIC_KEY" ]; then
        err "Public key not found: ${PUBLIC_KEY}"
        exit 1
    fi

    local pub_hex key_id
    pub_hex=$(sed -n '1p' "$PUBLIC_KEY")
    key_id=$(sed -n '2p' "$PUBLIC_KEY")

    cat > "$output" << JSON
[
  {
    "key_id": "${key_id}",
    "valid_from": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "valid_until": null,
    "public_key_hex": "${pub_hex}"
  }
]
JSON

    ok "Exported keyring JSON: ${output}"
    ok "  Copy to /var/lib/agnos/marketplace/keys/ for package verification"
}

# -----------------------------------------------------------------------
# Sign all .ark files in a directory
# -----------------------------------------------------------------------
sign_directory() {
    local dir="$1"
    local count=0
    local failed=0

    if [ ! -d "$dir" ]; then
        err "Directory not found: ${dir}"
        exit 1
    fi

    log "Signing all .ark packages in ${dir}"

    while IFS= read -r -d '' ark_file; do
        if sign_file "$ark_file"; then
            count=$((count + 1))
        else
            failed=$((failed + 1))
        fi
    done < <(find "$dir" -name '*.ark' -type f -print0 | sort -z)

    echo ""
    if [ $count -eq 0 ] && [ $failed -eq 0 ]; then
        warn "No .ark files found in ${dir}"
    else
        ok "Signed ${count} packages (${failed} failed)"
    fi
}

# -----------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------
case "${1:-}" in
    --generate-key|-g)
        generate_keypair
        ;;
    --verify|-v)
        shift
        check_openssl
        if [ -d "${1:-}" ]; then
            # Verify all .ark files in directory
            failed=0
            verified=0
            while IFS= read -r -d '' ark_file; do
                if verify_file "$ark_file"; then
                    verified=$((verified + 1))
                else
                    failed=$((failed + 1))
                fi
            done < <(find "$1" -name '*.ark' -type f -print0 | sort -z)
            echo ""
            ok "Verified: ${verified}, Failed: ${failed}"
            [ $failed -eq 0 ] || exit 1
        else
            verify_file "${1:?Usage: ark-sign.sh --verify <file.ark>}"
        fi
        ;;
    --export-keyring|-e)
        shift
        export_keyring_json "${1:-}"
        ;;
    --help|-h)
        cat << 'HELP'
ark-sign.sh — Ed25519 package signing for .ark packages (sigil-compatible)

Usage:
    ark-sign.sh <file.ark>              Sign a single package
    ark-sign.sh <directory>             Sign all .ark files in directory
    ark-sign.sh --generate-key          Generate Ed25519 signing keypair
    ark-sign.sh --verify <file.ark>     Verify package signature
    ark-sign.sh --verify <directory>    Verify all .ark files in directory
    ark-sign.sh --export-keyring [out]  Export public key as sigil JSON

Environment Variables:
    ARK_SIGNING_KEY    Path to signing key     (default: ~/.config/agnos/signing.key)
    ARK_PUBLIC_KEY     Path to public key      (default: ~/.config/agnos/signing.pub)
    ARK_KEYS_DIR       Key storage directory   (default: ~/.config/agnos)

Signature Format (.ark.sig):
    Line 1: Ed25519 signature (128 hex chars)
    Line 2: Key ID (first 8 bytes of public key, 16 hex chars)
    Line 3: SHA-256 of .ark file
    Line 4: Public key (64 hex chars)
    Line 5: Timestamp (ISO 8601)
    Line 6: Artifact type ("package")

Compatible with:
    - sigil.rs (SigilVerifier trust store)
    - marketplace/trust.rs (PublisherKeyring verification)
    - marketplace/local_registry.rs (.sig sidecar files)
HELP
        ;;
    "")
        err "No target specified"
        echo "Usage: ark-sign.sh <file.ark|directory|--generate-key|--verify|--help>"
        exit 1
        ;;
    *)
        check_openssl
        if [ -d "$1" ]; then
            sign_directory "$1"
        else
            sign_file "$1"
        fi
        ;;
esac
