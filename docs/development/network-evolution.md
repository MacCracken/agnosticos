# Network Evolution Roadmap

> **Status**: Architectural Note | **Last Updated**: 2026-03-22
>
> Plan for evolving AGNOS network transport from TCP/HTTP to QUIC-native
> with a purpose-built binary agent protocol.

---

## Problem

AGNOS uses TCP/HTTP everywhere for remote communication:

- **daimon** (port 8090): HTTP/1.1 REST API via reqwest/hyper
- **hoosh** (port 8088): HTTP/1.1 OpenAI-compatible API
- **majra**: TCP pub/sub for agent messaging and fleet relay
- **sutra**: SSH for remote host management
- **edge fleet**: HTTP to daimon `/v1/edge/*` endpoints

This works. But TCP/HTTP leaves significant performance on the table for agent-to-agent communication:

| Problem | Impact | Where it hits |
|---------|--------|--------------|
| TCP head-of-line blocking | One lost packet stalls everything behind it | Edge fleet on unreliable networks |
| TLS handshake latency | 1-2 RTT before first byte | Agent registration on reconnect |
| HTTP framing overhead | ~200+ bytes headers per request for tiny messages | Heartbeats, metrics, event pub/sub |
| Kernel syscall overhead | Context switch per send/recv | High-frequency agent communication |
| No connection migration | Connection dies when IP changes | Edge devices on cellular/WiFi roaming |
| No unreliable datagrams | Must use TCP for game state even when latest-only matters | joshua multiplayer, sensor data |

---

## Current Transport Map

```
Local (same machine):
  daimon ←→ hoosh      Unix domain socket (/run/agnos/)
  daimon ←→ agents     Unix domain socket (per-agent)
  tanur  ←→ irfan      Unix domain socket
  → Already optimal. No change needed.

Remote (cross-machine):
  daimon ←→ edge nodes       HTTP/1.1 over TCP+TLS
  majra  ←→ majra (relay)    TCP pub/sub
  sutra  ←→ remote hosts     SSH (russh)
  murti  ←→ registries       HTTPS (model downloads)
  → All candidates for QUIC upgrade.
```

---

## Three Tiers

### Tier 1 — QUIC Transport (Post-v1.0)

Replace TCP+TLS with QUIC for all remote communication. QUIC provides:

- **0-RTT connection resumption** — reconnecting edge devices skip the handshake
- **No head-of-line blocking** — multiplexed streams, one lost packet doesn't stall others
- **Built-in encryption** — TLS 1.3 integrated, no separate TLS layer
- **Connection migration** — survives IP changes (WiFi → cellular, DHCP renewal)
- **Unreliable datagrams** — fire-and-forget for real-time data (game state, sensor readings)

**Rust implementation**: `quinn` — mature, pure Rust, async, production-ready.

#### What Changes

| Component | Before | After |
|-----------|--------|-------|
| **majra** | TCP socket relay | QUIC streams (ordered) + datagrams (unordered) |
| **daimon edge** | HTTP/1.1 over TCP | HTTP/3 over QUIC (hyper h3 support) |
| **sutra transport** | SSH via russh | QUIC stream (SSH fallback for non-AGNOS hosts) |
| **murti fleet** | HTTPS downloads | QUIC multi-stream parallel chunk download |
| **nein** | nftables TCP rules | nftables UDP rules for QUIC (port-based) |

#### What Doesn't Change

- **Local IPC** — Unix domain sockets stay. Can't beat zero-network-stack.
- **HTTP API surface** — daimon's REST API keeps the same endpoints. Transport changes underneath.
- **External consumers** — curl, browsers, third-party tools still connect via HTTP. QUIC is for internal AGNOS-to-AGNOS communication.

#### QUIC in majra

majra is the primary beneficiary. Current TCP relay becomes QUIC-native:

```rust
// Current: TCP stream per connection
let stream = TcpStream::connect(relay_addr).await?;

// Tier 1: QUIC with multiplexed streams
let connection = quinn::Endpoint::connect(relay_addr, "majra").await?;
let (mut send, mut recv) = connection.open_bi().await?;  // ordered stream
connection.send_datagram(data)?;                          // unordered, fire-and-forget
```

Benefits for majra consumers:
- **joshua multiplayer**: Game state via datagrams (latest-wins, no TCP retransmit delay)
- **daimon federation**: Multi-node cluster sync without HOL blocking
- **edge fleet**: Heartbeats survive network transitions

### Tier 2 — io_uring Socket Backend (V2)

Linux 5.1+ io_uring eliminates syscall overhead for network I/O:

- **Batched syscalls**: Submit 100 send/recv in one kernel transition
- **Zero-copy**: NIC buffer → userspace without kernel memcpy
- **Completion-based**: No epoll polling — kernel notifies when I/O completes

**Impact**: 2-5x throughput on high-frequency small messages.

**Where**: Optional backend in majra for high-throughput nodes (GPU servers, federation hubs). Not needed for edge devices.

