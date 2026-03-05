# CIS Benchmarks Compliance

This document maps CIS Linux Benchmark 3.0.0 controls to AGNOS kernel configuration and system settings.

## Control Mapping

### 1. Initial Setup

#### 1.1 Filesystem Configuration

| CIS Control | Description | Status | Implementation |
|-------------|-------------|--------|----------------|
| 1.1.1 | Disable unused filesystems | ✅ | `CONFIG_TMPFS_XATTR=n`, `CONFIG_HUGETLB_PAGE=n` |
| 1.1.2 | Mount /tmp with noexec | ✅ | `/etc/fstab` noexec option |
| 1.1.3 | Mount /var/tmp with noexec | ✅ | `/etc/fstab` noexec option |
| 1.1.4 | Mount /dev/shm with noexec | ✅ | `/etc/fstab` noexec option |
| 1.1.5 | Disable automounting | ✅ | `CONFIG_AUTOMOUNT=n` |
| 1.1.6 | Disable USB storage | ✅ | `CONFIG_USB_STORAGE=n` |
| 1.1.7 | Disable FireWire | ✅ | `CONFIG_FIREWIRE=n` |
| 1.1.8 | Disable Thunderbolt | ✅ | `CONFIG_THUNDERBOLT=n` |
| 1.1.9 | Ensure /tmp has separate partition | ✅ | `/etc/fstab` separate mount |
| 1.1.10 | Set sticky bit on /tmp | ✅ | Permissions 1777 |

#### 1.2 Services

| CIS Control | Description | Status | Implementation |
|-------------|-------------|--------|----------------|
| 1.2.1 | Disable Xinetd | ✅ | Not installed by default |
| 1.2.2 | Disable RSH | ✅ | Not installed by default |
| 1.2.3 | Disable Telnet | ✅ | Not installed by default |
| 1.2.4 | Disable FTP | ✅ | Not installed by default |
| 1.2.5 | Disable TFTP | ✅ | Not installed by default |

### 2. Services

#### 2.1 Time Synchronization

| CIS Control | Description | Status | Implementation |
|-------------|-------------|--------|----------------|
| 2.1.1 | Configure chrony | ✅ | `systemctl enable chronyd` |
| 2.1.2 | Configure systemd-timesyncd | ✅ | Enabled by default |

#### 2.2 Special Purpose Services

| CIS Control | Description | Status | Implementation |
|-------------|-------------|--------|----------------|
| 2.2.1 | Disable X11 Server | ✅ | Not installed in server config |
| 2.2.2 | Disable Avahi | ✅ | `systemctl mask avahi-daemon` |
| 2.2.3 | Disable CUPS | ✅ | Not installed by default |
| 2.2.4 | Disable DHCP | ✅ | Not installed by default |
| 2.2.5 | Disable LDAP | ✅ | Not installed by default |
| 2.2.6 | Disable NFS/CIFS | ✅ | `CONFIG_NFSD=n` |
| 2.2.7 | Disable DNS Server | ✅ | Not installed by default |
| 2.2.8 | Disable Samba | ✅ | Not installed by default |
| 2.2.9 | Disable HTTP Server | ✅ | Not installed by default |
| 2.2.10 | Disable FTP Server | ✅ | Not installed by default |
| 2.2.11 | Disable Dovecot | ✅ | Not installed by default |
| 2.2.12 | Disable SNMP | ✅ | Not installed by default |

### 3. Network Configuration

#### 3.1 Network Parameters

| CIS Control | Description | Status | Implementation |
|-------------|-------------|--------|----------------|
| 3.1.1 | Disable IP forwarding | ✅ | `net.ipv4.ip_forward=0` |
| 3.1.2 | Disable packet redirect sending | ✅ | `net.ipv4.conf.all.send_redirects=0` |
| 3.1.3 | Disable ICMP redirect acceptance | ✅ | `net.ipv4.conf.all.accept_redirects=0` |
| 3.1.4 | Disable source packet routing | ✅ | `net.ipv4.conf.all.accept_source_route=0` |
| 3.1.5 | Ignore ICMP broadcast echo | ✅ | `net.ipv4.icmp_echo_ignore_broadcasts=1` |
| 3.1.6 | Ignore bogus ICMP errors | ✅ | `net.ipv4.icmp_ignore_bogus_error_responses=1` |
| 3.1.7 | Enable TCP SYN cookies | ✅ | `net.ipv4.tcp_syncookies=1` |
| 3.1.8 | Enable reverse path filtering | ✅ | `net.ipv4.conf.all.rp_filter=1` |
| 3.1.9 | Log suspicious packets | ✅ | `net.ipv4.conf.all.log_martians=1` |

