# Third-Party Packaged Applications

> Software packaged via takumi recipes for AGNOS. Not built by the AGNOS team — maintained upstream.
>
> **Strategy**: Ship lean essentials in the OS. Everything optional lives in Bazaar (`ark bazaar install <pkg>`).
> Daemons and backends are in the OS — GUI frontends are in Bazaar.

---

## OS — Ships with ISO

| App | Package | Recipe | Notes |
|-----|---------|--------|-------|
| Web Browser | Firefox ESR 128.9.0 | `recipes/browser/firefox.toml` | Aegis phishing detection, sandboxed |
| Web Browser | Chromium 134.0.6998.88 | `recipes/browser/chromium.toml` | Alternative, sandboxed |
| Terminal | Foot | `recipes/desktop/foot.toml` | Wayland-native, fast, minimal deps |
| Text Editor | Helix | `recipes/desktop/helix.toml` | Rust-native, default config included |
| File Manager | yazi | `recipes/desktop/yazi.toml` | Rust TUI, async, rich previews |
| PDF Viewer | Zathura | `recipes/desktop/zathura.toml` | Lightweight, plugin-based (PDF/DJVU/PS) |
| Image Viewer | imv | `recipes/desktop/imv.toml` | Wayland-native, HEIF/SVG/WebP |
| Media Player | mpv | `recipes/desktop/mpv.toml` | PipeWire, Vulkan, VA-API hwdec |
| Notifications | mako | `recipes/desktop/mako.toml` | Wayland-native, systemd user service |
| Clipboard | cliphist | `recipes/desktop/cliphist.toml` | Go-based, wl-clipboard integration |
| App Launcher | fuzzel | `recipes/desktop/fuzzel.toml` | Wayland-native dmenu/rofi alternative |
| Printing | CUPS | `recipes/desktop/cups.toml` | Print daemon |
| Fonts | fontconfig + Noto | `recipes/desktop/` | Core rendering |

## Bazaar — Community Repository

Installed via `ark bazaar install <pkg>`. 90 recipes across 8 categories.

| Category | Count | Highlights |
|----------|-------|------------|
| AI | 13 | ollama, llama.cpp, whisper.cpp, stable-diffusion.cpp, onnxruntime, vllm, piper-tts, aider, open-webui, comfyui, fabric, lmstudio, pytorch |
| Desktops | 35 | Sway, Hyprland, Thunar, Evince, Blueman, nm-applet, dunst, feh, GParted, GNOME Disks, firewall-config, system-config-printer, GTK3/Qt5/libadwaita |
| Tools | 21 | ripgrep, fd, bat, eza, fzf, tmux, htop, btop, lazygit, starship, zoxide, dust, tokei, hyperfine, git-delta, docker, podman, k9s, syncthing, gimp, inkscape, libreoffice |
| Editors | 3 | neovim, vim, micro |
| Networking | 4 | wireguard-tools, bandwhich, mtr, tailscale |
| Security | 3 | keepassxc, age, pass |
| Media | 4 | ffmpeg, yt-dlp, obs-studio, audacity |
| Games | 1 | retroarch |

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
- yazi now → AI file manager later (Priority 1)
- nm-applet in bazaar → AI network manager later (Priority 2)

---

*Last Updated: 2026-03-18*
