#!/usr/bin/env bash
# Bump AGNOS version across all files that reference it.
# Usage: ./scripts/version-bump.sh 2026.3.30
set -euo pipefail

[ $# -ne 1 ] && echo "Usage: $0 <version>" && exit 1

NEW_VERSION="$1"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# 1. VERSION file (single source of truth)
echo -n "$NEW_VERSION" > "$REPO_ROOT/VERSION"
echo "VERSION           → $NEW_VERSION"

# 2. Workspace Cargo.toml
sed -i "0,/^version = \".*\"/s/^version = \".*\"/version = \"${NEW_VERSION}\"/" \
    "$REPO_ROOT/userland/Cargo.toml"
echo "userland/Cargo.toml → $NEW_VERSION"

# 3. Regenerate lockfile
cd "$REPO_ROOT/userland" && cargo generate-lockfile 2>/dev/null || true

echo ""
echo "Bumped to ${NEW_VERSION}."
echo "Next: git add -A && git commit && git tag ${NEW_VERSION} && git push --tags"
