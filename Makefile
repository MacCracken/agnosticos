# AGNOS Makefile
# Top-level build orchestration

# Configuration
VERSION := $(shell cat VERSION 2>/dev/null || echo '2026.3.5')
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

.PHONY: all help deps build build-kernel build-userland build-initramfs iso install clean test test-unit test-integration test-security test-coverage lint format check docker-dev release

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
