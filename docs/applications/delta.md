# Delta

> **Delta** (Greek: change) — self-hosted code hosting platform with native CI/CD

| Field | Value |
|-------|-------|
| Status | Released |
| Version | latest |
| Repository | `MacCracken/delta` |
| Runtime | native-binary |
| Recipe | `recipes/marketplace/delta.toml` |
| MCP Tools | 5 `delta_*` |
| Agnoshi Intents | 5 |
| Port | 8070 |

---

## Why First-Party

Delta replaces the external Git hosting dependency for AGNOS development with a self-hosted platform that has native CI/CD, an artifact registry, and AI code review built in. No existing self-hosted Git platform (Gitea, Forgejo) integrates with a local LLM for automated code review or with daimon for agent-driven CI pipelines.

## What It Does

- Git repository hosting with web interface for browsing, PRs, and issues
- Native CI/CD pipeline engine with YAML workflow definitions
- Artifact registry for build outputs and release assets
- AI-powered code review via hoosh: automated suggestions, security scanning, style checks
- Agent-driven automation through daimon integration (auto-merge, deployment triggers)

## AGNOS Integration

- **Daimon**: Registers as an agent; publishes repository events (push, PR, CI status); uses agent capabilities for automated workflows
- **Hoosh**: AI code review, commit message suggestions, PR summarization, security vulnerability detection
- **MCP Tools**: `delta_repo`, `delta_pr`, `delta_ci`, `delta_search`, `delta_review`
- **Agnoshi Intents**: `delta repo <action>`, `delta pr <action>`, `delta ci <action>`, `delta search <query>`, `delta review <pr>`
- **Marketplace**: Developer/Infrastructure category; sandbox profile allows network (port 8070), read-write repository storage, Git binary access

## Architecture

- **Crates**:
  - `delta-core` — repository model, user management, permissions
  - `delta-vcs` — Git operations, diff engine, merge logic
  - `delta-api` — REST API, authentication, webhook delivery
  - `delta-ci` — CI/CD pipeline runner, workflow parser, job scheduling
  - `delta-registry` — artifact storage, release management, container registry
  - `delta-web` — web frontend, repository browser, PR review UI
- **Dependencies**: libgit2 (Git operations), SQLite (metadata), daimon agent API

## Roadmap

- Federation with other Delta instances for cross-org collaboration
- Container image building in CI pipelines
- AI-assisted issue triage and duplicate detection
- Integration with takumi for automated package builds on push
