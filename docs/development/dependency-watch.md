# Dependency Watch

> Known vulnerabilities, unmaintained transitive dependencies, and upgrade blockers.
> Updated when `cargo audit` findings change.

---

## Active Advisories (allowed in CI)

| Advisory | Crate | Version | Severity | Root Cause | Resolution |
|----------|-------|---------|----------|------------|------------|
| RUSTSEC-2026-0049 | rustls-webpki | 0.102.8 | Medium | rumqttc 0.25 → rustls 0.22 → old webpki | Wait for rumqttc to upgrade to rustls 0.23+ |
| RUSTSEC-2025-0134 | rustls-pemfile | 2.2.0 | Warning | rumqttc 0.25 uses unmaintained crate | Wait for rumqttc to drop rustls-pemfile |

Both are transitive through **rumqttc** (MQTT client for ESP32 edge bridge in agent-runtime). No direct fix available — upstream must update their TLS stack.

**Monitoring**: Check `cargo search rumqttc` for 0.26+ which should move to rustls 0.23.

**Risk**: rumqttc (bytebeamio/rumqtt) appears stale — last commit 2025-11-21, 4 months inactive. If no update by Q3 2026, evaluate alternatives: `paho-mqtt`, or a thin custom MQTT client using `tokio` + `rustls 0.23` directly. The edge MQTT bridge (`agent-runtime/src/edge/mqtt_bridge.rs`) only uses basic pub/sub — minimal surface to replace.

## Resolved

| Date | Advisory | Crate | Fix |
|------|----------|-------|-----|
| 2026-03-25 | RUSTSEC-2026-0049 | rustls-webpki 0.103.9 | `cargo update` → 0.103.10 |
| 2026-03-25 | RUSTSEC-2026-0067 | tar 0.4.44 | `cargo update` → 0.4.45 |
| 2026-03-25 | RUSTSEC-2026-0068 | tar 0.4.44 | `cargo update` → 0.4.45 |
| 2026-03-14 | RUSTSEC-2025-0134 | rustls-pemfile 1.x | reqwest 0.11 → 0.12 (H26) |
| 2026-03-14 | RUSTSEC-2025-0057 | fxhash | wasmtime 36 → 42 (H37) |

## Upgrade Blockers

| Crate | Current | Latest | Blocked By | Notes |
|-------|---------|--------|------------|-------|
| rumqttc | 0.25.1 | 0.25.1 | — | Latest, but still uses rustls 0.22 internally |
| rustls (via rumqttc) | 0.22.4 | 0.23.x | rumqttc pinning | rumqttc bundles own rustls version |

---

*Last Updated: 2026-03-25*
