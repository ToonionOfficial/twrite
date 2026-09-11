# Getting Started

## Prerequisites

- Rust stable toolchain (`rustup` is fine).
- Linux system libraries for GPUI windowing and fonts:

```sh
sudo apt-get install -y pkg-config libfontconfig1-dev libwayland-dev \
  libx11-xcb-dev libxkbcommon-x11-dev libxkbcommon-dev
```

## Run the examples in order

Each example builds on the previous one:

```sh
cargo run --example simple    # 1. bare editor window + find
cargo run --example hooks     # 2. writing your own EditorHook
cargo run --example syntax    # 3. custom syntax highlighting
cargo run --example prompt    # 4. input boxes and palettes on ctx.prompt
cargo run --example markdown --features markdown  # 5. the Markdown battery
cargo run --example vim       # 6. a full modal Vim system on hooks alone
```

Every example file opens with a header comment stating its number,
prerequisites, and what to read next; when in doubt, follow those pointers.

## Feature flags

`markdown` pulls in the Markdown battery (`pulldown-cmark`, no default
features). Everything else (buffer, hooks, prompt, search) is on by
default with zero optional dependencies.
