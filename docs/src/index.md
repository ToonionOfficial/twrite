# TWrite Documentation

TWrite is a fast, modular text editor engine for Rust, built on the GPU-accelerated [GPUI](https://github.com/zed-industries/zed) framework. It provides an extensible foundation for building everything from simple single-line inputs to full-featured Markdown note-taking tools and modal code editors.

## Highlights

- **GPU Acceleration**: Fluid 60+ FPS text rendering and smooth scrolling powered by GPUI.
- **Rope Buffer Engine**: Backed by `ropey` for fast inserts, deletes, and multi-megabyte document handling with transaction-based undo/redo.
- **Extensible Hook Architecture**: Custom keybindings, auto-pairs, status indicators, and modal editing via pure-Rust hooks with zero GPUI boilerplate.
- **Batteries-Included Markdown**: Live syntax highlighting, concealed formatting (`Dimmed`, `Hidden`, `Off`), interactive task checkboxes, tables, and hyperlinks.
- **Interactive Find & Replace**: Built-in search engine with regex support, whole-word and case toggles, caret steppers, and single-undo replace all.
- **Command Palettes & Prompts**: Headless prompt engine for floating `F1` palettes, `:goto-line` bottom bars, and fuzzy-filtered item lists.
- **Multi-Click Selection**: Word double-click, line triple-click, and boundary-snapping drag selection.

## Where to Start

1. **[Getting Started](getting-started.md)**: Add `twrite` to your `Cargo.toml` and launch your first editor window in under 35 lines of code.
2. **Recipes**:
   - [Simple Text Editor](recipe-simple.md): Basic editor setup, configuration, and buffer operations.
   - [Markdown Note Editor](recipe-markdown.md): Enable Markdown formatting, task lists, and conceal modes.
   - [Find & Replace](recipe-search.md): Wire up the built-in search toolbar.
   - [Custom Hooks & Shortcuts](recipe-hooks.md): Intercept keys, track edits, and add status bars.
   - [Command Palettes & Prompts](recipe-prompt.md): Build floating palettes and command bars.
   - [Modal & Vim Editing](recipe-vim.md): Build modal editors with hooks alone.
3. **Reference**:
   - [Configuration Reference](configuration.md): Full options table for `EditorConfig`.
   - [API Reference](api.md): Links to rustdoc API reference.
   - [Changelog](changelog.md): Release history and notes.
