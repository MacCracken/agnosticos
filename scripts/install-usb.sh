#!/bin/bash
# install-usb.sh — back-compat shim. This script was renamed to install-media.sh, which now
# also handles the internal NVMe and adds --update-fs (refresh /bin/agnsh on the agnos-fs
# without a wipe). All args forward unchanged; prefer install-media.sh going forward.
echo "note: install-usb.sh → install-media.sh (forwarding)..." >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/install-media.sh" "$@"
