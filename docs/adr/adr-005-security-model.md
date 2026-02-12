# ADR-005: Security Model and Human Override

**Status:** Accepted

**Date:** 2026-02-11

**Authors:** AGNOS Team

## Context

AGNOS must balance AI autonomy with human control:
- Agents need permissions to perform tasks
- Users must retain ultimate control
- Security must be transparent and auditable
- Emergency stop must be available

## Decision

We implement a **tiered permission system** with mandatory human oversight:

### Permission Levels

1. **Implicit** - Basic read-only operations (no prompt)
2. **Automatic** - Low-risk operations (notification only)
3. **Confirmation** - Medium-risk (requires approval)
4. **Override** - High-risk (blocks until human approves)

### Security Features

- **Landlock** - Filesystem sandboxing
- **Seccomp** - System call filtering
- **Namespaces** - Process isolation
- **Audit Logging** - All actions recorded
- **Emergency Kill Switch** - Immediate all-agent shutdown

## Consequences

### Positive
- Users maintain control over critical operations
- Clear audit trail for accountability
- Graduated response based on risk level
- Multiple layers of protection

### Negative
- Can interrupt agent workflow
- UI complexity for permission management
- Performance overhead from security checks
- Requires user education

## Permission Categories

| Category | Examples | Default Level |
|----------|----------|---------------|
| file:read | Read files in /home/user | Implicit |
| file:write | Modify files | Confirmation |
| file:delete | Delete files/directories | Override |
| network:outbound | External connections | Confirmation |
| process:spawn | Start new processes | Override |
| agent:delegate | Create sub-agents | Override |

## Human Override Flow

```
┌─────────────┐
│ Agent       │
│ Action      │
└──────┬──────┘
       │
       v
┌─────────────┐
│ Security    │
│ Check       │
└──────┬──────┘
       │
       ├────────── High Risk?
       │          
       │    No    │    Yes
       │    │     │    │
       v    v     v    v
┌────────┐  ┌──────────┐
│Execute │  │ Prompt   │
│        │  │ User     │
└────────┘  └────┬─────┘
                 │
           ┌─────┴─────┐
           │ Approve?  │
           └─────┬─────┘
                 │
           ┌─────┴─────┐
           │ Yes │ No │
           └──┬──┴──┬──┘
              │     │
              v     v
         ┌────────┐ ┌────────┐
         │Execute │ │Block   │
         │        │ │        │
         └────────┘ └────────┘
```

## References

- [Landlock Security Module](https://docs.kernel.org/userspace-api/landlock.html)
- [Seccomp BPF](https://www.kernel.org/doc/Documentation/prctl/seccomp_filter.txt)
- [Linux Capabilities](https://man7.org/linux/man-pages/man7/capabilities.7.html)
