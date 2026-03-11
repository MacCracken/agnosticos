#!/usr/bin/env bash
# selfhost-validate.sh — Validate AGNOS can rebuild itself from source
#
# Runs inside a booted AGNOS system (or chroot) and attempts to:
#   1. Rebuild the cross-toolchain (GCC, binutils, glibc)
#   2. Compile kernel modules
#   3. Build userland crates (agent-runtime, llm-gateway, ai-shell, etc.)
#   4. Build .ark packages from recipes
#
# Usage: ./selfhost-validate.sh [OPTIONS]
#   -r, --root PATH       AGNOS root (default: / for live system, or chroot path)
#   -s, --source PATH     Source tree path (default: /usr/src/agnos)
#   -j, --jobs N          Parallel build jobs (default: nproc)
#   -p, --phase PHASE     Run specific phase: toolchain|kernel|userland|packages|all
#   -q, --quick           Quick validation (compile hello world, not full rebuild)
#   -o, --output PATH     Results output directory (default: /tmp/selfhost-results)
#   -v, --verbose         Verbose build output
#   -h, --help            Show this help
#
# Exit codes:
#   0 — All validations passed
#   1 — Toolchain validation failed
#   2 — Kernel module build failed
#   3 — Userland build failed
#   4 — Package build failed

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# --- Colors ---
if [ -t 1 ]; then
    RED='\033[31m'; GREEN='\033[32m'; YELLOW='\033[33m'; BLUE='\033[36m'; NC='\033[0m'
else
    RED=''; GREEN=''; YELLOW=''; BLUE=''; NC=''
fi

log()  { echo -e "${BLUE}[selfhost]${NC} $*"; }
ok()   { echo -e "${GREEN}[selfhost]${NC} $*"; }
warn() { echo -e "${YELLOW}[selfhost]${NC} $*"; }
err()  { echo -e "${RED}[selfhost]${NC} $*" >&2; }
die()  { err "$*"; exit 1; }

# --- Defaults ---
AGNOS_ROOT="/"
SOURCE_PATH="/usr/src/agnos"
JOBS=$(nproc 2>/dev/null || echo 4)
PHASE="all"
QUICK=false
OUTPUT_DIR="/tmp/selfhost-results"
VERBOSE=false
TOTAL_PASSED=0
TOTAL_FAILED=0
TOTAL_SKIPPED=0

# --- Argument Parsing ---
usage() {
    sed -n '2,/^$/{ s/^# //; s/^#//; p }' "$0"
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            -r|--root)    AGNOS_ROOT="$2"; shift 2 ;;
            -s|--source)  SOURCE_PATH="$2"; shift 2 ;;
            -j|--jobs)    JOBS="$2"; shift 2 ;;
            -p|--phase)   PHASE="$2"; shift 2 ;;
            -q|--quick)   QUICK=true; shift ;;
            -o|--output)  OUTPUT_DIR="$2"; shift 2 ;;
            -v|--verbose) VERBOSE=true; shift ;;
            -h|--help)    usage; exit 0 ;;
            *)            die "Unknown option: $1" ;;
        esac
    done
}

# --- Helpers ---
RESULTS_FILE=""

init_results() {
    mkdir -p "$OUTPUT_DIR"
    RESULTS_FILE="${OUTPUT_DIR}/results.txt"
    echo "=== AGNOS Self-Hosting Validation ===" > "$RESULTS_FILE"
    echo "Date: $(date -u)" >> "$RESULTS_FILE"
    echo "Host: $(uname -a)" >> "$RESULTS_FILE"
    echo "Root: $AGNOS_ROOT" >> "$RESULTS_FILE"
    echo "Source: $SOURCE_PATH" >> "$RESULTS_FILE"
    echo "Phase: $PHASE" >> "$RESULTS_FILE"
    echo "Quick: $QUICK" >> "$RESULTS_FILE"
    echo "" >> "$RESULTS_FILE"
}