**Rust**: `tokio-uring` or `glommio`. Feature-gated — not all Linux kernels/configs support it.

### Tier 3 — Binary Agent Protocol (V2-V3)

Purpose-built binary protocol for internal agent communication, running on QUIC:

```
AGNOS Agent Protocol (AAP)
├── Stream 0: Control (register, heartbeat, deregister)
├── Stream 1: Events (pub/sub, fire-and-forget)
├── Stream 2: RPC (request-response, tool calls)
├── Stream 3: Bulk (model transfer, RAG ingestion)
└── Datagrams: Real-time (game state, sensor data)
```

Frame format — 8 bytes vs HTTP's ~200+ bytes:
```
┌──────┬──────┬──────────┬─────────┐
│ Type │ Len  │ Agent ID │ Payload │
│ 1B   │ 3B   │ 4B       │ N bytes │
└──────┴──────┴──────────┴─────────┘
```

**Why**: HTTP is designed for browsers requesting web pages. Agent messages are small (heartbeats: ~100 bytes), high-frequency (60Hz game state, 1Hz × 1000 agents), often one-way (events), and typed (schema known at compile time). HTTP framing is pure overhead.

**Backwards compatibility**: HTTP API stays for external access. AAP is for AGNOS-to-AGNOS only. Agents negotiate protocol on connect — if peer speaks AAP, use it; otherwise fall back to HTTP.

---

## Crate Impact

| Crate | Tier 1 (QUIC) | Tier 2 (io_uring) | Tier 3 (AAP) |
|-------|--------------|-------------------|-------------|
| **majra** | QUIC transport backend, datagram support | io_uring socket option | AAP framing + dispatch |
| **nein** | UDP firewall rules for QUIC | — | AAP port policy |
| **daimon** | HTTP/3 via hyper-h3, QUIC edge transport | — | AAP listener for internal agents |
| **hoosh** | QUIC for model downloads | — | — |
| **sutra** | QUIC transport for AGNOS hosts | — | — |
| **murti** | Multi-stream parallel model pull | — | — |
| **kiran** | — (uses majra) | — | — (uses majra) |
| **joshua** | — (uses majra) | — | — (uses majra) |
| **libro** | — | — | AAP audit events |
| **t-ron** | — | — | AAP security gate |
| **quinn** (external) | Direct dependency in majra | — | Transport layer |

---

## Dependencies

```
Tier 1:
  quinn = "0.11"          # QUIC implementation
  rustls = "0.23"         # TLS 1.3 (quinn uses rustls)

Tier 2 (optional):
  tokio-uring = "0.5"     # io_uring backend
  # or glommio

Tier 3:
  No new external deps — binary protocol is hand-rolled on QUIC
```

---

## Migration Strategy

### Phase 1 — QUIC in majra (non-breaking)

Add QUIC as an **alternative** transport in majra alongside TCP. Feature-gated:

```toml
[features]
default = ["tcp"]
tcp = []
quic = ["dep:quinn", "dep:rustls"]
```

Consumers opt in. TCP remains default. No breaking changes.

### Phase 2 — daimon QUIC edge transport

daimon edge fleet communication moves to QUIC. HTTP API stays on TCP for external access. Edge nodes negotiate: if both sides speak QUIC, use it; otherwise TCP fallback.

### Phase 3 — AAP protocol definition

Define the binary protocol spec. Implement in majra. daimon adds AAP listener alongside HTTP. Agents negotiate on connect.

### Phase 4 — io_uring (optional)

Feature-gated io_uring backend in majra for nodes that need maximum throughput. Not required — regular tokio async works for most deployments.

---

## What Stays the Same

- **Unix domain sockets** for local IPC — fastest possible, no change
- **HTTP REST API** surface — same endpoints, same JSON, same curl compatibility
- **SSH** in sutra for non-AGNOS hosts — not every remote machine runs AGNOS
- **reqwest** for outbound HTTPS — model registries, GitHub API, external services
- **Certificate pinning** — moves from TLS cert pins to QUIC cert pins (same concept)

---

## Security Considerations

- QUIC includes TLS 1.3 — no separate TLS layer to configure or misconfigure
- Connection migration needs rate limiting (prevent connection hijacking)
- Unreliable datagrams must not carry security-critical data (use ordered streams)
- nein firewall rules need UDP support for QUIC traffic
- t-ron monitors AAP tool calls same as HTTP tool calls
- libro audits protocol negotiation (which peers use AAP vs HTTP)

---

## Timeline

- **Current**: TCP/HTTP everywhere. Works, debuggable, curl-friendly.
- **Post-v1.0**: Tier 1 — QUIC transport in majra (feature-gated, opt-in)
- **V2**: Tier 2 + 3 — io_uring backend, AAP protocol definition
- **V3**: AAP as default for internal AGNOS communication, HTTP for external only

---

*Last Updated: 2026-03-22*
