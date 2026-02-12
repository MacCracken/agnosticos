#!/bin/bash
# build-iso.sh - Build AGNOS bootable ISO

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK_DIR="$SCRIPT_DIR/../build/iso"
OUTPUT_DIR="$SCRIPT_DIR/../output"
CONFIG_DIR="$SCRIPT_DIR/../config"

# Defaults
ISO_NAME="agnos"
ISO_VERSION="0.1.0"
KERNEL_VERSION="6.6-lts"
ARCH="x86_64"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

usage() {
    cat << EOF
Usage: $0 [options]

Build AGNOS bootable ISO image

Options:
    -n, --name NAME         ISO name (default: agnos)
    -v, --version VERSION   AGNOS version (default: 0.1.0)
    -k, --kernel VERSION    Kernel version to include (default: 6.6-lts)
    -a, --arch ARCH         Target architecture (default: x86_64)
    -o, --output DIR        Output directory (default: output/)
    -h, --help              Show this help message

Examples:
    $0                      # Build default ISO
    $0 -k 6.x-stable        # Build with stable kernel
    $0 -n agnos-dev -v 0.2.0 # Build development version
EOF
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            -n|--name)
                ISO_NAME="$2"
                shift 2
                ;;
            -v|--version)
                ISO_VERSION="$2"
                shift 2
                ;;
            -k|--kernel)
                KERNEL_VERSION="$2"
                shift 2
                ;;
            -a|--arch)
                ARCH="$2"
                shift 2
                ;;
            -o|--output)
                OUTPUT_DIR="$2"
                shift 2
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                usage
                exit 1
                ;;
        esac
    done
}

setup_directories() {
    log_info "Setting up ISO build directories..."
    
    rm -rf "$WORK_DIR"
    mkdir -p "$WORK_DIR"
    mkdir -p "$WORK_DIR/iso/boot/grub"
    mkdir -p "$WORK_DIR/iso/boot/agnos"
    mkdir -p "$WORK_DIR/iso/agnos"
    mkdir -p "$WORK_DIR/squashfs"
    mkdir -p "$OUTPUT_DIR"
}

copy_kernel() {
    log_info "Copying kernel..."
    
    local kernel_package="$OUTPUT_DIR/kernel-$(echo $KERNEL_VERSION | tr ':' '-')"
    
    if [ ! -d "$kernel_package" ]; then
        log_error "Kernel package not found: $kernel_package"
        log_info "Run build-kernel.sh -v $KERNEL_VERSION first"
        exit 1
    fi
    
    # Copy kernel and modules
    cp "$kernel_package/boot/"* "$WORK_DIR/iso/boot/agnos/"
    
    # Extract modules to squashfs directory
    if [ -d "$kernel_package/lib/modules" ]; then
        cp -r "$kernel_package/lib/modules" "$WORK_DIR/squashfs/lib/"
    fi
}

create_initramfs() {
    log_info "Creating initramfs..."
    
    local initramfs_dir="$WORK_DIR/initramfs"
    mkdir -p "$initramfs_dir"
    
    # Create basic initramfs structure
    mkdir -p "$initramfs_dir/"{bin,sbin,etc,proc,sys,dev,run,tmp,lib,lib64}
    mkdir -p "$initramfs_dir/usr/"{bin,sbin}
    
    # Copy essential binaries
    for binary in /bin/busybox /bin/sh /sbin/modprobe /bin/mount /bin/umount; do
        if [ -f "$binary" ]; then
            cp "$binary" "$initramfs_dir/bin/" 2>/dev/null || true
        fi
    done
    
    # Create init script
    cat > "$initramfs_dir/init" << 'INITEOF'
#!/bin/sh
# AGNOS Initramfs Init

mount -t proc none /proc
mount -t sysfs none /sys
mount -t devtmpfs none /dev

# Load essential modules
modprobe loop 2>/dev/null || true
modprobe squashfs 2>/dev/null || true
modprobe overlay 2>/dev/null || true

# Find AGNOS root
mkdir -p /mnt/agnos
device=""

# Try to find AGNOS partition
for dev in /dev/sd* /dev/nvme* /dev/vd* /dev/hd*; do
    if [ -b "$dev" ]; then
        if mount -o ro "$dev" /mnt/agnos 2>/dev/null; then
            if [ -f /mnt/agnos/agnos/system.squashfs ]; then
                device="$dev"
                break
            fi
            umount /mnt/agnos 2>/dev/null || true
        fi
    fi
done

if [ -z "$device" ]; then
    echo "ERROR: Could not find AGNOS root filesystem"
    echo "Dropping to emergency shell..."
    /bin/sh
fi

# Setup overlay
mkdir -p /mnt/overlay
mkdir -p /mnt/work
mount -t tmpfs none /mnt/overlay
mount -t overlay overlay -o lowerdir=/mnt/agnos,upperdir=/mnt/overlay,workdir=/mnt/work /newroot

# Switch to new root
cd /newroot
mkdir -p oldroot
mount --move . /newroot
pivot_root . oldroot

# Cleanup
umount /oldroot/proc 2>/dev/null || true
umount /oldroot/sys 2>/dev/null || true
umount /oldroot/dev 2>/dev/null || true
umount /oldroot 2>/dev/null || true
rmdir /oldroot 2>/dev/null || true

# Start init
exec /sbin/init
INITEOF
    
    chmod +x "$initramfs_dir/init"
    
    # Create initramfs archive
    cd "$initramfs_dir"
    find . | cpio -H newc -o | gzip > "$WORK_DIR/iso/boot/agnos/initramfs.img"
    cd - > /dev/null
    
    log_info "  -> Initramfs created"
}

