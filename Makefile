# AGNOS Makefile — Genesis build orchestration
# The sovereign boot pipeline. Cyrius all the way down.

VERSION := $(shell cat VERSION 2>/dev/null || echo 'dev')
ARCH := $(shell uname -m)
BUILD_DIR := build
DIST_DIR := dist

# Cyrius toolchain — always invoke via the `cyrius` wrapper so this
# Makefile doesn't couple to the compiler binary name (cc5 today,
# cyc after v6.0.0).
CYRIUS_HOME := $(HOME)/.cyrius
CYRIUS := $(CYRIUS_HOME)/bin/cyrius

# Sibling repos
AGNOS_REPO := ../agnos
CYRIUS_REPO := ../cyrius
ZUGOT_REPO := ../zugot

# Colors
BLUE := \033[36m
GREEN := \033[32m
YELLOW := \033[33m
RED := \033[31m
NC := \033[0m

.PHONY: all help check boot boot-test boot-iso iso-check scripts clean version

all: help

help:
	@echo "$(BLUE)AGNOS Genesis Build System$(NC) (v$(VERSION))"
	@echo ""
	@echo "$(GREEN)Boot targets:$(NC)"
	@echo "  $(YELLOW)boot$(NC)          - Direct boot AGNOS kernel in QEMU"
	@echo "  $(YELLOW)boot-test$(NC)     - Boot + validate serial output"
	@echo "  $(YELLOW)iso-check$(NC)     - Verify all ISO components are present"
	@echo "  $(YELLOW)boot-iso$(NC)      - Build bootable ISO (not yet implemented)"
	@echo ""
	@echo "$(GREEN)Build targets:$(NC)"
	@echo "  $(YELLOW)scripts$(NC)       - Build Cyrius boot scripts"
	@echo "  $(YELLOW)check$(NC)         - Check build environment"
	@echo "  $(YELLOW)clean$(NC)         - Clean build artifacts"
	@echo ""
	@echo "$(GREEN)Info targets:$(NC)"
	@echo "  $(YELLOW)version$(NC)       - Show version info"
	@echo "  $(YELLOW)status$(NC)        - Show component status"
	@echo ""
	@echo "$(GREEN)Kernel:$(NC)  $(AGNOS_REPO)/build/agnos (built by agnos repo)"
	@echo "$(GREEN)Cyrius:$(NC) $(shell $(CYRIUS) --version 2>/dev/null || echo 'not installed')"

# Check build environment
check:
	@echo "$(BLUE)Checking build environment...$(NC)"
	@which $(CYRIUS) > /dev/null 2>&1 || (echo "$(RED)Error: Cyrius toolchain not found at $(CYRIUS_HOME)$(NC)" && exit 1)
	@which qemu-system-x86_64 > /dev/null 2>&1 || (echo "$(RED)Error: qemu-system-x86_64 not found$(NC)" && exit 1)
	@echo "$(GREEN)Build environment OK$(NC)"
	@echo "  Cyrius:  $(shell $(CYRIUS) --version 2>/dev/null)"
	@echo "  QEMU:    $(shell qemu-system-x86_64 --version 2>/dev/null | head -1)"
	@test -f $(AGNOS_REPO)/build/agnos && echo "  Kernel:  $(shell wc -c < $(AGNOS_REPO)/build/agnos) bytes" || echo "  Kernel:  $(RED)not found at $(AGNOS_REPO)/build/agnos$(NC)"

# Build boot scripts (Cyrius)
scripts:
	@echo "$(BLUE)Building boot scripts...$(NC)"
	cd scripts && $(CYRIUS) deps && $(CYRIUS) build src/boot.cyr build/boot
	@echo "$(GREEN)Boot scripts built: scripts/build/boot ($(shell wc -c < scripts/build/boot 2>/dev/null || echo '?') bytes)$(NC)"

# Boot targets
boot: scripts
	@echo "$(BLUE)Booting AGNOS...$(NC)"
	cd scripts && ./build/boot --kernel $(CURDIR)/$(AGNOS_REPO)/build/agnos

boot-test: scripts
	@echo "$(BLUE)Boot + validate...$(NC)"
	cd scripts && ./build/boot --test --kernel $(CURDIR)/$(AGNOS_REPO)/build/agnos

iso-check: scripts
	@echo "$(BLUE)Checking ISO components...$(NC)"
	cd scripts && ./build/boot --iso-check

boot-iso: iso-check
	@echo "$(RED)ISO assembly not yet implemented$(NC)"
	@echo "Run 'make iso-check' to see component readiness."
	@exit 1

# Version info
version:
	@echo "AGNOS $(VERSION)"
	@echo "  Cyrius:    $(shell $(CYRIUS) --version 2>/dev/null || echo 'not installed')"
	@echo "  Kernel:    $(shell cat $(AGNOS_REPO)/VERSION 2>/dev/null || echo 'unknown')"
	@echo "  Toolchain: $(shell grep -oP '(?<=^cyrius = ")[^"]+' scripts/cyrius.cyml 2>/dev/null || echo 'unknown')"

# Component status
status:
	@echo "$(BLUE)Component Status$(NC)"
	@echo ""
	@test -f $(AGNOS_REPO)/build/agnos && echo "  $(GREEN)kernel$(NC)    $(shell wc -c < $(AGNOS_REPO)/build/agnos) bytes (v$(shell cat $(AGNOS_REPO)/VERSION 2>/dev/null))" || echo "  $(RED)kernel$(NC)    not built"
	@test -f scripts/build/boot && echo "  $(GREEN)boot$(NC)      $(shell wc -c < scripts/build/boot) bytes" || echo "  $(RED)boot$(NC)      not built (run: make scripts)"
	@test -x $(CYRIUS) && echo "  $(GREEN)cyrius$(NC)    $(shell $(CYRIUS) --version)" || echo "  $(RED)cyrius$(NC)    not installed"
	@test -d $(ZUGOT_REPO) && echo "  $(GREEN)zugot$(NC)     $(shell ls $(ZUGOT_REPO)/base/*.toml 2>/dev/null | wc -l) base recipes" || echo "  $(YELLOW)zugot$(NC)     not found at $(ZUGOT_REPO)"

# Clean
clean:
	@echo "$(BLUE)Cleaning...$(NC)"
	rm -rf scripts/build/*
	rm -rf $(BUILD_DIR)/*
	rm -rf $(DIST_DIR)/*
	@echo "$(GREEN)Clean$(NC)"
