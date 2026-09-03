# Changelog

All notable changes to the `twrite` editor engine will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-09-03

### Added
- **Syntax-agnostic core**: new `HighlightTag::{Blockquote, HorizontalRule, TaskUnchecked, TaskChecked}` structural tags; `SyntaxHighlighter::extract_links` and `EditorHook::on_click` extension points (both with default impls).
- **Battery registry**: `twrite_core::batteries` module documenting the contract and checklist for adding feature-gated batteries; Markdown moved to `batteries/markdown.rs` with the public `twrite_core::markdown` / `twrite::markdown` paths unchanged.
- **Viewport input cache**: new `twrite_gpui::{LayoutCache, CachedInput}` sharing highlight/conceal/link work across prepaint and hit-testing, with hit/miss stats.
- **Thousand-line perf proofs**: headless `highlight_perf` / `layout_perf` integration tests (deterministic fixtures, `#[ignore]`d timing cases) asserting cache hit rates.
- `MarkdownHook::with_config` and `interactive_tasks` toggle.
- **Font handling**: `EditorConfig::{font_family, code_font_family}` (`None` inherits
  the host GPUI text style); `Editor::face_availability` surfacing bold/italic
  face probe results from prepaint; `RunFonts` param bundle for
  `build_line_text_runs` (code spans use the code font).
- **Font auto-select**: `EditorConfig::platform_monospace_candidates` plus
  `pick_family` choosing the first candidate with bold + italic faces at paint
  time; explicit `font_family` is trusted verbatim; `Editor::selected_font_family`
  and family-aware status reporting; examples use the default auto-select path.
- **Tag extensibility**: `HighlightTag::Heading(u8)` levels and open
  `HighlightTag::Custom(&'static str)` extension point with
  `SyntaxTheme::set_custom_tag_color` (unregistered names fall back to the
  foreground); heading metrics use a single min-level scan.

### Changed
- **BREAKING**: `Editor::offset_for_position` now takes `&mut self`.
- **BREAKING**: `VisibleLineLayout` gains `task_state`; `Editor` gains `layout_cache` and `highlighter_rev` (struct literals need updating).
- **BREAKING**: new `HighlightTag` variants (exhaustive matches need new arms); theme maps them to comment/punctuation/string.
- **BREAKING**: `HighlightTag::{Heading1..Heading6, Speaker, Dialogue, Choice}` removed
  in favor of `Heading(u8)` and `Custom(&'static str)`; `SyntaxTheme::{speaker, dialogue, choice}`
  fields removed (register custom colors instead).
- `LineMetrics::for_line` derives quote/divider/task state solely from tags (no raw string checks); code-block detection via full-line `Code` spans.
- Hit-testing (`offset_for_position`, link and checkbox lookup) uses binary search over visible lines.
- Click handling dispatches task toggles to hooks with `after_edit` / `on_selection_change` notifications; cursor offset preserved.

## [0.1.0] - 2026-09-03

### Added
- **Core Buffer Engine (`twrite-core`)**:
  - Rope-backed text storage with byte and char indexing.
  - Granular undo and redo transaction history.
  - Multi-line cursor navigation, word boundaries, and line-end movements.
  - Monotonic document version counter.
- **Styling & Syntax Engine**:
  - Multi-span interval splitting algorithm (`split_line_intervals`).
  - Semantic highlight tags (`HighlightTag`) and explicit font styles (`TextStyle`).
  - Catppuccin Mocha syntax theme integration in GPUI canvas.
  - Live shaping with bold weights, italic fonts, and underline decorations.
- **Versatile Hook System**:
  - `HookContext` providing mutable access to text buffer, selection, and cursor styles.
  - `EditorHook` lifecycle (`on_key`, `before_insert`, `after_edit`, `on_selection_change`, `status_text`).
  - Built-in `AutoPairsHook` with auto-closing, selection wrapping, and smart backspacing.
  - Dynamic cursor shapes (`Bar`, `Block`, `Underline`, `Hidden`).
- **Examples**:
  - `simple`: Minimal baseline editor.
  - `syntax`: Markdown headings, inline code, and story script dialogue.
  - `hooks`: Concurrent hook composition with auto-pairs and markdown shortcuts.
  - `vim`: Full modal editing (Normal, Insert, Visual) built 100% via hooks.
- **Package Architecture**:
  - Centralized workspace versioning with `version.workspace = true`.
  - Single top-level facade import (`use twrite::Editor;`).
  - Pinned GPUI dependency revision for reproducible downstream builds.
