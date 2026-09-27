# Third-Party Packaged Applications

> Software packaged via takumi recipes for AGNOS. Not built by the AGNOS team — maintained upstream.
>
> **Strategy**: Ship lean essentials in the OS. Everything optional lives in Bazaar (`ark bazaar install <pkg>`).
> Daemons and backends are in the OS — GUI frontends are in Bazaar.

---

## OS — Ships with ISO *(Linux-era plan)*

> ⚠ **Superseded as the ISO plan.** This list predates the sovereign ISO decision: the Stage-4 base image's rootfs manifest (decided 2026-06-01 — [`iso-stage4-plan.md`](../../development/iso-stage4-plan.md) § Base-OS manifest) ships only statically-linked sovereign Cyrius tool binaries, none of the packages below. They remain zugot recipes for a Linux host. The sovereign desktop compositor, aethersafha, speaks setu, not Wayland; foreign apps reach it only through mehman's swallow stage, which is post-MVP.

| App | Package | Recipe | Notes |
|-----|---------|--------|-------|
| Web Browser | Firefox | `zugot/browser/firefox.cyml` | Aegis phishing detection, sandboxed |
| Web Browser | Chromium | `zugot/browser/chromium.cyml` | Alternative, sandboxed |
| Terminal | Foot | `zugot/desktops/foot.cyml` | Wayland-native, fast, minimal deps |
| Text Editor | Helix | `zugot/desktops/helix.cyml` | Rust-native, default config included |
| File Manager | yazi | `zugot/desktops/yazi.cyml` | **Retired interim** — superseded by first-party `crab` 🦀; still bazaar-installable |
| PDF Viewer | Zathura | `zugot/desktops/zathura.cyml` | Lightweight, plugin-based (PDF/DJVU/PS) |
| Image Viewer | imv | `zugot/desktops/imv.cyml` | Wayland-native, HEIF/SVG/WebP |
| Media Player | mpv | `zugot/desktops/mpv.cyml` | PipeWire, Vulkan, VA-API hwdec |
| Notifications | mako | `zugot/desktops/mako.cyml` | Wayland-native, systemd user service |
| Clipboard | cliphist | `zugot/desktops/cliphist.cyml` | Go-based, wl-clipboard integration |
| App Launcher | fuzzel | `zugot/desktops/fuzzel.cyml` | Wayland-native dmenu/rofi alternative |
| Printing | CUPS | `zugot/desktops/cups.cyml` | Print daemon |
| Fonts | fontconfig + Noto | `zugot/desktops/` | Core rendering |

## Bazaar — Community Repository

Installed via `ark bazaar install <pkg>`. Recipes are `.cyml` files under the [bazaar](https://github.com/MacCracken/bazaar) repo's `recipes/<category>/` — the live list is there, not here.

| Category | Highlights |
|----------|------------|
| AI | ollama, llama.cpp, whisper.cpp, stable-diffusion.cpp, onnxruntime, vllm, piper-tts, aider, open-webui, comfyui, fabric, lmstudio, pytorch |
| Desktops | Sway, Hyprland, Thunar, Evince, Blueman, nm-applet, dunst, feh, GParted, GNOME Disks, firewall-config, system-config-printer, GTK3/Qt5/libadwaita |
| Tools | ripgrep, fd, bat, eza, fzf, tmux, htop, btop, lazygit, starship, zoxide, dust, tokei, hyperfine, git-delta, docker, podman, k9s, syncthing, gimp, inkscape, libreoffice |
| Editors | neovim, vim, micro |
| Networking | wireguard-tools, bandwhich, mtr, tailscale |
| Security | keepassxc, age, pass |
| Media | ffmpeg, yt-dlp, obs-studio, audacity |
| Games | retroarch |

## When to Package vs Build

**Package (third-party recipe)** when:
- The upstream tool works well as-is
- AI integration can be added via agnoshi intents or MCP tool wrappers without forking
- Maintenance burden of a fork isn't justified

**Build first-party** when:
- AI is the primary value proposition and can't be bolted on
- Deep OS integration (daimon, hoosh, aegis) is required
- No existing tool covers the domain

**Hybrid** — ship the package now, build AI-native later:
- Zathura now → AI PDF suite later (Priority 1)
- ~~yazi now → AI file manager later~~ → **`crab` 🦀 first-party** — scaffolded at v0.1.0, now shipping (browses, writes, previews; its AI-native organization is still a design target) — the later is now (yazi retired as interim, Priority 1)
- nm-applet in bazaar → AI network manager later (Priority 2)

---

*Last Updated: 2026-09-27*
