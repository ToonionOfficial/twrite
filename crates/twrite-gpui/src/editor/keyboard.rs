use gpui::{Context, KeyDownEvent, Window};
use twrite_core::{
    HookContext, HookOutcome, KeyCode, KeyEvent, Modifiers, Point as BufferPoint, SearchAction,
    Selection,
};

use super::Editor;

impl Editor {
    pub(crate) fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(key_event) = crate::input::translate_key_down(event) {
            if self.context_menu.is_open() {
                let plain = !key_event.modifiers.ctrl
                    && !key_event.modifiers.alt
                    && !key_event.modifiers.meta;
                match &key_event.code {
                    KeyCode::Escape => {
                        self.dismiss_context_menu(cx);
                        return;
                    }
                    KeyCode::Up if plain => {
                        self.move_context_menu_selection(false, cx);
                        return;
                    }
                    KeyCode::Down if plain => {
                        self.move_context_menu_selection(true, cx);
                        return;
                    }
                    KeyCode::Enter if plain => {
                        if self.context_menu_selected.is_some() {
                            self.activate_context_menu_selected(window, cx);
                        } else {
                            self.dismiss_context_menu(cx);
                        }
                        return;
                    }
                    // Any other key dismisses the menu and falls through
                    // so typing still lands in the buffer.
                    _ => self.dismiss_context_menu(cx),
                }
            }
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
            self.finish_consumed_action(window, cx, initial_version);
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
            // Physical fallbacks from translation are already lowercase, so
            // `Char` codes match directly (the old string path lowercased).
            match &key_event.code {
                KeyCode::Char('z') => {
                    if key_event.modifiers.shift {
                        self.buffer.redo();
                    } else {
                        self.buffer.undo();
                    }
                    self.selection = None;
                    edited = true;
                }
                KeyCode::Char('y') => {
                    self.buffer.redo();
                    self.selection = None;
                    edited = true;
                }
                KeyCode::Char('a') => {
                    self.selection = Some(Selection::range(0, self.buffer.len_bytes()));
                }
                KeyCode::Char('c') => {
                    self.copy(cx);
                }
                KeyCode::Char('x') => {
                    edited = self.cut(cx);
                }
                KeyCode::Char('v') => {
                    edited = self.paste(cx);
                }
                KeyCode::Backspace => {
                    if !self.delete_selection() {
                        edited = self.buffer.delete_prev_word();
                    } else {
                        edited = true;
                    }
                    self.selection = None;
                }
                KeyCode::Delete => {
                    if !self.delete_selection() {
                        edited = self.buffer.delete_next_word();
                    } else {
                        edited = true;
                    }
                    self.selection = None;
                }
                KeyCode::Left => {
                    let target = self.buffer.prev_word_offset();
                    self.move_cursor_to(target, select);
                }
                KeyCode::Right => {
                    let target = self.buffer.next_word_offset();
                    self.move_cursor_to(target, select);
                }
                KeyCode::Home => {
                    self.move_cursor_to(0, select);
                }
                KeyCode::End => {
                    self.move_cursor_to(self.buffer.len_bytes(), select);
                }
                KeyCode::Up => {
                    self.scroll_up(1);
                    self.flush_effects();
                    cx.notify();
                    return;
                }
                KeyCode::Down => {
                    self.scroll_down(1);
                    self.flush_effects();
                    cx.notify();
                    return;
                }
                _ => {}
            }
        } else {
            match &key_event.code {
                KeyCode::Backspace => {
                    if !self.delete_selection() {
                        self.buffer.backspace();
                    }
                    self.selection = None;
                    edited = true;
                }
                KeyCode::Delete => {
                    if !self.delete_selection() {
                        self.buffer.delete();
                    }
                    self.selection = None;
                    edited = true;
                }
                KeyCode::Enter => {
                    self.replace_selection_or_insert("\n");
                    self.selection = None;
                    edited = true;
                }
                KeyCode::Tab => {
                    self.replace_selection_or_insert(&" ".repeat(self.config.tab_size));
                    self.selection = None;
                    edited = true;
                }
                KeyCode::Char(' ') => {
                    self.replace_selection_or_insert(" ");
                    self.selection = None;
                    edited = true;
                }
                KeyCode::Left => {
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
                KeyCode::Right => {
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
                KeyCode::Up => {
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
                KeyCode::Down => {
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
                KeyCode::Home => {
                    let target = self.buffer.line_start_offset();
                    self.move_cursor_to(target, select);
                }
                KeyCode::End => {
                    let target = self.buffer.line_end_offset();
                    self.move_cursor_to(target, select);
                }
                KeyCode::Char(c)
                    if !key_event.modifiers.alt
                        && !key_event.modifiers.ctrl
                        && !key_event.modifiers.meta =>
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
                        if self.hooks[hook_idx].before_insert(&mut ctx, *c) == HookOutcome::Consumed
                        {
                            insert_consumed = true;
                            break;
                        }
                        hook_idx += 1;
                    }

                    if !insert_consumed {
                        let mut buf = [0u8; 4];
                        self.replace_selection_or_insert(c.encode_utf8(&mut buf));
                        self.selection = None;
                        edited = true;
                    }
                }
                _ => {}
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
    /// keyboard path (e.g. `Alt+C` toggles Match Case, plain `Down`
    /// walks to the next match).
    pub fn press_search_key(
        &mut self,
        code: KeyCode,
        ctrl: bool,
        alt: bool,
        shift: bool,
        window: Option<&Window>,
        cx: &mut Context<Self>,
    ) {
        let key_event = KeyEvent {
            code,
            modifiers: Modifiers {
                ctrl,
                alt,
                shift,
                meta: false,
            },
        };
        self.dispatch_key(&key_event, window, cx);
    }

    /// Feeds a synthetic search-panel action (prompt-bar clicks) through the
    /// hook chain with the same post-processing as consumed keys.
    ///
    /// Pointer chrome cannot produce a [`KeyCode`], so actions travel on
    /// [`EditorHook::on_search_action`] instead of through [`KeyEvent`].
    pub fn press_search_action(
        &mut self,
        action: SearchAction,
        window: Option<&Window>,
        cx: &mut Context<Self>,
    ) {
        self.reset_blink_cursor(cx);
        let initial_version = self.buffer.version();

        let mut hook_idx = 0;
        while hook_idx < self.hooks.len() {
            let mut ctx = HookContext::new(
                &mut self.buffer,
                &mut self.selection,
                &mut self.cursor_style,
                &mut self.prompt,
                &mut self.pending_effects,
            );
            if self.hooks[hook_idx].on_search_action(&mut ctx, action) == HookOutcome::Consumed {
                break;
            }
            hook_idx += 1;
        }

        self.finish_consumed_action(window, cx, initial_version);
    }

    /// Post-processing shared by consumed keys and synthetic actions:
    /// selection callbacks, scrolling, effect + search sync, notify.
    fn finish_consumed_action(
        &mut self,
        window: Option<&Window>,
        cx: &mut Context<Self>,
        initial_version: usize,
    ) {
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
    }
}
