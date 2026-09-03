//! Baseline proof: thousands-of-lines highlight pipeline timing (headless, std-only).
//!
//! Heavy cases are `#[ignore]` so default `cargo test` stays fast:
//! `cargo test --release -p twrite-core --test highlight_perf -- --ignored --nocapture --features markdown`

use std::hint::black_box;
use std::time::{Duration, Instant};

use twrite_core::{ConcealedLine, EditorBuffer, split_line_intervals};

fn xorshift(seed: u64) -> impl FnMut() -> u64 {
    let mut x = if seed == 0 {
        0x1234_5678_9abc_def1
    } else {
        seed
    };
    move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    }
}

/// Deterministic mixed-syntax fixture exercising headings, fences, tasks,
/// quotes, links, long lines, and unicode/char-boundary paths.
fn make_fixture(lines: usize, seed: u64) -> String {
    let mut rng = xorshift(seed);
    let mut s = String::with_capacity(lines * 64);
    for i in 0..lines {
        match rng() % 8 {
            0 => {
                s.push_str(&format!("# Heading {i}\n"));
            }
            1 => {
                s.push_str("```rust\nfn f() { let x = 42; }\n```\n");
            }
            2 => {
                s.push_str(&format!("- [ ] task {i} with `code` and **bold**\n"));
            }
            3 => {
                s.push_str(&format!(
                    "> quote {i} with [link](https://example.com/{i})\n"
                ));
            }
            4 => {
                s.push_str(&format!(
                    "\"dialogue line {i} with unicode h\u{e9}llo \u{1f30d}\"\n"
                ));
            }
            5 => {
                s.push_str(&format!("long line {i} {}\n", "lorem ".repeat(20)));
            }
            _ => {
                s.push_str(&format!("plain line {i} with some words to move over\n"));
            }
        }
    }
    s
}

#[test]
fn fixture_shape_is_deterministic() {
    let a = make_fixture(200, 0x1234);
    let b = make_fixture(200, 0x1234);
    assert_eq!(a, b);
    let buf = EditorBuffer::new(&a);
    assert!(buf.len_lines() >= 200);
    assert!(a.contains("# Heading"));
    assert!(a.contains("- [ ] task"));
    assert!(a.contains("https://example.com/"));
}

#[cfg(feature = "markdown")]
fn run_full_pipeline(buf: &EditorBuffer, rows: std::ops::Range<usize>) -> Duration {
    use twrite_core::{MarkdownHighlighter, SyntaxHighlighter};
    let highlighter = MarkdownHighlighter::new();
    let start = Instant::now();
    for row in rows {
        let line_text = buf.line_to_string(row);
        let line = line_text.trim_end_matches(['\r', '\n']);
        let spans = highlighter.highlight_line(buf, row, black_box(line));
        let concealed = ConcealedLine::build(line, &spans);
        let _segments = split_line_intervals(concealed.display_text.len(), &concealed.spans, None);
        let _links = highlighter.extract_links(buf, row, line);
        black_box((_segments, _links, concealed.display_text.len()));
    }
    start.elapsed()
}

#[cfg(not(feature = "markdown"))]
fn run_full_pipeline(buf: &EditorBuffer, rows: std::ops::Range<usize>) -> Duration {
    let start = Instant::now();
    for row in rows {
        let line_text = buf.line_to_string(row);
        let line = line_text.trim_end_matches(['\r', '\n']);
        let concealed = ConcealedLine::build(line, &[]);
        let _segments = split_line_intervals(concealed.display_text.len(), &concealed.spans, None);
        black_box((_segments, concealed.display_text.len()));
    }
    start.elapsed()
}

fn report_and_guard(label: &str, lines: usize, elapsed: Duration) {
    let ms = elapsed.as_secs_f64() * 1000.0;
    let per_line_us = ms * 1000.0 / lines as f64;
    eprintln!(
        "[baseline] {label}: {lines} lines in {ms:.1}ms ({per_line_us:.2}us/line, {:.0} lines/sec)",
        lines as f64 / elapsed.as_secs_f64().max(1e-9),
    );
    // Generous guard: documents catastrophic regression without flaking CI.
    assert!(
        elapsed < Duration::from_secs(60),
        "{label} took {elapsed:?}, expected < 60s"
    );
}

#[test]
#[ignore]
fn perf_highlight_2k_lines() {
    let text = make_fixture(2_000, 0x1234);
    let buf = EditorBuffer::new(&text);
    let rows = 0..buf.len_lines();
    let n = buf.len_lines();
    let _warm = run_full_pipeline(&buf, 0..n.min(10));
    let elapsed = run_full_pipeline(&buf, rows);
    report_and_guard("highlight_2k", n, elapsed);
}

#[test]
#[ignore]
fn perf_highlight_5k_lines() {
    let text = make_fixture(5_000, 0x1234);
    let buf = EditorBuffer::new(&text);
    let rows = 0..buf.len_lines();
    let n = buf.len_lines();
    let _warm = run_full_pipeline(&buf, 0..n.min(10));
    let elapsed = run_full_pipeline(&buf, rows);
    report_and_guard("highlight_5k", n, elapsed);
}

#[test]
#[ignore]
fn perf_highlight_10k_lines() {
    let text = make_fixture(10_000, 0x1234);
    let buf = EditorBuffer::new(&text);
    let rows = 0..buf.len_lines();
    let n = buf.len_lines();
    let _warm = run_full_pipeline(&buf, 0..n.min(10));
    let elapsed = run_full_pipeline(&buf, rows);
    report_and_guard("highlight_10k", n, elapsed);
}
