#!/usr/bin/env bash
# check-releases.sh — Check latest GitHub release tags for all AGNOS projects
#
# Usage:
#   ./scripts/check-releases.sh              # Show all release versions
#   ./scripts/check-releases.sh --recipes    # Compare against recipe versions
#
# Uses curl to GitHub API (no gh CLI)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RECIPES_DIR="$REPO_ROOT/recipes/marketplace"

# GitHub org
ORG="MacCracken"

# All projects with GitHub repos
declare -A PROJECTS=(
    # Shared crates
    ["tarang"]="MacCracken/tarang"
    ["ai-hwaccel"]="MacCracken/ai-hwaccel"
    ["hoosh"]="MacCracken/hoosh"
    ["majra"]="MacCracken/majra"
    ["kavach"]="MacCracken/kavach"
    ["ranga"]="MacCracken/ranga"
    ["dhvani"]="MacCracken/dhvani"
    ["aethersafta"]="MacCracken/aethersafta"
    ["agnosai"]="MacCracken/agnosai"
    ["libro"]="MacCracken/libro"
    ["bote"]="MacCracken/bote"
    ["szal"]="MacCracken/szal"
    ["stiva"]="MacCracken/stiva"
    ["nein"]="MacCracken/nein"
    ["murti"]="MacCracken/murti"
    ["t-ron"]="MacCracken/t-ron"
    ["impetus"]="MacCracken/impetus"
    ["abaco"]="MacCracken/abaco"
    # Consumer apps
    ["agnostic"]="MacCracken/agnostic"
    ["secureyeoman"]="MacCracken/SecureYeoman"
    ["irfan"]="MacCracken/ifran"
    ["bullshift"]="MacCracken/BullShift"
    ["photisnadi"]="MacCracken/PhotisNadi"
    ["synapse"]="MacCracken/synapse"
    ["delta"]="MacCracken/delta"
    ["aequi"]="anomalyco/aequi"
    ["shruti"]="MacCracken/shruti"
    ["tazama"]="MacCracken/tazama"
    ["rasa"]="MacCracken/rasa"
    ["mneme"]="MacCracken/mneme"
    ["nazar"]="MacCracken/nazar"
    ["selah"]="MacCracken/selah"
    ["abaco"]="MacCracken/abaco"
    ["abacus"]="MacCracken/abacus"
    ["rahd"]="MacCracken/rahd"
    ["jalwa"]="MacCracken/jalwa"
    ["sutra"]="MacCracken/sutra"
    ["sutra-community"]="MacCracken/sutra-community"
    ["tanur"]="MacCracken/tanur"
    ["vidhana"]="MacCracken/vidhana"
)

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
NC='\033[0m'

MODE="${1:-list}"

get_latest_release() {
    local repo="$1"
    local tag
    tag=$(curl -fsSL "https://api.github.com/repos/${repo}/releases/latest" 2>/dev/null \
        | grep '"tag_name"' | head -1 | sed 's/.*: *"\([^"]*\)".*/\1/')
    # Strip leading 'v' if present
    tag="${tag#v}"
    echo "$tag"
}

get_latest_tag() {
    local repo="$1"
    local tag
    # Fall back to tags if no releases
    tag=$(curl -fsSL "https://api.github.com/repos/${repo}/tags?per_page=1" 2>/dev/null \
        | grep '"name"' | head -1 | sed 's/.*: *"\([^"]*\)".*/\1/')
    tag="${tag#v}"
    echo "$tag"
}

