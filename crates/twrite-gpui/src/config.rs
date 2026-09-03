use gpui::{Font, Pixels, SharedString, px};

/// Layout and visual settings for the editor canvas.
#[derive(Debug, Clone)]
pub struct EditorConfig {
    /// Whether to render line numbers in the left gutter.
    pub line_numbers: bool,
    /// Vertical line height in pixels.
    pub line_height: Pixels,
    /// Text font size in pixels.
    pub font_size: Pixels,
    /// Number of spaces per tab indentation.
    pub tab_size: usize,
    /// Whether to highlight the background of the active cursor line.
    pub highlight_active_line: bool,
    /// Default cursor shape: true for block, false for line/bar.
    pub block_cursor: bool,
    /// Whether to soft-wrap lines at the viewport boundary.
    pub line_wrap: bool,
    /// Base font family override (`None` auto-selects, see below).
    ///
    /// When unset, the editor probes [`Self::platform_monospace_candidates`]
    /// at paint time and uses the first family with bold + italic faces. An
    /// explicitly set family is trusted verbatim (still probed, so
    /// `Editor::face_availability` stays truthful). Missing faces fall back
    /// silently at the OS level, which is why auto-select exists.
    pub font_family: Option<SharedString>,
    /// Font family for `Code` spans (`None` reuses the base family).
    pub code_font_family: Option<SharedString>,
    /// Markdown WYSIWYG and syntax configuration.
    #[cfg(feature = "markdown")]
    pub markdown: twrite_core::markdown::MarkdownConfig,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            line_numbers: false,
            line_height: px(22.0),
            font_size: px(16.0),
            tab_size: 4,
            highlight_active_line: false,
            block_cursor: false,
            line_wrap: true,
            font_family: None,
            code_font_family: None,
            #[cfg(feature = "markdown")]
            markdown: twrite_core::markdown::MarkdownConfig::default(),
        }
    }
}

impl EditorConfig {
    /// Ordered monospace fallback families for font auto-select.
    ///
    /// Ordered by likelihood of shipping full (regular/bold/italic/bold-italic)
    /// faces: a partial set (e.g. regular+bold only) can never satisfy emphasis,
    /// so completeness outranks name recognition.
    pub fn platform_monospace_candidates() -> Vec<SharedString> {
        if cfg!(target_os = "macos") {
            vec!["Menlo".into(), "Monaco".into(), "Courier New".into()]
        } else if cfg!(target_os = "windows") {
            vec![
                "Consolas".into(),
                "Cascadia Mono".into(),
                "Courier New".into(),
            ]
        } else {
            vec![
                "Liberation Mono".into(),
                "DejaVu Sans Mono".into(),
                "Noto Sans Mono".into(),
                "monospace".into(),
            ]
        }
    }

    /// Candidate families for auto-select: the explicit family alone when set,
    /// otherwise the platform list.
    pub fn font_candidates(&self) -> Vec<SharedString> {
        match &self.font_family {
            Some(family) => vec![family.clone()],
            None => Self::platform_monospace_candidates(),
        }
    }

    /// Picks the first candidate with bold + italic faces (else the first with
    /// either, else `None`). Pure to stay headless-testable; callers pass a
    /// probe comparing resolved `FontId`s.
    pub fn pick_family(
        candidates: &[SharedString],
        mut probe: impl FnMut(&str) -> (bool, bool),
    ) -> Option<&SharedString> {
        let mut partial = None;
        for candidate in candidates {
            match probe(candidate.as_ref()) {
                (true, true) => return Some(candidate),
                (false, false) => {}
                _ => {
                    if partial.is_none() {
                        partial = Some(candidate);
                    }
                }
            }
        }
        partial
    }

    /// Resolves the base font: explicit family, else auto-selected family, else host.
    pub fn base_font(&self, host: &Font, selected: Option<&SharedString>) -> Font {
        let mut font = host.clone();
        if let Some(family) = self.font_family.as_ref().or(selected) {
            font.family = family.clone();
        }
        font
    }

    /// Resolves the font for `Code` spans: explicit code family, else whatever
    /// [`Self::base_font`] resolves (so code follows auto-select by default).
    pub fn code_font(&self, host: &Font, selected: Option<&SharedString>) -> Font {
        let mut font = self.base_font(host, selected);
        if let Some(family) = &self.code_font_family {
            font.family = family.clone();
        }
        font
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe_for(
        full: Vec<&'static str>,
        partial: Vec<&'static str>,
    ) -> impl FnMut(&str) -> (bool, bool) {
        move |name: &str| {
            if full.contains(&name) {
                (true, true)
            } else if partial.contains(&name) {
                (true, false)
            } else {
                (false, false)
            }
        }
    }

    #[test]
    fn pick_family_prefers_full_faces() {
        let candidates: Vec<SharedString> = vec!["A".into(), "B".into(), "C".into()];
        let picked = EditorConfig::pick_family(&candidates, probe_for(vec!["B"], vec!["A"]));
        assert_eq!(picked.map(|s| s.as_ref()), Some("B"));
    }

    #[test]
    fn pick_family_falls_back_to_partial_then_none() {
        let candidates: Vec<SharedString> = vec!["A".into(), "B".into()];
        let picked = EditorConfig::pick_family(&candidates, probe_for(vec![], vec!["B"]));
        assert_eq!(picked.map(|s| s.as_ref()), Some("B"));

        let picked = EditorConfig::pick_family(&candidates, probe_for(vec![], vec![]));
        assert!(picked.is_none());
    }

    #[test]
    fn explicit_candidates_shortcircuit_to_single_family() {
        let config = EditorConfig {
            font_family: Some("Mine".into()),
            ..EditorConfig::default()
        };
        assert_eq!(config.font_candidates(), vec![SharedString::from("Mine")]);
    }

    #[test]
    fn base_font_precedence_is_explicit_selected_host() {
        use gpui::Font;
        let host = Font::default();
        let selected: SharedString = "Selected".into();
        let config = EditorConfig::default();

        assert_eq!(
            config.base_font(&host, Some(&selected)).family.as_ref(),
            "Selected"
        );
        assert_eq!(
            config.base_font(&host, None).family.as_ref(),
            host.family.as_ref()
        );

        let config = EditorConfig {
            font_family: Some("Explicit".into()),
            ..EditorConfig::default()
        };
        assert_eq!(
            config.base_font(&host, Some(&selected)).family.as_ref(),
            "Explicit"
        );
        // Code follows the selected base unless explicitly overridden.
        assert_eq!(
            config.code_font(&host, Some(&selected)).family.as_ref(),
            "Explicit"
        );
        let config = EditorConfig::default();
        assert_eq!(
            config.code_font(&host, Some(&selected)).family.as_ref(),
            "Selected"
        );
    }
}
