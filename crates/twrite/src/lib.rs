//! TWrite is a fast, modular text editor engine for GPUI.
//!
//! It provides rope-backed text editing, multi-span syntax highlighting,
//! a versatile hook system for modal editing and smart pairs, and GPU-accelerated rendering.

/// Core buffer, syntax, movement, and hook primitives.
pub use twrite_core::*;

/// GPUI canvas rendering, theming, configuration, and editor view.
pub use twrite_gpui::*;
