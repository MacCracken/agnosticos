# Sahifa + Scriba — PDF / Document Suite

> **Status**: P0 — Design Phase | **Last Updated**: 2026-03-30
>
> AI-native PDF & document suite for AGNOS.
> Replaces Adobe Acrobat Pro for daily professional workflows.
> Requirements sourced from real user (daily Acrobat Pro user).

---

## Names

- **Sahifa** (صحيفة — Arabic: page, document) — PDF engine library crate, `GPL-3.0-only`
- **Scriba** (Latin: scribe) — Desktop GUI application, `AGPL-3.0-only`

---

## User Requirements (from Acrobat Pro usage)

### 1. Advanced Editing & Organization
- Edit text directly in PDFs (reflow-aware, font matching)
- Edit/replace images within PDFs
- Reorder, delete, insert, crop, rotate pages
- Merge multiple PDFs into one
- Split PDF into separate files
- Page thumbnail sidebar for drag-and-drop reorder

### 2. AI Assistant
- Generate summaries of document content
- Answer questions about document content (RAG over the PDF)
- Extract key insights, dates, names, amounts
- Multi-document Q&A (ask across a folder of PDFs)
- Integration: hoosh for LLM inference, daimon for RAG/vector store

### 3. Convert & Create
- **PDF → Office**: Export to Word (.docx), Excel (.xlsx), PowerPoint (.pptx)
- **Office → PDF**: Import and convert from Office formats
- **Image → PDF**: Create PDFs from images (JPEG, PNG, TIFF)
- **Web → PDF**: Capture web pages as PDFs (headless render)
- **Scanner → PDF**: Direct scanner/SANE integration for paper-to-PDF pipeline
- Batch conversion (folder of files → folder of PDFs)

### 4. Security & Redaction
- **Redaction**: Permanently remove sensitive text/images (burn-in, not just overlay)
- Search-and-redact (find all instances of SSN, email, phone patterns)
- AI-assisted redaction suggestions (PII detection via phylax patterns)
- Password protection (AES-256 encryption)
- Permission controls (print, copy, edit restrictions)
- Digital certificate signatures (X.509)

### 5. E-Signatures & Forms
- Create fillable form fields (text, checkbox, radio, dropdown, date)
- Draw/type/upload signature
- Request signatures from others (email workflow)
- Track signature status in real time
- Form data export (CSV, JSON)
- Auto-detect form fields from scanned documents

### 6. Scan & OCR
- Scanner integration via SANE/libsane
- OCR engine: Tesseract (recipe exists) or custom
- Convert scanned images to searchable, selectable, editable text
- Language detection and multi-language OCR
- Deskew, denoise, contrast enhancement pre-processing
- Batch scan (ADF/multi-page feeder support)

### 7. Document Comparison
- Side-by-side visual diff of two PDF versions
- Highlight additions, deletions, modifications
- Text-level and visual-level comparison modes
- Summary report of all changes
- Navigate between differences

---

## Architecture

### Crate Structure

Two repos, same pattern as ifran (engine) + tanur (GUI):

```
sahifa/                              # Engine — GPL-3.0-only
├── Cargo.toml                       # Flat crate, feature-gated
├── src/
│   ├── lib.rs                       # Public API
│   ├── parser.rs                    # PDF object model (cross-ref, streams, fonts)
│   ├── renderer.rs                  # Page rasterization (cairo or skia)
│   ├── editor.rs                    # Text/image editing, page manipulation
│   ├── writer.rs                    # PDF serialization (incremental save)
│   ├── ocr/                         # Scan & OCR pipeline
│   │   ├── scanner.rs               # SANE integration
│   │   ├── preprocess.rs            # Deskew, denoise, threshold
│   │   └── recognize.rs             # Tesseract FFI
│   ├── convert/                     # Format conversion
│   │   ├── docx.rs                  # PDF ↔ Word
│   │   ├── xlsx.rs                  # PDF → Excel (table extraction)
│   │   ├── pptx.rs                  # PDF → PowerPoint
│   │   └── image.rs                 # PDF ↔ image formats
│   ├── security/                    # Encryption, redaction, signatures
│   │   ├── encrypt.rs               # AES-256, permissions
│   │   ├── redact.rs                # Permanent content removal
│   │   ├── sign.rs                  # Digital signatures (X.509)
│   │   └── pii.rs                   # PII pattern detection
│   ├── forms/                       # Fillable forms
│   │   ├── fields.rs                # Form field types
│   │   ├── esign.rs                 # Signature data model
│   │   └── export.rs                # Form data export (CSV, JSON)
│   ├── compare/                     # Document comparison
│   │   ├── diff.rs                  # Text-level diff
│   │   └── visual.rs                # Pixel-level diff
│   └── ai/                          # AI integration (feature-gated)
│       ├── daimon.rs                # Agent registration, RAG ingestion
│       ├── assistant.rs             # Q&A, summarization, insights
│       └── mcp.rs                   # MCP tool server

scriba/                              # Desktop GUI — AGPL-3.0-only
├── Cargo.toml                       # Depends on sahifa
├── src/
│   ├── main.rs                      # App entry
│   ├── app.rs                       # Main window, toolbar, sidebar
│   ├── viewer.rs                    # Page display, zoom, scroll
│   ├── annotate.rs                  # Highlight, comment, stamp
│   ├── forms_ui.rs                  # Form field editor
│   ├── redact_ui.rs                 # Redaction tool UI
│   ├── compare_ui.rs               # Side-by-side diff view
│   ├── scan_ui.rs                   # Scanner workflow UI
│   └── esign_ui.rs                  # Signature request/tracking UI
```

