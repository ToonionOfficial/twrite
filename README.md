<h1 align="center">TWrite</h1>

<p align="center">
  <a href="https://crates.io/crates/twrite"><img src="https://img.shields.io/crates/v/twrite.svg" alt="Release"></a>
  <a href="https://docs.rs/twrite"><img src="https://docs.rs/twrite/badge.svg" alt="Docs"></a>
  <a href="https://github.com/ToonionOfficial/twrite/actions/workflows/ci.yml"><img src="https://github.com/ToonionOfficial/twrite/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/ToonionOfficial/twrite/blob/main/LICENSE"><img src="https://img.shields.io/crates/l/twrite.svg" alt="Licence"></a>
</p>

TWrite is a fast, modular text editor crate for Rust. Leveraging the GPU-accelerated GPUI framework developed by [Zed](https://github.com/zed-industries/zed), it provides a responsive, extensible base for building modern text, Markdown, and custom-language editors

Full guides plus the API reference live at
[https://toonionofficial.github.io/twrite/docs](https://toonionofficial.github.io/twrite/docs).

## Install

```sh
cargo add gpui@0.2 twrite
```

For Markdown support:

```sh
cargo add twrite --features markdown
```

For a specific GPU backend on Linux (Wayland or X11):

```sh
cargo add gpui@0.2 --no-default-features --features wayland
cargo add twrite --no-default-features --features wayland
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
