#!/bin/bash
# coverage.sh — Run test coverage for the AGNOS userland workspace
#
# Uses cargo-tarpaulin with per-crate parallelism for speed.
#
# Usage:
#   ./scripts/coverage.sh              # All crates, parallel
#   ./scripts/coverage.sh agent_runtime # Single crate
#   ./scripts/coverage.sh --html       # Generate HTML report
#   ./scripts/coverage.sh --quick      # Skip slow crates (desktop-env)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
USERLAND="$REPO_ROOT/userland"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

CRATES=(agent_runtime llm_gateway ai_shell desktop_environment agnos_common agnos_sys)
QUICK_CRATES=(agent_runtime llm_gateway ai_shell)
OUTPUT_FORMAT="Stdout"
SINGLE_CRATE=""
QUICK=false
JOBS=4
TIMEOUT=300

usage() {
    cat << 'EOF'
Usage: coverage.sh [OPTIONS] [CRATE]

Run tarpaulin code coverage for the AGNOS userland workspace.

Arguments:
    CRATE               Single crate to measure (e.g., agent_runtime)

Options:
    --html              Generate HTML report in coverage/
    --xml               Generate Cobertura XML for CI
    --quick             Skip desktop-environment and agnos-sys (faster)
    -j, --jobs N        Parallel test threads (default: 4)
    -t, --timeout N     Per-test timeout in seconds (default: 300)
    -h, --help          Show this help
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --html)
            OUTPUT_FORMAT="Html"
            shift
            ;;
        --xml)
            OUTPUT_FORMAT="Xml"
            shift
            ;;
        --quick)
            QUICK=true
            shift
            ;;
        -j|--jobs)
            JOBS="$2"
            shift 2
            ;;
        -t|--timeout)
            TIMEOUT="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            SINGLE_CRATE="$1"
            shift
            ;;
    esac
done

# Check tarpaulin
if ! command -v cargo-tarpaulin &>/dev/null; then
    echo -e "${RED}cargo-tarpaulin not installed${NC}"
    echo "Install with: cargo install cargo-tarpaulin"
    exit 1
fi

cd "$USERLAND"

if [[ -n "$SINGLE_CRATE" ]]; then
    echo -e "${BLUE}Running coverage for: ${SINGLE_CRATE}${NC}"
    cargo tarpaulin -p "$SINGLE_CRATE" \
        --timeout "$TIMEOUT" \
        --jobs "$JOBS" \
        --out "$OUTPUT_FORMAT" \
        ${OUTPUT_FORMAT:+$([ "$OUTPUT_FORMAT" = "Html" ] && echo "--output-dir $REPO_ROOT/coverage" || true)}
    exit 0
fi

# Parallel per-crate coverage
if [[ "$QUICK" == true ]]; then
    TARGET_CRATES=("${QUICK_CRATES[@]}")
    echo -e "${YELLOW}Quick mode: skipping desktop-environment, agnos-common, agnos-sys${NC}"
else
    TARGET_CRATES=("${CRATES[@]}")
fi

echo -e "${BLUE}Running coverage for ${#TARGET_CRATES[@]} crates (${JOBS} threads each)${NC}"
echo ""

TMPDIR=$(mktemp -d)
PIDS=()

for crate in "${TARGET_CRATES[@]}"; do
    echo -e "${GREEN}Starting: ${crate}${NC}"
    cargo tarpaulin -p "$crate" \
        --timeout "$TIMEOUT" \
        --jobs "$JOBS" \
        --out Stdout 2>&1 | tee "$TMPDIR/${crate}.out" &
    PIDS+=($!)
done

# Wait and collect results
FAILED=0
for i in "${!PIDS[@]}"; do
    if ! wait "${PIDS[$i]}"; then
        echo -e "${RED}FAILED: ${TARGET_CRATES[$i]}${NC}"
        FAILED=$((FAILED + 1))
    fi
done

echo ""
echo -e "${BLUE}=== Coverage Summary ===${NC}"
echo ""

TOTAL_LINES=0
TOTAL_COVERED=0

for crate in "${TARGET_CRATES[@]}"; do
    if [[ -f "$TMPDIR/${crate}.out" ]]; then
        # Extract the coverage percentage line
        COV_LINE=$(grep -oP '\d+\.\d+% coverage, \d+/\d+ lines covered' "$TMPDIR/${crate}.out" 2>/dev/null | tail -1)
        if [[ -n "$COV_LINE" ]]; then
            PCT=$(echo "$COV_LINE" | grep -oP '^\d+\.\d+')
            COVERED=$(echo "$COV_LINE" | grep -oP '\d+(?=/\d+ lines)')
            TOTAL=$(echo "$COV_LINE" | grep -oP '(?<=/)(\d+)(?= lines)')
            printf "  %-25s %6s%%  (%s/%s lines)\n" "$crate" "$PCT" "$COVERED" "$TOTAL"
            TOTAL_COVERED=$((TOTAL_COVERED + COVERED))
            TOTAL_LINES=$((TOTAL_LINES + TOTAL))
        else
            printf "  %-25s  %s\n" "$crate" "no data"
        fi
    fi
done

echo ""
if [[ $TOTAL_LINES -gt 0 ]]; then
    OVERALL=$(echo "scale=1; $TOTAL_COVERED * 100 / $TOTAL_LINES" | bc)
    echo -e "${GREEN}Overall: ${OVERALL}% (${TOTAL_COVERED}/${TOTAL_LINES} lines)${NC}"
else
    echo -e "${RED}No coverage data collected${NC}"
fi

if [[ $FAILED -gt 0 ]]; then
    echo -e "${RED}${FAILED} crate(s) failed${NC}"
    exit 1
fi

rm -rf "$TMPDIR"
echo ""
echo "Done."
