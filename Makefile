# AGNOS Makefile
# Top-level build orchestration

# Configuration
VERSION := $(shell cat VERSION 2>/dev/null || echo '2026.3.6')
KERNEL_VERSION := 6.6.0
ARCH := $(shell uname -m)
BUILD_DIR := build
DIST_DIR := dist
CONFIG_DIR := config

# Toolchain
CC := gcc
CXX := g++
CARGO := cargo
RUSTC := rustc
MAKE := make
DOCKER := docker

# Colors for output
BLUE := \033[36m
GREEN := \033[32m
YELLOW := \033[33m
RED := \033[31m
NC := \033[0m # No Color

.PHONY: all help deps build build-kernel build-userland build-initramfs iso install clean test test-unit test-integration test-security test-coverage lint format check docker-dev release ark-build ark-build-all ark-build-python docker-ark-build docker-ark-build-python ark-sign ark-sign-all ark-verify ark-keygen ark-bundle ark-bundle-all

# Default target
all: help

# Help target
help:
	@echo "$(BLUE)AGNOS Build System$(NC)"
	@echo ""
	@echo "$(GREEN)Setup targets:$(NC)"
	@echo "  $(YELLOW)deps$(NC)          - Install build dependencies"
	@echo "  $(YELLOW)check$(NC)         - Check build environment"
	@echo ""
	@echo "$(GREEN)Build targets:$(NC)"
	@echo "  $(YELLOW)build$(NC)         - Build everything (kernel + userland)"
	@echo "  $(YELLOW)build-kernel$(NC)  - Build Linux kernel"
	@echo "  $(YELLOW)build-userland$(NC)- Build userland components"
	@echo "  $(YELLOW)build-initramfs$(NC)- Build initial ramdisk"
	@echo "  $(YELLOW)iso$(NC)           - Create bootable ISO image"
	@echo ""
	@echo "$(GREEN)Installation targets:$(NC)"
	@echo "  $(YELLOW)install$(NC)       - Install AGNOS to target device"
	@echo ""
	@echo "$(GREEN)Test targets:$(NC)"
	@echo "  $(YELLOW)test$(NC)          - Run all tests"
	@echo "  $(YELLOW)test-unit$(NC)     - Run unit tests only"
	@echo "  $(YELLOW)test-integration$(NC)- Run integration tests"
	@echo "  $(YELLOW)test-security$(NC) - Run security tests"
	@echo "  $(YELLOW)test-coverage$(NC) - Run tests with coverage"
	@echo ""
	@echo "$(GREEN)Quality targets:$(NC)"
	@echo "  $(YELLOW)lint$(NC)          - Run linters"
	@echo "  $(YELLOW)format$(NC)        - Format code"
	@echo "  $(YELLOW)security-scan$(NC) - Run security scanners"
	@echo ""
	@echo "$(GREEN)Package targets:$(NC)"
	@echo "  $(YELLOW)ark-build$(NC)     - Build single .ark package (RECIPE=path/to/recipe.toml)"
	@echo "  $(YELLOW)ark-build-all$(NC) - Build all .ark packages from recipes/"
	@echo "  $(YELLOW)ark-build-python$(NC) - Build all Python .ark packages"
	@echo "  $(YELLOW)docker-ark-build$(NC) - Build .ark package in container (RECIPE=...)"
	@echo "  $(YELLOW)docker-ark-build-python$(NC) - Build all Python .ark packages in container"
	@echo ""
	@echo ""
	@echo "$(GREEN)Marketplace targets:$(NC)"
	@echo "  $(YELLOW)ark-bundle$(NC)    - Bundle single .agnos-agent package (RECIPE=recipes/marketplace/foo.toml)"
	@echo "  $(YELLOW)ark-bundle-all$(NC)- Bundle all marketplace recipes"
	@echo ""
	@echo "$(GREEN)Signing targets:$(NC)"
	@echo "  $(YELLOW)ark-keygen$(NC)    - Generate Ed25519 signing keypair"
	@echo "  $(YELLOW)ark-sign$(NC)      - Sign a single .ark package (PKG=path/to/file.ark)"
	@echo "  $(YELLOW)ark-sign-all$(NC)  - Sign all .ark packages in dist/ark/"
	@echo "  $(YELLOW)ark-verify$(NC)    - Verify .ark package signature (PKG=path/to/file.ark)"
	@echo ""
	@echo "$(GREEN)Development targets:$(NC)"
	@echo "  $(YELLOW)docker-dev$(NC)    - Build and run development container"
	@echo "  $(YELLOW)clean$(NC)         - Clean build artifacts"
	@echo "  $(YELLOW)release$(NC)       - Create release build"
	@echo ""
	@echo "$(GREEN)Documentation targets:$(NC)"
	@echo "  $(YELLOW)docs$(NC)          - Build documentation"
	@echo "  $(YELLOW)docs-serve$(NC)    - Serve documentation locally"

# Setup targets
deps:
	@echo "$(BLUE)Installing build dependencies...$(NC)"
	./scripts/install-build-deps.sh
	@echo "$(GREEN)Dependencies installed$(NC)"

check:
	@echo "$(BLUE)Checking build environment...$(NC)"
	@which $(CC) > /dev/null || (echo "$(RED)Error: $(CC) not found$(NC)" && exit 1)
	@which $(CARGO) > /dev/null || (echo "$(RED)Error: cargo not found$(NC)" && exit 1)
	@which $(MAKE) > /dev/null || (echo "$(RED)Error: make not found$(NC)" && exit 1)
	@echo "$(GREEN)Build environment OK$(NC)"
	@echo "$(BLUE)Versions:$(NC)"
	@echo "  GCC: $(shell $(CC) --version | head -1)"
	@echo "  Cargo: $(shell $(CARGO) --version)"
	@echo "  Make: $(shell $(MAKE) --version | head -1)"

# Build targets
build: check build-kernel build-userland build-initramfs
	@echo "$(GREEN)Build complete$(NC)"

build-kernel:
	@echo "$(BLUE)Building kernel...$(NC)"
	$(MAKE) -C kernel defconfig
	$(MAKE) -C kernel -j$$(nproc)
	@echo "$(GREEN)Kernel built$(NC)"

build-userland:
	@echo "$(BLUE)Building userland...$(NC)"
	$(CARGO) build --release
	@echo "$(GREEN)Userland built$(NC)"

build-initramfs:
	@echo "$(BLUE)Building initramfs...$(NC)"
	./scripts/build-initramfs.sh
	@echo "$(GREEN)initramfs built$(NC)"

iso: build
	@echo "$(BLUE)Creating ISO image...$(NC)"
	./scripts/create-iso.sh
	@echo "$(GREEN)ISO created: $(DIST_DIR)/agnos-$(VERSION)-$(ARCH).iso$(NC)"

# Installation targets
install:
	@if [ -z "$(TARGET)" ]; then \
		echo "$(RED)Error: TARGET not specified$(NC)"; \
		echo "Usage: make install TARGET=/dev/sdX"; \
		exit 1; \
	fi
	@echo "$(YELLOW)WARNING: This will erase all data on $(TARGET)$(NC)"
	@read -p "Are you sure? [y/N] " confirm && [ $$confirm = y ] || exit 1
	./scripts/install.sh $(TARGET)

# Test targets
test: test-unit test-integration
	@echo "$(GREEN)All tests passed$(NC)"

test-unit:
	@echo "$(BLUE)Running unit tests...$(NC)"
	$(CARGO) test --lib
	@echo "$(GREEN)Unit tests passed$(NC)"

test-integration:
	@echo "$(BLUE)Running integration tests...$(NC)"
	$(CARGO) test --test '*'
	@echo "$(GREEN)Integration tests passed$(NC)"

test-security:
	@echo "$(BLUE)Running security tests...$(NC)"
	$(CARGO) test --features security-tests
	@echo "$(GREEN)Security tests passed$(NC)"

test-coverage:
	@echo "$(BLUE)Running tests with coverage...$(NC)"
	$(CARGO) install cargo-tarpaulin 2>/dev/null || true
	$(CARGO) tarpaulin --out Xml --out Html
	@echo "$(GREEN)Coverage report generated$(NC)"

# Quality targets
lint:
	@echo "$(BLUE)Running linters...$(NC)"
	$(CARGO) clippy -- -D warnings
	@echo "$(GREEN)Linting complete$(NC)"

format:
	@echo "$(BLUE)Formatting code...$(NC)"
	$(CARGO) fmt
	@echo "$(GREEN)Formatting complete$(NC)"

format-check:
	@echo "$(BLUE)Checking formatting...$(NC)"
	$(CARGO) fmt -- --check

security-scan:
	@echo "$(BLUE)Running security scans...$(NC)"
	$(CARGO) audit
	@echo "$(GREEN)Security scan complete$(NC)"

# Development targets
docker-dev:
	@echo "$(BLUE)Building development container...$(NC)"
	$(DOCKER) build -f Dockerfile.dev -t agnos:dev .
	@echo "$(GREEN)Development container built$(NC)"
	@echo "$(BLUE)Starting development container...$(NC)"
	$(DOCKER) run -it --rm \
		-v $(PWD):/workspace \
		-v agnos-build-cache:/cache \
		agnos:dev /bin/bash

docker-build:
	@echo "$(BLUE)Building in container...$(NC)"
	$(DOCKER) build -f Dockerfile.dev -t agnos:dev .
	$(DOCKER) run --rm \
		-v $(PWD):/workspace \
		-v agnos-build-cache:/cache \
		agnos:dev make build

clean:
	@echo "$(BLUE)Cleaning build artifacts...$(NC)"
	$(CARGO) clean
	rm -rf $(BUILD_DIR)/*
	rm -rf $(DIST_DIR)/*
	$(MAKE) -C kernel clean || true
	@echo "$(GREEN)Clean complete$(NC)"

# Ark package build targets
ark-build:
	@if [ -z "$(RECIPE)" ]; then \
		echo "$(RED)Error: RECIPE not specified$(NC)"; \
		echo "Usage: make ark-build RECIPE=recipes/python/cpython-3.12.toml [SIGN=1] [TARGET=aarch64]"; \
		exit 1; \
	fi
	@echo "$(BLUE)Building ark package from $(RECIPE)...$(NC)"
	./scripts/ark-build.sh $(if $(SIGN),--sign) $(if $(TARGET),--target $(TARGET)) $(RECIPE)
	@echo "$(GREEN)Ark package built$(NC)"

ark-build-all:
	@echo "$(BLUE)Building all ark packages...$(NC)"
	@for recipe in $$(find recipes -name '*.toml' -type f); do \
		echo "$(YELLOW)Building $$recipe...$(NC)"; \
		./scripts/ark-build.sh $$recipe || exit 1; \
	done
	@echo "$(GREEN)All ark packages built$(NC)"

ark-build-python:
	@echo "$(BLUE)Building Python ark packages...$(NC)"
	@for recipe in $$(find recipes/python -name '*.toml' -type f); do \
		echo "$(YELLOW)Building $$recipe...$(NC)"; \
		./scripts/ark-build.sh $$recipe || exit 1; \
	done
	@echo "$(GREEN)Python ark packages built$(NC)"

docker-ark-build:
	@if [ -z "$(RECIPE)" ]; then \
		echo "$(RED)Error: RECIPE not specified$(NC)"; \
		echo "Usage: make docker-ark-build RECIPE=recipes/python/cpython-3.12.toml"; \
		exit 1; \
	fi
	@echo "$(BLUE)Building ark package in container from $(RECIPE)...$(NC)"
	$(DOCKER) build -f docker/Dockerfile.takumi-builder -t agnos:takumi-builder .
	$(DOCKER) run --rm \
		-v $(PWD)/recipes:/recipes:ro \
		-v $(PWD)/$(DIST_DIR)/ark:/output \
		agnos:takumi-builder /recipes/$(notdir $(RECIPE))
	@echo "$(GREEN)Ark package built in container$(NC)"

docker-ark-build-python:
	@echo "$(BLUE)Building all Python ark packages in container...$(NC)"
	$(DOCKER) build -f docker/Dockerfile.takumi-builder -t agnos:takumi-builder .
	@for recipe in $$(find recipes/python -name '*.toml' -type f); do \
		echo "$(YELLOW)Building $$recipe in container...$(NC)"; \
		$(DOCKER) run --rm \
			-v $(PWD)/recipes:/recipes:ro \
			-v $(PWD)/$(DIST_DIR)/ark:/output \
			agnos:takumi-builder /recipes/python/$$(basename $$recipe) || exit 1; \
	done
	@echo "$(GREEN)Python ark packages built in container$(NC)"

# Marketplace bundle targets
ark-bundle:
	@if [ -z "$(RECIPE)" ]; then \
		echo "$(RED)Error: RECIPE not specified$(NC)"; \
		echo "Usage: make ark-bundle RECIPE=recipes/marketplace/secureyeoman.toml"; \
		exit 1; \
	fi
	@echo "$(BLUE)Bundling marketplace package from $(RECIPE)...$(NC)"
	./scripts/ark-bundle.sh $(if $(SIGN),--sign) $(RECIPE)

ark-bundle-all:
	@echo "$(BLUE)Bundling all marketplace packages...$(NC)"
	./scripts/ark-bundle.sh $(if $(SIGN),--sign) recipes/marketplace/
	@echo "$(GREEN)All marketplace packages bundled$(NC)"

# Signing targets
ark-keygen:
	@echo "$(BLUE)Generating Ed25519 signing keypair...$(NC)"
	./scripts/ark-sign.sh --generate-key
	@echo "$(GREEN)Keypair generated$(NC)"

ark-sign:
	@if [ -z "$(PKG)" ]; then \
		echo "$(RED)Error: PKG not specified$(NC)"; \
		echo "Usage: make ark-sign PKG=dist/ark/redis7-7.4.2-x86_64.ark"; \
		exit 1; \
	fi
	@echo "$(BLUE)Signing package $(PKG)...$(NC)"
	./scripts/ark-sign.sh $(PKG)

ark-sign-all:
	@echo "$(BLUE)Signing all .ark packages in dist/ark/...$(NC)"
	./scripts/ark-sign.sh $(DIST_DIR)/ark/
	@echo "$(GREEN)All packages signed$(NC)"

ark-verify:
	@if [ -z "$(PKG)" ]; then \
		echo "$(RED)Error: PKG not specified$(NC)"; \
		echo "Usage: make ark-verify PKG=dist/ark/redis7-7.4.2-x86_64.ark"; \
		exit 1; \
	fi
	@echo "$(BLUE)Verifying package $(PKG)...$(NC)"
	./scripts/ark-sign.sh --verify $(PKG)

# Release targets
release: clean
	@echo "$(BLUE)Creating release build...$(NC)"
	VERSION=$(VERSION) $(MAKE) build
	./scripts/create-release.sh $(VERSION)
	@echo "$(GREEN)Release $(VERSION) created$(NC)"

# Documentation targets
docs:
	@echo "$(BLUE)Building documentation...$(NC)"
	$(CARGO) doc --no-deps
	@echo "$(GREEN)Documentation built$(NC)"

docs-serve:
	@echo "$(BLUE)Serving documentation...$(NC)"
	$(CARGO) doc --no-deps --open

# CI targets for automation
ci-build: check build

ci-test: test lint format-check security-scan

ci-docs: docs

# Convenience targets
kernel-config:
	$(MAKE) -C kernel menuconfig

kernel-clean:
	$(MAKE) -C kernel clean
	$(MAKE) -C kernel mrproper

# Release signing (requires GPG key)
sign-release:
	@if [ -z "$(VERSION)" ]; then \
		echo "$(RED)Error: VERSION not specified$(NC)"; \
		exit 1; \
	fi
	@echo "$(BLUE)Signing release $(VERSION)...$(NC)"
	gpg --armor --detach-sign $(DIST_DIR)/agnos-$(VERSION)-$(ARCH).iso
	@echo "$(GREEN)Release signed$(NC)"

# Verify release signature
verify-release:
	@if [ -z "$(VERSION)" ]; then \
		echo "$(RED)Error: VERSION not specified$(NC)"; \
		exit 1; \
	fi
	@echo "$(BLUE)Verifying release signature...$(NC)"
	gpg --verify $(DIST_DIR)/agnos-$(VERSION)-$(ARCH).iso.asc
	@echo "$(GREEN)Signature verified$(NC)"
