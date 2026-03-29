# Vidya

> **Vidya** (Sanskrit: विद्या — knowledge, learning) — Programming reference library and queryable corpus

| Field | Value |
|-------|-------|
| Status | Planned |
| Version | `0.1.0` |
| Repository | `MacCracken/vidya` |
| Runtime | library crate (Rust) + content directory |

---

## What It Does

Vidya is both a curated programming reference and a Rust crate that serves it. The content directory contains tested implementations of programming concepts across multiple languages. The crate makes it queryable.

### Content (no compilation needed)

- **Concept documentation**: best practices, instructional explanations, patterns and anti-patterns
- **Multi-language implementations**: Rust, Python, C, Go, TypeScript — each tested and proven correct
- **Topics**: strings, concurrency, error handling, data structures, memory management, pattern matching, type systems, I/O, testing, algorithms, and more
- **Every code example is a test**: CI compiles/runs every implementation in every language

### Crate (Rust library)

- **Concept registry**: structured types for `Concept`, `Language`, `Example`, `BestPractice`
- **Search**: full-text and tag-based lookup across concepts and languages
- **Compare**: side-by-side implementations across languages for the same concept
- **Validate**: compile and run examples, verify correctness programmatically
- **Exceptionally documented**: `cargo doc` generates a browsable programming reference

### MCP Tools

- `vidya_lookup` — find a concept's implementation in a specific language
- `vidya_compare` — side-by-side comparison across languages
- `vidya_best_practice` — best practices for a concept
- `vidya_search` — full-text search across all content
- `vidya_languages` — list available languages for a concept

## Architecture

```
vidya/
├── content/           # Raw corpus — markdown + source files (no compilation needed)
│   ├── strings/       # concept.md + rust.rs + python.py + c.c + go.go + typescript.ts
│   ├── concurrency/
│   ├── error_handling/
│   └── ...
├── src/               # Rust crate — queryable interface, exceptionally documented
│   ├── lib.rs
│   ├── concept.rs     # Concept, Language, Example, BestPractice types
│   ├── search.rs      # Full-text + tag search
│   ├── validate.rs    # Compile/run examples, verify correctness
│   └── mcp.rs         # MCP tool implementations
└── tests/
    └── validate_all.rs  # CI runs every example in every language
```

## Consumers

- **agnoshi** — AI shell can query programming references via MCP
- **hoosh** — LLM gateway can ground answers in tested implementations
- **daimon** — agents can look up best practices before generating code
- AI model training — the content directory is a curated, tested corpus for Rosetta Stone quality training data

## Why It Exists

AI models learn programming from whatever exists online — Stack Overflow answers, blog posts, tutorials of varying quality. Vidya provides a curated, tested, multi-language reference where every example compiles, runs, and demonstrates the correct way to handle a concept. The crate is the interface; the content is the corpus; the tests are the proof.
