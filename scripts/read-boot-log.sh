#!/usr/bin/env bash
# read-boot-log.sh — thin wrapper around the sovereign Cyrius
# implementation at src/read-boot-log.cyr.
#
# The bash version was retired 2026-05-13: /dev/nvram on
# archaemenid (AMD Beelink SER, UEFI) refuses every read with
# -EIO because the Linux nvram driver's CMOS checksum check
# rejects the BIOS's checksum layout. The Cyrius binary
# bypasses the driver via iopl(3) + direct port I/O.
#
# Usage: sudo ./scripts/read-boot-log.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SRC="$SCRIPT_DIR/src/read-boot-log.cyr"
BIN="$SCRIPT_DIR/build/read-boot-log"

# Resolve cyrius even under sudo (secure_path strips ~/.cyrius/bin).
# Prefer the invoking user's install, then PATH, then root's install.
find_cyrius() {
    if [ -n "${SUDO_USER:-}" ]; then
        local home
        home="$(getent passwd "$SUDO_USER" | cut -d: -f6)"
        if [ -x "$home/.cyrius/bin/cyrius" ]; then
            echo "$home/.cyrius/bin/cyrius"
            return
        fi
    fi
    if command -v cyrius >/dev/null 2>&1; then
        command -v cyrius
        return
    fi
    if [ -x "$HOME/.cyrius/bin/cyrius" ]; then
        echo "$HOME/.cyrius/bin/cyrius"
        return
    fi
    return 1
}

if [ ! -x "$BIN" ] || [ "$SRC" -nt "$BIN" ]; then
    CYRIUS_BIN="$(find_cyrius)" || {
        echo "error: cyrius not found. Build the binary first as your user:" >&2
        echo "  (cd $SCRIPT_DIR && cyrius build src/read-boot-log.cyr build/read-boot-log)" >&2
        exit 1
    }
    echo "(rebuilding $BIN from $SRC)" >&2
    # Build as the invoking user when under sudo, so artifacts stay user-owned.
    if [ -n "${SUDO_USER:-}" ] && [ "$(id -u)" = "0" ]; then
        ( cd "$SCRIPT_DIR" && sudo -u "$SUDO_USER" "$CYRIUS_BIN" build src/read-boot-log.cyr build/read-boot-log >&2 )
    else
        ( cd "$SCRIPT_DIR" && "$CYRIUS_BIN" build src/read-boot-log.cyr build/read-boot-log >&2 )
    fi
fi

exec "$BIN" "$@"