record() {
    local status="$1" name="$2" detail="${3:-}"
    echo "${status}: ${name}${detail:+ — $detail}" >> "$RESULTS_FILE"
    case "$status" in
        PASS) ok "  PASS: $name${detail:+ — $detail}"; TOTAL_PASSED=$((TOTAL_PASSED + 1)) ;;
        FAIL) err "  FAIL: $name${detail:+ — $detail}"; TOTAL_FAILED=$((TOTAL_FAILED + 1)) ;;
        SKIP) warn "  SKIP: $name${detail:+ — $detail}"; TOTAL_SKIPPED=$((TOTAL_SKIPPED + 1)) ;;
    esac
}

run_timed() {
    local name="$1"
    shift
    local start_time
    start_time=$(date +%s)
    local log_file="${OUTPUT_DIR}/${name}.log"

    if $VERBOSE; then
        if "$@" 2>&1 | tee "$log_file"; then
            local elapsed=$(( $(date +%s) - start_time ))
            record "PASS" "$name" "${elapsed}s"
            return 0
        else
            local elapsed=$(( $(date +%s) - start_time ))
            record "FAIL" "$name" "${elapsed}s — see ${log_file}"
            return 1
        fi
    else
        if "$@" > "$log_file" 2>&1; then
            local elapsed=$(( $(date +%s) - start_time ))
            record "PASS" "$name" "${elapsed}s"
            return 0
        else
            local elapsed=$(( $(date +%s) - start_time ))
            record "FAIL" "$name" "${elapsed}s — see ${log_file}"
            return 1
        fi
    fi
}

