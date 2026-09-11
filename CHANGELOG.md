# Changelog

All notable changes to the `twrite` editor engine will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Multi-click selection & drag-expansion snapping (`twrite-core`, `twrite-gpui`)**:
  - Double-click highlights the clicked word under the cursor; triple-click highlights the entire line (including trailing line terminator).
  - Multi-click drag selection extends with boundary snapping: dragging after double-click expands word-by-word; dragging after triple-click expands line-by-line.
  - New boundary helpers in `twrite-core`: `find_word_range_at`, `find_line_range_at`, and convenience methods `EditorBuffer::{word_range_at, line_range_at}`.
  - New `twrite_gpui::SelectionGranularity` enum (`Character`, `Word`, `Line`) tracking active drag granularity on `Editor`.
  - Hyperlink opening and task checkbox toggling restricted to single clicks (`click_count == 1`) so double-clicking links selects text without re-opening browsers.

## [0.5.0] - 2026-09-10

### Added
- **Getting-started pass (`examples/`, `README.md`):** numbered example order
  (`simple` to `vim`) with header pointers, a README run map, a battery-neutral
  hooks demo, and a new `prompt` example (goto-line plus command palette on
  `ctx.prompt`).
- **Headless find & replace engine (`twrite-core`)**: new `search` module with
  `SearchQuery` (literal + regex, case and whole-word toggles),
  `find_matches` / `find_next` / `find_prev` (wrapping), version-cached
  `SearchState` for interactive find, and single-undo batch replace
  (`EditorBuffer::replace_many`, `replace_all_query` with `$1` / `$name`
  capture expansion, `replace_one_query`, line-scoped `collect_replacements`).
  New `regex` workspace dependency; `EditorError::{EmptySearchPattern, InvalidRegex}`.
- **Headless prompt primitive (`twrite-core`)**: new `prompt` module with
  `PromptState` (UTF-8-safe single-line editing, `Ctrl+W/U/A/E` shortcuts,
  submitted-input history with draft restore, item list + wrapping selection,
  `Tab` completion, `fuzzy_score` / `fuzzy_filter`) in `BottomBar` and
  `TopPalette` placements, plus `HookEffect::{Save, Load, Quit, Message}` for
  app-level requests frontends drain.
- **Stock `SearchHook` (`twrite-core`)**: `Ctrl+F` find (selection seeds the
  query) with live refresh, `Enter` / `F3` next, `Shift+F3` previous,
  `Ctrl+H` replace field, `Ctrl+Enter` replace-one, `Alt+A` single-undo
  replace-all, `Escape` close, and `SEARCH n/m` / `REPLACE` status text.
  Exposes `navigate_next_from` / `navigate_prev_from` for vim `*` / `#`.
- **Vim search & ex-commands (`examples/vim.rs`)**: `/` / `?` live search with
  `n` / `N` following the direction, `*` / `#` word under cursor, and a `:`
  prompt supporting `:w [path]`, `:q[!]`, `:wq`, `:e path`, `:<num>`, and
  `:s` / `:%s` with `[g][i]` flags (regex, `$1` captures, single undo).
- **Prompt box renderer (`twrite-gpui`)**: new `PromptBar` drawing the shared
  `PromptState` as a bottom line or floating `F1` palette (input with block
  cursor, item rows, message line). `Editor` owns the state, swallows buffer
  keys while it is open, and `flush_effects()` executes file effects inline
  (`take_effects()` leaves `Quit` / `Message` for hosts). The `markdown`
  example wires `Ctrl+F` / `Ctrl+H` for free.
- **Search toggles + highlight-all (`twrite-core`, `twrite-gpui`)**: `SearchHook`
  gains Match Case / Whole Word / Highlight All flags (status shows
  `[Aa] [w] [H]`), `Alt+C` / `Alt+W` / `Alt+H` shortcuts, `Up` / `Down` match
  stepping (`Alt+Up` / `Alt+Down` still reach input history), and a
  `SearchSnapshot` bridge (`EditorHook::search_snapshot`, default `None`)
  that composite hooks forward. `Editor` syncs `search_matches` after input
  and `PromptBar` renders clickable `Aa` / `W` / `All` chips plus `↑` / `↓`
  steppers that dispatch through the same key path; the canvas paints a
  viewport-clipped `theme.search_match` wash under the text.