create_squashfs() {
    log_info "Creating squashfs root filesystem..."
    
    # Create basic filesystem structure
    mkdir -p "$WORK_DIR/squashfs/"{bin,boot,dev,etc,home,lib,lib64,mnt,opt,proc,root,run,sbin,srv,sys,tmp,usr,var}
    mkdir -p "$WORK_DIR/squashfs/usr/"{bin,sbin,lib,lib64,include,share,src,local}
    mkdir -p "$WORK_DIR/squashfs/var/"{cache,lib,log,spool,tmp}
    mkdir -p "$WORK_DIR/squashfs/var/lib/agnos/"{agents,models,cache,audit}
    mkdir -p "$WORK_DIR/squashfs/etc/agnos"
    mkdir -p "$WORK_DIR/squashfs/run/agnos"
    
    # Copy systemd units
    if [ -d "$CONFIG_DIR/systemd/system" ]; then
        mkdir -p "$WORK_DIR/squashfs/etc/systemd/system"
        cp "$CONFIG_DIR/systemd/system/"*.service "$WORK_DIR/squashfs/etc/systemd/system/" 2>/dev/null || true
    fi
    
    # Copy init scripts
    if [ -d "$CONFIG_DIR/init" ]; then
        mkdir -p "$WORK_DIR/squashfs/usr/lib/agnos/init"
        cp "$CONFIG_DIR/init/"*.sh "$WORK_DIR/squashfs/usr/lib/agnos/init/" 2>/dev/null || true
        chmod +x "$WORK_DIR/squashfs/usr/lib/agnos/init/"*.sh 2>/dev/null || true
    fi
    
    # Create mksquashfs
    mksquashfs "$WORK_DIR/squashfs" "$WORK_DIR/iso/agnos/system.squashfs" \
        -comp zstd -Xcompression-level 15 \
        -noappend \
        -wildcards \
        -e "boot/*" \
        2>/dev/null || {
        log_error "Failed to create squashfs. Is squashfs-tools installed?"
        exit 1
    }
    
    log_info "  -> Root filesystem created ($(du -h "$WORK_DIR/iso/agnos/system.squashfs" | cut -f1))"
}

create_grub_config() {
    log_info "Creating GRUB configuration..."
    
    cat > "$WORK_DIR/iso/boot/grub/grub.cfg" << EOF
set timeout=5
set default=0

menuentry "AGNOS $ISO_VERSION ($KERNEL_VERSION)" {
    linux /boot/agnos/vmlinuz-agnos-$(echo $KERNEL_VERSION | tr ':' '-') quiet loglevel=3 agnos.security.enforce=1
    initrd /boot/agnos/initramfs.img
}

menuentry "AGNOS $ISO_VERSION (Debug Mode)" {
    linux /boot/agnos/vmlinuz-agnos-$(echo $KERNEL_VERSION | tr ':' '-') debug loglevel=7 systemd.debug_shell
    initrd /boot/agnos/initramfs.img
}

menuentry "AGNOS $ISO_VERSION (Recovery)" {
    linux /boot/agnos/vmlinuz-agnos-$(echo $KERNEL_VERSION | tr ':' '-') single init=/bin/bash
    initrd /boot/agnos/initramfs.img
}
EOF
}

create_iso() {
    log_info "Creating ISO image..."
    
    local iso_file="$OUTPUT_DIR/${ISO_NAME}-${ISO_VERSION}-${ARCH}.iso"
    
    # Check for grub-mkrescue or mkisofs/genisoimage
    if command -v grub-mkrescue &> /dev/null; then
        grub-mkrescue -o "$iso_file" "$WORK_DIR/iso" \
            --modules="part_gpt part_msdos fat iso9660 zstd" \
            2>/dev/null || {
            log_error "grub-mkrescue failed"
            exit 1
        }
    elif command -v mkisofs &> /dev/null || command -v genisoimage &> /dev/null; then
        local mkisofs_cmd=$(command -v mkisofs || command -v genisoimage)
        
        $mkisofs_cmd -R -b boot/grub/grub.img \
            -no-emul-boot -boot-load-size 4 -boot-info-table \
            -o "$iso_file" "$WORK_DIR/iso" 2>/dev/null || {
            log_error "ISO creation failed"
            exit 1
        }
    else
        log_error "No ISO creation tool found. Install grub-common or genisoimage."
        exit 1
    fi
    
    # Calculate checksums
    sha256sum "$iso_file" > "$iso_file.sha256"
    
    log_info "ISO created: $iso_file"
    log_info "  Size: $(du -h "$iso_file" | cut -f1)"
    log_info "  SHA256: $(sha256sum "$iso_file" | cut -d' ' -f1)"
}

main() {
    parse_args "$@"
    
    log_info "Building AGNOS ISO"
    log_info "  Name: $ISO_NAME"
    log_info "  Version: $ISO_VERSION"
    log_info "  Kernel: $KERNEL_VERSION"
    log_info "  Architecture: $ARCH"
    
    setup_directories
    copy_kernel
    create_initramfs
    create_squashfs
    create_grub_config
    create_iso
    
    log_info "Build complete!"
    log_info "Output: $OUTPUT_DIR/${ISO_NAME}-${ISO_VERSION}-${ARCH}.iso"
}

main "$@"
