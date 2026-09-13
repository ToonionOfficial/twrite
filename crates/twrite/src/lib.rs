//! TWrite is a fast, modular text editor engine for GPUI.
//!
//! It provides rope-backed text editing, multi-span syntax highlighting,
//! a versatile hook system for modal editing and smart pairs, and GPU-accelerated rendering.
//!
//! # Layout
//!
//! Most users need only this facade crate: [`Editor`] plus the hook traits
//! and batteries. It re-exports two layers:
//!
//! - `twrite-core`: headless buffer, movement, syntax,
//!   hooks, and structured keys (`KeyCode`, `KeyHint`). Usable without
//!   GPUI (servers, CLIs, tests). See <https://docs.rs/twrite-core>.
//! - `twrite-gpui`: canvas rendering, theming, configuration, and the
//!   interactive [`Editor`] view.
//!
//! # Quick start
//!
//! ```toml
//! [dependencies]
//! gpui = "0.2"
//! twrite = "0.9"
//! ```
//!
//! ```rust,no_run
//! use gpui::*;
//! use twrite::Editor;
//!
//! Application::new().run(|cx: &mut App| {
//!     cx.open_window(WindowOptions::default(), |_, cx| {
//!         cx.new(|cx| Editor::new("Hello from TWrite!", cx))
//!     })
//!     .unwrap();
//!     cx.activate(true);
//! });
//! ```
//!
//! # Features
//!
//! - `markdown`: CommonMark/GFM highlighter (`markdown` module) and task widgets.
//! - `wayland`, `x11`, `font-kit`: platform-backend passthrough to `gpui`
//!   (its defaults already include all three).
//!
//! [`Editor`]: https://docs.rs/twrite-gpui/latest/twrite_gpui/struct.Editor.html

/// Core buffer, syntax, movement, and hook primitives.
pub use twrite_core::*;

/// GPUI canvas rendering, theming, configuration, and editor view.
pub use twrite_gpui::*;

/// CommonMark and GitHub Flavored Markdown highlighter and interactive hook.
#[cfg(feature = "markdown")]
pub mod markdown {
    pub use twrite_core::markdown::*;
}
