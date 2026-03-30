# Jnana

> **Jnana** (Sanskrit: ज्ञान — knowledge, wisdom) — The foundation of knowing

| Field | Value |
|-------|-------|
| Status | Pre-1.0 |
| Version | `0.1.0` |
| Repository | `MacCracken/jnana` |
| Runtime | library crate (Rust) + content directory |

---

## What It Does

Unified knowledge system that distills human understanding into 1-10GB of structured, tested, queryable, offline-accessible data.

- **Internal knowledge**: Draws from all AGNOS science crates (hisab, prakash, kimiya, tanmatra, bodh, vidya, etc.) — formulas, constants, reference data, all verified by tests
- **External knowledge**: Curated open sources (medical guides, survival manuals, agricultural science, repair guides) — downloaded, checksummed, and served alongside internal content
- **20 knowledge domains**: Mathematics through Geography, sciences through applied skills
- **Storage profiles**: Survival (2GB), Developer (3GB), Homesteader (5GB), Educator (8GB), Full (10GB)
- **Budget calculator**: Given disk space and a profile, determines what fits
- **Portal generation**: Auto-generates web UI from the knowledge registry (no hand-coded HTML)

## Consumers

- **AGNOS** — ships as the OS's offline knowledge layer
- **agnoshi** — AI shell queries knowledge via MCP
- **hoosh** — LLM gateway grounds answers in verified knowledge
- **daimon** — agents look up facts, procedures, constants
- Any device with storage — fits on a phone, USB stick, or SSD
