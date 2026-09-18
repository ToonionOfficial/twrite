# Changelog

All notable changes to the `twrite` editor engine will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.16.0] - 2026-09-18

### Added
- **Move lines and list items with Alt+Up/Down (`twrite-core`, `twrite-gpui`)**:
  `Alt+Up` and `Alt+Down` move the current line or selected block of lines up
  and down, preserving column positions and updating selection bounds.
  Carries list markers (bullets, checkboxes, and numbers) along with line
  contents. Sibling ordered list items are renumbered consecutively across
  the affected list block. Atomically undoable via single transactions.
  Configurable via `MarkdownConfig::list_reordering`.
  Closes [#66](https://github.com/ToonionOfficial/twrite/issues/66).

## [0.15.0] - 2026-09-18

### Added
- **List indentation and sibling renumbering (`twrite-core`)**: `Tab` indents
  and nests list items, while `Shift+Tab` unnests them. Sibling ordered list
  items are automatically renumbered consecutively at each indentation level.
  Supports unordered bullets, task checkboxes, and ordered lists, multi-line
  selections, and atomic undo/redo via `replace_many`. Configurable via
  `MarkdownConfig::list_indentation` and `MarkdownConfig::list_indent_size`.
  Closes [#65](https://github.com/ToonionOfficial/twrite/issues/65).
- **Pointer cursor for folding indicators (`twrite-gpui`)**: hovering over
  gutter fold chevrons or collapsed inline indicator badges now displays a
  pointing-hand cursor.

## [0.14.0] - 2026-09-18

### Added
- **Heading and list folding (`twrite-core`, `twrite-gpui`)**: foldable
  ranges computed line-based from heading levels and list indent (no
  tree-sitter), with `FoldState` tracking collapsed starts. Collapsed rows
  skip paint, Up/Down step over them, the cursor clamps to headers, gutter
  chevrons toggle on click, and `Alt+Left`/`Alt+Right` collapse/expand at
  the cursor. Closes [#58](https://github.com/ToonionOfficial/twrite/issues/58).

## [0.13.0] - 2026-09-18

### Added
- **Callouts and admonitions (`twrite-core`, `twrite-gpui`)**: quote blocks
  opening with `> [!KIND]` parse into a new `HighlightTag::Callout` carrying
  the kind (NOTE, TIP, WARNING, CAUTION, IMPORTANT, others fall back to the
  note accent). The bar and body wash take the kind accent from new theme
  fields, the marker conceals word-level, the title renders bold, and body
  text dims to quote gray under inline formatting; the `>` prefix conceals
  word-level like other markers. Body
  rows inherit the header kind until a blank or non-quote row; collapsing
  bodies and icons remain follow-ups.
  Closes [#52](https://github.com/ToonionOfficial/twrite/issues/52).

## [0.12.1] - 2026-09-18

### Fixed
- **Frontmatter renders as metadata (`twrite-core`)**: a document-leading
  `---` block no longer misrenders as a horizontal rule plus plain text.
  Fences and body rows dim as metadata in every conceal mode, YAML
  punctuation skips inline parsing, and link-shaped values stay
  unclickable. Unclosed openers and mid-document fences keep rule styling.
  Partially addresses [#55](https://github.com/ToonionOfficial/twrite/issues/55);
  the key-value properties view remains a follow-up.

## [0.12.0] - 2026-09-18

### Added
- **Text highlight with `== ==` (`twrite-core`, `twrite-gpui`)**: marked
  text parses into a new `HighlightTag::Highlight` and renders with a
  `syntax.highlight_bg` background tint, following the inline-code pill
  pattern. Markers conceal and reveal word by word under the active conceal
  mode; `==` inside code spans and link URLs stays literal, and nesting
  inside emphasis keeps working.
  Closes [#53](https://github.com/ToonionOfficial/twrite/issues/53).

## [0.11.0] - 2026-09-18

### Added
- **Word-level conceal reveal (`twrite-core`)**: in Hidden conceal mode,
  inline markers (bold, italic, strikethrough, code spans, links) now stay
  hidden until the cursor enters the formatted word, instead of revealing
  the whole line at once. Task checkboxes stay always visible, with the raw
  marker revealing only while the cursor sits on it, and the heading `#`
  prefix hides the moment the cursor leaves it. Quote prefixes still reveal
  row-wide.
  Closes [#71](https://github.com/ToonionOfficial/twrite/issues/71).

### Changed
- **Layout cache keys on cursor position (`twrite-gpui`, breaking)**:
  `LayoutCache::cached_input` now takes the cursor as a `Point`
  (replacing the `cursor_row` argument) so cursor-row entries invalidate
  when the cursor slides along the row.

### Fixed
- **Caret on concealed task lines (`twrite-gpui`)**: the cursor quad and
  cursor pixel now use the checkbox-shifted text origin, so stepping past
  a concealed `- [ ]` marker places the caret after the checkbox instead
  of before it.
- **Heading metrics on a bare prefix (`twrite-core`)**: a fresh `# ` with
  no text yet already carries its `Heading` tag, so line metrics and the
  caret scale on the space instead of waiting for the first character.

## [0.10.3] - 2026-09-18

### Fixed
- **Underscore bold concealment parity (`twrite-core`)**: verified that
  `__bold__` conceals exactly like `**bold**` in concealed markdown mode
  (same `Bold` tag, same delimiter hiding), with cursor-row passthrough
  and intra-word underscores staying literal per CommonMark. Locked in
  with a regression test. Closes [#51](https://github.com/ToonionOfficial/twrite/issues/51).

## [0.10.2] - 2026-09-18

### Fixed
- **Heading line-height proportions at larger font sizes (`twrite-gpui`)**:
  `LineMetrics::for_line` used independent fixed multipliers tuned for
  16px/22px (H1: font x2.0 but line height x1.8), so at larger base sizes
  the scaled line height fell behind the scaled font size and headings
  overflowed their lines. Heading line heights are now derived from the
  scaled font size with a consistent ratio, floored at the default
  proportions so raising `font_size` without raising `line_height` still
  leaves breathing room. Fixes [#45](https://github.com/ToonionOfficial/twrite/issues/45).
- **Body row line height when the font outgrows the pitch (`twrite-gpui`)**:
  body rows now keep at least the default line-height ratio (22/16), so
  e.g. 24px type on the default 22px pitch renders with a 33px line box
  instead of a cramped 22px one. Roomier custom settings are untouched.
- **Gutter line numbers (`twrite-gpui`)**: numbers now render at 80% of the
  body font size instead of full size, so consecutive rows keep breathing
  room when the configured font size meets or exceeds the line height.

## [0.10.1] - 2026-09-16

### Fixed
- **Context menu overlay position (`twrite-gpui`)**: the right-click anchor
  is window-space but the popup's absolute offsets resolve against the
  editor root, so the menu drifted by the editor's window offset in nested
  layouts (sidebar, toolbar, padding) and could land off-screen. The anchor
  is now clamped in window space, then shifted into editor-local
  coordinates.

## [0.10.0] - 2026-09-14

### Added
- **Relative line numbers (`twrite-gpui`)**: configurable relative line numbers
  in the left gutter via `EditorConfig::relative_line_numbers` (defaults to `false`).
  Lines above and below the cursor display their relative distance from the
  active cursor line. When paired with `line_numbers = true`, the active line
  displays its absolute line number (Vim-style hybrid line numbers).

## [0.9.2] - 2026-09-13

### Fixed
- **`Ctrl+Up` / `Ctrl+Down` cursor follow (`twrite-gpui`)**: scrolling the
  viewport with these keys now keeps the cursor visible. When the scroll
  pushes the cursor out of view the cursor moves to the first or last visible
  row while preserving the column (Vim `Ctrl+E` / `Ctrl+Y` style). If the
  cursor is already visible it is not moved. `Ctrl+Shift+Up` / `Ctrl+Shift+Down`
  extend the selection while scrolling. Fixes [#40](https://github.com/ToonionOfficial/twrite/issues/40).

## [0.9.1] - 2026-09-13

### Fixed
- **Crates.io packaging**: point all three crates at the workspace `README.md`
  so the registry page renders documentation (previously shipped no readme),
  and expand the `twrite` facade crate docs (layout overview, quick start,
  feature list).
- **Heading click mapping in Hidden conceal mode (`twrite-gpui`)**:
  `offset_for_position` now shapes hit-test text with the same
  `LineMetrics` font size and code-block flag used by paint, so mouse
  clicks on concealed headings (`#` through `######`) land on the clicked
  glyph instead of shifting right.

## [0.9.0] - 2026-09-13

### Changed
- **Structured `KeyCode` enum replaces stringly key names (breaking)**:
  - New crossterm-style `twrite_core::KeyCode` (`Char`, `Enter`/`Tab`/`Escape`/
    `Backspace`/`Delete`/`Insert`, arrows, `Home`/`End`/`PageUp`/`PageDown`,
    `F(u8)`, `Unidentified`). Space is `Char(' ')`, not a named variant.
  - `KeyEvent { key: String }` becomes `{ code: KeyCode, … }`;
    `translate_key_down` is now the single GPUI-string boundary (verified
    against gpui 0.2.2's Linux keysym table).
  - `ContextMenuItem::with_hint` takes `KeyHint`; hints render as kbd chips
    (one per modifier + key) instead of free text.
  - Synthetic prompt-bar actions (`focus_search`, …) leave the key channel
    for a dedicated `SearchAction` channel (`EditorHook::on_search_action`,
    `Editor::press_search_action`).
  - `EditorHook::before_insert(&str)` becomes `before_insert(char)`;
    `Modifiers` gains `ctrl`/`alt`/`shift`/`meta` constructors.

## [0.8.0] - 2026-09-13

### Changed
- **Migrated from git-pinned GPUI to official `gpui 0.2.2` from crates.io**:
  - Workspace `gpui`/`gpui_platform` git pins (`zed-industries/zed@d85eade`)
    replaced by registry `gpui = "0.2.2"`; the `gpui_platform` crate is gone
    (official `gpui` folds the platform backends in as cargo features).
  - App entry becomes `Application::new().run(|cx: &mut App| …)` instead of
    `gpui_platform::application().run(…)` (all 7 examples + docs updated).
  - `FocusHandle::focus(window, cx)` becomes `focus(window)`;
    `ShapedLine::paint` drops its explicit `TextAlign`/`bounds` args
    (0.2.2 bakes in `TextAlign::Left` — same pixels for gutter numbers).
  - `Font::default()` and `KeyDownEvent::prefer_character_input` do not exist
    in the 0.2.2 API: font probing goes through `SharedString::new`, and
    Alt-modified keys always keep physical key names (AltGr/macOS Option
    accents fall back; revisit when upgrading past 0.2.2).
  - Downstream migration: delete any `gpui_platform` dependency and `gpui`
    `rev` pins; `gpui = "0.2"` + `twrite = "0.8"` share a single `gpui`
    via semver unification.

### Added
- **Platform-backend passthrough features (`twrite`)**: `wayland`, `x11`,
  `font-kit` map to `gpui/...`. Wayland-only example:
  `twrite = { version = "0.8", default-features = false, features = ["wayland"] }`
  alongside matching `gpui` features.

## [0.7.0] - 2026-09-11

### Added
- **Expandable right-click context menu (`twrite-core`, `twrite-gpui`)**:
  - Headless `context_menu` module in `twrite-core`: `ContextMenuItem` (id, label, hint, enabled, divider), `ContextMenuCaps` (host-supplied clipboard/undo/selection capabilities), `ContextMenuContext` (click row/col snapshot for hooks), and `ContextMenuState` (open/items store mirroring `PromptState`).
  - Built-in edit rows with enablement matrix: Undo, Redo, Cut, Copy, Paste, Delete, Select All.
  - New `EditorHook` methods: `context_menu_items` (hook rows append after built-ins; reusing a well-known id overrides that default in place) and `on_context_menu_action` (runs before built-in dispatch; `Consumed` halts).
  - GPUI right-click handling with VS Code-style selection policy (click inside selection keeps it, else cursor moves), custom themed overlay (`EditorTheme::menu_*`) with viewport clamping, keyboard navigation (`Up`/`Down` + `Enter`, `Escape` dismisses), and `EditorConfig::{context_menu, show_default_menu_items}` flags.
  - New `EditorBuffer::{can_undo, can_redo}` helpers.
  - New `context_menu` example (hook-contributed UPPERCASE/separator actions) and `recipe-context-menu.md` docs page; `hooks` example gains demo menu rows.

## [0.6.0] - 2026-09-11

### Added
- **Blinking cursor support (`twrite-gpui`)**:
  - Configurable periodic cursor blinking via `EditorConfig::cursor_blink` (defaults to `true`).
  - Cursor blink resets to fully visible immediately on keyboard typing, mouse navigation/selection, and scrolling.
  - New `Editor::reset_blink_cursor` and `Editor::set_cursor_blink` API methods for external control and configuration.
- **Multi-click selection & drag-expansion snapping (`twrite-core`, `twrite-gpui`)**:
  - Double-click highlights the clicked word under the cursor; triple-click highlights the entire line (including trailing line terminator).
  - Multi-click drag selection extends with boundary snapping: dragging after double-click expands word-by-word; dragging after triple-click expands line-by-line.
  - New boundary helpers in `twrite-core`: `find_word_range_at`, `find_line_range_at`, and convenience methods `EditorBuffer::{word_range_at, line_range_at}`.
  - New `twrite_gpui::SelectionGranularity` enum (`Character`, `Word`, `Line`) tracking active drag granularity on `Editor`.
  - Hyperlink opening and task checkbox toggling restricted to single clicks (`click_count == 1`) so double-clicking links selects text without re-opening browsers.
- **Find & replace toolbar UI redesign (`twrite-gpui`)**:
  - Replaced abbreviated chips (`Aa`, `W`, `All`) with full-word checkboxes (`Highlight All`, `Match Case`, `Whole Words`) with keyboard mnemonic underlines.
  - Redesigned search bar with rounded framed input box, `Find in page` placeholder, caret steppers (`^`, `v`), and close button (`✕`).
  - Added expandable replace row with `Replace with` input box and interactive `Replace` and `Replace All` action buttons.
- **Modularized editor and markdown syntax engines (`twrite-core`, `twrite-gpui`)**:
  - Modularized `crates/twrite-gpui/src/editor.rs` into focused submodules: `mod`, `mouse`, `keyboard`, `clipboard`, `geometry`, and `render`.
  - Extracted inline markdown syntax parser into `crates/twrite-core/src/batteries/markdown/inline.rs`.

### Fixed
- **Find & replace UX fixes (`twrite-core`, `twrite-gpui`)**:
  - Fixed keyboard submit on replace: pressing `Enter` while focused in the replace input replaces the current match and advances to the next match.
  - Fixed mouse click closing replace box: clicking into the replace box focuses it cleanly without toggling/closing, and toolbar background clicks no longer propagate to the editor canvas.
  - Normal bar cursor in prompt inputs: replaced inverted block cursor with a standard vertical bar cursor in text inputs (`find`, `replace`, and prompt line).
  - Extended text navigation & editing shortcuts in prompts: added `Ctrl+Left` / `Ctrl+Right` (word movement), `Ctrl+Backspace` / `Ctrl+Delete` (word deletion), `Ctrl+K` (clear to end), and `Alt`/`Meta` equivalents.
- **Find & replace functionality (`twrite-core`, `twrite-gpui`)**:
  - Fixed replacement text not updating while editing in the replace prompt.
  - Fixed `replace_current` and `replace_all` failing to read live input from the active prompt.
  - Added wrap-around match replacement in `replace_current` when the cursor is past the last match.
  - Added `Tab` shortcut and click-to-focus navigation between find and replace fields.
  - Exposed `replace_mode`, `is_replace_prompt`, `query`, and `replacement` on `SearchSnapshot`.

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
