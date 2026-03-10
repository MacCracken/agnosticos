#!/usr/bin/env bash
# ark-publish.sh — Publish .agnos-agent bundles to the mela marketplace registry.
#
# Usage:
#   ark-publish.sh <bundle.agnos-agent>           # Publish single bundle
#   ark-publish.sh dist/marketplace/               # Publish all bundles in directory
#
# Environment:
#   MELA_REGISTRY_URL   Registry endpoint (default: https://registry.agnos.org)
#   MELA_API_TOKEN      Publisher API token (required)
#   MELA_SIGN           Set to 1 to sign before publishing (requires ark-sign.sh)
#   MELA_DRY_RUN        Set to 1 to validate without uploading
#
# Requires: curl, jq, sha256sum

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

MELA_REGISTRY_URL="${MELA_REGISTRY_URL:-https://registry.agnos.org}"
MELA_API_TOKEN="${MELA_API_TOKEN:-}"
MELA_SIGN="${MELA_SIGN:-0}"
MELA_DRY_RUN="${MELA_DRY_RUN:-0}"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

log()  { echo "==> $*"; }
info() { echo "    $*"; }
warn() { echo "WARNING: $*" >&2; }
die()  { echo "ERROR: $*" >&2; exit 1; }

require_tool() {
    command -v "$1" &>/dev/null || die "Required tool not found: $1"
}

# ---------------------------------------------------------------------------
# Validation
# ---------------------------------------------------------------------------

validate_bundle() {
    local bundle="$1"

    [[ -f "$bundle" ]] || die "Bundle not found: $bundle"
    [[ "$bundle" == *.agnos-agent ]] || die "Not an .agnos-agent bundle: $bundle"

    # Extract and validate manifest
    local tmpdir
    tmpdir="$(mktemp -d)"
    trap "rm -rf '$tmpdir'" RETURN

    tar xzf "$bundle" -C "$tmpdir" 2>/dev/null \
        || die "Failed to extract bundle: $bundle"

    local manifest="$tmpdir/manifest.json"
    [[ -f "$manifest" ]] || die "Bundle missing manifest.json: $bundle"

    # Validate required fields
    local name version publisher
    name="$(jq -r '.name // empty' "$manifest")"
    version="$(jq -r '.version // empty' "$manifest")"
    publisher="$(jq -r '.publisher // empty' "$manifest")"

    [[ -n "$name" ]]      || die "manifest.json missing 'name'"
    [[ -n "$version" ]]   || die "manifest.json missing 'version'"
    [[ -n "$publisher" ]] || die "manifest.json missing 'publisher'"

    echo "$name" "$version" "$publisher"
}

# ---------------------------------------------------------------------------
# Publishing
# ---------------------------------------------------------------------------

publish_bundle() {
    local bundle="$1"
    local name version publisher
    read -r name version publisher < <(validate_bundle "$bundle")

    local size
    size="$(stat -c %s "$bundle" 2>/dev/null || stat -f %z "$bundle" 2>/dev/null)"
    local sha256
    sha256="$(sha256sum "$bundle" | cut -d' ' -f1)"

    log "Publishing: $name v$version by $publisher"
    info "Bundle: $bundle ($size bytes)"
    info "SHA-256: $sha256"

    # Sign if requested
    local sig_file="${bundle}.sig"
    if [[ "$MELA_SIGN" == "1" ]]; then
        if [[ -x "$SCRIPT_DIR/ark-sign.sh" ]]; then
            log "Signing bundle..."
            "$SCRIPT_DIR/ark-sign.sh" sign "$bundle"
        else
            warn "ark-sign.sh not found, skipping signing"
        fi
    fi

    # Dry run — validate only
    if [[ "$MELA_DRY_RUN" == "1" ]]; then
        log "DRY RUN: Would publish $name v$version to $MELA_REGISTRY_URL"
        info "  Size: $size bytes"
        info "  SHA-256: $sha256"
        [[ -f "$sig_file" ]] && info "  Signature: $sig_file"
        return 0
    fi

    # Require API token for actual publishing
    [[ -n "$MELA_API_TOKEN" ]] \
        || die "MELA_API_TOKEN required for publishing (set MELA_DRY_RUN=1 for validation only)"

    # Upload bundle
    local upload_url="${MELA_REGISTRY_URL}/v1/packages/publish"

    local curl_args=(
        -s -w "%{http_code}"
        -X POST
        -H "Authorization: Bearer $MELA_API_TOKEN"
        -H "X-Package-Name: $name"
        -H "X-Package-Version: $version"
        -H "X-Package-SHA256: $sha256"
        -F "bundle=@$bundle"
    )

    # Attach signature if present
    if [[ -f "$sig_file" ]]; then
        curl_args+=(-F "signature=@$sig_file")
        info "Including signature: $sig_file"
    fi

    local response_file
    response_file="$(mktemp)"
    local http_code
    http_code="$(curl "${curl_args[@]}" -o "$response_file" "$upload_url")"

    if [[ "$http_code" -ge 200 && "$http_code" -lt 300 ]]; then
        log "Published $name v$version to $MELA_REGISTRY_URL"
        if [[ -s "$response_file" ]]; then
            jq . "$response_file" 2>/dev/null || cat "$response_file"
        fi
    else
        warn "Publish failed (HTTP $http_code):"
        cat "$response_file" >&2
        rm -f "$response_file"
        return 1
    fi

    rm -f "$response_file"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

require_tool curl
require_tool jq
require_tool sha256sum

if [[ $# -lt 1 ]]; then
    echo "Usage: ark-publish.sh <bundle.agnos-agent | directory>"
    echo ""
    echo "Environment:"
    echo "  MELA_REGISTRY_URL   Registry URL (default: $MELA_REGISTRY_URL)"
    echo "  MELA_API_TOKEN      Publisher API token (required for upload)"
    echo "  MELA_SIGN=1         Sign bundles before publishing"
    echo "  MELA_DRY_RUN=1      Validate without uploading"
    exit 1
fi

target="$1"
failed=0
published=0

if [[ -d "$target" ]]; then
    # Publish all bundles in directory
    shopt -s nullglob
    bundles=("$target"/*.agnos-agent)
    shopt -u nullglob

    if [[ ${#bundles[@]} -eq 0 ]]; then
        die "No .agnos-agent bundles found in $target"
    fi

    log "Publishing ${#bundles[@]} bundle(s) from $target"
    echo ""

    for bundle in "${bundles[@]}"; do
        if publish_bundle "$bundle"; then
            published=$((published + 1))
        else
            failed=$((failed + 1))
        fi
        echo ""
    done

    log "Summary: $published published, $failed failed"
    [[ "$failed" -eq 0 ]] || exit 1
else
    publish_bundle "$target"
fi
