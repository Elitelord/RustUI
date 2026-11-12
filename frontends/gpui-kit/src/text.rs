//! Text for screen readers.
//!
//! GPUI doesn't put plain strings in the accessibility tree, so the lab draws
//! its text with gpui-kit's `Label`, which exposes each one as a text node
//! (longbridge/gpui-kit#3937; on releases without it this only changes how
//! the text is built).
//!
//! A label's id is the line that builds it. Helpers such as `kv` build many
//! labels from one line, so they wrap each row in an element with its own id;
//! two labels with the same id under one parent would make GPUI's debug
//! builds panic while a screen reader is connected.

use gpui_kit::component::label::Label;
use gpui_kit::*;

/// A [`Label`] laid out like the plain text it replaces: `Label` fixes its
/// line height at 1.25rem, and this keeps GPUI's default.
#[track_caller]
pub fn text(text: impl Into<SharedString>) -> Label {
    Label::new(text).line_height(phi())
}
