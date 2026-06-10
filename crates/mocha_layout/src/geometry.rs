//! Geometry primitives shared across the layout engine.

/// An axis-aligned rectangle in pixels with the origin at the top-left. For
/// boxes this is the border box (content + padding + border).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

impl Rect {
    /// The x coordinate of the right edge.
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    /// The y coordinate of the bottom edge.
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// Per-side lengths used for margins, padding, and (uniform) borders.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EdgeSizes {
    /// Top edge.
    pub top: f32,
    /// Right edge.
    pub right: f32,
    /// Bottom edge.
    pub bottom: f32,
    /// Left edge.
    pub left: f32,
}

impl EdgeSizes {
    /// A uniform inset on all four sides.
    pub fn uniform(value: f32) -> EdgeSizes {
        EdgeSizes {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    /// Combined left + right inset.
    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    /// Combined top + bottom inset.
    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_edges() {
        let rect = Rect {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
        };
        assert_eq!(rect.right(), 40.0);
        assert_eq!(rect.bottom(), 60.0);
    }

    #[test]
    fn edge_sizes_sums() {
        let edges = EdgeSizes {
            top: 1.0,
            right: 2.0,
            bottom: 3.0,
            left: 4.0,
        };
        assert_eq!(edges.horizontal(), 6.0);
        assert_eq!(edges.vertical(), 4.0);
        assert_eq!(EdgeSizes::uniform(5.0).horizontal(), 10.0);
    }
}