### Changed
- **BREAKING**: `HookContext` gains `prompt: &mut PromptState` and
  `effects: &mut Vec<HookEffect>`; constructors take five arguments.
- **BREAKING**: `Editor` gains `prompt`, `pending_effects`, and `file_path`
  fields (struct literals need updating); `load_file` records the path for
  `Save { path: None }`.
- Multi-edit undo/redo now track length shifts, so growing batch
  replacements round-trip exactly in one undo step.

### Fixed
- **Capital and shifted-symbol input (`twrite-gpui`)**: `translate_key_down`
  now prefers `key_char` (the typed character) over the physical key name, so
  Shift+letter, shifted symbols, and non-US layouts type correctly everywhere.
  Command combos keep physical names; Option/Alt keeps toggle bindings unless
  the platform prefers character input. Also revives Shift-bound keys
  (`G`, `N`, `*`, `:`, `$`) in the vim example.

## [0.4.0] - 2026-09-07

### Added
- **FPS counter & performance HUD (`twrite-gpui`)**: new `fps` module with
  `FrameStats` (rolling 120-frame window tracking FPS, average frame duration,
  and worst frame hitch) and `fps_badge` (color-coded GPUI status element:
  green ≥ 50fps, amber ≥ 25fps, red below). `Editor::frame_stats` records
  frame intervals on paint without forcing repaints; integrated into the
  `markdown` and `simple` examples.
- **Indexed fence query helpers (`twrite-core`)**: `fence_rows`, `is_fenced_row`,
  `table_block_at_with_fences`, and `table_layouts_with_fences` allow callers
  to hoist the `O(N)` fence scan out of hot per-row loops and evaluate
  fenced status in `O(log F)` time via binary search.

### Fixed
- **Markdown selection recalculation & table query jank**:
  - `MarkdownHighlighter::highlight_line`, `should_wrap_line`, and `expand_line`
    now serve point queries from a shared, version-cached table layout pass
    (`cached_layouts`), eliminating upward buffer walks and per-row allocations
    that previously caused multi-millisecond hitches when selection changes
    flipped line cache states in large tables.
  - Table fence detection shares one cached linear fence scan per document
    version (`cached_fence_rows`), removing quadratic `0..row` fence rescanning.

## [0.3.0] - 2026-09-04

### Added
- **Markdown GFM tables (battery-only, no core changes)**: `table_block_at`,
  `parse_delimiter_row`, `find_unescaped_pipes`, `split_table_cells`,
  `TableBlock` / `TableRowKind` / `TableAlignment`, and
  `TABLE_{HEADER,CELL,DELIMITER}_TAG` custom tags (`markdown.table.*`,
  styled with existing `Punctuation` / `Bold` / `Dimmed` (pipes are never
  `Hidden`, so concealment preserves column mapping). `MarkdownConfig::{visual_tables,
  table_navigation}` (both default `true`) and `MarkdownHook` `Tab` /
  `Shift+Tab` cell navigation (appends a skeleton row past the last cell)
  plus `Enter` row continuation / blank-row table exit.
- **Aligned table columns**: `MarkdownConfig::table_alignment` (default `true`)
  pads every column to the block's max display width (measured on unconcealed
  source text, so widths are cursor-stable; delimiter dashes are extended and
  `:--` / `--:` / `:-:` alignment is honored), and table rows opt out of
  soft-wrapping so the grid never breaks mid-row.
- **Generic display-expansion engine**: `ConcealedLine::{expanded, DisplayPad}`,
  `display_width` (Unicode column widths), and defaulted
  `SyntaxHighlighter::{expand_line, should_wrap_line}` hooks wired through
  `LayoutCache::CachedInput::{concealed, allow_wrap}` into canvas prepaint,
  hit-testing, and scroll estimation. Markdown tables are the first client;
  other highlighters are unaffected (defaults are no-ops).
- New `unicode-width` workspace dependency backing `display_width`.

### Changed
- **Battery layout**: the Markdown battery is now `batteries/markdown/`
  (`config`, `table`, `highlight`, `hook`, `links` modules with colocated
  tests); public paths `twrite_core::markdown` / `twrite::markdown` unchanged.

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
