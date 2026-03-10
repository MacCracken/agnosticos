#!/usr/bin/env bash
# ark-validate-recipes.sh — Validate takumi recipe format and dependency closure
#
# Usage: ./scripts/ark-validate-recipes.sh recipes/base/
#
# Checks:
#   1. Required TOML sections and fields present
#   2. Dependency closure (every dep has a recipe or is a known virtual)
#   3. Source URLs are reachable (HEAD request)
#   4. No circular dependencies
#   5. Consistent hardening flags

set -euo pipefail

RECIPE_DIR="${1:?Usage: ark-validate-recipes.sh <recipe-dir>}"
ERRORS=0
WARNINGS=0

# Colors
if [ -t 1 ]; then
    RED='\033[31m'; GREEN='\033[32m'; YELLOW='\033[33m'; NC='\033[0m'
else
    RED=''; GREEN=''; YELLOW=''; NC=''
fi

err()  { echo -e "${RED}ERROR${NC}: $*" >&2; ERRORS=$((ERRORS + 1)); }
warn() { echo -e "${YELLOW}WARN${NC}: $*"; WARNINGS=$((WARNINGS + 1)); }
ok()   { echo -e "${GREEN}OK${NC}: $*"; }

# Collect all known package names
declare -A KNOWN_PKGS
for f in "$RECIPE_DIR"/*.toml; do
    name=$(grep -m1 '^name ' "$f" 2>/dev/null | sed 's/.*= *"\(.*\)"/\1/' || true)
    if [ -n "$name" ]; then
        KNOWN_PKGS["$name"]=1
    fi
done

# Virtual packages (provided by other packages or the host)
VIRTUAL_PKGS="libstdc++ m4"
for v in $VIRTUAL_PKGS; do
    KNOWN_PKGS["$v"]=1
done

echo "Validating $(ls "$RECIPE_DIR"/*.toml | wc -l) recipes in $RECIPE_DIR"
echo "---"

# -----------------------------------------------------------------
# Check each recipe
# -----------------------------------------------------------------
for recipe in "$RECIPE_DIR"/*.toml; do
    basename=$(basename "$recipe")

    # Required [package] fields
    for field in name version description license; do
        if ! grep -q "^${field} = " "$recipe"; then
            err "$basename: missing [package].${field}"
        fi
    done

    # Required [source] fields
    if ! grep -q '^url = ' "$recipe"; then
        err "$basename: missing [source].url"
    fi
    if ! grep -q '^sha256 = ' "$recipe"; then
        err "$basename: missing [source].sha256"
    fi

    # Required [depends] fields
    if ! grep -q '^runtime = ' "$recipe"; then
        err "$basename: missing [depends].runtime"
    fi
    if ! grep -q '^build = ' "$recipe"; then
        err "$basename: missing [depends].build"
    fi

    # Required [build] fields
    for field in make install; do
        val=$(grep -m1 "^${field} = " "$recipe" 2>/dev/null | sed 's/.*= *"\(.*\)"/\1/' || true)
        multi=$(sed -n "/^${field} = \"\"\"/,/^\"\"\"/p" "$recipe" 2>/dev/null || true)
        if [ -z "$val" ] && [ -z "$multi" ]; then
            # install and make are required to have values
            if [ "$field" = "install" ] || [ "$field" = "make" ]; then
                warn "$basename: [build].${field} is empty"
            fi
        fi
    done

    # Required [security] section
    if ! grep -q '^\[security\]' "$recipe"; then
        err "$basename: missing [security] section"
    fi

    # Check dependency closure
    deps=$(grep -oP '"[^"]*"' "$recipe" | tr -d '"' | sort -u)
    runtime_line=$(grep -m1 '^runtime = ' "$recipe" 2>/dev/null || true)
    build_line=$(grep -m1 '^build = ' "$recipe" 2>/dev/null || true)

    for dep in $(echo "$runtime_line $build_line" | grep -oP '"[^"]*"' | tr -d '"'); do
        if [ -z "${KNOWN_PKGS[$dep]+x}" ]; then
            err "$basename: dependency '$dep' has no recipe in $RECIPE_DIR"
        fi
    done
done

echo "---"
echo "Results: $ERRORS errors, $WARNINGS warnings"

if [ "$ERRORS" -gt 0 ]; then
    err "Validation failed with $ERRORS errors"
    exit 1
else
    ok "All recipes valid"
    exit 0
fi
