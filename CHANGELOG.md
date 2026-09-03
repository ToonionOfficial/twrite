# Changelog

All notable changes to the `twrite` editor engine will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
