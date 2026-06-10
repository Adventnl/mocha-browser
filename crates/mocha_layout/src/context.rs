//! Layout-wide parameters: the viewport.

/// Default viewport width used by the shell.
pub const DEFAULT_VIEWPORT_WIDTH: f32 = 800.0;
/// Default viewport height. Vertical content may exceed this in Milestone 3.
pub const DEFAULT_VIEWPORT_HEIGHT: f32 = 600.0;

/// The available drawing area. Only `width` influences layout today; `height`
/// does not clamp content (there is no scrolling or overflow).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutViewport {
    /// Viewport width in pixels.
    pub width: f32,
    /// Viewport height in pixels (currently informational only).
    pub height: f32,
}

impl Default for LayoutViewport {
    fn default() -> Self {
        LayoutViewport {
            width: DEFAULT_VIEWPORT_WIDTH,
            height: DEFAULT_VIEWPORT_HEIGHT,
        }
    }
}
