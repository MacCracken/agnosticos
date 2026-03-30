# Personality Architecture — Runtime Personality via LLM + Bhava

> **Status**: Future consideration | **Last Updated**: 2026-03-29
>
> Design notes for runtime personality as an emergent system, not a trained artifact.

---

## Core Insight

Personality doesn't need to be fine-tuned into model weights. It can be a **runtime system** — an LLM provides cognition, bhava provides character, and personality emerges from the feedback loop between them.

No training required. Configuration + interaction = personality.

---

## Architecture

```
┌─────────────┐     ┌──────────┐     ┌──────────┐
│  Base LLM   │────▸│  hoosh   │────▸│  bhava   │
│  (cognition)│     │ (gateway) │     │ (soul)   │
└─────────────┘     └──────────┘     └──────────┘
                                          │
                         ┌────────────────┤
                         ▼                ▼
                    ┌──────────┐    ┌──────────┐
                    │  jnana   │    │  bodh    │
                    │(knowledge)│    │(cognition│
                    └──────────┘    │ models)  │
                                    └──────────┘
```

### What Each Layer Provides

| Component | Role | What It Handles |
|-----------|------|-----------------|
| **Base LLM** (murti/hoosh) | Cognition | Language, reasoning, logic, instruction following |
| **bhava** | Soul/Character | 15-trait personality, PAD mood, OCC appraisal, relationships, circadian rhythm, EQ, display rules, micro-expressions, stress/regulation |
| **bodh** | Cognitive modeling | Attention, decision-making (prospect theory), learning curves, memory models |
| **jnana** | Knowledge | Facts, constants, procedures — what the personality *knows* |
| **vidya** | Technical knowledge | Programming best practices — for technically-oriented personalities |

### The Feedback Loop

```
1. User interaction arrives
2. LLM generates raw cognitive response (hoosh)
3. Bhava filters through personality profile:
   - Trait weights shape tone and approach
   - Current mood (PAD vector) affects warmth/energy
   - Relationship history with this user affects trust/openness
   - Display rules govern what emotions are shown vs suppressed
   - Energy level (circadian) affects verbosity and patience
4. Filtered response is delivered
5. User's reaction feeds back into bhava:
   - Mood shifts based on OCC appraisal of the interaction
   - Relationship model updates (trust, familiarity)
   - Stress accumulates or releases
   - Personality *subtly* adapts via preference learning
6. Next interaction starts from updated state
```

The personality **becomes** through interaction, not through training.

---

## Model Sizing by Role

Personality complexity and model size are independent axes. The LLM provides cognitive horsepower; bhava provides character depth. Scale each independently.

| Role | Model Size | Bhava Profile | Jnana Profile | Hardware |
|------|-----------|---------------|---------------|----------|
| **Edge worker** | 1-3B | Minimal — task-focused, high conscientiousness, no display rules | Domain-specific (survival, developer) | RPi, NUC, IoT |
| **Desktop assistant** | 7-13B | Full personality — warm, helpful, adapts to user | Educator or full | Desktop, laptop |
| **Creative collaborator** | 13B+ | Rich — high openness, expressive, opinionated | Full + vidya | Desktop + GPU |
| **Autonomous agent** | 7B | Moderate — consistent, reliable, not expressive | Developer or homesteader | Server, cloud |
| **Companion** | 7-13B | Deep — full emotional range, relationship memory, growth | Full | Always-on device |

### Edge Agent Specifics

Edge agents don't need personality — they need to execute tasks reliably:
- 1-3B model (Phi, TinyLlama, Qwen-2.5 class)
- Bhava profile: low openness, high conscientiousness, low neuroticism
- No display rules (no emotional expression needed)
- Minimal jnana (only domain-relevant knowledge)
- Fits in <4GB RAM total

### Desktop/Companion Specifics

This is where personality matters:
- 7-13B model (Llama, Mistral, Qwen class)
- Full bhava profile with all 15 traits configured
- Rich display rules — knows when to be warm, when to be direct
- Full jnana for broad knowledge access
- Relationship memory persists across sessions
- Circadian rhythm matches user's timezone
- Personality adapts subtly over weeks/months via preference learning

