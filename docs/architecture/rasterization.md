# Milestone 11–12: Rasterization

**Milestone 11** introduced `mocha_raster`, a software rasterizer that converts
the document's display list into a pixel buffer. **Milestone 12** uses it to
draw both page content and browser chrome to a window.

## Purpose

The rasterizer is the bridge between the engine's display list (vector commands)
and the pixel framebuffer (window or screenshot). It consumes high-level drawing
commands and produces pixels.

## Architecture

### mocha_raster Crate

**Module:** `crates/mocha_raster/src/lib.rs`

- `Surface` — the drawing surface (pixel buffer + helpers)
- `rasterize()` — main entry point that converts display list to pixels
- Rendering methods for each display command type

### Data Flow

```
mocha_engine
    ↓
display list: Vec<DisplayCommand>
    ↓
mocha_raster::rasterize(surface, commands, images, scroll_y)
    ↓
Surface (pixel buffer)
    ↓
mocha_desktop::window (minifb update)
    ↓
OS window
```

The rasterizer is **stateless**: it takes a display list + images and produces
pixels. It does not own the window, the event loop, or the browser state.

### Surface

The `Surface` struct is a pixel framebuffer with drawing helpers:

```rust
pub struct Surface {
    width: u32,
    height: u32,
    buffer: Vec<u32>,  // ARGB32
}
```

Methods:
- `draw_rect(x, y, w, h, color)` — solid rectangle
- `draw_rect_outline(x, y, w, h, thickness, color)` — stroked rectangle
- `draw_text_at(text, x, y, scale, color)` — debug bitmap font
- `buffer()` — pixel data for display
- `clear(color)` — fill entire surface

### Display List Commands

The rasterizer handles these command types (defined in `mocha_paint`):

**`DrawRect`**
- Rectangle with solid color and optional border
- No gradients, patterns, or effects

**`DrawBorder`**
- Stroked rectangle (border only, no fill)
- Single width, solid color

**`DrawText`**
- Text run with color and scale
- Uses debug bitmap font (no font shaping, no metrics, no antialiasing)
- Positioning is relative to the draw command; clipping uses scroll offset

**`DrawImage`**
- References an image by ID
- The rasterizer looks up pixels in the images vec
- Scales to the command's dest rect using nearest-neighbor
- No antialiasing, no subpixel rendering

**`DrawControl`**
- Form control (checkbox, radio, text input, button, etc.)
- The rasterizer draws a simple representation:
  - Checkbox/radio: small box, filled if checked
  - Text input: box with underline, text label
  - Button: box with label
  - Select: box with dropdown indicator
- No native widgets, no text editing, no focus highlight

### Rasterization Process

1. Clear the surface to white
2. For each command in the display list (in order):
   - Clipping: commands are clipped to the page viewport (below chrome)
   - Draw: call the appropriate `Surface` method
3. For desktop mode: draw chrome on top (via `window.rs`)
4. Return the pixel buffer to the window

### Clipping

The rasterizer respects the scroll offset and page viewport. The page content is
clipped to the visible area below the chrome (address bar + toolbar). This
prevents the page from overwriting the toolbar.

Chrome is drawn in a separate phase after page rasterization, so it is always visible.

### Scroll Handling

The display list is generated at scroll offset 0. The rasterizer applies the
scroll offset:

```
screen_y = command.y - scroll_y
```

If `screen_y + height < 0` or `screen_y > viewport.height`, the command is
outside the viewport and not drawn.

## Limitations

### Text Rendering

- **Debug bitmap font only** — a simple pixelated glyph set (lowercase/uppercase/digits/punctuation)
- **No font loading** — no system fonts, no web fonts, no fallback chain
- **No font metrics** — text width/height estimated during layout, not measured
- **No shaping** — no kerning, no ligatures, no complex script support
- **No subpixel rendering** — glyph pixels are aligned to device pixels
- **No antialiasing** — glyph edges are aliased
- **No baseline alignment** — all text is top-aligned

### Image Rendering

- **Nearest-neighbor scaling only** — no interpolation
- **No antialiasing** — block artifacts on downscaling
- **No compositing** — images are opaque, no blending modes
- **No color management** — no ICC profiles, no gamma correction
- **No lazy loading** — all images are decoded upfront (by `mocha_image`)

### Drawing Primitives

- **Solid colors only** — no gradients, patterns, or effects
- **Axis-aligned rectangles** — no rotation, skew, or transforms
- **No shadows, blur, or filters**
- **No z-order / stacking contexts** — draw order follows the display list
- **No antialiasing** — edges are aliased
- **No subpixel rendering**

### Form Control Rendering

- **Simple rectangles and text** — no native look-and-feel
- **No visual feedback** — no hover/press/focus states
- **No accessibility** — no ARIA, no screen reader support

### Window Integration

- **No GPU acceleration** — fully CPU-based, single-threaded
- **No compositing** — no layers or async updates
- **No dirty region tracking** — entire buffer is redrawn each frame
- **No vsync or frame rate control** — relies on window library (`minifb`)

## Performance

The rasterizer is optimized for clarity, not speed:

- Single-threaded
- No SIMD
- No caching or memoization
- Full framebuffer redraw each frame

For a small experimental browser at 800×600, performance is acceptable. It is
not suitable for high-resolution displays or real-time animation.

## Testing

`mocha_raster` has no direct unit tests (the Surface is not directly tested).
Integration tests exist in `tests/integration/` that verify the full pipeline
(load → render → rasterize).

Visual regression testing is not implemented (would require image comparison,
which is out of scope).

## Files

- `crates/mocha_raster/src/lib.rs` — `Surface`, rasterization logic
- `crates/mocha_desktop/src/window.rs` — window integration (calls `rasterize()`)

## Integration with Other Layers

**Upstream (display list):**

The display list comes from `mocha_paint` (which generates it from layout trees).
The same display list is consumed by:
- `mocha_raster` (pixels)
- `mocha_shell` (text output for terminal)

**Downstream (window):**

`mocha_desktop::window` calls `mocha_raster::rasterize()` each frame and passes
the pixel buffer to `minifb` for display.

## Future Expansion

Potential improvements (out of scope for M12):

- Real font rendering (via a font library)
- GPU-accelerated rasterization (compute shader or graphics API)
- Dirty region tracking (only redraw changed areas)
- Subpixel rendering and antialiasing
- Image scaling quality controls
- CSS gradients and filters
- Native form control rendering
- Accessibility / semantic rendering
