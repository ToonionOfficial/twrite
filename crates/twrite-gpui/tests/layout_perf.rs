//! Baseline proof: thousands-of-lines layout pipeline timing (headless, std-only).
//!
//! Covers the GPUI-independent half of the frame: tag-driven `LineMetrics`,
//! `build_line_text_runs`, and theme resolution. Shaping (`shape_text`, needs
//! `Window`) stays in the manual `examples/perf_scroll` harness.
//!
//! Heavy cases are `#[ignore]`: `cargo test --release -p twrite-gpui --test layout_perf -- --ignored --nocapture`

use std::hint::black_box;
use std::time::{Duration, Instant};

use gpui::{Font, px};
use twrite_core::{EditorBuffer, HighlightTag, StyleSpan};
use twrite_gpui::{EditorTheme, LayoutCache, LineMetrics, build_line_text_runs};

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

/// Deterministic tag spans mirroring what a highlighter would emit, so the
/// layout half can be measured without a `Window` or markdown dependency.
fn fake_spans(line: &str) -> Vec<StyleSpan> {
    let t = line.trim_start();
    if t.starts_with("# ") {
        return vec![StyleSpan::tag(0..line.len(), HighlightTag::Heading1)];
    }
    if t.starts_with("```") {
        return vec![StyleSpan::tag(0..line.len(), HighlightTag::Code)];
    }
    if t.starts_with("- [ ] ") {
        return vec![StyleSpan::tag(
            0..6.min(line.len()),
            HighlightTag::TaskUnchecked,
        )];
    }
    if t.starts_with("> ") {
        return vec![StyleSpan::tag(
            0..2.min(line.len()),
            HighlightTag::Blockquote,
        )];
    }
    if t.trim_end() == "---" {
        return vec![StyleSpan::tag(0..line.len(), HighlightTag::HorizontalRule)];
    }
    if line.contains("**")
        && let Some(s) = line.find("**")
    {
        let e = (s + 8).min(line.len());
        return vec![StyleSpan::tag(s..e, HighlightTag::Bold)];
    }
    Vec::new()
}

fn run_layout_pass(buf: &EditorBuffer, rows: std::ops::Range<usize>) -> Duration {
    let theme = EditorTheme::default();
    let font = Font::default();
    let start = Instant::now();
    for row in rows {
        let raw = buf.line_to_string(row);
        let line = raw.trim_end_matches(['\r', '\n']);
        let spans = fake_spans(black_box(line));
        let metrics = LineMetrics::for_line(line, line, black_box(&spans), px(16.0), px(22.0));
        let checked = metrics.task_state == Some(true);
        let runs = build_line_text_runs(
            line,
            &spans,
            None,
            &font,
            &theme,
            metrics.is_code_block,
            checked,
        );
        black_box((metrics.line_height, runs.len()));
    }
    start.elapsed()
}

fn report_and_guard(label: &str, lines: usize, elapsed: Duration) {
    let ms = elapsed.as_secs_f64() * 1000.0;
    eprintln!(
        "[baseline] {label}: {lines} lines in {ms:.1}ms ({:.2}us/line, {:.0} lines/sec)",
        ms * 1000.0 / lines as f64,
        lines as f64 / elapsed.as_secs_f64().max(1e-9),
    );
    assert!(
        elapsed < Duration::from_secs(60),
        "{label} took {elapsed:?}, expected < 60s"
    );
}

#[test]
fn layout_fixture_is_deterministic() {
    let a = make_fixture(200, 0x1234);
    let b = make_fixture(200, 0x1234);
    assert_eq!(a, b);
    let buf = EditorBuffer::new(&a);
    assert!(buf.len_lines() >= 200);
    // Spot-check tag-driven metrics without markdown.
    let spans = fake_spans("- [ ] task 1");
    let m = LineMetrics::for_line("- [ ] task 1", "- [ ] task 1", &spans, px(16.0), px(22.0));
    assert_eq!(m.task_state, Some(false));
}

