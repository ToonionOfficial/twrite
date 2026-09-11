mod clipboard;
mod geometry;
mod keyboard;
mod mouse;
mod render;

use std::ops::Range;
use std::sync::Arc;

use gpui::{Bounds, Context, FocusHandle, Font, Pixels, Point, SharedString, Task};
use twrite_core::{
    CursorStyle, EditorBuffer, EditorHook, HookEffect, PromptState, SearchSnapshot, Selection,
    SyntaxHighlighter,
};

use crate::{config::EditorConfig, fps::FrameStats, layout_cache::LayoutCache, theme::EditorTheme};

/// The main GPUI text editor view and controller.
pub struct Editor {
    /// The underlying text buffer managing document contents and undo/redo history.
    pub buffer: EditorBuffer,
    /// Visual color palette for canvas background, text, cursor, and syntax tokens.
    pub theme: EditorTheme,
    /// Display and layout configurations (font size, line height, line numbers, wrapping).
    pub config: EditorConfig,
    /// Extensible hooks chain intercepting keystrokes, edits, and selection changes.
    pub hooks: Vec<Box<dyn EditorHook>>,
    /// Active syntax highlighter computing semantic and direct style spans.
    pub highlighter: Option<Arc<dyn SyntaxHighlighter>>,
    /// Revision bumped on every highlighter swap so [`LayoutCache`] epochs bust
    /// even when the buffer version is unchanged (e.g. conceal-mode cycling).
    pub highlighter_rev: u64,
    /// Per-version cache of highlight/conceal/link inputs, shared by prepaint
    /// and hit-testing so each row is parsed once per epoch, not per frame.
    pub layout_cache: LayoutCache,
    /// Bold/italic face availability from the last prepaint probe (`None` before first paint).
    pub face_availability: Option<FaceAvailability>,
    /// Base family picked by candidate auto-select (`None` before first paint,
    /// or when no candidate has emphasis faces).
    pub selected_font_family: Option<SharedString>,
    /// Inputs the face probe last ran against: explicit families + host font.
    /// Re-probed on change only.
    pub face_probe_key: Option<(Option<SharedString>, Option<SharedString>, Font)>,
    /// Focus handle for keyboard input tracking within GPUI.
    pub focus_handle: FocusHandle,
    /// First visible row index in the viewport.
    pub scroll_row: usize,
    /// Active selection range, if any.
    pub selection: Option<Selection>,
    /// Active visual cursor style (Bar, Block, Underline, Hidden).
    pub cursor_style: CursorStyle,
    /// Whether the visual cursor is currently visible in its blink cycle.
    pub cursor_visible: bool,
    /// Background timer task driving cursor blinking.
    blink_task: Option<Task<()>>,
    /// Whether the user is currently mouse-drag selecting text.
    pub is_selecting: bool,
    /// Active selection granularity for drag selection.
    pub selection_granularity: SelectionGranularity,
    /// Anchor range for multi-click drag selection expansion.
    pub drag_initial_range: Option<Range<usize>>,
    /// Whether the mouse cursor is currently hovering over an interactive task checkbox.
    pub is_hovering_task: bool,
    /// Target URL if the mouse cursor is currently hovering over a hyperlink.
    pub hovered_link: Option<String>,
    /// Last rendered bounds in window pixel coordinates.
    pub last_bounds: Option<Bounds<Pixels>>,
    /// Last rendered cursor position in window pixel coordinates, computed during canvas prepaint.
    pub last_cursor_pixel: Option<Point<Pixels>>,
    /// Layout metrics and screen coordinates of currently visible lines, cached during prepaint.
    pub visible_lines: Vec<VisibleLineLayout>,
    /// Rolling frame-rate samples, recorded once per canvas prepaint.
    ///
    /// Powers the [`crate::fps_badge`] testing HUD: it updates whenever the
    /// editor repaints (typing, selection drags, scrolling) and freezes when
    /// idle, with no forced repaints of its own.
    pub frame_stats: FrameStats,
    /// Shared headless prompt / input-box state, passed to hooks via
    /// [`twrite_core::HookContext`] and rendered by [`crate::prompt_bar::PromptBar`] when open.
    pub prompt: PromptState,
    /// App-level requests queued by hooks; [`Self::flush_effects`] executes
    /// file effects inline, hosts drain the rest via [`Self::take_effects`].
    pub pending_effects: Vec<HookEffect>,
    /// File path for `:w`-style saves (`Save { path: None }`); set by
    /// [`Self::load_file`].
    pub file_path: Option<std::path::PathBuf>,
    /// Last synced search matches for the highlight-all wash.
    pub search_matches: Vec<Range<usize>>,
    /// Whether the highlight-all wash is enabled (from the search snapshot).
    pub search_highlight_all: bool,
}

/// Selection granularity when mouse-drag selecting text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionGranularity {
    /// Character-by-character selection.
    #[default]
    Character,
    /// Word-by-word selection (initiated by double-click).
    Word,
    /// Line-by-line selection (initiated by triple-click).
    Line,
}

