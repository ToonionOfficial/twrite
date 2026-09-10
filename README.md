# TWrite

TWrite is a fast, modular text editor crate for Rust. Leveraging the GPU-accelerated GPUI framework developed by [Zed](https://github.com/zed-industries/zed), it provides a responsive, extensible base for building modern text, Markdown, and custom-language editors

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
