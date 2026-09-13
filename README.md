# TWrite

[![Docs.rs](https://docs.rs/twrite/badge.svg)](https://docs.rs/twrite)

TWrite is a fast, modular text editor crate for Rust. Leveraging the GPU-accelerated GPUI framework developed by [Zed](https://github.com/zed-industries/zed), it provides a responsive, extensible base for building modern text, Markdown, and custom-language editors

Full guides plus the API reference live at
[https://toonionofficial.github.io/twrite/docs](https://toonionofficial.github.io/twrite/docs).

## Install

```toml
[dependencies]
gpui = "0.2"
twrite = "0.9"

# Optional batteries and backends:
# twrite = { version = "0.9", features = ["markdown"] }
# gpui = { version = "0.2", default-features = false, features = ["wayland"] }
# twrite = { version = "0.9", default-features = false, features = ["wayland"] }
```

Linux also needs GPUI system libraries; see the
[full setup](https://toonionofficial.github.io/twrite/docs/getting-started.html).

## Usage

```rust
use twrite::{Editor, SearchHook};

let editor = cx.new(|cx| {
    let mut ed = Editor::new("Hello, TWrite!", cx);
    ed.config.line_numbers = true;
    ed.add_hook(SearchHook::new()); // Ctrl+F find, Ctrl+H replace
    ed
});
```

`cx` is your GPUI window context; `examples/simple.rs` shows the complete
shell around this fragment.

## Getting started

Run the examples in order; each builds on the previous one:

```sh
cargo run --example simple    # 1. bare editor window + find (start here)
cargo run --example hooks     # 2. writing your own EditorHook
cargo run --example syntax    # 3. custom syntax highlighting
cargo run --example prompt    # 4. input boxes and palettes on ctx.prompt
cargo run --example markdown --features markdown  # 5. the Markdown battery
cargo run --example vim       # 6. a full modal Vim system on hooks alone
cargo run --example context_menu  # supplemental: right-click menu + hook rows (after hooks)
```

The pattern everywhere is the same: `Editor` owns the buffer, hooks
(`EditorHook` + `HookContext`) own behavior, and batteries bundle the two.
Start from `examples/simple.rs` and follow the `Next:` pointers.

## Links

- [Documentation](https://toonionofficial.github.io/twrite/docs) (guides plus API reference)
- [Changelog](CHANGELOG.md)
- [License](LICENSE)