/// A hyperlink visible on screen with its screen pixel bounds and target URL.
#[derive(Debug, Clone)]
pub struct VisibleLink {
    /// Screen pixel bounds of the clickable link label.
    pub bounds: Bounds<Pixels>,
    /// The destination URL.
    pub url: String,
}

/// Availability of bold/italic font faces for the editor's base font.
///
/// Computed during prepaint by comparing resolved `FontId`s: a missing face
/// silently falls back to the regular face, which would make emphasis
/// invisible. Host apps can surface this (e.g. in a status bar) and point
/// users at `EditorConfig::font_family`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FaceAvailability {
    /// Whether bold text resolves to a distinct face.
    pub bold: bool,
    /// Whether italic text resolves to a distinct face.
    pub italic: bool,
}

/// Layout metrics and screen coordinates for a visible line, cached during prepaint for instant hit testing.
#[derive(Debug, Clone)]
pub struct VisibleLineLayout {
    /// Zero-based buffer row index.
    pub row: usize,
    /// Top Y position in window coordinates.
    pub top: Pixels,
    /// Bottom Y position in window coordinates.
    pub bottom: Pixels,
    /// Starting byte offset in the buffer.
    pub line_start_byte: usize,
    /// Length of the line text in bytes.
    pub line_len_bytes: usize,
    /// Left X position where text starts.
    pub text_origin_x: Pixels,
    /// Vertical line height.
    pub line_height: Pixels,
    /// Whether this is a task list line with a rendered checkbox.
    pub is_task_checkbox: bool,
    /// Left X position of the checkbox box.
    pub checkbox_box_x: Pixels,
    /// Task state for this line, if the highlighter reports one.
    /// `Some(false)` = unchecked, `Some(true)` = checked.
    pub task_state: Option<bool>,
    /// Hyperlinks located on this line.
    pub links: Vec<VisibleLink>,
}

impl Editor {
    /// Creates a new editor entity with `initial_text`.
    pub fn new(initial_text: &str, cx: &mut Context<Self>) -> Self {
        let config = EditorConfig::default();
        let cursor_style = if config.block_cursor {
            CursorStyle::Block
        } else {
            CursorStyle::Bar
        };
        let mut ed = Self {
            buffer: EditorBuffer::new(initial_text),
            theme: EditorTheme::default(),
            config,
            hooks: Vec::new(),
            highlighter: None,
            highlighter_rev: 0,
            layout_cache: LayoutCache::new(),
            face_availability: None,
            selected_font_family: None,
            face_probe_key: None,
            focus_handle: cx.focus_handle(),
            scroll_row: 0,
            selection: None,
            cursor_style,
            cursor_visible: true,
            blink_task: None,
            is_selecting: false,
            selection_granularity: SelectionGranularity::Character,
            drag_initial_range: None,
            is_hovering_task: false,
            hovered_link: None,
            last_bounds: None,
            last_cursor_pixel: None,
            visible_lines: Vec::new(),
            frame_stats: FrameStats::new(),
            prompt: PromptState::new(),
            pending_effects: Vec::new(),
            file_path: None,
            search_matches: Vec::new(),
            search_highlight_all: false,
        };
        ed.reset_blink_cursor(cx);
        ed
    }

