# TWrite Docs

TWrite is a fast, modular text editor crate for Rust. Built on the
GPU-accelerated [GPUI](https://github.com/zed-industries/zed) framework from
Zed, it gives you a responsive, extensible base for text, Markdown, and
custom-language editors.

One pattern runs through everything here:

- **`Editor`** owns the buffer and the window.
- **Hooks** (`EditorHook` + `HookContext`) own behavior.
- **Batteries** bundle the two into drop-in features.

Start with [Getting Started](getting-started.md), then follow the guides in
order. For exact types and signatures, see the [API Reference](api.md).
