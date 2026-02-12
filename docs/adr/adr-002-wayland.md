# ADR-002: Wayland for Desktop Environment

**Status:** Accepted

**Date:** 2026-02-05

**Authors:** AGNOS Team

## Context

AGNOS requires a display server protocol for its desktop environment. The protocol must:
- Support modern GPU acceleration
- Enable fine-grained security controls
- Allow AI-augmented window management
- Support agent visualization features
- Be maintainable and future-proof

## Decision

We will use **Wayland** as the display server protocol with:
- Custom compositor implementation using `smithay` or `wlroots`
- AGNOS-specific protocols for agent window management
- Security extensions for screenshot/access control
- AI context protocol for workspace management

## Consequences

### Positive
- Modern protocol designed for security from ground up
- Better performance than X11 (less overhead)
- Per-application sandboxing capability
- Easier to extend with custom protocols
- Growing industry adoption

### Negative
- Legacy application compatibility issues
- Remote desktop more complex than X11
- Some applications still require XWayland
- Smaller ecosystem than X11

## Alternatives Considered

### X11
**Rejected:** X11's architecture makes fine-grained security impossible. Any client can access any other client's windows.

### Custom Protocol
**Rejected:** While possible, the effort to recreate what Wayland provides is not justified for MVP.

### No GUI (Terminal Only)
**Rejected:** GUI is essential for human oversight interface and accessibility.

## Implementation Notes

- Use `smithay` crate for compositor foundation
- Implement `zwp_security_context_v1` for agent isolation
- Create `agnos_agent_surface_v1` protocol for agent HUD
- Support both native Wayland and XWayland for compatibility

## References

- [Wayland Documentation](https://wayland.freedesktop.org/)
- [Smithay Framework](https://smithay.github.io/)
- [Wayland Security Considerations](https://wayland.freedesktop.org/docs/html/ch05.html)
