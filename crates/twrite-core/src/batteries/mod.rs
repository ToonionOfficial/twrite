//! Battery registry: optional, feature-gated editor batteries.
//!
//! Each battery is a self-contained language or behavior pack built **only** on
//! the public core API (`SyntaxHighlighter`, `EditorHook`, `HighlightTag`,
//! `ConcealedLine`, …) — the same surface external users get. Batteries emit
//! only existing `HighlightTag` variants (never add new ones per battery) and
//! never require `twrite-gpui`-side code; they wire up via `set_highlighter` /
//! `add_hook`, as demonstrated by `examples/vim.rs`.
//!
//! ## Adding a battery `<name>`
//!
//! 1. Create `batteries/<name>.rs` (promote to `batteries/<name>/mod.rs` when
//!    it outgrows one file) with a fixed template: `Config` (plain `Clone`
//!    data + `Default`), `Highlighter` (`new` + `with_config`), `Hook`
//!    (`new` + `with_config`). Hook-only batteries omit the highlighter.
//! 2. Declare the feature in `twrite-core/Cargo.toml` (`<name> = [...]`, with
//!    `dep:<parser-crate>` only if the battery needs a parser dependency).
//! 3. Register the one-line path shim in `super::lib` (see `markdown` there):
//!    the public path stays `twrite_core::<name>` regardless of file layout.
//! 4. Re-export from the `twrite` facade as `twrite::<name>` behind the same
//!    feature name, so users write `twrite = { features = ["<name>"] }`.
//! 5. Add colocated unit tests in the battery module and an `examples/<name>.rs`
//!    demo (with `required-features` only if the demo needs the battery).
//!
//! <!-- New batteries are registered here as they land. -->
