# AGNOS Long-Term Improvements

Roadmap for expanding the AGNOS application ecosystem with AI-native tools.

---

## Tier 1 — Highest AI Leverage

### Tazama — AI-Native Video Editor
*Swahili: to watch, to observe*

- **Type**: Non-linear video editor with deep AI integration
- **Language**: Rust + GPU compute (Vulkan)
- **AI Features**: Auto-cut, scene detection, AI voiceover/TTS, subtitle generation, b-roll suggestions, style transfer, AI color grading, smart transitions
- **Infrastructure**: GStreamer, Vulkan, PipeWire already in desktop recipes
- **Pairs with**: Shruti (audio), screen capture/recording APIs
- **Why**: No credible open-source AI-native video editor exists. Highest differentiation potential of any creative tool.

### Rasa — AI-Native Image Editor & Design Tool
*Sanskrit: रस (essence, flavor, aesthetic emotion — the core concept in Indian aesthetics)*

- **Type**: Raster/vector image editor with generative AI
- **Language**: Rust + GPU compute
- **AI Features**: Inpainting, upscaling, background removal, generative fill, style transfer, text-to-image, AI-assisted selection
- **Infrastructure**: Cairo, Pixman, libpng, libjpeg-turbo, Vulkan in recipes; local Stable Diffusion via hoosh/Synapse
- **Pairs with**: Tazama (creative suite), screen capture API
- **Why**: Local-first alternative to cloud-dependent AI image tools. Completes the creative suite alongside Tazama and Shruti.

### Mneme — AI-Native Knowledge Base & Notes
*Greek: μνήμη (memory — muse of memory)*

- **Type**: Personal knowledge management with semantic AI
- **Language**: Rust
- **AI Features**: Semantic search, auto-linking, summarization, RAG over personal docs, concept extraction, AI-assisted writing, knowledge graph visualization
- **Infrastructure**: RAG API, vector store, knowledge endpoints already exist in daimon
- **Pairs with**: Existing `/v1/rag/*`, `/v1/vectors/*`, `/v1/knowledge/*` APIs
- **Why**: Directly leverages existing AGNOS infrastructure (RAG, vector store). Minimal new plumbing needed — highest ROI of any Tier 1 item.

---

## Tier 2 — Strong Utility + AI Enhancement

### PDF / Document Suite
- Reader, annotator, form filler
- AI: OCR (Tesseract already in Aequi), summarization, translation, document Q&A
- No document viewer recipe currently exists

### Email Client
- Local-first, privacy-respecting
- AI: Smart compose, priority sorting, thread summarization, phishing detection (aegis integration)
- No email recipe currently exists

### 3D Modeler / CAD
- AI-assisted parametric design, text-to-3D
- Infrastructure: Vulkan + Mesa ready
- Niche but extremely high AI leverage

---

## Tier 3 — Expected Desktop Applications

### ~~Calendar / Contacts~~ → **Rahd** (Ruznam Ahd) ✓
- **Status**: Scaffolded (`/home/macro/Repos/rahd`), 49 tests, CI/CD, marketplace recipe
- Named: Ruznam Ahd (Persian: daily record + Arabic: appointment), CLI: rahd
- 5 crates: core, store (SQLite), schedule (conflicts/free slots), ai (NL parsing), mcp

### ~~System Monitor / Task Manager~~ → **Nazar** ✓
- **Status**: Scaffolded (`/home/macro/Repos/nazar`), 27 tests, CI/CD, marketplace recipe
- Named: Nazar (Arabic/Persian: نظر — watchful eye)
- 5 crates: core, api, ui (egui), ai (anomaly + prediction), mcp

### ~~Screenshot / Annotation Tool~~ → **Selah** ✓
- **Status**: Scaffolded (`/home/macro/Repos/selah`), 50 tests, CI/CD, marketplace recipe
- Named: Selah (Hebrew: סלה — pause/capture a moment)
- 4 crates: core, capture (daimon API), annotate (SVG overlay), ai (OCR + redaction)

### ~~Calculator / Unit Converter~~ → **Abaco** ✓
- **Status**: Scaffolded (`/home/macro/Repos/abaco`), 61 tests, CI/CD, marketplace recipe
- Named: Abaco (Italian/Spanish: abacus)
- 4 crates: core, eval (expression parser), units (60+ units), ai (NL math)

---

## Implementation Notes

- All Tier 1 apps follow the proven consumer project pattern: Rust crates, 5 MCP tools, 5 agnoshi intents, marketplace recipe, `.agnos-agent` bundle
- Names follow project convention (Swahili/Greek/Sanskrit/Japanese/Persian/Latin)
- Tier 1 creative suite (Tazama + Rasa + Shruti) forms a coherent AI-native creative platform
- Tier 2-3 items can be community-contributed or built as needed for beta completeness
- Nazar (system monitor) is the first Tier 3 app — separate repo, follows consumer project pattern
