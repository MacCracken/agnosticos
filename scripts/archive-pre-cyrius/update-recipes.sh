#!/usr/bin/env bash
# update-recipes.sh — Sync marketplace recipe versions with crates.io / local VERSION files
#
# Usage:
#   ./scripts/update-recipes.sh              # Dry run — show what would change
#   ./scripts/update-recipes.sh --apply      # Apply version updates to recipe files
#   ./scripts/update-recipes.sh --crates-io  # Use crates.io as source of truth
#   ./scripts/update-recipes.sh --local      # Use local VERSION files as source of truth
#
# Default source of truth: crates.io for published crates, local VERSION for unpublished

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CRATES_DIR="${CRATES_DIR:-$(dirname "$REPO_ROOT")}"
RECIPES_DIR="$REPO_ROOT/recipes/marketplace"

DRY_RUN=true
SOURCE="auto"

# Crates we own on crates.io — only these use crates.io as version source
OUR_CRATES="tarang ai-hwaccel hoosh majra kavach ranga dhvani aethersafta agnosai libro bote szal stiva nein abaco murti t-ron impetus"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --apply) DRY_RUN=false; shift ;;
        --crates-io) SOURCE="crates-io"; shift ;;
        --local) SOURCE="local"; shift ;;
        --help|-h)
            echo "Usage: $0 [--apply] [--crates-io|--local]"
            echo ""
            echo "  --apply       Apply changes (default: dry run)"
            echo "  --crates-io   Use crates.io versions as source of truth"
            echo "  --local       Use local VERSION files as source of truth"
            echo "  (default)     crates.io for published, local for unpublished"
            exit 0
            ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
NC='\033[0m'

updated=0
skipped=0
errors=0

if $DRY_RUN; then
    echo -e "${CYAN}=== Recipe Version Sync (dry run) ===${NC}"
else
    echo -e "${CYAN}=== Recipe Version Sync (applying) ===${NC}"
fi
echo ""
printf "  %-20s %-14s %-14s %s\n" "RECIPE" "CURRENT" "NEW" "ACTION"
printf "  %-20s %-14s %-14s %s\n" "------" "-------" "---" "------"

for recipe_file in "$RECIPES_DIR"/*.toml; do
    [[ ! -f "$recipe_file" ]] && continue

    name=$(grep '^name' "$recipe_file" | head -1 | sed 's/.*= *"\([^"]*\)".*/\1/')
    recipe_ver=$(grep '^version' "$recipe_file" | head -1 | sed 's/.*= *"\([^"]*\)".*/\1/')

    [[ -z "$name" || -z "$recipe_ver" ]] && continue

    # Determine new version based on source preference
    new_ver=""

    case "$SOURCE" in
        crates-io)
            result=$(cargo search "$name" --limit 1 2>/dev/null | head -1)
            if echo "$result" | grep -q "^${name} = "; then
                new_ver=$(echo "$result" | sed 's/.*= "\([^"]*\)".*/\1/')
            fi
            ;;
        local)
            if [[ -f "$CRATES_DIR/$name/VERSION" ]]; then
                new_ver=$(cat "$CRATES_DIR/$name/VERSION")
            fi
            ;;
        auto)
            # Only check crates.io for crates we actually own
            is_our_crate=false
            for oc in $OUR_CRATES; do
                [[ "$oc" == "$name" ]] && is_our_crate=true && break
            done

            if $is_our_crate; then
                # Our shared crate — check crates.io first, fall back to local
                result=$(cargo search "$name" --limit 1 2>/dev/null | head -1)
                if echo "$result" | grep -q "^${name} = "; then
                    new_ver=$(echo "$result" | sed 's/.*= "\([^"]*\)".*/\1/')
                elif [[ -f "$CRATES_DIR/$name/VERSION" ]]; then
                    new_ver=$(cat "$CRATES_DIR/$name/VERSION")
                fi
            else
                # Consumer app — local VERSION only (crates.io name collision)
                if [[ -f "$CRATES_DIR/$name/VERSION" ]]; then
                    new_ver=$(cat "$CRATES_DIR/$name/VERSION")
                fi
            fi
            ;;
    esac

    if [[ -z "$new_ver" ]]; then
        printf "  %-20s %-14s %-14s ${YELLOW}no source found${NC}\n" "$name" "$recipe_ver" "—"
        skipped=$((skipped + 1))
        continue
    fi

    if [[ "$recipe_ver" == "$new_ver" ]]; then
        printf "  %-20s %-14s %-14s ${GREEN}up to date${NC}\n" "$name" "$recipe_ver" "$new_ver"
        skipped=$((skipped + 1))
        continue
    fi

    # Version mismatch — update needed
    if $DRY_RUN; then
        printf "  %-20s %-14s %-14s ${YELLOW}would update${NC}\n" "$name" "$recipe_ver" "$new_ver"
    else
        # Update the version line in the recipe
        sed -i "0,/^version = \"${recipe_ver}\"/s//version = \"${new_ver}\"/" "$recipe_file"

        # Also update any Lib: comment that references the old version
        sed -i "s/${name} = \"${recipe_ver}\"/${name} = \"${new_ver}\"/g" "$recipe_file"

        # Update Status comment if it references the old version
        sed -i "s/v${recipe_ver}/v${new_ver}/g" "$recipe_file"

        printf "  %-20s %-14s %-14s ${GREEN}updated${NC}\n" "$name" "$recipe_ver" "$new_ver"
    fi
    updated=$((updated + 1))
done

echo ""
if $DRY_RUN; then
    if [[ $updated -gt 0 ]]; then
        echo -e "  ${YELLOW}$updated recipe(s) would be updated${NC} (run with --apply to update)"
    else
        echo -e "  ${GREEN}All recipes up to date${NC}"
    fi
else
    if [[ $updated -gt 0 ]]; then
        echo -e "  ${GREEN}$updated recipe(s) updated${NC}"
    else
        echo -e "  ${GREEN}All recipes already up to date${NC}"
    fi
fi
echo -e "  $skipped skipped"
echo ""
