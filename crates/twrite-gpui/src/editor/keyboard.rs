use gpui::{Context, KeyDownEvent, Window};
use twrite_core::{HookContext, HookOutcome, KeyEvent, Modifiers, Point as BufferPoint, Selection};

use super::Editor;

impl Editor {
    pub(crate) fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(key_event) = crate::input::translate_key_down(event) {
            self.dispatch_key(&key_event, Some(window), cx);
        }
    }

    /// Feeds a translated key through the hook chain with full post-processing
    /// (selection callbacks, scrolling, effect + search sync, notify).
    ///
    /// Used by [`Self::handle_key_down`] and by synthetic prompt-bar clicks
    /// via [`Self::press_search_key`].
    pub(crate) fn dispatch_key(
        &mut self,
        key_event: &KeyEvent,
        window: Option<&Window>,
        cx: &mut Context<Self>,
    ) {
        self.reset_blink_cursor(cx);
        let initial_version = self.buffer.version();
        let mut consumed = false;

        let mut hook_idx = 0;
        while hook_idx < self.hooks.len() {
            let mut ctx = HookContext::new(
                &mut self.buffer,
                &mut self.selection,
                &mut self.cursor_style,
                &mut self.prompt,
                &mut self.pending_effects,
            );
            let outcome = self.hooks[hook_idx].on_key(&mut ctx, key_event);
            if outcome == HookOutcome::Consumed {
                consumed = true;
                break;
            }
            hook_idx += 1;
        }

        if consumed {
            if self.buffer.version() != initial_version {
                for hook in &mut self.hooks {
                    hook.after_edit(&mut self.buffer);
                }
            }
            for hook in &mut self.hooks {
                hook.on_selection_change(&self.buffer, self.selection.as_ref());
            }
            self.scroll_to_cursor(window);
            self.flush_effects();
            self.sync_search_state();
            cx.notify();
            return;
        }

        if self.prompt.is_open() {
            for hook in &mut self.hooks {
                hook.on_selection_change(&self.buffer, self.selection.as_ref());
            }
            self.flush_effects();
            self.sync_search_state();
            cx.notify();
            return;
        }

        let mut edited = false;
        let select = key_event.modifiers.shift;

        if key_event.modifiers.ctrl || key_event.modifiers.meta {
            match key_event.key.to_lowercase().as_str() {
                "z" => {
                    if key_event.modifiers.shift {
                        self.buffer.redo();
                    } else {
                        self.buffer.undo();
                    }
                    self.selection = None;
                    edited = true;
                }
                "y" => {
                    self.buffer.redo();
                    self.selection = None;
                    edited = true;
                }
                "a" => {
                    self.selection = Some(Selection::range(0, self.buffer.len_bytes()));
                }
                "c" => {
                    self.copy(cx);
                }
                "x" => {
                    edited = self.cut(cx);
                }
                "v" => {
                    edited = self.paste(cx);
                }
                "backspace" => {
                    if !self.delete_selection() {
                        edited = self.buffer.delete_prev_word();
                    } else {
                        edited = true;
                    }
                    self.selection = None;
                }
                "delete" => {
                    if !self.delete_selection() {
                        edited = self.buffer.delete_next_word();
                    } else {
                        edited = true;
                    }
                    self.selection = None;
                }
                "left" | "arrowleft" => {
                    let target = self.buffer.prev_word_offset();
                    self.move_cursor_to(target, select);
                }
                "right" | "arrowright" => {
                    let target = self.buffer.next_word_offset();
                    self.move_cursor_to(target, select);
                }
                "home" => {
                    self.move_cursor_to(0, select);
                }
                "end" => {
                    self.move_cursor_to(self.buffer.len_bytes(), select);
                }
                "up" | "arrowup" => {
                    self.scroll_up(1);
                    self.flush_effects();
                    cx.notify();
                    return;
                }
                "down" | "arrowdown" => {
                    self.scroll_down(1);
                    self.flush_effects();
                    cx.notify();
                    return;
                }
                _ => {}
            }
        } else {
            match key_event.key.as_str() {
                "backspace" => {
                    if !self.delete_selection() {
                        self.buffer.backspace();
                    }
                    self.selection = None;
                    edited = true;
                }
                "delete" => {
                    if !self.delete_selection() {
                        self.buffer.delete();
                    }
                    self.selection = None;
                    edited = true;
                }
                "enter" => {
                    self.replace_selection_or_insert("\n");
                    self.selection = None;
                    edited = true;
                }
                "tab" => {
                    self.replace_selection_or_insert(&" ".repeat(self.config.tab_size));
                    self.selection = None;
                    edited = true;
                }
                "space" | " " => {
                    self.replace_selection_or_insert(" ");
                    self.selection = None;
                    edited = true;
                }
                "left" | "arrowleft" => {
                    if !select && self.selection.is_some() {
                        let sel = self.selection.take().unwrap();
                        self.buffer.set_cursor_offset(sel.byte_range().start);
                    } else {
                        let target = if self.buffer.cursor_offset() > 0 {
                            let char_idx =
                                self.buffer.text().byte_to_char(self.buffer.cursor_offset());
                            self.buffer.text().char_to_byte(char_idx - 1)
                        } else {
                            0
                        };
                        self.move_cursor_to(target, select);
                    }
                }
                "right" | "arrowright" => {
                    if !select && self.selection.is_some() {
                        let sel = self.selection.take().unwrap();
                        self.buffer.set_cursor_offset(sel.byte_range().end);
                    } else {
                        let target = if self.buffer.cursor_offset() < self.buffer.len_bytes() {
                            let char_idx =
                                self.buffer.text().byte_to_char(self.buffer.cursor_offset());
                            self.buffer
                                .text()
                                .char_to_byte((char_idx + 1).min(self.buffer.text().len_chars()))
                        } else {
                            self.buffer.len_bytes()
                        };
                        self.move_cursor_to(target, select);
                    }
                }
                "up" | "arrowup" => {
                    let point = self.buffer.cursor_point();
                    if point.row > 0 {
                        let target = self
                            .buffer
                            .point_to_offset(BufferPoint::new(point.row - 1, point.column));
                        self.move_cursor_to(target, select);
                    } else {
                        self.move_cursor_to(0, select);
                    }
                }
                "down" | "arrowdown" => {
                    let point = self.buffer.cursor_point();
                    let total_lines = self.buffer.len_lines();
                    if point.row + 1 < total_lines {
                        let target = self
                            .buffer
                            .point_to_offset(BufferPoint::new(point.row + 1, point.column));
                        self.move_cursor_to(target, select);
                    } else {
                        self.move_cursor_to(self.buffer.len_bytes(), select);
                    }
                }
                "home" => {
                    let target = self.buffer.line_start_offset();
                    self.move_cursor_to(target, select);
                }
                "end" => {
                    let target = self.buffer.line_end_offset();
                    self.move_cursor_to(target, select);
                }
                key => {
                    if !key_event.modifiers.alt
                        && !key_event.modifiers.ctrl
                        && !key_event.modifiers.meta
                        && key.chars().count() == 1
                    {
                        let mut insert_consumed = false;
                        let mut hook_idx = 0;
                        while hook_idx < self.hooks.len() {
                            let mut ctx = HookContext::new(
                                &mut self.buffer,
                                &mut self.selection,
                                &mut self.cursor_style,
                                &mut self.prompt,
                                &mut self.pending_effects,
                            );
                            if self.hooks[hook_idx].before_insert(&mut ctx, key)
                                == HookOutcome::Consumed
                            {
                                insert_consumed = true;
                                break;
                            }
                            hook_idx += 1;
                        }

                        if !insert_consumed {
                            self.replace_selection_or_insert(key);
                            self.selection = None;
                            edited = true;
                        }
                    }
                }
            }
        }

        if edited {
            for hook in &mut self.hooks {
                hook.after_edit(&mut self.buffer);
            }
        }

        for hook in &mut self.hooks {
            hook.on_selection_change(&self.buffer, self.selection.as_ref());
        }

        self.scroll_to_cursor(window);
        self.flush_effects();
        self.sync_search_state();
        cx.notify();
    }

    /// Feeds a synthetic key through the hook chain with full post-processing.
    ///
    /// Used by prompt-bar chips and arrow buttons so clicks share the exact
    /// keyboard path (e.g. `Alt+C` toggles Match Case, plain `arrowdown`
    /// walks to the next match).
    pub fn press_search_key(
        &mut self,
        key: &str,
        ctrl: bool,
        alt: bool,
        shift: bool,
        window: Option<&Window>,
        cx: &mut Context<Self>,
    ) {
        let key_event = KeyEvent {
            key: key.to_string(),
            modifiers: Modifiers {
                ctrl,
                alt,
                shift,
                meta: false,
            },
        };
        self.dispatch_key(&key_event, window, cx);
    }
}
