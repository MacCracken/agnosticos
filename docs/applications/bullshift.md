# BullShift

> **BullShift** (English: wordplay on market shifts) — AI-native trading platform

| Field | Value |
|-------|-------|
| Status | Released |
| Version | latest |
| Repository | `MacCracken/BullShift` |
| Runtime | native-binary (~2.8MB) |
| Recipe | `recipes/marketplace/bullshift.toml` |
| MCP Tools | 7 `bullshift_*` |
| Agnoshi Intents | 7 |
| Port | N/A |

---

## Why First-Party

No existing open-source trading platform integrates with a local LLM for AI-native sentiment analysis, pattern recognition, and natural language portfolio queries. BullShift connects directly to hoosh for market intelligence and to daimon for agent-based trading automation, keeping all data and inference local rather than depending on cloud APIs.

## What It Does

- Real-time market data aggregation and portfolio tracking
- AI-powered sentiment analysis on news and social feeds via hoosh
- Pattern recognition and technical analysis with LLM-generated explanations
- NL portfolio queries ("what's my exposure to tech?", "show my best performers this month")
- Automated trading strategies with agent-based execution and approval workflows

## AGNOS Integration

- **Daimon**: Registers as an agent; publishes market events; uses approval system for trade execution; RAG for market research
- **Hoosh**: Sentiment analysis, pattern explanation, NL query interpretation, market summarization
- **MCP Tools**: `bullshift_portfolio`, `bullshift_trade`, `bullshift_analyze`, `bullshift_watch`, `bullshift_history`, `bullshift_strategy`, `bullshift_sentiment`
- **Agnoshi Intents**: `bullshift portfolio`, `bullshift trade <action>`, `bullshift analyze <symbol>`, `bullshift watch <symbol>`, `bullshift history <range>`, `bullshift strategy <name>`, `bullshift sentiment <topic>`
- **Marketplace**: Finance/Trading category; sandbox profile allows network access (market data feeds), encrypted storage for credentials, no filesystem write outside data directory

## Architecture

- **Crates**:
  - `bullshift-core` — portfolio model, trade engine, market data types
  - `bullshift-data` — market data feeds, aggregation, caching
  - `bullshift-ai` — sentiment analysis, pattern recognition, daimon/hoosh integration
  - `bullshift-ui` — TUI dashboard, charts, portfolio views
  - `bullshift-strategy` — automated trading strategies, backtesting
- **Dependencies**: SQLite (portfolio database), TLS (market data connections), daimon agent API

## Roadmap

- Paper trading mode for strategy backtesting
- Multi-exchange support
- Tax reporting and P&L analysis
- Collaborative portfolio management via federation
