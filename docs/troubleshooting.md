# Troubleshooting Guide

This guide covers common issues you may encounter when using AGNOS and how to resolve them.

## Table of Contents

1. [Installation Issues](#installation-issues)
2. [Boot Problems](#boot-problems)
3. [AI Shell Issues](#ai-shell-issues)
4. [Agent Runtime Issues](#agent-runtime-issues)
5. [LLM Gateway Issues](#llm-gateway-issues)
6. [Desktop Environment Issues](#desktop-environment-issues)
7. [Security Issues](#security-issues)
8. [Performance Issues](#performance-issues)

---

## Installation Issues

### ISO Won't Boot

**Symptoms:** System doesn't boot from USB/installer

**Solutions:**
- Verify ISO integrity: `sha256sum -c agnos-*.sha256`
- Disable Secure Boot in UEFI settings
- Use Ventoy or Etcher for USB creation
- Ensure virtualization is enabled in BIOS

### Installation Freezes

**Symptoms:** Installer hangs at "Loading..."

**Solutions:**
- Try adding kernel parameters: `nomodeset` or `noapic`
- Check for incompatible hardware
- Try running from different USB port

---

## Boot Problems

### Kernel Panic on Boot

**Symptoms:** "Kernel panic - not syncing" error

**Solutions:**
- Check kernel parameters match your hardware
- Verify memory (RAM) is not faulty
- Disable problematic kernel modules

### System Won't Boot After Update

**Symptoms:** System fails to start after package update

**Solutions:**
- Select previous kernel from GRUB menu
- Boot into recovery mode
- Roll back packages: `ark downgrade <package>`

---

## AI Shell Issues

### Shell Doesn't Respond

**Symptoms:** agnsh prompt appears but commands fail

**Solutions:**
```bash
# Check shell is running
ps aux | grep agnsh

# Restart shell
pkill agnsh
agnsh
```

### Natural Language Processing Fails

**Symptoms:** "Unable to understand command"

**Solutions:**
- Ensure LLM gateway is running: `systemctl status llm-gateway`
- Check model is loaded: `agnsh> status`
- Switch to simpler commands first
- Check logs: `journalctl -u llm-gateway`

---

## Agent Runtime Issues

### Agent Won't Start

**Symptoms:** `daimon` shows agent in "Failed" state

**Solutions:**
```bash
# Check agent daemon status
systemctl status daimon

# View agent logs
journalctl -u daimon -n 100

# Restart daemon
systemctl restart daimon
```

### Agent Sandbox Violations

**Symptoms:** "Sandbox violation" errors in logs

**Solutions:**
- Review agent permissions in Security UI
- Check Landlock rules: `ls -la /proc/self/attr/`
- Adjust sandbox configuration in agent config

### IPC Communication Fails

**Symptoms:** Agents can't communicate

**Solutions:**
- Verify agent-runtime is running: `systemctl status agent-runtime`
- Check Unix sockets exist: `ls /run/agnos/agents/`
- Review IPC logs: `journalctl -u agent-runtime | grep IPC`

---

## LLM Gateway Issues

### No Models Available

**Symptoms:** "No models loaded" error

**Solutions:**
```bash
# List available models
llm-gateway-cli list

# Pull a model
llm-gateway-cli pull llama2

# Check gateway status
systemctl status llm-gateway
```

### Inference Timeout

**Symptoms:** Requests hang or timeout

**Solutions:**
- Increase timeout in config: `/etc/agnos/llm-gateway.yaml`
- Check system resources: `top`, `free -h`
- Use smaller models for faster inference
- Check network if using cloud provider

### Out of Memory

**Symptoms:** OOM killer activates during inference

**Solutions:**
- Reduce model size
- Adjust `max_concurrent_requests`
- Add more RAM or swap
- Enable model quantization

---

## Desktop Environment Issues

### Wayland Compositor Crashes

**Symptoms:** Desktop won't start or shows blank screen

**Solutions:**
```bash
# Switch to console
Ctrl+Alt+F3

# Restart desktop
systemctl restart agnos-desktop

# Check logs
journalctl -u agnos-desktop -n 50
```

### Applications Won't Open

**Symptoms:** Clicking apps does nothing

**Solutions:**
- Check display server: `echo $XDG_SESSION_TYPE`
- Verify GPU drivers installed
- Try starting app from terminal to see errors
- Check file permissions on app binaries

### AI Features Not Working

**Suggestions don't appear or are wrong**

**Solutions:**
- Ensure LLM gateway is running
- Check context detection: `journalctl | grep context`
- Verify model has enough context window
- Adjust AI feature sensitivity in settings

---

## Security Issues

### Permission Denied Errors

**Symptoms:** Agent can't access resources

**Solutions:**
1. Open Security UI (system tray icon)
2. Navigate to Agent Permissions
3. Add required permissions for agent
4. Save and restart agent

### Audit Log Shows Unauthorized Access

**Symptoms:** Security alerts for unknown actions

**Solutions:**
- Review audit log: `audit-viewer`
- Check for compromised credentials
- Enable stricter sandboxing
- Review security policies

### Emergency Kill Switch

**System feels compromised**

**Solutions:**
- Use keyboard shortcut: `Ctrl+Alt+Shift+K`
- Run: `agnos-ctl emergency-stop`
- Physical power button (5 seconds for hard shutdown)

---

## Performance Issues

### High CPU Usage

**System feels sluggish**

**Solutions:**
```bash
# Check CPU usage
top

# Identify processes
ps aux --sort=-%cpu | head

# Limit agent CPU via resource manager
daimon-cli set-limits --cpu=50%
```

### High Memory Usage

**System runs out of memory**

**Solutions:**
- Check memory: `free -h`
- Kill unnecessary agents: `daimon-cli list` then `daimon-cli terminate <id>`
- Reduce model sizes
- Enable swap: `swapon /dev/sdXN`

### Slow Boot Time

**System takes too long to start**

**Solutions:**
- Disable unnecessary services: `systemctl list-unit-files | grep enabled`
- Check fsck on boot: `systemd-analyze critical-chain`
- Review startup services: `systemd-analyze blame`
- **Note:** AGNOS uses the argonaut init system by default. If booting with argonaut, use `argonaut status` and `argonaut list` instead of systemd commands. Argonaut targets <3 second boot from kernel handoff to agent-runtime ready.

---

## Getting Help

### Collect Debug Information

```bash
# System info
uname -a > debug.txt
systemctl list-units --failed >> debug.txt

# Logs
journalctl -b -0 --no-pager >> debug.txt

# AGNOS-specific
agnos-ctl status >> debug.txt
```

### Community Support

- **Matrix**: #agnos:matrix.org
- **Discord**: discord.gg/agnos
- **Forum**: discourse.agnos.io

### Reporting Bugs

See [SECURITY.md](../SECURITY.md) for security-related issues.

For general bugs, include:
- Steps to reproduce
- Expected vs actual behavior
- Debug information above
- Hardware/software specifications
