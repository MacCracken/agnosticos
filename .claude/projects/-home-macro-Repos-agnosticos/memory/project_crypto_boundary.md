---
name: Crypto boundary — AGNOS vs SY
description: PQC and crypto responsibilities split between AGNOS (pqc.rs) and SecureYeoman (sy-crypto), needs fleshing out
type: project
---

AGNOS agent-runtime has `pqc.rs` — hybrid classical + PQC (ML-KEM/ML-DSA) simulated with SHA-256, production interfaces ready for real crate swap. SY has `sy-crypto` for agent-side crypto.

**Why:** Both sides protect overlapping concerns (key exchange, signatures, trust). Current state has duplication and potential gaps at the boundary.

**How to apply:** When working on crypto in either project, check the other side's coverage. The boundary needs fleshing out — decide what lives in AGNOS (OS-level, trust infrastructure) vs SY (agent-level, session crypto). Avoid duplicating primitives; share via agnostik or a common crate if possible.
