#!/usr/bin/env bash
# crates-check.sh — Check build status and crates.io availability for all AGNOS shared crates
#
# Usage:
#   ./scripts/crates-check.sh              # Full check (build + search)
#   ./scripts/crates-check.sh --search     # crates.io search only
#   ./scripts/crates-check.sh --build      # cargo check only
#   ./scripts/crates-check.sh --pull       # git pull all repos
#   ./scripts/crates-check.sh --test       # run tests on all repos
#   ./scripts/crates-check.sh --recipes    # check recipe versions vs crates.io/local
#
# Set CRATES_DIR to override the parent directory of crate repos (default: parent of this repo)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CRATES_DIR="${CRATES_DIR:-$(dirname "$REPO_ROOT")}"

# All AGNOS shared crates — published + scaffolded
CRATES=(
    # Published (crates.io)
    "tarang"
    "ai-hwaccel"
    "hoosh"
    "majra"
    "kavach"
    "ranga"
    "dhvani"
    "aethersafta"
    # Scaffolded (not yet published)
    "libro"
    "bote"
    "szal"
    "murti"
    "stiva"
    "nein"
    "tanur"
)

# Consumer apps with marketplace recipes
CONSUMER_APPS=(
    "agnostic"
    "secureyeoman"
    "photisnadi"
    "bullshift"
    "synapse"
    "delta"
    "aequi"
    "shruti"
    "tazama"
    "rasa"
    "mneme"
    "nazar"
    "selah"
    "abaco"
    "rahd"
    "tarang"
    "jalwa"
    "sutra"
    "irfan"
    "vidhana"
)

RECIPES_DIR="$REPO_ROOT/recipes/marketplace"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
NC='\033[0m'

MODE="${1:-all}"

do_build() {
    echo -e "${CYAN}=== cargo check ===${NC}"
    local pass=0 fail=0 skip=0
    for crate in "${CRATES[@]}"; do
        local dir="$CRATES_DIR/$crate"
        if [[ ! -d "$dir" ]]; then
            printf "  %-20s ${YELLOW}NOT FOUND${NC}\n" "$crate"
            skip=$((skip + 1))
            continue
        fi
        if cd "$dir" && cargo check 2>/dev/null; then
            printf "  %-20s ${GREEN}OK${NC}\n" "$crate"
            pass=$((pass + 1))
        else
            printf "  %-20s ${RED}FAIL${NC}\n" "$crate"
            fail=$((fail + 1))
        fi
    done
    echo ""
    echo -e "  ${GREEN}$pass passed${NC}, ${RED}$fail failed${NC}, ${YELLOW}$skip not found${NC}"
    echo ""
}

do_search() {
    echo -e "${CYAN}=== crates.io status ===${NC}"
    printf "  %-20s %-12s %s\n" "CRATE" "VERSION" "STATUS"
    printf "  %-20s %-12s %s\n" "-----" "-------" "------"
    for crate in "${CRATES[@]}"; do
        local result
        result=$(cargo search "$crate" --limit 1 2>/dev/null | head -1)
        if echo "$result" | grep -q "^${crate} = "; then
            local ver
            ver=$(echo "$result" | sed 's/.*= "\([^"]*\)".*/\1/')
            printf "  %-20s %-12s ${GREEN}published${NC}\n" "$crate" "$ver"
        else
            printf "  %-20s %-12s ${YELLOW}available${NC}\n" "$crate" "—"
        fi
    done
    echo ""
}

do_pull() {
    echo -e "${CYAN}=== git pull ===${NC}"
    for crate in "${CRATES[@]}"; do
        local dir="$CRATES_DIR/$crate"
        if [[ ! -d "$dir" ]]; then
            printf "  %-20s ${YELLOW}NOT FOUND${NC}\n" "$crate"
            continue
        fi
        if cd "$dir" && git pull --ff-only 2>/dev/null; then
            printf "  %-20s ${GREEN}OK${NC}\n" "$crate"
        else
            printf "  %-20s ${YELLOW}already up to date or no remote${NC}\n" "$crate"
        fi
    done
    echo ""
}

do_test() {
    echo -e "${CYAN}=== cargo test ===${NC}"
    local pass=0 fail=0 skip=0 total_tests=0
    for crate in "${CRATES[@]}"; do
        local dir="$CRATES_DIR/$crate"
        if [[ ! -d "$dir" ]]; then
            printf "  %-20s ${YELLOW}NOT FOUND${NC}\n" "$crate"
            skip=$((skip + 1))
            continue
        fi
        local output
        if output=$(cd "$dir" && cargo test 2>&1); then
            local count
            count=$(echo "$output" | grep "test result" | grep -oP '\d+ passed' | head -1 | grep -oP '\d+' || echo "0")
            printf "  %-20s ${GREEN}OK${NC} (%s tests)\n" "$crate" "$count"
            pass=$((pass + 1))
            total_tests=$((total_tests + count))
        else
            printf "  %-20s ${RED}FAIL${NC}\n" "$crate"
            fail=$((fail + 1))
        fi
    done
    echo ""
    echo -e "  ${GREEN}$pass passed${NC}, ${RED}$fail failed${NC}, ${YELLOW}$skip not found${NC} ($total_tests total tests)"
    echo ""
}

