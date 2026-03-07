# ADR-107: Scale, Collaboration, and Future

**Status:** Accepted
**Date:** 2026-03-07

## Context

AGNOS is designed to grow beyond a single-user, single-node system. This record covers federation at scale, human-AI collaboration models, cloud services, and research directions that inform future development.

## Decisions

### Multi-Node Federation

Multiple AGNOS nodes form a cluster:

- **Coordination** — Raft consensus elects a coordinator node. All nodes accept API requests; non-coordinators forward scheduling decisions.
- **Node identity** — Ed25519 keypair per node, mTLS authentication via shared federation CA.
- **Discovery** — static config (`federation.toml`), mDNS/DNS-SD (LAN), DNS SRV (production).
- **Control plane** — gRPC on port 8092.
- **Data plane** — agent IPC tunneled over mTLS TCP.
- **Health** — heartbeats every 5s, suspect after 15s, dead after 30s, agents rescheduled.

Federation activates when 2+ nodes join; single-node mode has zero overhead.

### Cloud Services

Optional cloud deployment for users who need it:

- **Cloud agents** — agents deployed to cloud nodes with the same sandbox and audit guarantees
- **Cross-device sync** — agent state, configuration, and knowledge synced across devices
- **Collaborative workspaces** — multi-user access with RBAC (owner, editor, viewer)
- **Billing** — per-resource tier (compute, storage, inference tokens)
- **Security** — mTLS everywhere, encryption at rest and in transit, no plaintext secrets

All cloud features are opt-in. AGNOS remains fully functional as a local-only system.

### Human-AI Collaboration

Five collaboration modes, dynamically adjustable:

| Mode | Human Role | Agent Role |
|------|-----------|------------|
| **Full Autonomy** | Notified after completion | Plans and executes independently |
| **Supervised** | Reviews agent's plan before execution | Proposes actions, waits for approval |
| **Collaborative** | Works alongside agent | Suggests, assists, fills gaps |
| **Guided** | Provides step-by-step direction | Executes instructions |
| **Teaching** | Demonstrates desired behavior | Observes, learns patterns |

**Trust calibration** — continuous measurement of agent reliability informs automatic mode adjustment. As an agent demonstrates competence, the system suggests increased autonomy.

**Cognitive load management** — limits concurrent notifications, batches low-priority updates, presents information at appropriate complexity.

**Structured feedback** — user corrections, ratings, and demonstrations are captured as training signal for fine-tuning and RL optimization.

### Research Directions (Implemented)

These capabilities are implemented but expected to evolve:

- **Explainability** — decision records with confidence scores, factor breakdowns, natural language summaries, audit integration for compliance
- **AI safety** — declarative policies, prompt injection detection, safety circuit breakers, default policies protecting critical system files
- **Fine-tuning** — automated training example collection, LoRA/QLoRA/full methods, model registry with lineage
- **Formal verification** — ~15 security properties checked (audit chain integrity, sandbox isolation, state machine validity)
- **Novel sandboxing** — capability tokens, information flow control, time-bounded sandboxes, learned policies
- **RL optimization** — UCB1 + Q-learning for tool selection, policy gradients for resource tuning

All research features are constrained by the safety policy framework — RL cannot learn actions that violate safety rules.

## Consequences

### Positive
- AGNOS scales from single laptop to multi-node cluster
- Collaboration modes adapt to user skill and agent trustworthiness
- Cloud features available without sacrificing local-first philosophy
- Research features provide competitive advantages in agent intelligence

### Negative
- Federation adds distributed systems complexity (consensus, split-brain, eventual consistency)
- Collaboration mode selection adds UX complexity
- Cloud introduces infrastructure costs and availability concerns
- Research features may need significant iteration as the field evolves