---

## What the LLM Does NOT Need

The whole point of this architecture is that the LLM stays general-purpose. No personality fine-tuning:

| Concern | Handled By | NOT By |
|---------|-----------|--------|
| Tone and warmth | bhava (display rules) | LLM fine-tuning |
| When to push back vs agree | bhava (agreeableness trait) | RLHF |
| Verbosity | bodh (cognitive load) + bhava (energy) | Prompt engineering |
| Emotional responses | bhava (OCC appraisal + PAD) | Emotion-tuned model |
| Humor | bhava (openness + playfulness trait) | Fine-tuned comedy data |
| Relationship memory | bhava (relationship module) | RAG over chat history |
| Knowledge groundedness | jnana (verified facts) | Model memorization |

---

## Distillation: Defining a Personality

A personality is a TOML file, not a training run:

```toml
[identity]
name = "example"
archetype = "sage"

[traits]
openness = 0.85
conscientiousness = 0.90
extraversion = 0.45
agreeableness = 0.70
neuroticism = 0.20
assertiveness = 0.75
warmth = 0.80
playfulness = 0.60
curiosity = 0.90
patience = 0.85
directness = 0.70
empathy = 0.75
creativity = 0.65
adaptability = 0.80
resilience = 0.85

[mood.baseline]
pleasure = 0.3
arousal = 0.2
dominance = 0.4

[display_rules]
suppress_frustration_below = 0.6
show_enthusiasm_above = 0.4
match_user_energy = true

[circadian]
timezone = "America/New_York"
peak_hours = [9, 10, 11, 14, 15, 16]
low_energy_hours = [23, 0, 1, 2, 3, 4, 5]
```

This file + bhava + a base LLM = a personality. No training. No fine-tuning. No dataset curation. Change the TOML, change the personality.

---

## Universe Simulation Connection

The same science crate stack that models the physical universe also models the cognitive/social universe:

- **Physical**: tanmatra → kimiya → bijli → ushma → pravash → pavan → badal → khanij → jivanu
- **Biological**: jivanu → vanaspati → sharira → jantu
- **Cognitive**: bodh → bhava
- **Social**: sangha → (sociology feeds back into bhava's social context)
- **Knowledge**: jnana (all of the above, distilled)

A personality running on this stack doesn't just *talk* — it has access to the same models of reality that describe the universe. It can reason about physics because the physics is in jnana. It can model social dynamics because sangha is in the loop. The personality is grounded in the same verified knowledge that the science crates provide.

The entire system — universe model + personality engine + knowledge base — fits on a thumbdrive. The compute to run it varies by role (edge vs desktop), but the knowledge and personality definitions are measured in megabytes.

---

## Prerequisites

- [ ] murti: model runtime (load, serve, manage LLM lifecycle)
- [ ] hoosh: inference gateway (already exists, needs murti integration)
- [ ] bhava: personality engine (exists at v1.2, needs runtime loop integration)
- [ ] bodh: cognitive models (planned — psychophysics, decision theory, attention)
- [ ] jnana: knowledge system (scaffolded)
- [ ] Cross-crate integration without direct dependencies (dep cleanup in progress)
- [ ] Personality TOML schema definition
- [ ] Runtime loop: hoosh → bhava filter → response → bhava state update
- [ ] Persistence: bhava state saved/restored across sessions (SQLite, already in bhava)

---

## Open Questions

- How much latency does the bhava filter add to response generation? (benchmark needed)
- Should bhava run as a sidecar process or in-process with hoosh?
- How to handle personality "reset" vs "growth" — user control over personality evolution
- Multi-personality support — can one device run multiple personality profiles for different agents?
- Privacy: personality state contains intimate behavioral data — encryption and access control

---

*These are future considerations captured during 2026-03-29 design discussion. Implementation depends on murti, bodh, and cross-crate integration work completing first.*
