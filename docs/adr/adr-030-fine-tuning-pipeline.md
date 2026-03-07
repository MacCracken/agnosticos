# ADR-030: Fine-Tuning Pipeline

**Status:** Accepted

**Date:** 2026-03-07

**Authors:** AGNOS Team

## Context

AGNOS runs local LLMs via the LLM Gateway (hoosh). General-purpose models work for
most tasks, but agents performing specialized work (code review, security scanning,
domain-specific analysis) benefit significantly from fine-tuned models adapted to their
specific task patterns.

Currently, fine-tuning requires manual data collection, external tooling (HuggingFace,
Axolotl), and manual model deployment. This is too complex for most users and doesn't
integrate with AGNOS agent lifecycle.

## Decision

### Training Data Collection

Agents automatically collect training examples from:
- **User feedback**: Approved/rejected agent outputs with quality scores
- **Agent self-play**: Successful task completions as positive examples
- **Curated datasets**: Manually provided domain-specific data
- **Conversation logs**: High-quality conversation turns

Each example includes input, output, quality score, source, and metadata.

### Dataset Management

`TrainingDataset` provides full lifecycle:
- Add/remove examples, filter by quality score
- Statistics: example count, length distributions, quality/source breakdowns
- Minimum example threshold before training (configurable, default varies by method)

### Fine-Tuning Methods

| Method | VRAM | Quality | Speed | Use Case |
|--------|------|---------|-------|----------|
| Full Fine-Tune | ~4x model | Best | Slow | Large datasets, maximum quality |
| LoRA | ~1.2x model | Good | Fast | Most common, balanced |
| QLoRA | ~0.5-0.7x model | Good | Fast | Limited GPU memory |
| Prefix Tuning | ~1.1x model | OK | Fastest | Quick adaptation, small datasets |

### Job Management

`FineTunePipeline` orchestrates the full workflow:
1. Validate config (learning rate, epochs, dataset size)
2. Queue job with status tracking
3. Report progress (epoch, step, loss, ETA)
4. Register completed model in `ModelRegistry`
5. Track failed/cancelled jobs

### Model Registry

Fine-tuned models are tracked with:
- Lineage (base model, training job, agent)
- Metrics (final loss, eval loss, perplexity)
- `best_model_for_agent()` — automatic selection by performance

### VRAM Estimation

Before submitting a job, `estimate_vram()` predicts GPU memory requirements
based on model size, method, and batch size. Prevents OOM failures.

## Consequences

### What becomes easier
- Agents improve at their specific tasks over time
- Users don't need ML expertise to fine-tune models
- Full provenance from training data to deployed model
- Resource planning with VRAM estimation

### What becomes harder
- GPU resources required for training (mitigated by QLoRA for consumer GPUs)
- Training data quality directly impacts model quality
- Model versioning and rollback adds operational complexity

## References

- ADR-004: LLM Gateway Service Design
- ADR-010: Advanced Agent Capabilities & Lifecycle
- LoRA: https://arxiv.org/abs/2106.09685
- QLoRA: https://arxiv.org/abs/2305.14314