#[test]
#[ignore]
fn perf_layout_2k_lines() {
    let text = make_fixture(2_000, 0x1234);
    let buf = EditorBuffer::new(&text);
    let n = buf.len_lines();
    let _warm = run_layout_pass(&buf, 0..n.min(10));
    let elapsed = run_layout_pass(&buf, 0..n);
    report_and_guard("layout_2k", n, elapsed);
}

#[test]
#[ignore]
fn perf_layout_5k_lines() {
    let text = make_fixture(5_000, 0x1234);
    let buf = EditorBuffer::new(&text);
    let n = buf.len_lines();
    let _warm = run_layout_pass(&buf, 0..n.min(10));
    let elapsed = run_layout_pass(&buf, 0..n);
    report_and_guard("layout_5k", n, elapsed);
}

#[test]
#[ignore]
fn perf_layout_10k_lines() {
    let text = make_fixture(10_000, 0x1234);
    let buf = EditorBuffer::new(&text);
    let n = buf.len_lines();
    let _warm = run_layout_pass(&buf, 0..n.min(10));
    let elapsed = run_layout_pass(&buf, 0..n);
    report_and_guard("layout_10k", n, elapsed);
}

fn run_cached_pass(buf: &EditorBuffer, cache: &mut LayoutCache) -> Duration {
    let start = Instant::now();
    let n = buf.len_lines();
    for row in 0..n {
        let raw = buf.line_to_string(row);
        let line = raw.trim_end_matches(['\r', '\n']);
        let cached = cache.cached_input(buf, None, 0, usize::MAX, row, line);
        black_box((
            cached.spans.len(),
            cached.concealed.display_text.len(),
            cached.link_src.len(),
        ));
    }
    start.elapsed()
}

#[test]
fn layout_cache_second_pass_is_all_hits() {
    let text = make_fixture(500, 0x1234);
    let buf = EditorBuffer::new(&text);
    let mut cache = LayoutCache::new();
    let n = buf.len_lines();
    for row in 0..n {
        let raw = buf.line_to_string(row);
        let line = raw.trim_end_matches(['\r', '\n']);
        cache.cached_input(&buf, None, 0, usize::MAX, row, line);
    }
    assert_eq!(cache.stats().1, n as u64, "first pass must miss every row");
    for row in 0..n {
        let raw = buf.line_to_string(row);
        let line = raw.trim_end_matches(['\r', '\n']);
        cache.cached_input(&buf, None, 0, usize::MAX, row, line);
    }
    assert_eq!(
        cache.stats().0,
        n as u64,
        "second identical pass must hit every row"
    );
}

#[test]
#[ignore]
fn perf_layout_cached_10k_lines() {
    let text = make_fixture(10_000, 0x1234);
    let buf = EditorBuffer::new(&text);
    let n = buf.len_lines();
    let mut cache = LayoutCache::new();
    // Full populate: every row misses once (also exercises cap eviction).
    let first = run_cached_pass(&buf, &mut cache);
    let misses_pop = cache.stats().1;
    assert_eq!(misses_pop, n as u64, "populate must compute each row once");
    // Steady state: repaint a 60-row viewport 5x, as prepaint does per frame.
    let win_end = 5060.min(n);
    let win = 5000..win_end;
    let win_len = (win_end - 5000) as u64;
    let start = Instant::now();
    for _ in 0..5 {
        for row in win.clone() {
            let raw = buf.line_to_string(row);
            let line = raw.trim_end_matches(['\r', '\n']);
            cache.cached_input(&buf, None, 0, usize::MAX, row, line);
        }
    }
    let repaint = start.elapsed() / 5;
    let (hits, misses) = cache.stats();
    assert_eq!(
        misses - misses_pop,
        win_len,
        "viewport window populates once, then hits"
    );
    assert_eq!(hits, win_len * 4, "repeat viewport repaints must all hit");
    eprintln!(
        "[cached] layout_cached_10k: populate {n} rows in {:.1}ms, viewport repaint ({} rows) in {:.1}ms ({:.2}us/line), hits={hits}, misses={misses}",
        first.as_secs_f64() * 1000.0,
        win_len,
        repaint.as_secs_f64() * 1000.0,
        repaint.as_secs_f64() * 1e6 / win_len as f64,
    );
}
