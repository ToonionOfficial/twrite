# Configuration Reference

The `EditorConfig` struct in `twrite-gpui` controls all visual and typographic settings for an `Editor` instance.

## Overview

You can adjust settings either on initialization or dynamically at runtime:

```rust
use gpui::*;
use twrite::Editor;
use twrite::markdown::ConcealMode;

let editor = cx.new(|cx| {
    let mut ed = Editor::new("Hello world\n", cx);

    // Typography
    ed.config.font_size = px(15.0);
    ed.config.line_height = px(24.0);
    ed.config.font_family = Some("JetBrains Mono".into());

    // Appearance
    ed.config.line_numbers = true;
    ed.config.cursor_blink = true;

    // Markdown (when feature enabled)
    ed.config.markdown.conceal_mode = ConcealMode::Dimmed;

    ed
});
```

## Options Table

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `line_numbers` | `bool` | `false` | When `true`, displays a left gutter with line numbers matching the document rope row. |
| `cursor_blink` | `bool` | `true` | When `true`, the text cursor blinks periodically. Blinking pauses and remains fully visible while typing or navigating. |
| `font_size` | `Pixels` | `px(14.0)` | Base font size used for buffer text rendering. |
| `line_height` | `Pixels` | `px(22.0)` | Vertical height of each line in pixels. |
| `font_family` | `Option<SharedString>` | `None` | Monospace font family name. When `None`, TWrite auto-detects the first monospace font with bold and italic faces on the system. |
| `markdown.conceal_mode` | `ConcealMode` | `Dimmed` | Controls Markdown token visibility (`Dimmed`, `Hidden`, or `Off`). |

## Font Auto-Selection

When `config.font_family` is set to `None`, TWrite automatically queries the platform font kit and selects the first available monospace font family that provides both full bold and italic faces (for example, Fira Code, JetBrains Mono, Menlo, Consolas, or Liberation Mono).

You can inspect the selected font and its face availability directly:

```rust
let ed = editor.read(cx);
println!("Selected family: {:?}", ed.selected_font_family);
println!("Face availability: {:?}", ed.face_availability);
```

## Cursor Blinking Control

In addition to `config.cursor_blink = false`, you can control or reset the cursor blink state programmatically:

```rust
// Temporarily reset blink timer so the cursor is immediately solid:
ed.reset_blink_cursor(cx);

// Dynamically toggle cursor blinking on or off:
ed.set_cursor_blink(true, cx);
```