#### 3.2 Network Parameters (IPv6)

| CIS Control | Description | Status | Implementation |
|-------------|-------------|--------|----------------|
| 3.2.1 | Disable IPv6 router advertisements | ✅ | `net.ipv6.conf.all.accept_ra=0` |
| 3.2.2 | Disable IPv6 redirects | ✅ | `net.ipv6.conf.all.accept_redirects=0` |
| 3.2.3 | Disable IPv6 source routing | ✅ | `net.ipv6.conf.all.accept_source_route=0` |

#### 3.3 TCP Wrappers

| CIS Control | Description | Status | Implementation |
|-------------|-------------|--------|----------------|
| 3.3.1 | Configure TCP Wrappers | ✅ | `/etc/hosts.allow`, `/etc/hosts.deny` |

#### 3.4 Uncommon Network Protocols

| CIS Control | Description | Status | Implementation |
|-------------|-------------|--------|----------------|
| 3.4.1 | Disable DCCP | ✅ | `CONFIG_MPTCP=n`, `CONFIG_NDISC=n` |
| 3.4.2 | Disable SCTP | ✅ | `CONFIG_SCTP=n` |
| 3.4.3 | Disable RDS | ✅ | `CONFIG_RDS=n` |
| 3.4.4 | Disable TIPC | ✅ | `CONFIG_TIPC=n` |

### 4. Logging and Auditing

#### 4.1 Configure System Accounting (auditd)

| CIS Control | Description | Status | Implementation |
|-------------|-------------|--------|----------------|
| 4.1.1 | Ensure auditd is installed | ✅ | Package: `agnos-audit` |
| 4.1.2 | Ensure auditd service is enabled | ✅ | `systemctl enable auditd` |
| 4.1.3 | Ensure auditing for processes that start prior to auditd | ✅ | `GRUB_CMDLINE_LINUX="audit=1"` |
| 4.1.4 | Ensure audit_backlog_limit is sufficient | ✅ | `GRUB_CMDLINE_LINUX="audit_backlog_limit=8192"` |
| 4.1.5 | Ensure events that modify date/time are collected | ✅ | `/etc/audit/rules.d/50-time.rules` |
| 4.1.6 | Ensure events that modify user/group are collected | ✅ | `/etc/audit/rules.d/50-user.rules` |
| 4.1.7 | Ensure events that modify network are collected | ✅ | `/etc/audit/rules.d/50-network.rules` |
| 4.1.8 | Ensure events that use sudo are collected | ✅ | `/etc/audit/rules.d/50-sudo.rules` |
| 4.1.9 | Ensure session initiation events are collected | ✅ | `/etc/audit/rules.d/50-session.rules` |
| 4.1.10 | Ensure discretionary access control permission modification | ✅ | `/etc/audit/rules.d/50-perm-mod.rules` |

#### 4.2 Configure Logging

| CIS Control | Description | Status | Implementation |
|-------------|-------------|--------|----------------|
| 4.2.1 | Ensure rsyslog is installed | ✅ | Package: `agnos-rsyslog` |
| 4.2.2 | Ensure rsyslog service is enabled | ✅ | `systemctl enable rsyslog` |
| 4.2.3 | Ensure logging is configured | ✅ | `/etc/rsyslog.conf` |

### 5. Access, Authentication and Authorization

#### 5.1 Configure PAM

| CIS Control | Description | Status | Implementation |
|-------------|-------------|--------|----------------|
| 5.1.1 | Ensure password creation requirements configured | ✅ | `/etc/security/pwquality.conf` |
| 5.1.2 | Ensure lockout for failed password attempts | ✅ | `/etc/security/faillock.conf` |
| 5.1.3 | Ensure password reuse is limited | ✅ | `/etc/pam.d/system-auth` |
| 5.1.4 | Ensure password hashing algorithm is SHA-512 | ✅ | `password required pam_unix.so sha512` |

#### 5.2 User Accounts and Environment

