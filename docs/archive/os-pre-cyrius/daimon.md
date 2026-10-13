# Daimon — Agent Orchestrator

- **Version**: 0.5.0
- **Repo**: [MacCracken/daimon](https://github.com/MacCracken/daimon)
- **License**: GPL-3.0-only
- **Port**: 8090
- **Role**: Core agent runtime — HTTP API, supervisor, IPC, MCP dispatch

Agent lifecycle (register, heartbeat, deregister), supervisor, scheduler integration, memory/vector store, RAG, edge fleet, federation. Uses bote::host for MCP tool registry.

**Consumers**: all consumer apps, agnoshi, hoosh
