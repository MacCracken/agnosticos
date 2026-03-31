#!/usr/bin/env bash
# validate-workflows.sh — Validate GitHub Actions workflow integrity
#
# Checks:
#   1. All YAML files parse correctly
#   2. Every download-artifact has a skip guard or continue-on-error
#   3. No gh CLI usage (project policy: curl to GitHub API only)
#   4. No duplicate workflow names
#   5. Release asset downloads use auth tokens
#
# Usage: ./scripts/validate-workflows.sh

set -euo pipefail

WORKFLOWS_DIR=".github/workflows"
PASS=0
FAIL=0
WARN=0

pass() { echo "  ✓ $1"; PASS=$((PASS + 1)); }
fail() { echo "  ✗ $1"; FAIL=$((FAIL + 1)); }
warn() { echo "  ⚠ $1"; WARN=$((WARN + 1)); }

# ═══════════════════════════════════════════════════════════════════════
# Check 1: YAML validity
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "═══ Check 1: YAML Syntax ═══"
for f in "$WORKFLOWS_DIR"/*.yml; do
  if python3 -c "import yaml; yaml.safe_load(open('$f'))" 2>/dev/null; then
    pass "$(basename "$f")"
  else
    fail "$(basename "$f") — invalid YAML"
  fi
done

# ═══════════════════════════════════════════════════════════════════════
# Check 2: download-artifact safety
#
# Every download-artifact must be protected against skipped upload jobs.
# Safe: has `continue-on-error: true` OR has `if:` guard on the step.
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "═══ Check 2: download-artifact Safety ═══"
for f in "$WORKFLOWS_DIR"/*.yml; do
  fname=$(basename "$f")
  # Get all line numbers with download-artifact
  mapfile -t da_lines < <(grep -n 'uses: actions/download-artifact' "$f" 2>/dev/null | cut -d: -f1)

  for lineno in "${da_lines[@]}"; do
    [[ -z "$lineno" ]] && continue

    # Get artifact name from next few lines
    artifact_name=$(sed -n "$((lineno+1)),$((lineno+5))p" "$f" \
      | grep -m1 'name:' | sed 's/.*name:\s*//' | tr -d ' "'"'" || true)
    artifact_name="${artifact_name:-"(pattern/merge)"}"

    # Check for continue-on-error within 8 lines after
    has_continue=$(sed -n "${lineno},$((lineno+8))p" "$f" \
      | grep -c 'continue-on-error:\s*true' || true)

    # Check for if: guard within 4 lines before
    start=$((lineno > 4 ? lineno-4 : 1))
    has_if_guard=$(sed -n "${start},${lineno}p" "$f" \
      | grep -c 'if:' || true)

    if [[ "$has_continue" -gt 0 ]]; then
      pass "$fname:$lineno ($artifact_name) — continue-on-error"
    elif [[ "$has_if_guard" -gt 0 ]]; then
      pass "$fname:$lineno ($artifact_name) — if: guard"
    else
      # Workflows with skip-conditional jobs (selfhost, build-iso) are dangerous.
      # release.yml's create-release and container jobs have unconditional needs:
      # on build-release, so those are safe. selfhost-build and selfhost-validation
      # have jobs that skip on cache hit, making unguarded downloads fatal.
      if echo "$fname" | grep -qE 'selfhost|build-iso|full-matrix'; then
        fail "$fname:$lineno ($artifact_name) — NO skip guard or continue-on-error (skip-prone workflow)"
      else
        warn "$fname:$lineno ($artifact_name) — no explicit guard (OK if needs: guarantees upload)"
      fi
    fi
  done
done

# ═══════════════════════════════════════════════════════════════════════
# Check 3: No gh CLI usage
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "═══ Check 3: No gh CLI Usage ═══"
for f in "$WORKFLOWS_DIR"/*.yml; do
  fname=$(basename "$f")
  mapfile -t gh_lines < <(grep -nE '\bgh (release|pr|issue|run|api|auth)\b' "$f" 2>/dev/null || true)

  if [[ ${#gh_lines[@]} -eq 0 ]] || [[ -z "${gh_lines[0]}" ]]; then
    pass "$fname — no gh CLI"
  else
    for line in "${gh_lines[@]}"; do
      [[ -z "$line" ]] && continue
      fail "$fname — gh CLI found: $line"
    done
  fi
done

# ═══════════════════════════════════════════════════════════════════════
# Check 4: Duplicate workflow names
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "═══ Check 4: Unique Workflow Names ═══"
declare -A seen_names
for f in "$WORKFLOWS_DIR"/*.yml; do
  wf_name=$(grep -m1 '^name:' "$f" | sed 's/^name:\s*//' | tr -d '"'"'")
  fname=$(basename "$f")
  if [[ -n "${seen_names[$wf_name]:-}" ]]; then
    fail "Duplicate name '$wf_name': $fname and ${seen_names[$wf_name]}"
  else
    seen_names["$wf_name"]="$fname"
    pass "$fname: '$wf_name'"
  fi
done

# ═══════════════════════════════════════════════════════════════════════
# Check 5: Release asset downloads use auth
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "═══ Check 5: Authenticated Release Downloads ═══"
for f in "$WORKFLOWS_DIR"/*.yml; do
  fname=$(basename "$f")
  # Only check actual curl/download lines, not comments
  mapfile -t rel_lines < <(grep -n 'releases/download/' "$f" 2>/dev/null | grep -v '^\s*#' | grep -v '^[0-9]*:\s*#' || true)

  for entry in "${rel_lines[@]}"; do
    [[ -z "$entry" ]] && continue
    lineno=$(echo "$entry" | cut -d: -f1)

    start=$((lineno > 10 ? lineno-10 : 1))
    context=$(sed -n "${start},${lineno}p" "$f")
    if echo "$context" | grep -qi 'authorization\|GITHUB_TOKEN'; then
      pass "$fname:$lineno — authenticated"
    else
      fail "$fname:$lineno — UNAUTHENTICATED release download"
    fi
  done
done

# ═══════════════════════════════════════════════════════════════════════
# Summary
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "══════════════════════════════════════════"
echo "  Passed:   $PASS"
echo "  Failed:   $FAIL"
echo "  Warnings: $WARN"
echo "══════════════════════════════════════════"

if [[ "$FAIL" -gt 0 ]]; then
  echo ""
  echo "WORKFLOW VALIDATION FAILED"
  exit 1
fi

echo ""
echo "ALL CHECKS PASSED"