    /// Resets the cursor blink cycle to visible and schedules periodic toggling.
    pub fn reset_blink_cursor(&mut self, cx: &mut Context<Self>) {
        self.cursor_visible = true;
        drop(self.blink_task.take());
        if !self.config.cursor_blink {
            return;
        }

        self.blink_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(500))
                    .await;
                let res = this.update(cx, |editor, cx| {
                    if !editor.config.cursor_blink {
                        editor.cursor_visible = true;
                        false
                    } else {
                        editor.cursor_visible = !editor.cursor_visible;
                        cx.notify();
                        true
                    }
                });
                match res {
                    Ok(true) => {}
                    _ => break,
                }
            }
        }));
    }

    /// Sets whether cursor blinking is enabled and resets the blink cycle.
    pub fn set_cursor_blink(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.config.cursor_blink = enabled;
        self.reset_blink_cursor(cx);
        cx.notify();
    }

    /// Adds an editor hook to the execution chain.
    pub fn add_hook(&mut self, hook: impl EditorHook) {
        self.hooks.push(Box::new(hook));
    }

    /// Clears all registered editor hooks.
    pub fn clear_hooks(&mut self) {
        self.hooks.clear();
    }

    /// Returns the active status or mode text reported by registered hooks.
    pub fn status_text(&self) -> Option<&str> {
        self.hooks.iter().find_map(|h| h.status_text())
    }

    /// Returns true if the cursor is currently in block mode.
    pub fn is_block_cursor(&self) -> bool {
        self.cursor_style == CursorStyle::Block || self.config.block_cursor
    }

    /// Resolves the base font: explicit family, else auto-selected family, else host.
    pub fn resolved_base_font(&self, host: &Font) -> Font {
        self.config
            .base_font(host, self.selected_font_family.as_ref())
    }

    /// Resolves the `Code`-span font (follows auto-select unless overridden).
    pub fn resolved_code_font(&self, host: &Font) -> Font {
        self.config
            .code_font(host, self.selected_font_family.as_ref())
    }

    /// Sets the active syntax highlighter.
    pub fn set_highlighter(&mut self, highlighter: impl SyntaxHighlighter) {
        self.highlighter = Some(Arc::new(highlighter));
        self.highlighter_rev = self.highlighter_rev.wrapping_add(1);
        self.layout_cache.clear();
    }

    /// Clears the active syntax highlighter, reverting to plain text.
    pub fn clear_highlighter(&mut self) {
        self.highlighter = None;
        self.highlighter_rev = self.highlighter_rev.wrapping_add(1);
        self.layout_cache.clear();
    }

    /// Enables out-of-the-box CommonMark and GFM editing using `self.config.markdown`.
    ///
    /// Automatically configures [`twrite_core::MarkdownHighlighter`], [`twrite_core::AutoPairsHook`],
    /// and [`twrite_core::MarkdownHook`].
    #[cfg(feature = "markdown")]
    pub fn enable_markdown(&mut self) {
        self.enable_markdown_with_config(self.config.markdown);
    }

    /// Enables out-of-the-box CommonMark and GFM editing with custom Markdown configuration.
    #[cfg(feature = "markdown")]
    pub fn enable_markdown_with_config(&mut self, config: twrite_core::markdown::MarkdownConfig) {
        use twrite_core::{AutoPairsHook, MarkdownHighlighter, MarkdownHook};
        self.config.markdown = config;
        self.set_highlighter(MarkdownHighlighter::with_config(config));
        self.add_hook(AutoPairsHook::new());
        self.add_hook(MarkdownHook::with_config(config));
    }

    /// Loads document text from a file into the editor, resetting cursor and undo history.
    pub fn load_file<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
    ) -> Result<(), twrite_core::EditorError> {
        let new_buffer = EditorBuffer::from_file(path.as_ref())?;
        self.buffer = new_buffer;
        self.scroll_row = 0;
        self.selection = None;
        self.file_path = Some(path.as_ref().to_path_buf());
        self.layout_cache.clear();
        Ok(())
    }

    /// Executes queued file effects (`Save` / `Load`) inline, keeping
    /// app-level effects (`Quit` / `Message`) queued for the host to drain
    /// via [`Self::take_effects`]. Runs automatically after input events.
    pub fn flush_effects(&mut self) {
        let queued = std::mem::take(&mut self.pending_effects);
        let mut unhandled = Vec::new();
        for effect in queued {
            match effect {
                HookEffect::Save { path } => {
                    let target = path
                        .map(std::path::PathBuf::from)
                        .or_else(|| self.file_path.clone());
                    match target {
                        Some(p) => {
                            if let Err(e) = self.save_file(&p) {
                                unhandled.push(HookEffect::Message(format!("save failed: {e}")));
                            }
                        }
                        None => {
                            unhandled.push(HookEffect::Message("E32: No file name".to_string()));
                        }
                    }
                }
                HookEffect::Load { path } => match self.load_file(&path) {
                    Ok(()) => {}
                    Err(e) => unhandled.push(HookEffect::Message(format!("load failed: {e}"))),
                },
                other => unhandled.push(other),
            }
        }
        self.pending_effects = unhandled;
    }

    /// Takes app-level effects (`Quit` / `Message`) left by [`Self::flush_effects`].
    pub fn take_effects(&mut self) -> Vec<HookEffect> {
        std::mem::take(&mut self.pending_effects)
    }

    /// Returns the live search-panel snapshot from the first hook that
    /// provides one ([`twrite_core::SearchHook`]; composite hooks forward their own).
    pub fn search_snapshot(&self) -> Option<SearchSnapshot> {
        self.hooks.iter().find_map(|h| h.search_snapshot())
    }

    /// Refreshes [`Self::search_matches`] / [`Self::search_highlight_all`]
    /// from the hook snapshot; clears both when no hook is active.
    /// Runs automatically after key input (see [`Self::dispatch_key`]).
    pub fn sync_search_state(&mut self) {
        match self.search_snapshot() {
            Some(snapshot) => {
                self.search_matches = snapshot.matches;
                self.search_highlight_all = snapshot.highlight_all;
            }
            None => {
                self.search_matches.clear();
                self.search_highlight_all = false;
            }
        }
    }

    /// Saves the current editor document contents to a file.
    pub fn save_file<P: AsRef<std::path::Path>>(
        &self,
        path: P,
    ) -> Result<(), twrite_core::EditorError> {
        self.buffer.save_to_file(path)
    }
}
