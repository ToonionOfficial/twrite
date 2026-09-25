#[cfg(feature = "gpui-new-api")]
use gpui::TextAlign;
use gpui::{App, FocusHandle, Pixels, Point, ShapedLine, Window};

/// Paints a shaped auxiliary line (fold chevron, fold text, gutter number).
///
/// gpui 0.2.2 bakes left alignment into `ShapedLine::paint`, while the new
/// GPUI ABI (gpui-ce, zed v1.0+, bezel 0.3.12+) takes explicit alignment.
/// Paint errors are ignored here to match every other paint call in the
/// canvas render path: a failed paint only drops one frame of decoration.
pub(crate) fn paint_shaped_line(
    shaped: &ShapedLine,
    origin: Point<Pixels>,
    line_height: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    #[cfg(not(feature = "gpui-new-api"))]
    {
        let _ = shaped.paint(origin, line_height, window, cx);
    }
    #[cfg(feature = "gpui-new-api")]
    {
        let _ = shaped.paint(origin, line_height, TextAlign::Left, None, window, cx);
    }
}

/// Moves focus to the editor handle.
///
/// The new GPUI ABI threads the app context through focus changes.
pub(crate) fn focus_editor(handle: &FocusHandle, window: &mut Window, cx: &mut App) {
    #[cfg(not(feature = "gpui-new-api"))]
    {
        let _ = cx;
        handle.focus(window);
    }
    #[cfg(feature = "gpui-new-api")]
    {
        handle.focus(window, cx);
    }
}
