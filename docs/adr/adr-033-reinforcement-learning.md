# ADR-033: Reinforcement Learning for Agent Policy Optimization

**Status:** Accepted

**Date:** 2026-03-07

**Authors:** AGNOS Team

## Context

AGNOS agents make decisions using static heuristics: tool selection based on pattern
matching, resource allocation based on fixed thresholds, scheduling based on priority
ordering. These work for common cases but miss optimization opportunities:

1. An agent that always selects the same tool misses that another tool is faster for certain inputs
2. Static resource thresholds waste capacity or cause OOM
3. Scheduling priorities don't adapt to workload patterns

The existing `learning.rs` implements UCB1 (multi-armed bandit) for strategy selection.
This ADR extends that with full reinforcement learning: agents learn optimal policies
from experience, adapting to their specific environment.

## Decision

### RL Framework

State-action-reward-state (SARS) experiences are collected during agent operation and
used to train optimal policies.

### Algorithms

| Algorithm | Use Case | Complexity |
|-----------|----------|-----------|
| Q-Learning (tabular) | Small state/action spaces, tool selection | Low |
| Policy Gradient (REINFORCE) | Continuous action spaces, resource tuning | Medium |
| Multi-Armed Bandit (UCB1) | Already in learning.rs, extends naturally | Low |

### Components

1. **Experience Replay Buffer**: Circular buffer storing (s, a, r, s') tuples with
   prioritized sampling for important experiences
2. **Q-Table**: Tabular Q-learning with Bellman updates for discrete decisions
3. **Epsilon-Greedy Exploration**: Balance exploration vs. exploitation with decaying ε
4. **Policy Gradient**: Softmax policy with REINFORCE updates for continuous decisions
5. **Reward Shaping**: Configurable reward functions with weighted components

### Integration Points

- **Tool selection**: Q-learning over tool choices, reward = task success + speed
- **Resource allocation**: Policy gradient for memory/CPU limits
- **Scheduling**: Bandit over scheduling strategies per workload type
- **Safety**: RL policies are always overridden by safety constraints (ADR-029)

### Safety Guarantees

RL policies operate within safety bounds:
- Safety engine (safety.rs) has veto power over any RL-selected action
- Exploration is bounded by safety policies (no exploring dangerous actions)
- Reward includes negative component for safety violations

## Consequences

### What becomes easier
- Agents automatically improve at their tasks over time
- Resource utilization adapts to workload patterns
- No manual tuning of heuristics per deployment

### What becomes harder
- Non-deterministic behavior (exploration) may confuse debugging
- Cold start: new agents have no experience, fall back to heuristics
- Reward function design requires domain knowledge
- Training instability if reward signal is noisy

## References

- ADR-010: Advanced Agent Capabilities & Lifecycle
- ADR-029: AI Safety Mechanisms (safety overrides)
- Sutton & Barto: Reinforcement Learning: An Introduction
- UCB1: Auer et al., "Finite-time Analysis of the Multi-armed Bandit Problem"