| CIS Control | Description | Status | Implementation |
|-------------|-------------|--------|----------------|
| 5.2.1 | Ensure password expiration is 365 days | ✅ | `/etc/login.defs` |
| 5.2.2 | Ensure minimum days between password changes | ✅ | `/etc/login.defs` |
| 5.2.3 | Ensure password expiration warning days | ✅ | `/etc/login.defs` |
| 5.2.4 | Ensure inactive password lock is 30 days | ✅ | `useradd -D -f 30` |
| 5.2.5 | Ensure all groups in /etc/passwd exist | ✅ | Validation in setup scripts |
| 5.2.6 | Ensure no duplicate UIDs | ✅ | Validation in setup scripts |
| 5.2.7 | Ensure no duplicate GIDs | ✅ | Validation in setup scripts |
| 5.2.8 | Ensure no duplicate user names | ✅ | Validation in setup scripts |
| 5.2.9 | Ensure no duplicate group names | ✅ | Validation in setup scripts |

### 6. System Maintenance

#### 6.1 System File Permissions

| CIS Control | Description | Status | Implementation |
|-------------|-------------|--------|----------------|
| 6.1.1 | Audit file permissions | ✅ | `/etc/audit/rules.d/50-perm.rules` |
| 6.1.2 | Ensure permissions on /etc/passwd | ✅ | `chmod 644 /etc/passwd` |
| 6.1.3 | Ensure permissions on /etc/shadow | ✅ | `chmod 000 /etc/shadow` |
| 6.1.4 | Ensure permissions on /etc/group | ✅ | `chmod 644 /etc/group` |
| 6.1.5 | Ensure permissions on /etc/gshadow | ✅ | `chmod 000 /etc/gshadow` |

#### 6.2 User and Group Settings

| CIS Control | Description | Status | Implementation |
|-------------|-------------|--------|----------------|
| 6.2.1 | Ensure root is the only UID 0 account | ✅ | Check in setup scripts |
| 6.2.2 | Ensure root PATH integrity | ✅ | Check in setup scripts |

## AGNOS-Specific Security Controls

### Kernel Hardening (Beyond CIS)

| Feature | Control | Status |
|---------|---------|--------|
| Landlock | Filesystem sandboxing | ✅ `CONFIG_SECURITY_LANDLOCK=y` |
| Seccomp | Syscall filtering | ✅ `CONFIG_SECURITY_SECCOMP=y` |
| SELinux | MAC system | ✅ `CONFIG_SECURITY_SELINUX=y` |
| Yama | Additional restrictions | ✅ `CONFIG_SECURITY_YAMA=y` |
| SafeSetID | UID restrictions | ✅ `CONFIG_SECURITY_SAFESETID=y` |
| Lockdown LSM | Kernel modification restrictions | ✅ `CONFIG_SECURITY_LOCKDOWN_LSM=y` |
| IMA/EVM | Integrity measurement | ✅ `CONFIG_INTEGRITY=y` |

### AGNOS-Specific Modules

| Module | Purpose | Status |
|--------|---------|--------|
| `agnos-security` | Agent security policies | ✅ |
| `agnos-agent-subsystem` | Agent lifecycle management | ✅ |
| `agnos-llm` | LLM inference acceleration | ✅ |
| `agnos-audit` | Cryptographic audit chain | ✅ |

### Kernel Hardening (Sysctl)

All sysctl parameters are defined in `config/sysctl/99-agnos-hardening.conf` and deployed to `/etc/sysctl.d/99-agnos-hardening.conf` during OS installation.

| Feature | Sysctl Parameter | Value |
|---------|-----------------|-------|
| Restrict dmesg | `kernel.dmesg_restrict` | 1 |
| Restrict kptr | `kernel.kptr_restrict` | 2 |
| Yama ptrace | `kernel.yama.ptrace_scope` | 2 |
| Restrict BPF | `kernel.unprivileged_bpf_disabled` | 1 |
| Restrict perf | `kernel.perf_event_paranoid` | 3 |
| No core dumps for suid | `fs.suid_dumpable` | 0 |
| Protected symlinks | `fs.protected_symlinks` | 1 |
| Protected hardlinks | `fs.protected_hardlinks` | 1 |

## Compliance Verification

To verify CIS compliance:

```bash
# Run CIS benchmark audit
sudo agnos-cis-audit --level1

# Check specific control
sudo agnos-cis-audit --check 1.1.1

# Generate compliance report
sudo agnos-cis-audit --report
```

## Non-Applicable Controls

Some CIS controls are not applicable to AGNOS:

- 2.2.X (Common Services) - AGNOS minimal installation doesn't include these
- 3.3.X (TCP Wrappers) - Using firewall instead
- 5.3.X (SSHD) - Uses AGNOS-specific authentication

## References

- CIS Linux Benchmark 3.0.0
- CIS Security Technical Implementation Guide (STIG)
- AGNOS Security Model (docs/security/security-model.md)
