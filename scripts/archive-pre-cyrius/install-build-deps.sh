#!/bin/bash
#
# AGNOS Build Dependencies Installation Script
# Installs all required dependencies for building AGNOS
#

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Check if running as root
if [ "$EUID" -ne 0 ]; then 
    echo -e "${RED}Error: This script must be run as root${NC}"
    echo "Usage: sudo ./install-build-deps.sh"
    exit 1
fi

echo -e "${BLUE}AGNOS Build Dependencies Installation${NC}"
echo "======================================"
echo ""

# Detect distribution
if [ -f /etc/os-release ]; then
    . /etc/os-release
    DISTRO=$ID
    DISTRO_VERSION=$VERSION_ID
else
    echo -e "${RED}Error: Cannot detect Linux distribution${NC}"
    exit 1
fi

echo -e "${BLUE}Detected distribution: $DISTRO $DISTRO_VERSION${NC}"
echo ""

# Function to install on Debian/Ubuntu
install_debian() {
    echo -e "${YELLOW}Installing dependencies for Debian/Ubuntu...${NC}"
    
    apt-get update
    
    apt-get install -y \
        build-essential \
        bc \
        bison \
        flex \
        libssl-dev \
        libncurses5-dev \
        libncursesw5-dev \
        wget \
        curl \
        git \
        ccache \
        fakeroot \
        libelf-dev \
        dwarves \
        rustc \
        cargo \
        rustfmt \
        clippy \
        python3 \
        python3-pip \
        python3-venv \
        asciidoc \
        xmlto \
        vim \
        tree \
        jq \
        iputils-ping \
        net-tools \
        iproute2 \
        xz-utils \
        lzop \
        lz4 \
        zstd \
        squashfs-tools \
        genisoimage \
        qemu-system-x86 \
        qemu-utils \
        libseccomp-dev \
        libcap-dev \
        gdb \
        strace \
        ltrace \
        git-lfs
    
    echo -e "${GREEN}Dependencies installed successfully${NC}"
}

# Function to install on Fedora
install_fedora() {
    echo -e "${YELLOW}Installing dependencies for Fedora...${NC}"
    
    dnf update -y
    
    dnf install -y \
        make \
        gcc \
        gcc-c++ \
        bc \
        bison \
        flex \
        openssl-devel \
        ncurses-devel \
        wget \
        curl \
        git \
        ccache \
        elfutils-libelf-devel \
        dwarves \
        rust \
        cargo \
        clippy \
        rustfmt \
        python3 \
        python3-pip \
        python3-devel \
        asciidoc \
        xmlto \
        vim \
        tree \
        jq \
        iputils \
        net-tools \
        iproute \
        xz \
        lzo \
        lz4 \
        zstd \
        squashfs-tools \
        genisoimage \
        qemu-system-x86 \
        qemu-img \
        libseccomp-devel \
        libcap-devel \
        gdb \
        strace \
        ltrace \
        git-lfs
    
    echo -e "${GREEN}Dependencies installed successfully${NC}"
}

# Function to install on Arch Linux
install_arch() {
    echo -e "${YELLOW}Installing dependencies for Arch Linux...${NC}"
    
    pacman -Syu --noconfirm
    
    pacman -S --noconfirm \
        base-devel \
        bc \
        bison \
        flex \
        openssl \
        ncurses \
        wget \
        curl \
        git \
        ccache \
        elfutils \
        pahole \
        rust \
        cargo \
        clippy \
        rustfmt \
        python \
        python-pip \
        asciidoc \
        xmlto \
        vim \
        tree \
        jq \
        iputils \
        net-tools \
        iproute2 \
        xz \
        lzo \
        lz4 \
        zstd \
        squashfs-tools \
        cdrtools \
        qemu-full \
        libseccomp \
        libcap \
        gdb \
        strace \
        ltrace \
        git-lfs
    
    echo -e "${GREEN}Dependencies installed successfully${NC}"
}

# Function to install Rust components
install_rust_components() {
    echo -e "${YELLOW}Installing additional Rust components...${NC}"
    
    # Install rustup if not present
    if ! command -v rustup &> /dev/null; then
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
    fi
    
    # Install components
    rustup component add rust-src rustc-dev llvm-tools-preview 2>/dev/null || true
    
    # Install cargo tools
    cargo install cargo-audit cargo-tarpaulin cargo-outdated 2>/dev/null || true
    
    echo -e "${GREEN}Rust components installed successfully${NC}"
}

# Function to install Python packages
install_python_packages() {
    echo -e "${YELLOW}Installing Python packages...${NC}"
    
    pip3 install --break-system-packages 2>/dev/null || pip3 install \
        pytest \
        pytest-cov \
        black \
        ruff \
        mypy \
        bandit \
        safety
    
    echo -e "${GREEN}Python packages installed successfully${NC}"
}

# Main installation
main() {
    case $DISTRO in
        ubuntu|debian)
            install_debian
            ;;
        fedora)
            install_fedora
            ;;
        arch|manjaro)
            install_arch
            ;;
        *)
            echo -e "${YELLOW}Warning: Distribution $DISTRO not officially supported${NC}"
            echo -e "${YELLOW}Attempting Debian/Ubuntu installation method...${NC}"
            install_debian
            ;;
    esac
    
    # Install Rust and Python components
    install_rust_components
    install_python_packages
    
    echo ""
    echo -e "${GREEN}======================================${NC}"
    echo -e "${GREEN}All dependencies installed successfully!${NC}"
    echo -e "${GREEN}======================================${NC}"
    echo ""
    echo "You can now build AGNOS with:"
    echo "  make build"
    echo ""
    echo "Or use the development container:"
    echo "  make docker-dev"
}

# Run main function
main
