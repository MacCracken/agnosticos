#!/usr/bin/env bash
# ark-build-all.sh — Build multiple .ark packages from takumi recipes
#
# Usage:
#   ./scripts/ark-build-all.sh                           # build all recipes
#   ./scripts/ark-build-all.sh recipes/database/          # build one category
#   ./scripts/ark-build-all.sh recipes/python/ recipes/database/  # multiple
#
# Environment variables:
#   Same as ark-build.sh, plus:
#   ARK_CONTINUE_ON_ERROR — set to 1 to continue after failures (default: 0)
#   ARK_DRY_RUN           — set to 1 to list recipes without building
#
# Flags (passed through to ark-build.sh):
#   --sign                — sign packages after build
#   --target ARCH         — cross-compile for target architecture

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ARK_BUILD="${SCRIPT_DIR}/ark-build.sh"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CONTINUE="${ARK_CONTINUE_ON_ERROR:-0}"
DRY_RUN="${ARK_DRY_RUN:-0}"

# Parse flags that pass through to ark-build.sh
BUILD_FLAGS=()
RECIPE_DIRS_RAW=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --sign)        BUILD_FLAGS+=("--sign"); shift ;;
        --target)      BUILD_FLAGS+=("--target" "$2"); shift 2 ;;
        --target=*)    BUILD_FLAGS+=("--target" "${1#--target=}"); shift ;;
        *)             RECIPE_DIRS_RAW+=("$1"); shift ;;
    esac
done

set -- "${RECIPE_DIRS_RAW[@]+"${RECIPE_DIRS_RAW[@]}"}"

# Colors
if [ -t 1 ]; then
    BLUE='\033[36m'; GREEN='\033[32m'; YELLOW='\033[33m'; RED='\033[31m'; BOLD='\033[1m'; NC='\033[0m'
else
    BLUE=''; GREEN=''; YELLOW=''; RED=''; BOLD=''; NC=''
fi

log()  { echo -e "${BLUE}[takumi-all]${NC} $*"; }
ok()   { echo -e "${GREEN}[takumi-all]${NC} $*"; }
warn() { echo -e "${YELLOW}[takumi-all]${NC} $*"; }
err()  { echo -e "${RED}[takumi-all]${NC} $*" >&2; }

# -----------------------------------------------------------------------
# Collect recipes
# -----------------------------------------------------------------------
RECIPE_DIRS=("$@")
if [ ${#RECIPE_DIRS[@]} -eq 0 ]; then
    RECIPE_DIRS=("${REPO_ROOT}/recipes")
fi

RECIPES=()
for dir in "${RECIPE_DIRS[@]}"; do
    # Handle relative paths
    if [[ "$dir" != /* ]]; then
        dir="${REPO_ROOT}/${dir}"
    fi

    if [ -f "$dir" ] && [[ "$dir" == *.toml ]]; then
        RECIPES+=("$dir")
    elif [ -d "$dir" ]; then
        while IFS= read -r -d '' recipe; do
            RECIPES+=("$recipe")
        done < <(find "$dir" -name '*.toml' -type f -print0 | sort -z)
    else
        err "Not a file or directory: $dir"
        exit 1
    fi
done

if [ ${#RECIPES[@]} -eq 0 ]; then
    err "No recipes found"
    exit 1
fi

# -----------------------------------------------------------------------
# Filter out local-only / marketplace recipes (they need special handling)
# -----------------------------------------------------------------------
BUILDABLE=()
SKIPPED=()
for recipe in "${RECIPES[@]}"; do
    if grep -q '^local = true' "$recipe" 2>/dev/null; then
        SKIPPED+=("$recipe (local source)")
    else
        BUILDABLE+=("$recipe")
    fi
done

# -----------------------------------------------------------------------
# Display plan
# -----------------------------------------------------------------------
echo ""
log "${BOLD}Build plan: ${#BUILDABLE[@]} recipes${NC}${BUILD_FLAGS[*]:+ (flags: ${BUILD_FLAGS[*]})}"
echo ""
for i in "${!BUILDABLE[@]}"; do
    name=$(grep -m1 '^name ' "${BUILDABLE[$i]}" | sed 's/.*= *"\(.*\)"/\1/')
    ver=$(grep -m1 '^version ' "${BUILDABLE[$i]}" | sed 's/.*= *"\(.*\)"/\1/')
    rel_path="${BUILDABLE[$i]#${REPO_ROOT}/}"
    printf "  %2d. %-25s %-12s %s\n" $((i+1)) "$name" "$ver" "$rel_path"
done

if [ ${#SKIPPED[@]} -gt 0 ]; then
    echo ""
    warn "Skipping ${#SKIPPED[@]} local-source recipes:"
    for s in "${SKIPPED[@]}"; do
        echo "      ${s#${REPO_ROOT}/}"
    done
fi

echo ""

if [ "$DRY_RUN" = "1" ]; then
    log "Dry run — exiting"
    exit 0
fi

# -----------------------------------------------------------------------
# Build each recipe
# -----------------------------------------------------------------------
PASSED=0
FAILED=0
FAILED_LIST=()
TOTAL_START=$(date +%s)

for i in "${!BUILDABLE[@]}"; do
    recipe="${BUILDABLE[$i]}"
    name=$(grep -m1 '^name ' "$recipe" | sed 's/.*= *"\(.*\)"/\1/')
    ver=$(grep -m1 '^version ' "$recipe" | sed 's/.*= *"\(.*\)"/\1/')
    rel_path="${recipe#${REPO_ROOT}/}"

    echo ""
    log "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log "[$(( i + 1 ))/${#BUILDABLE[@]}] Building ${name} ${ver}"
    log "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    if "$ARK_BUILD" "${BUILD_FLAGS[@]+"${BUILD_FLAGS[@]}"}" "$recipe"; then
        PASSED=$((PASSED + 1))
        ok "[$(( i + 1 ))/${#BUILDABLE[@]}] ${name} ${ver} — SUCCESS"
    else
        FAILED=$((FAILED + 1))
        FAILED_LIST+=("$name $ver ($rel_path)")
        err "[$(( i + 1 ))/${#BUILDABLE[@]}] ${name} ${ver} — FAILED"
        if [ "$CONTINUE" != "1" ]; then
            err "Stopping on first failure. Set ARK_CONTINUE_ON_ERROR=1 to continue."
            exit 1
        fi
    fi
done

TOTAL_END=$(date +%s)
TOTAL_ELAPSED=$((TOTAL_END - TOTAL_START))

# -----------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------
echo ""
log "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
log "Build Summary (${TOTAL_ELAPSED}s)"
log "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
ok  "  Passed:  ${PASSED}"
if [ $FAILED -gt 0 ]; then
    err "  Failed:  ${FAILED}"
    for f in "${FAILED_LIST[@]}"; do
        err "    - $f"
    done
fi
if [ ${#SKIPPED[@]} -gt 0 ]; then
    warn "  Skipped: ${#SKIPPED[@]} (local source)"
fi
echo ""

if [ $FAILED -gt 0 ]; then
    exit 1
fi

ok "All ${PASSED} packages built successfully"
