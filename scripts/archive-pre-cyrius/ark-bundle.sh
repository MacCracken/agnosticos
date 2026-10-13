#!/usr/bin/env bash
# ark-bundle.sh — Build .agnos-agent marketplace bundles from recipes
#
# Usage:
#   ./scripts/ark-bundle.sh recipes/marketplace/secureyeoman.toml
#   ./scripts/ark-bundle.sh recipes/marketplace/                     # bundle all
#   ./scripts/ark-bundle.sh --sign recipes/marketplace/photisnadi.toml
#
# Environment variables:
#   ARK_OUTPUT_DIR  — where to place bundles (default: dist/marketplace)
#   ARK_SIGN        — set to 1 to sign bundles after creation
#   GITHUB_TOKEN    — optional GitHub token for private repos / rate limits
#
# This script reads marketplace recipes and creates .agnos-agent bundles by:
#   1. Downloading release assets from GitHub (via github_release in recipe)
#   2. Reading manifest metadata from the recipe
#   3. Collecting binaries/artifacts from the downloaded release
#   4. Generating manifest.json and sandbox.json
#   5. Creating a gzipped tarball (.agnos-agent)
#   6. Optionally signing with ark-sign.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUTPUT_DIR="${ARK_OUTPUT_DIR:-${PWD}/dist/marketplace}"
SIGN_AFTER="${ARK_SIGN:-0}"

# Parse flags
POSITIONAL=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --sign)     SIGN_AFTER=1; shift ;;
        -*)         echo "Unknown flag: $1" >&2; exit 1 ;;
        *)          POSITIONAL+=("$1"); shift ;;
    esac
done
set -- "${POSITIONAL[@]+"${POSITIONAL[@]}"}"

# Colors
if [ -t 1 ]; then
    BLUE='\033[36m'; GREEN='\033[32m'; YELLOW='\033[33m'; RED='\033[31m'; NC='\033[0m'
else
    BLUE=''; GREEN=''; YELLOW=''; RED=''; NC=''
fi

log()  { echo -e "${BLUE}[mela]${NC} $*"; }
ok()   { echo -e "${GREEN}[mela]${NC} $*"; }
warn() { echo -e "${YELLOW}[mela]${NC} $*"; }
err()  { echo -e "${RED}[mela]${NC} $*" >&2; }

# -----------------------------------------------------------------------
# Lightweight TOML parsing (reused from ark-build.sh)
# -----------------------------------------------------------------------
parse_field() {
    local file="$1" field="$2"
    { grep -m1 "^${field} " "$file" 2>/dev/null || true; } | sed 's/.*= *"\(.*\)"/\1/'
}

parse_bool() {
    local file="$1" field="$2"
    { grep -m1 "^${field} " "$file" 2>/dev/null || true; } | sed 's/.*= *//' | tr -d ' '
}

parse_array() {
    local file="$1" field="$2"
    local line
    line=$(grep -m1 "^${field} = \[" "$file" 2>/dev/null) || true
    if [ -z "$line" ]; then return 0; fi
    if echo "$line" | grep -q '\]'; then
        echo "$line" | grep -oP '"[^"]*"' | tr -d '"'
    else
        sed -n "/^${field} = \[/,/^\]/p" "$file" 2>/dev/null \
            | grep -oP '"[^"]*"' | tr -d '"' || true
    fi
}

# Parse a field within a specific TOML section (e.g., [marketplace])
parse_section_field() {
    local file="$1" section="$2" field="$3"
    sed -n "/^\[${section}\]/,/^\[/p" "$file" 2>/dev/null \
        | grep -m1 "^${field} " \
        | sed 's/.*= *"\(.*\)"/\1/' \
        || true
}

parse_section_bool() {
    local file="$1" section="$2" field="$3"
    sed -n "/^\[${section}\]/,/^\[/p" "$file" 2>/dev/null \
        | grep -m1 "^${field} " \
        | sed 's/.*= *//' | tr -d ' ' \
        || true
}