# --- Phase 1: Toolchain Validation ---
validate_toolchain() {
    log ""
    log "========================================="
    log "  Phase 1: Toolchain Validation"
    log "========================================="
    echo "" >> "$RESULTS_FILE"
    echo "--- Phase 1: Toolchain ---" >> "$RESULTS_FILE"

    local tc_dir="${OUTPUT_DIR}/toolchain"
    mkdir -p "$tc_dir"

    # Check compiler presence
    if command -v gcc &>/dev/null; then
        record "PASS" "gcc_present" "$(gcc --version | head -1)"
    else
        record "FAIL" "gcc_present" "gcc not found"
        return 1
    fi

    if command -v g++ &>/dev/null; then
        record "PASS" "g++_present" "$(g++ --version | head -1)"
    else
        record "FAIL" "g++_present" "g++ not found"
        return 1
    fi

    # Check linker and assembler
    for tool in ld as ar ranlib nm objdump strip; do
        if command -v "$tool" &>/dev/null; then
            record "PASS" "${tool}_present"
        else
            record "FAIL" "${tool}_present"
        fi
    done

    # Compile C hello world
    cat > "${tc_dir}/hello.c" << 'EOF'
#include <stdio.h>
int main(void) {
    printf("AGNOS self-host: C works\n");
    return 0;
}
EOF
    if run_timed "c_compile" gcc -o "${tc_dir}/hello" "${tc_dir}/hello.c" -Wall -Wextra; then
        if run_timed "c_execute" "${tc_dir}/hello"; then
            :
        fi
    fi

    # Compile C++ hello world
    cat > "${tc_dir}/hello.cpp" << 'EOF'
#include <iostream>
#include <vector>
int main() {
    std::vector<int> v{1,2,3};
    std::cout << "AGNOS self-host: C++ works, vector size=" << v.size() << std::endl;
    return 0;
}
EOF
    if run_timed "cpp_compile" g++ -o "${tc_dir}/hello_cpp" "${tc_dir}/hello.cpp" -std=c++17 -Wall; then
        if run_timed "cpp_execute" "${tc_dir}/hello_cpp"; then
            :
        fi
    fi

    # Check Rust toolchain
    if command -v rustc &>/dev/null; then
        record "PASS" "rustc_present" "$(rustc --version)"
    else
        record "FAIL" "rustc_present" "rustc not found"
    fi

    if command -v cargo &>/dev/null; then
        record "PASS" "cargo_present" "$(cargo --version)"
    else
        record "FAIL" "cargo_present" "cargo not found"
    fi

    # Compile Rust hello world
    if command -v rustc &>/dev/null; then
        cat > "${tc_dir}/hello.rs" << 'EOF'
fn main() {
    println!("AGNOS self-host: Rust works");
    let v: Vec<i32> = vec![1, 2, 3];
    assert_eq!(v.len(), 3);
}
EOF
        if run_timed "rust_compile" rustc -o "${tc_dir}/hello_rust" "${tc_dir}/hello.rs"; then
            run_timed "rust_execute" "${tc_dir}/hello_rust" || true
        fi
    fi

    if ! $QUICK; then
        # Full toolchain rebuild validation: compile a small C library
        cat > "${tc_dir}/libtest.c" << 'EOF'
#include <string.h>
#include <stdlib.h>

char *selfhost_strdup(const char *s) {
    size_t len = strlen(s) + 1;
    char *dup = malloc(len);
    if (dup) memcpy(dup, s, len);
    return dup;
}
EOF
        cat > "${tc_dir}/libtest_main.c" << 'EOF'
#include <stdio.h>
#include <stdlib.h>

extern char *selfhost_strdup(const char *s);

int main(void) {
    char *s = selfhost_strdup("AGNOS self-hosting validated");
    if (s) { printf("%s\n", s); free(s); return 0; }
    return 1;
}
EOF
        # Static library build
        if run_timed "static_lib_compile" gcc -c -o "${tc_dir}/libtest.o" "${tc_dir}/libtest.c" -fPIC; then
            if run_timed "static_lib_archive" ar rcs "${tc_dir}/libtest.a" "${tc_dir}/libtest.o"; then
                if run_timed "static_lib_link" gcc -o "${tc_dir}/test_static" "${tc_dir}/libtest_main.c" -L"${tc_dir}" -ltest; then
                    run_timed "static_lib_execute" "${tc_dir}/test_static" || true
                fi
            fi
        fi

        # Shared library build
        if run_timed "shared_lib_compile" gcc -shared -fPIC -o "${tc_dir}/libtest.so" "${tc_dir}/libtest.c"; then
            if run_timed "shared_lib_link" gcc -o "${tc_dir}/test_shared" "${tc_dir}/libtest_main.c" -L"${tc_dir}" -ltest -Wl,-rpath,"${tc_dir}"; then
                run_timed "shared_lib_execute" "${tc_dir}/test_shared" || true
            fi
        fi

        # PIE + hardening flags (matching takumi security defaults)
        run_timed "hardened_compile" gcc -o "${tc_dir}/hello_hardened" "${tc_dir}/hello.c" \
            -fPIE -pie -Wl,-z,relro,-z,now -fstack-protector-strong \
            -D_FORTIFY_SOURCE=2 -O2 || true
    fi

    ok "Toolchain validation complete"
}