do_list() {
    echo -e "${CYAN}=== GitHub Release Versions ===${NC}"
    echo ""
    printf "  %-22s %-16s %-16s %s\n" "PROJECT" "RELEASE" "LOCAL" "STATUS"
    printf "  %-22s %-16s %-16s %s\n" "-------" "-------" "-----" "------"

    # Sort keys
    for name in $(echo "${!PROJECTS[@]}" | tr ' ' '\n' | sort); do
        local repo="${PROJECTS[$name]}"
        local release_ver
        release_ver=$(get_latest_release "$repo")

        # Fall back to latest tag if no release
        if [[ -z "$release_ver" ]]; then
            release_ver=$(get_latest_tag "$repo")
        fi

        [[ -z "$release_ver" ]] && release_ver="—"

        # Local version
        local local_ver="—"
        local crates_dir="${CRATES_DIR:-$(dirname "$REPO_ROOT")}"
        if [[ -f "$crates_dir/$name/VERSION" ]]; then
            local_ver=$(cat "$crates_dir/$name/VERSION")
        fi

        # Status
        local status=""
        if [[ "$release_ver" == "—" ]]; then
            status="${YELLOW}no release${NC}"
        elif [[ "$local_ver" == "—" ]]; then
            status="${YELLOW}no local${NC}"
        elif [[ "$release_ver" == "$local_ver" ]]; then
            status="${GREEN}in sync${NC}"
        else
            status="${YELLOW}mismatch${NC}"
        fi

        printf "  %-22s %-16s %-16s %b\n" "$name" "$release_ver" "$local_ver" "$status"
    done
    echo ""
}

do_recipes() {
    echo -e "${CYAN}=== Recipe vs GitHub Release ===${NC}"
    echo ""
    printf "  %-22s %-14s %-14s %s\n" "RECIPE" "RECIPE_VER" "RELEASE" "STATUS"
    printf "  %-22s %-14s %-14s %s\n" "------" "----------" "-------" "------"

    local outdated=0

    for recipe_file in "$RECIPES_DIR"/*.toml; do
        [[ ! -f "$recipe_file" ]] && continue

        local name
        name=$(grep '^name' "$recipe_file" | head -1 | sed 's/.*= *"\([^"]*\)".*/\1/')
        local recipe_ver
        recipe_ver=$(grep '^version' "$recipe_file" | head -1 | sed 's/.*= *"\([^"]*\)".*/\1/')

        [[ -z "$name" || -z "$recipe_ver" ]] && continue

        local repo="${PROJECTS[$name]:-}"
        if [[ -z "$repo" ]]; then
            printf "  %-22s %-14s %-14s ${YELLOW}no repo mapping${NC}\n" "$name" "$recipe_ver" "—"
            continue
        fi

        local release_ver
        release_ver=$(get_latest_release "$repo")
        if [[ -z "$release_ver" ]]; then
            release_ver=$(get_latest_tag "$repo")
        fi

        if [[ -z "$release_ver" ]]; then
            printf "  %-22s %-14s %-14s ${YELLOW}no release${NC}\n" "$name" "$recipe_ver" "—"
            continue
        fi

        if [[ "$recipe_ver" == "$release_ver" ]]; then
            printf "  %-22s %-14s %-14s ${GREEN}ok${NC}\n" "$name" "$recipe_ver" "$release_ver"
        else
            printf "  %-22s %-14s %-14s ${YELLOW}outdated${NC}\n" "$name" "$recipe_ver" "$release_ver"
            outdated=$((outdated + 1))
        fi
    done

    echo ""
    if [[ $outdated -gt 0 ]]; then
        echo -e "  ${YELLOW}$outdated recipe(s) behind GitHub releases${NC}"
    else
        echo -e "  ${GREEN}All recipes match GitHub releases${NC}"
    fi
    echo ""
}

case "$MODE" in
    list|--list|-l)
        do_list
        ;;
    --recipes|-r)
        do_recipes
        ;;
    --help|-h)
        echo "Usage: $0 [--list|--recipes|--help]"
        echo ""
        echo "  --list, -l     Show latest GitHub release for each project (default)"
        echo "  --recipes, -r  Compare recipe versions against GitHub releases"
        echo ""
        ;;
    *)
        echo "Unknown option: $MODE (try --help)"
        exit 1
        ;;
esac
