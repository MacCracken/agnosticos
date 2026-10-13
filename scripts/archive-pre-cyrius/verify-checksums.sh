#!/usr/bin/env bash
# verify-checksums.sh — Fetch source tarballs and compute real SHA256 checksums
# for all AGNOS recipes that have placeholder (all-zero or empty) sha256 values.
#
# Usage:
#   ./scripts/verify-checksums.sh              # dry-run: print what would change
#   ./scripts/verify-checksums.sh --apply      # update recipe files in-place
#
# Skips:
#   - Marketplace recipes (no url field, use github_release)
#   - Recipes that already have a valid (non-zero) sha256
#
# Caches downloaded hashes in /tmp/agnos-checksum-cache/ so re-runs are fast.

set -uo pipefail

RECIPE_DIR="$(cd "$(dirname "$0")/.." && pwd)/recipes"
CACHE_DIR="${TMPDIR:-/tmp}/agnos-checksum-cache"
APPLY=false
MAX_SIZE=500  # Skip downloads larger than this (MB)
FAILED=0
UPDATED=0
SKIPPED=0
ALREADY_VALID=0

for arg in "$@"; do
    case "$arg" in
        --apply) APPLY=true ;;
        --max-size=*) MAX_SIZE="${arg#--max-size=}" ;;
        -h|--help)
            echo "Usage: $0 [--apply] [--max-size=MB]"
            echo "  --apply        Update recipe files in-place (default: dry-run)"
            echo "  --max-size=MB  Skip downloads larger than MB (default: 500)"
            exit 0
            ;;
        *) echo "Unknown arg: $arg"; exit 1 ;;
    esac
done

mkdir -p "$CACHE_DIR"

ZERO_HASH="0000000000000000000000000000000000000000000000000000000000000000"

# Collect all recipe files
mapfile -t RECIPES < <(find "$RECIPE_DIR" -name '*.toml' -type f | sort)

echo "=== AGNOS Recipe SHA256 Verification ==="
echo "Recipes found: ${#RECIPES[@]}"
echo "Cache dir:     $CACHE_DIR"
echo "Mode:          $($APPLY && echo 'APPLY' || echo 'DRY-RUN')"
echo ""

# Parse a TOML field value (simple single-line strings only)
get_field() {
    local file="$1" field="$2"
    grep -m1 "^${field} = " "$file" 2>/dev/null | sed 's/^[^"]*"//;s/"[^"]*$//'
}

for recipe in "${RECIPES[@]}"; do
    rel_path="${recipe#$RECIPE_DIR/}"

    url="$(get_field "$recipe" "url")"
    sha256="$(get_field "$recipe" "sha256")"
    version="$(get_field "$recipe" "version")"

    # Skip: no URL (marketplace recipes)
    if [[ -z "$url" ]]; then
        echo "SKIP  $rel_path  (no url)"
        ((SKIPPED++)) || true
        continue
    fi

    # Resolve ${version} interpolation
    if [[ "$url" == *'${'* ]]; then
        if [[ -n "$version" ]]; then
            url="${url//\$\{version\}/$version}"
        else
            echo "SKIP  $rel_path  (unresolvable variable in url)"
            ((SKIPPED++)) || true
            continue
        fi
    fi

    # Skip: already has a valid checksum
    if [[ -n "$sha256" && "$sha256" != "$ZERO_HASH" && "$sha256" != "" ]]; then
        ((ALREADY_VALID++)) || true
        continue
    fi

    # Use faster GNU mirror for downloads (content is identical)
    download_url="${url/ftp.gnu.org/ftpmirror.gnu.org}"

    # Check cache first (keyed by original URL hash for consistency)
    url_hash="$(echo -n "$url" | sha256sum | cut -d' ' -f1)"
    hash_file="$CACHE_DIR/$url_hash.sha256"

    if [[ -f "$hash_file" ]]; then
        real_sha256="$(cat "$hash_file")"
    else
        # Check file size via HEAD request before downloading
        content_length="$(curl -fsSIL --max-time 10 "$download_url" 2>/dev/null | grep -i '^content-length:' | tail -1 | tr -d '\r' | awk '{print $2}' || echo "")"
        if [[ -n "$content_length" && "$content_length" =~ ^[0-9]+$ ]] && (( content_length > MAX_SIZE * 1024 * 1024 )); then
            size_mb=$(( content_length / 1024 / 1024 ))
            echo "SKIP  $rel_path  (${size_mb}MB exceeds ${MAX_SIZE}MB limit)"
            ((SKIPPED++)) || true
            continue
        fi

        # Download and compute hash
        tmp_file="$CACHE_DIR/$url_hash.download"
        if ! curl -fsSL --retry 2 --retry-delay 3 --max-time 300 -o "$tmp_file" "$download_url" 2>/dev/null; then
            echo "FAIL  $rel_path  (download failed: $download_url)"
            rm -f "$tmp_file"
            ((FAILED++)) || true
            continue
        fi

        real_sha256="$(sha256sum "$tmp_file" | cut -d' ' -f1)"
        echo "$real_sha256" > "$hash_file"
        rm -f "$tmp_file"
    fi

    if $APPLY; then
        old_line="sha256 = \"${sha256}\""
        new_line="sha256 = \"${real_sha256}\""
        if [[ -n "$sha256" ]]; then
            sed -i "s|${old_line}|${new_line}|" "$recipe"
        else
            # No sha256 field — insert after the url line
            sed -i "/^url = /a sha256 = \"${real_sha256}\"" "$recipe"
        fi
        echo "OK    $rel_path  $real_sha256"
    else
        echo "WOULD $rel_path  $real_sha256"
    fi
    ((UPDATED++)) || true
done

echo ""
echo "=== Summary ==="
echo "Updated:       $UPDATED"
echo "Already valid: $ALREADY_VALID"
echo "Skipped:       $SKIPPED"
echo "Failed:        $FAILED"
echo "Total:         ${#RECIPES[@]}"

if ((FAILED > 0)); then
    exit 1
fi