parse_section_array() {
    local file="$1" section="$2" field="$3"
    local section_text
    section_text=$(sed -n "/^\[${section}\]/,/^\[/p" "$file" 2>/dev/null) || true
    local line
    line=$(echo "$section_text" | grep -m1 "^${field} = \[") || true
    if [ -z "$line" ]; then return 0; fi
    if echo "$line" | grep -q '\]'; then
        echo "$line" | grep -oP '"[^"]*"' | tr -d '"'
    else
        echo "$section_text" | sed -n "/^${field} = \[/,/^\]/p" \
            | grep -oP '"[^"]*"' | tr -d '"' || true
    fi
}

# -----------------------------------------------------------------------
# GitHub release download (curl only, no gh CLI)
# -----------------------------------------------------------------------
github_curl() {
    local url="$1"
    local auth_header=()
    if [ -n "${GITHUB_TOKEN:-}" ]; then
        auth_header=(-H "Authorization: token ${GITHUB_TOKEN}")
    fi
    curl -sL "${auth_header[@]+"${auth_header[@]}"}" "$url"
}

# Download the latest release asset from GitHub.
# Sets: _release_version (tag), _project_dir (extracted files)
download_release_asset() {
    local github_repo="$1" asset_pattern="$2" _unused="$3" stage_dir="$4"

    local download_dir="${stage_dir}/_download"
    mkdir -p "$download_dir"

    # Always fetch latest release — version comes from the release, not the recipe
    local release_json="" release_tag=""
    local api_url="https://api.github.com/repos/${github_repo}/releases/latest"

    log "  Fetching latest release..."
    release_json=$(github_curl "$api_url") || true

    # Check for API error
    if [ -z "$release_json" ] || echo "$release_json" | grep -q '"message"'; then
        err "No releases found for ${github_repo}"
        return 1
    fi

    release_tag=$(echo "$release_json" | grep -oP '"tag_name":\s*"\K[^"]*' | head -1)
    if [ -z "$release_tag" ]; then
        err "Could not parse release tag from ${github_repo}"
        return 1
    fi

    # Find matching asset URL
    local asset_url=""
    local all_urls
    all_urls=$(echo "$release_json" | grep -oP '"browser_download_url":\s*"\K[^"]*') || true

    if echo "$asset_pattern" | grep -q '[*?]'; then
        # Glob pattern match
        while read -r url; do
            [ -z "$url" ] && continue
            local name="${url##*/}"
            # shellcheck disable=SC2254
            case "$name" in $asset_pattern) asset_url="$url"; break;; esac
        done <<< "$all_urls"
    else
        # Exact match
        asset_url=$(echo "$all_urls" | grep -m1 "${asset_pattern}") || true
    fi

    if [ -z "$asset_url" ]; then
        err "Asset '${asset_pattern}' not found in ${github_repo}@${release_tag}"
        err "Available assets:"
        echo "$all_urls" | while read -r u; do
            [ -n "$u" ] && err "  - ${u##*/}"
        done
        return 1
    fi

    # Version always comes from the release tag
    _release_version="$release_tag"
    log "  Release: ${release_tag}"

    local asset_name="${asset_url##*/}"
    log "  Downloading: ${asset_name}"
    github_curl "$asset_url" > "${download_dir}/${asset_name}" || {
        err "Failed to download ${asset_url}"
        return 1
    }

    local file_size
    file_size=$(du -sh "${download_dir}/${asset_name}" | cut -f1)
    log "  Downloaded: ${file_size}"

    # Extract into project-like directory
    _project_dir="${stage_dir}/_project"
    mkdir -p "${_project_dir}/dist"

    case "$asset_name" in
        *.tar.gz|*.tgz)
            tar xzf "${download_dir}/${asset_name}" -C "${_project_dir}/dist"
            ;;
        *.zip)
            unzip -qo "${download_dir}/${asset_name}" -d "${_project_dir}/dist"
            ;;
        *)
            cp "${download_dir}/${asset_name}" "${_project_dir}/dist/"
            # Raw binary (no archive extension) — mark executable
            chmod +x "${_project_dir}/dist/${asset_name}"
            ;;
    esac

    local extracted_count
    extracted_count=$(find "${_project_dir}/dist" -type f | wc -l)
    log "  Extracted: ${extracted_count} files"

    rm -rf "$download_dir"
}

