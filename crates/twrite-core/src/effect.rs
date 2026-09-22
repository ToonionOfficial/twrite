/// App-level requests a hook cannot fulfil on its own (file I/O, quit).
///
/// Hooks push these into [`crate::HookContext::effects`]; frontends drain the
/// queue after each event. This keeps `twrite-core` headless while letting
/// hook-only code (vim `:w` / `:q` / `:e`) drive full editor workflows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookEffect {
    /// Save the buffer (`None` = current path, error if unknown).
    Save {
        /// Explicit destination path, if any.
        path: Option<String>,
    },
    /// Load a file into the buffer, resetting undo history.
    Load {
        /// File path to load.
        path: String,
    },
    /// Follow an internal link (e.g. a Markdown `[[target]]` wikilink).
    ///
    /// The core reports activation only; hosts resolve `target` against
    /// whatever storage they use (files, database, memory) and decide what
    /// following means (open, reveal, create, message). This keeps the core
    /// free of storage assumptions.
    FollowLink {
        /// Link target text (note name, document id, anchor name).
        target: String,
        /// Optional `#fragment` within the target.
        fragment: Option<String>,
        /// Optional display label (`|label` alias), if the link carries one.
        label: Option<String>,
    },
    /// Request application exit.
    Quit {
        /// Skip dirty-buffer checks.
        force: bool,
    },
    /// Show a transient message (status line, prompt error area).
    Message(String),
}