### Key Dependencies

| Need | Crate/Library |
|------|---------------|
| PDF parsing | `lopdf` or `pdf-rs` (evaluate both) |
| Page rendering | poppler (recipe done) or mupdf bindings |
| OCR | Tesseract via `tesseract-rs` FFI |
| Scanner | SANE via `libsane` FFI |
| Image processing | `ranga` (AGNOS image processing) |
| AI inference | `hoosh` client (LLM gateway) |
| RAG/vectors | daimon API (vector store, RAG ingestion) |
| PII detection | `phylax` patterns (regex + entropy) |
| GUI toolkit | gtk4-rs or iced (evaluate) |
| Office formats | `docx-rs`, `calamine` (xlsx), custom pptx |
| Crypto | openssl bindings (AES-256, X.509) |
| Text diff | `similar` crate |

### AGNOS Integration

| Integration | How |
|-------------|-----|
| **Daimon** | Tier 1 (lifecycle) + Tier 3 (RAG) + Tier 4 (inference) |
| **Hoosh** | Summarization, Q&A, PII detection, form field detection |
| **Phylax** | PII pattern matching for redaction suggestions |
| **MCP tools** | 5-8 tools: open, summarize, redact, convert, sign, compare, extract_text, extract_tables |
| **Agnoshi** | Intents: "open this PDF", "summarize document", "redact SSNs", "convert to Word" |
| **Marketplace** | Recipe in `recipes/marketplace/` |

---

## Feature Prioritization

### MVP (v0.1.0 — scaffold)
- [ ] PDF parsing and page rendering
- [ ] Page viewer with zoom, scroll, thumbnail sidebar
- [ ] Page reorder, delete, insert, merge, split
- [ ] Basic text selection and copy
- [ ] Print support

### v0.2.0 — Editing & OCR
- [ ] Text editing (in-place, font-aware)
- [ ] Image editing (replace, resize, crop)
- [ ] Scanner integration (SANE)
- [ ] OCR pipeline (Tesseract)
- [ ] Searchable PDF output from scans

### v0.3.0 — Security & Redaction
- [ ] Password protection (AES-256)
- [ ] Permission controls
- [ ] Manual redaction (select and burn-in)
- [ ] Search-and-redact (pattern matching)
- [ ] AI-assisted PII detection

### v0.4.0 — Conversion
- [ ] PDF → Word export
- [ ] PDF → Excel export (table extraction)
- [ ] PDF → PowerPoint export
- [ ] Image → PDF, Web → PDF
- [ ] Batch conversion

### v0.5.0 — Forms & E-Signatures
- [ ] Fillable form creation (field types)
- [ ] Auto-detect form fields from scanned docs
- [ ] Draw/type/upload signature
- [ ] Signature request workflow
- [ ] Form data export (CSV, JSON)

### v0.6.0 — AI Assistant
- [ ] Document summarization
- [ ] Q&A over document content
- [ ] Multi-document Q&A
- [ ] Key entity extraction
- [ ] MCP tools + agnoshi intents

### v0.7.0 — Document Comparison
- [ ] Text-level diff
- [ ] Visual diff (side-by-side)
- [ ] Change summary report
- [ ] Navigation between differences

### v1.0 — Production
- [ ] All features stable
- [ ] 80%+ test coverage
- [ ] Benchmark suite
- [ ] Marketplace recipe
- [ ] Full documentation

---

## Open Questions

- [ ] **Name**: What fits the AGNOS naming convention?
- [ ] **GUI toolkit**: gtk4-rs (GNOME native) vs iced (pure Rust) vs egui?
- [ ] **PDF engine**: lopdf (pure Rust, lighter) vs mupdf bindings (battle-tested, heavier)?
- [ ] **E-sign workflow**: Local-only or needs a server component for tracking?
- [ ] **Office export fidelity**: How close to Acrobat Pro quality do we need? 80%? 95%?

---

*Last Updated: 2026-03-30*
