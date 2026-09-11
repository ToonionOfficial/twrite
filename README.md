# TWrite

[![Docs](https://img.shields.io/badge/docs-notes.toonion.net%2Fdocs-blue)](https://notes.toonion.net/docs)

TWrite is a fast, modular text editor crate for Rust. Leveraging the GPU-accelerated GPUI framework developed by [Zed](https://github.com/zed-industries/zed), it provides a responsive, extensible base for building modern text, Markdown, and custom-language editors

Full guides plus the API reference live at
[notes.toonion.net/docs](https://notes.toonion.net/docs).

## Install

TWrite is not on crates.io yet; depend on git:

```toml
twrite = { git = "https://github.com/ToonionOfficial/twrite", version = "0.5" }
```

Linux also needs GPUI system libraries; see the
[full setup](https://notes.toonion.net/docs/getting-started.html).

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
```

The pattern everywhere is the same: `Editor` owns the buffer, hooks
(`EditorHook` + `HookContext`) own behavior, and batteries bundle the two.
Start from `examples/simple.rs` and follow the `Next:` pointers.

## Links

- [Documentation](https://notes.toonion.net/docs) (guides plus API reference)
- [Changelog](CHANGELOG.md)
- [License](LICENSE)