# -----------------------------------------------------------------------
# Bundle a single marketplace recipe
# -----------------------------------------------------------------------
bundle_recipe() {
    local recipe="$1"
    recipe="$(cd "$(dirname "$recipe")" && pwd)/$(basename "$recipe")"

    local pkg_name pkg_version pkg_desc pkg_license runtime category publisher
    pkg_name=$(parse_field "$recipe" "name")
    pkg_version=$(parse_field "$recipe" "version")
    pkg_desc=$(parse_field "$recipe" "description")
    pkg_license=$(parse_field "$recipe" "license")
    # Marketplace-specific fields (under [marketplace] section)
    runtime=$(parse_section_field "$recipe" "marketplace" "runtime")
    category=$(parse_section_field "$recipe" "marketplace" "category")
    publisher=$(parse_section_field "$recipe" "marketplace" "publisher")

    local tags_array
    tags_array=$(parse_section_array "$recipe" "marketplace" "tags" | paste -sd',' -)
    local min_agnos
    min_agnos=$(parse_section_field "$recipe" "marketplace" "min_agnos_version")
    # Default min_agnos_version to current AGNOS version if not set
    if [ -z "$min_agnos" ] && [ -f "${REPO_ROOT}/VERSION" ]; then
        min_agnos=$(cat "${REPO_ROOT}/VERSION" | tr -d '[:space:]')
    fi

    # Sandbox config (under [marketplace.sandbox] section)
    local seccomp_mode network_access data_dir
    seccomp_mode=$(parse_section_field "$recipe" "marketplace.sandbox" "seccomp_mode")
    network_access=$(parse_section_bool "$recipe" "marketplace.sandbox" "network_access")
    data_dir=$(parse_section_field "$recipe" "marketplace.sandbox" "data_dir")
    local allowed_hosts
    allowed_hosts=$(parse_section_array "$recipe" "marketplace.sandbox" "allowed_hosts" | sed 's/.*/"&"/' | paste -sd',' -)

    # GitHub release source (required)
    local github_release release_asset
    github_release=$(parse_section_field "$recipe" "source" "github_release")
    release_asset=$(parse_section_field "$recipe" "source" "release_asset")

    # Skip source-only crates (crates.io, sutra-modules, etc.)
    local crates_io
    crates_io=$(parse_section_field "$recipe" "source" "crates_io")
    if [ "$release_asset" = "source" ] || { [ -n "$crates_io" ] && [ -z "$github_release" ]; }; then
        log "Skipping: ${pkg_name} (source-only / crates.io)"
        return 0
    fi

    if [ -z "$github_release" ]; then
        err "${pkg_name}: missing github_release in [source] section"
        err "All marketplace recipes must specify a GitHub release source"
        return 1
    fi
    if [ -z "$release_asset" ]; then
        err "${pkg_name}: missing release_asset in [source] section"
        return 1
    fi

    log "Bundling: ${pkg_name} (${runtime})"
    log "  Source: github.com/${github_release}"

    # Create staging directory
    local stage_dir
    stage_dir=$(mktemp -d "/tmp/mela-${pkg_name}-XXXXXX")
    trap "rm -rf '$stage_dir'" RETURN

    # ---------------------------------------------------------------
    # Download latest release asset from GitHub
    # Version is always determined by the release tag, not the recipe.
    # ---------------------------------------------------------------
    _release_version=""
    _project_dir=""
    download_release_asset "$github_release" "$release_asset" "$pkg_version" "$stage_dir" || return 1

    local project_dir="$_project_dir"
    pkg_version="$_release_version"

    # ---------------------------------------------------------------
    # Collect artifacts based on runtime type
    # ---------------------------------------------------------------
    case "$runtime" in
        native-binary|native)
            local dist_dir="${project_dir}/dist"
            if [ ! -d "$dist_dir" ]; then
                err "dist/ directory not found after extraction"
                return 1
            fi

            mkdir -p "${stage_dir}/bin"

            # Find the primary binary — search multiple naming patterns
            local binary=""
            local search_patterns=(
                "${pkg_name}*linux*x64"
                "${pkg_name}*linux*amd64"
                "${pkg_name}-linux*"
                "${pkg_name}"
            )
            for pattern in "${search_patterns[@]}"; do
                binary=$(find "$dist_dir" -maxdepth 2 -name "$pattern" -type f 2>/dev/null | head -1)
                [ -n "$binary" ] && break
            done

            # Last resort: find any executable file
            if [ -z "$binary" ]; then
                binary=$(find "$dist_dir" -maxdepth 2 -type f -executable 2>/dev/null | head -1)
            fi

            if [ -n "$binary" ]; then
                cp "$binary" "${stage_dir}/bin/${pkg_name}"
                chmod +x "${stage_dir}/bin/${pkg_name}"
                log "  Binary: $(basename "$binary") → bin/${pkg_name}"
            else
                err "No binary found in extracted release"
                err "Contents:"
                find "$dist_dir" -type f -maxdepth 2 | head -10 | while read -r f; do err "  ${f#${dist_dir}/}"; done
                return 1
            fi

            # Copy migrations if present
            if [ -d "${dist_dir}/migrations" ]; then
                cp -r "${dist_dir}/migrations" "${stage_dir}/migrations"
                log "  Migrations: $(find "${stage_dir}/migrations" -type f | wc -l) files"
            fi

            # Copy config files if present in extracted release
            for cfg in ".env.example" "env.defaults" "config.example.toml" "config.toml"; do
                if [ -f "${dist_dir}/${cfg}" ] || [ -f "${project_dir}/${cfg}" ]; then
                    mkdir -p "${stage_dir}/etc"
                    local cfg_src="${dist_dir}/${cfg}"
                    [ -f "$cfg_src" ] || cfg_src="${project_dir}/${cfg}"
                    cp "$cfg_src" "${stage_dir}/etc/"
                    log "  Config: ${cfg}"
                fi
            done
            ;;

        flutter)
            local dist_dir="${project_dir}/dist"
            if [ ! -d "$dist_dir" ]; then
                err "dist/ directory not found after extraction"
                return 1
            fi

            mkdir -p "${stage_dir}/bin"

            # Flutter release archives typically contain the full bundle directory.
            # Look for the Flutter engine libraries and the AOT binary.

            # Find Flutter engine .so files
            local engine_count=0
            while IFS= read -r so; do
                [ -z "$so" ] && continue
                cp "$so" "${stage_dir}/bin/"
                log "  Engine: $(basename "$so")"
                engine_count=$((engine_count + 1))
            done < <(find "$dist_dir" -name 'libflutter*.so' -type f 2>/dev/null)

            # Find the app binary (usually the package name, executable)
            local app_binary
            app_binary=$(find "$dist_dir" -maxdepth 3 -name "${pkg_name}" -type f -executable 2>/dev/null | head -1)
            if [ -n "$app_binary" ]; then
                cp "$app_binary" "${stage_dir}/bin/${pkg_name}"
                chmod +x "${stage_dir}/bin/${pkg_name}"
                log "  App binary: ${pkg_name}"
            else
                # Try any executable that's not a .so
                app_binary=$(find "$dist_dir" -maxdepth 3 -type f -executable ! -name '*.so' 2>/dev/null | head -1)
                if [ -n "$app_binary" ]; then
                    cp "$app_binary" "${stage_dir}/bin/${pkg_name}"
                    chmod +x "${stage_dir}/bin/${pkg_name}"
                    log "  App binary: $(basename "$app_binary") → bin/${pkg_name}"
                fi
            fi

            # Copy flutter_assets
            local assets_dir
            assets_dir=$(find "$dist_dir" -type d -name 'flutter_assets' 2>/dev/null | head -1)
            if [ -n "$assets_dir" ]; then
                mkdir -p "${stage_dir}/assets"
                cp -r "$assets_dir" "${stage_dir}/assets/"
                log "  Assets: flutter_assets/"
            fi

            # Copy all .so files into lib/
            local so_count=0
            while IFS= read -r so; do
                [ -z "$so" ] && continue
                mkdir -p "${stage_dir}/lib"
                cp "$so" "${stage_dir}/lib/"
                so_count=$((so_count + 1))
            done < <(find "$dist_dir" -name '*.so' -type f ! -name 'libflutter*' 2>/dev/null)
            [ "$so_count" -gt 0 ] && log "  Libraries: ${so_count} shared objects"

            # Copy icudtl.dat if present (Flutter needs it)
            local icudata
            icudata=$(find "$dist_dir" -name 'icudtl.dat' -type f 2>/dev/null | head -1)
            if [ -n "$icudata" ]; then
                cp "$icudata" "${stage_dir}/bin/"
                log "  ICU data: icudtl.dat"
            fi
            ;;

        python-container)
            local dist_dir="${project_dir}/dist"
            mkdir -p "${stage_dir}/app"

            if [ -d "$dist_dir" ]; then
                # Copy wheel if available
                local wheel
                wheel=$(find "$dist_dir" -name '*.whl' -type f 2>/dev/null | head -1)
                if [ -n "$wheel" ]; then
                    cp "$wheel" "${stage_dir}/app/"
                    log "  Wheel: $(basename "$wheel")"
                fi
            fi

            # Copy requirements
            local req
            req=$(find "$project_dir" -maxdepth 2 -name 'requirements.txt' -type f 2>/dev/null | head -1)
            if [ -n "$req" ]; then
                cp "$req" "${stage_dir}/app/"
                log "  Requirements: requirements.txt"
            fi

            # Copy config files
            for cfg in ".env.example" "env.defaults"; do
                local cfg_file
                cfg_file=$(find "$project_dir" -maxdepth 2 -name "$cfg" -type f 2>/dev/null | head -1)
                if [ -n "$cfg_file" ]; then
                    mkdir -p "${stage_dir}/etc"
                    cp "$cfg_file" "${stage_dir}/etc/"
                fi
            done
            ;;

        *)
            err "Unknown runtime: ${runtime}"
            return 1
            ;;
    esac

    # ---------------------------------------------------------------
    # Generate manifest.json
    # ---------------------------------------------------------------
    local tags_json
    tags_json=$(parse_section_array "$recipe" "marketplace" "tags" | sed 's/.*/"&"/' | paste -sd',' - | sed 's/^/[/;s/$/]/')
    [ -z "$tags_json" ] && tags_json="[]"

    cat > "${stage_dir}/manifest.json" << MANIFEST
{
  "agent": {
    "name": "${pkg_name}",
    "version": "${pkg_version}",
    "description": "${pkg_desc}",
    "author": "${publisher}"
  },
  "publisher": {
    "name": "${publisher}",
    "key_id": "",
    "homepage": "https://github.com/${github_release}"
  },
  "category": "${category}",
  "runtime": "${runtime}",
  "license": "${pkg_license}",
  "min_agnos_version": "${min_agnos}",
  "tags": ${tags_json},
  "screenshots": [],
  "changelog": "",
  "dependencies": {},
  "source": "https://github.com/${github_release}/releases/tag/${pkg_version}",
  "bundle_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "bundler": "ark-bundle.sh"
}
MANIFEST

    # ---------------------------------------------------------------
    # Generate sandbox.json
    # ---------------------------------------------------------------
    local landlock_rules=""
    # Parse landlock rules from recipe
    while IFS= read -r path; do
        local access
        access=$(grep -A1 "^path = \"${path}\"" "$recipe" 2>/dev/null | grep 'access' | sed 's/.*= *"\(.*\)"/\1/')
        [ -z "$access" ] && access="ro"
        [ -n "$landlock_rules" ] && landlock_rules="${landlock_rules},"
        landlock_rules="${landlock_rules}
    {\"path\": \"${path}\", \"access\": \"${access}\"}"
    done < <(grep '^path = "' "$recipe" 2>/dev/null | sed 's/path = "\(.*\)"/\1/')

    local hosts_json
    hosts_json=$(parse_section_array "$recipe" "marketplace.sandbox" "allowed_hosts" | sed 's/.*/"&"/' | paste -sd',' - | sed 's/^/[/;s/$/]/')
    [ -z "$hosts_json" ] && hosts_json="[]"

    cat > "${stage_dir}/sandbox.json" << SANDBOX
{
  "seccomp_mode": "${seccomp_mode:-desktop}",
  "network": {
    "enabled": ${network_access:-false},
    "allowed_hosts": ${hosts_json}
  },
  "landlock_paths": [${landlock_rules}
  ],
  "data_dir": "${data_dir}"
}
SANDBOX

    # ---------------------------------------------------------------
    # Remove internal dirs before tarball
    # ---------------------------------------------------------------
    rm -rf "${stage_dir}/_project" "${stage_dir}/_download"

    # ---------------------------------------------------------------
    # Create .agnos-agent tarball
    # ---------------------------------------------------------------
    mkdir -p "$OUTPUT_DIR"
    local bundle_file="${OUTPUT_DIR}/${pkg_name}-${pkg_version}.agnos-agent"

    tar czf "$bundle_file" -C "$stage_dir" .

    local bundle_size
    bundle_size=$(du -sh "$bundle_file" | cut -f1)
    local file_count
    file_count=$(tar tzf "$bundle_file" | wc -l)

    ok "Bundled: ${bundle_file} (${bundle_size}, ${file_count} files)"

    # Sign if requested
    if [ "$SIGN_AFTER" = "1" ]; then
        local sign_script="${SCRIPT_DIR}/ark-sign.sh"
        if [ -x "$sign_script" ]; then
            "$sign_script" "$bundle_file"
        else
            warn "Signing requested but ark-sign.sh not found"
        fi
    fi
}

# -----------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------
if [ ${#POSITIONAL[@]} -eq 0 ]; then
    err "No recipe or directory specified"
    echo "Usage: ark-bundle.sh [--sign] <recipe.toml|directory>"
    exit 1
fi

TARGET="${POSITIONAL[0]}"
mkdir -p "$OUTPUT_DIR"

if [ -f "$TARGET" ]; then
    bundle_recipe "$TARGET"
elif [ -d "$TARGET" ]; then
    PASSED=0
    FAILED=0
    log "Bundling all marketplace recipes in ${TARGET}"
    echo ""

    while IFS= read -r -d '' recipe; do
        if bundle_recipe "$recipe"; then
            PASSED=$((PASSED + 1))
        else
            FAILED=$((FAILED + 1))
        fi
        echo ""
    done < <(find "$TARGET" -name '*.toml' -type f -print0 | sort -z)

    echo ""
    ok "Bundled: ${PASSED}, Failed: ${FAILED}"
    [ $FAILED -eq 0 ] || exit 1
else
    err "Not a file or directory: ${TARGET}"
    exit 1
fi