do_versions() {
    echo -e "${CYAN}=== local versions ===${NC}"
    printf "  %-20s %-12s %s\n" "CRATE" "VERSION" "PATH"
    printf "  %-20s %-12s %s\n" "-----" "-------" "----"
    for crate in "${CRATES[@]}"; do
        local dir="$CRATES_DIR/$crate"
        if [[ ! -d "$dir" ]]; then
            printf "  %-20s ${YELLOW}—${NC}            not found\n" "$crate"
            continue
        fi
        local ver="?"
        if [[ -f "$dir/VERSION" ]]; then
            ver=$(cat "$dir/VERSION")
        fi
        printf "  %-20s %-12s %s\n" "$crate" "$ver" "$dir"
    done
    echo ""
}

do_recipes() {
    echo -e "${CYAN}=== recipe version check ===${NC}"
    printf "  %-20s %-12s %-12s %-12s %s\n" "RECIPE" "RECIPE_VER" "LOCAL_VER" "CRATES.IO" "STATUS"
    printf "  %-20s %-12s %-12s %-12s %s\n" "------" "----------" "---------" "---------" "------"

    local outdated=0
    for recipe_file in "$RECIPES_DIR"/*.toml; do
        [[ ! -f "$recipe_file" ]] && continue
        local name
        name=$(grep '^name' "$recipe_file" | head -1 | sed 's/.*= *"\([^"]*\)".*/\1/')
        local recipe_ver
        recipe_ver=$(grep '^version' "$recipe_file" | head -1 | sed 's/.*= *"\([^"]*\)".*/\1/')

        [[ -z "$name" || -z "$recipe_ver" ]] && continue

        # Check local VERSION file
        local local_ver="—"
        local dir="$CRATES_DIR/$name"
        if [[ -f "$dir/VERSION" ]]; then
            local_ver=$(cat "$dir/VERSION")
        fi

        # Check crates.io (only for published crates)
        local crates_ver="—"
        local has_crates_io
        has_crates_io=$(grep 'crates_io' "$recipe_file" 2>/dev/null || true)
        if [[ -n "$has_crates_io" ]]; then
            local search_result
            search_result=$(cargo search "$name" --limit 1 2>/dev/null | head -1)
            if echo "$search_result" | grep -q "^${name} = "; then
                crates_ver=$(echo "$search_result" | sed 's/.*= "\([^"]*\)".*/\1/')
            fi
        fi

        # Determine status
        local status=""
        if [[ "$local_ver" != "—" && "$local_ver" != "$recipe_ver" ]]; then
            status="${YELLOW}recipe outdated (local: $local_ver)${NC}"
            outdated=$((outdated + 1))
        elif [[ "$crates_ver" != "—" && "$crates_ver" != "$recipe_ver" ]]; then
            status="${YELLOW}recipe outdated (crates.io: $crates_ver)${NC}"
            outdated=$((outdated + 1))
        else
            status="${GREEN}ok${NC}"
        fi

        printf "  %-20s %-12s %-12s %-12s %b\n" "$name" "$recipe_ver" "$local_ver" "$crates_ver" "$status"
    done
    echo ""
    if [[ $outdated -gt 0 ]]; then
        echo -e "  ${YELLOW}$outdated recipe(s) have version mismatches${NC}"
    else
        echo -e "  ${GREEN}All recipes up to date${NC}"
    fi
    echo ""

    # Check for crates missing recipes
    echo -e "${CYAN}=== missing recipes ===${NC}"
    local missing=0
    for crate in "${CRATES[@]}"; do
        if [[ ! -f "$RECIPES_DIR/$crate.toml" ]]; then
            printf "  %-20s ${RED}no recipe${NC}\n" "$crate"
            missing=$((missing + 1))
        fi
    done
    if [[ $missing -eq 0 ]]; then
        echo -e "  ${GREEN}All shared crates have marketplace recipes${NC}"
    else
        echo -e "\n  ${RED}$missing crate(s) missing marketplace recipes${NC}"
    fi
    echo ""
}

case "$MODE" in
    --build|-b)
        do_build
        ;;
    --search|-s)
        do_search
        ;;
    --pull|-p)
        do_pull
        ;;
    --test|-t)
        do_test
        ;;
    --versions|-v)
        do_versions
        ;;
    --recipes|-r)
        do_recipes
        ;;
    all|--all|-a)
        do_versions
        do_build
        do_search
        do_recipes
        ;;
    --help|-h)
        echo "Usage: $0 [--build|--search|--pull|--test|--versions|--recipes|--all|--help]"
        echo ""
        echo "  --build, -b      cargo check all crates"
        echo "  --search, -s     check crates.io availability"
        echo "  --pull, -p       git pull all repos"
        echo "  --test, -t       cargo test all crates"
        echo "  --versions, -v   show local versions"
        echo "  --recipes, -r    check recipe versions vs local/crates.io"
        echo "  --all, -a        versions + build + search + recipes (default)"
        echo ""
        echo "Set CRATES_DIR to override crate repo parent directory."
        ;;
    *)
        echo "Unknown option: $MODE (try --help)"
        exit 1
        ;;
esac
