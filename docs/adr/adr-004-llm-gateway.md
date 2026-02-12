# ADR-004: LLM Gateway Service Design

**Status:** Accepted

**Date:** 2026-02-10

**Authors:** AGNOS Team

## Context

AGNOS needs a centralized service to manage LLM access:
- Multiple provider support (local and cloud)
- Token accounting per agent
- Response caching
- Model switching and fallback
- API key management

## Decision

We will implement an **LLM Gateway Service** as a standalone process:
1. **Provider Abstraction** - Unified interface for different LLM backends
2. **Local Priority** - Prefer local models (Ollama, llama.cpp)
3. **Cloud Fallback** - Use cloud APIs when local unavailable
4. **Token Accounting** - Track usage per agent
5. **Response Caching** - Cache common queries

## Consequences

### Positive
- Centralized API key management
- Consistent interface for all agents
- Can enforce rate limits and quotas
- Easier to swap LLM providers
- Better cost control for cloud APIs

### Negative
- Additional network hop
- Gateway becomes critical component
- Caching adds complexity

## Provider Support

| Provider | Type | Status | Priority |
|----------|------|--------|----------|
| Ollama | Local | ✅ Implemented | High |
| llama.cpp | Local | ✅ Implemented | High |
| OpenAI | Cloud | Planned | Medium |
| Anthropic | Cloud | Planned | Medium |
| Google | Cloud | Planned | Low |

## Architecture

```
┌─────────────────────────────────────┐
│          LLM Gateway               │
│  ┌──────────┐  ┌──────────────┐   │
│  │ Providers│  │   Cache      │   │
│  ├──────────┤  ├──────────────┤   │
│  │ Ollama   │  │  Accounting  │   │
│  │ llama.cpp│  │              │   │
│  │ OpenAI   │  └──────────────┘   │
│  └──────────┘                       │
└─────────────────────────────────────┘
```

## References

- [Ollama API](https://github.com/ollama/ollama/blob/main/docs/api.md)
- [llama.cpp Server](https://github.com/ggerganov/llama.cpp/blob/master/examples/server/README.md)
- [OpenAI API](https://platform.openai.com/docs/api-reference)