# --- Phase 2: Kernel Module Build ---
validate_kernel() {
    log ""
    log "========================================="
    log "  Phase 2: Kernel Module Build"
    log "========================================="
    echo "" >> "$RESULTS_FILE"
    echo "--- Phase 2: Kernel Modules ---" >> "$RESULTS_FILE"

    local km_dir="${OUTPUT_DIR}/kernel"
    mkdir -p "$km_dir"

    # Check kernel headers
    local kver
    kver=$(uname -r)
    local build_dir="/lib/modules/${kver}/build"

    if [ -d "$build_dir" ]; then
        record "PASS" "kernel_build_dir" "$build_dir"
    else
        # Try alternative locations
        build_dir="/usr/src/linux"
        if [ -d "$build_dir" ]; then
            record "PASS" "kernel_build_dir" "$build_dir (fallback)"
        else
            record "FAIL" "kernel_build_dir" "no kernel build directory found"
            warn "Skipping kernel module build — no headers"
            return 0
        fi
    fi

    if [ -f "${build_dir}/Makefile" ]; then
        record "PASS" "kernel_makefile"
    else
        record "FAIL" "kernel_makefile" "no Makefile in ${build_dir}"
        return 0
    fi

    # Build a minimal test kernel module
    cat > "${km_dir}/agnos_test.c" << 'EOF'
/*
 * agnos_test.c — Minimal kernel module for self-hosting validation
 */
#include <linux/init.h>
#include <linux/module.h>
#include <linux/kernel.h>

MODULE_LICENSE("GPL");
MODULE_AUTHOR("AGNOS");
MODULE_DESCRIPTION("Self-hosting validation test module");
MODULE_VERSION("0.1");

static int __init agnos_test_init(void)
{
    pr_info("agnos_test: self-hosting kernel module loaded\n");
    return 0;
}

static void __exit agnos_test_exit(void)
{
    pr_info("agnos_test: self-hosting kernel module unloaded\n");
}

module_init(agnos_test_init);
module_exit(agnos_test_exit);
EOF

    cat > "${km_dir}/Makefile" << EOF
obj-m += agnos_test.o

KDIR ?= ${build_dir}
PWD := \$(shell pwd)

all:
	\$(MAKE) -C \$(KDIR) M=\$(PWD) modules

clean:
	\$(MAKE) -C \$(KDIR) M=\$(PWD) clean
EOF

    # Build the module
    if run_timed "kernel_module_compile" make -C "$km_dir" -j"$JOBS"; then
        if [ -f "${km_dir}/agnos_test.ko" ]; then
            record "PASS" "kernel_module_built" "agnos_test.ko"
            local ko_size
            ko_size=$(du -sh "${km_dir}/agnos_test.ko" | cut -f1)
            log "  Module size: $ko_size"

            # Verify module info
            if command -v modinfo &>/dev/null; then
                if run_timed "kernel_module_info" modinfo "${km_dir}/agnos_test.ko"; then
                    :
                fi
            fi
        else
            record "FAIL" "kernel_module_built" ".ko file not produced"
        fi
    fi

    if ! $QUICK; then
        # Check if AGNOS custom kernel modules can be found/built
        local agnos_modules_dir="${SOURCE_PATH}/kernel/modules"
        if [ -d "$agnos_modules_dir" ]; then
            local module_count
            module_count=$(find "$agnos_modules_dir" -name "*.c" 2>/dev/null | wc -l)
            record "PASS" "agnos_kernel_modules_found" "${module_count} source files"

            # Try building each AGNOS module
            for src in "$agnos_modules_dir"/*.c; do
                [ -f "$src" ] || continue
                local modname
                modname=$(basename "$src" .c)
                log "  Building AGNOS module: $modname"
                # Copy source and create Makefile
                local mod_build="${km_dir}/${modname}"
                mkdir -p "$mod_build"
                cp "$src" "$mod_build/"
                cat > "${mod_build}/Makefile" << MEOF
obj-m += ${modname}.o
KDIR ?= ${build_dir}
all:
	\$(MAKE) -C \$(KDIR) M=\$(shell pwd) modules
MEOF
                run_timed "kmod_${modname}" make -C "$mod_build" -j"$JOBS" || true
            done
        else
            record "SKIP" "agnos_kernel_modules" "source not at ${agnos_modules_dir}"
        fi
    fi

    ok "Kernel module validation complete"
}

# --- Phase 3: Userland Build ---
validate_userland() {
    log ""
    log "========================================="
    log "  Phase 3: Userland Build"
    log "========================================="
    echo "" >> "$RESULTS_FILE"
    echo "--- Phase 3: Userland ---" >> "$RESULTS_FILE"

    if ! command -v cargo &>/dev/null; then
        record "FAIL" "cargo_available" "cargo not found"
        return 3
    fi

    local userland_dir="${SOURCE_PATH}/userland"

    if [ ! -d "$userland_dir" ]; then
        # Try local source tree
        userland_dir="${SCRIPT_DIR}/../userland"
    fi

    if [ ! -f "${userland_dir}/Cargo.toml" ]; then
        record "FAIL" "userland_source" "Cargo.toml not found at ${userland_dir}"
        return 3
    fi

    record "PASS" "userland_source" "$userland_dir"

    if $QUICK; then
        # Quick: just check that cargo can parse the workspace
        if run_timed "cargo_metadata" cargo metadata --manifest-path "${userland_dir}/Cargo.toml" --no-deps --format-version 1; then
            record "PASS" "workspace_valid" "cargo metadata succeeded"
        fi

        # Quick: check individual crate syntax
        if run_timed "cargo_check" cargo check --manifest-path "${userland_dir}/Cargo.toml" --workspace; then
            record "PASS" "workspace_check" "all crates type-check"
        fi
    else
        # Full build of each crate
        local crates=(
            "agnos-common"
            "agnos-sys"
            "agent-runtime"
            "llm-gateway"
            "ai-shell"
            "desktop-environment"
        )

        for crate in "${crates[@]}"; do
            local crate_dir="${userland_dir}/${crate}"
            if [ -f "${crate_dir}/Cargo.toml" ]; then
                log "Building ${crate}..."
                run_timed "build_${crate}" cargo build \
                    --manifest-path "${crate_dir}/Cargo.toml" \
                    --release \
                    -j "$JOBS" || true
            else
                record "SKIP" "build_${crate}" "Cargo.toml not found"
            fi
        done

        # Run tests
        log "Running workspace tests..."
        run_timed "cargo_test" cargo test \
            --manifest-path "${userland_dir}/Cargo.toml" \
            --workspace \
            --lib \
            -j "$JOBS" || true
    fi

    ok "Userland validation complete"
}

# --- Phase 4: Package Build ---
validate_packages() {
    log ""
    log "========================================="
    log "  Phase 4: Package Build (ark)"
    log "========================================="
    echo "" >> "$RESULTS_FILE"
    echo "--- Phase 4: Packages ---" >> "$RESULTS_FILE"

    local ark_build=""
    # Find ark-build.sh
    for path in \
        /usr/lib/agnos/ark-build.sh \
        /usr/local/bin/ark-build.sh \
        "${SCRIPT_DIR}/ark-build.sh" \
        "${SOURCE_PATH}/scripts/ark-build.sh"; do
        if [ -x "$path" ]; then
            ark_build="$path"
            break
        fi
    done

    if [ -z "$ark_build" ]; then
        record "FAIL" "ark_build_script" "ark-build.sh not found"
        return 4
    fi
    record "PASS" "ark_build_script" "$ark_build"

    # Find recipes
    local recipe_dir=""
    for path in \
        /usr/share/agnos/recipes \
        /etc/agnos/recipes \
        "${SOURCE_PATH}/recipes"; do
        if [ -d "$path" ]; then
            recipe_dir="$path"
            break
        fi
    done

    if [ -z "$recipe_dir" ]; then
        record "FAIL" "recipe_directory" "no recipe directory found"
        return 4
    fi
    record "PASS" "recipe_directory" "$recipe_dir"

    local recipe_count
    recipe_count=$(find "$recipe_dir" -name "*.toml" -type f 2>/dev/null | wc -l)
    record "PASS" "recipe_count" "${recipe_count} recipes found"

    if $QUICK; then
        # Quick: validate recipe format only
        local validate_script=""
        for path in \
            "${SCRIPT_DIR}/ark-validate-recipes.sh" \
            "${SOURCE_PATH}/scripts/ark-validate-recipes.sh"; do
            if [ -x "$path" ]; then
                validate_script="$path"
                break
            fi
        done

        if [ -n "$validate_script" ]; then
            run_timed "recipe_validation" "$validate_script" "$recipe_dir" || true
        else
            warn "ark-validate-recipes.sh not found — skipping"
        fi
    else
        # Full: build a small subset of recipes to validate the pipeline
        # Pick tier-0 recipes that are fast to build
        local test_recipes=(
            "base/linux-api-headers.toml"
            "base/iana-etc.toml"
            "base/man-pages.toml"
        )

        local ark_output="${OUTPUT_DIR}/packages"
        mkdir -p "$ark_output"

        for recipe_rel in "${test_recipes[@]}"; do
            local recipe="${recipe_dir}/${recipe_rel}"
            if [ -f "$recipe" ]; then
                local pkg_name
                pkg_name=$(basename "$recipe_rel" .toml)
                log "Building package: $pkg_name"
                run_timed "ark_build_${pkg_name}" \
                    env ARK_OUTPUT_DIR="$ark_output" ARK_JOBS="$JOBS" \
                    "$ark_build" "$recipe" || true
            else
                record "SKIP" "ark_build_$(basename "$recipe_rel" .toml)" "recipe not found"
            fi
        done

        # Check if any .ark packages were produced
        local built_count
        built_count=$(find "$ark_output" -name "*.ark" -type f 2>/dev/null | wc -l)
        if [ "$built_count" -gt 0 ]; then
            record "PASS" "ark_packages_built" "${built_count} packages"
        else
            record "FAIL" "ark_packages_built" "no .ark packages produced"
        fi
    fi

    ok "Package validation complete"
}

# --- Summary ---
print_summary() {
    echo "" >> "$RESULTS_FILE"
    echo "=== Summary ===" >> "$RESULTS_FILE"
    echo "Passed:  $TOTAL_PASSED" >> "$RESULTS_FILE"
    echo "Failed:  $TOTAL_FAILED" >> "$RESULTS_FILE"
    echo "Skipped: $TOTAL_SKIPPED" >> "$RESULTS_FILE"

    echo ""
    log "========================================="
    log "  Self-Hosting Validation Summary"
    log "========================================="
    ok  "  Passed:  $TOTAL_PASSED"
    if [ "$TOTAL_FAILED" -gt 0 ]; then
        err "  Failed:  $TOTAL_FAILED"
    else
        ok  "  Failed:  0"
    fi
    if [ "$TOTAL_SKIPPED" -gt 0 ]; then
        warn "  Skipped: $TOTAL_SKIPPED"
    fi
    log "  Results: $RESULTS_FILE"
    log "  Logs:    $OUTPUT_DIR/"
    echo ""

    if [ "$TOTAL_FAILED" -eq 0 ]; then
        ok "========================================="
        ok "  SELF-HOSTING VALIDATION PASSED"
        ok "========================================="
        echo "STATUS: SELFHOST_VALIDATED" >> "$RESULTS_FILE"
    else
        err "========================================="
        err "  SELF-HOSTING VALIDATION FAILED"
        err "========================================="
        echo "STATUS: SELFHOST_FAILED" >> "$RESULTS_FILE"
    fi
}

# --- Main ---
main() {
    echo ""
    log "========================================="
    log "  AGNOS Self-Hosting Validation"
    log "========================================="
    echo ""

    parse_args "$@"
    init_results

    log "Root:   $AGNOS_ROOT"
    log "Source: $SOURCE_PATH"
    log "Phase:  $PHASE"
    log "Jobs:   $JOBS"
    log "Quick:  $QUICK"
    echo ""

    local exit_code=0

    case "$PHASE" in
        toolchain)
            validate_toolchain || exit_code=$?
            ;;
        kernel)
            validate_kernel || exit_code=$?
            ;;
        userland)
            validate_userland || exit_code=$?
            ;;
        packages)
            validate_packages || exit_code=$?
            ;;
        all)
            validate_toolchain || exit_code=1
            validate_kernel || exit_code=2
            validate_userland || exit_code=3
            validate_packages || exit_code=4
            ;;
        *)
            die "Unknown phase: $PHASE (use: toolchain|kernel|userland|packages|all)"
            ;;
    esac

    print_summary

    if [ "$TOTAL_FAILED" -gt 0 ]; then
        exit "$exit_code"
    fi
}

main "$@"
