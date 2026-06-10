# Images and Replaced Elements (Milestone 9)

Milestone 9 adds basic `<img>` support: discover the image, resolve its URL, load
the bytes, decode the intrinsic dimensions, lay the image out as a replaced
element, and emit a `DrawImage` display command.

**Nothing is rasterized to a window.** There is no graphics surface yet; Mocha
emits `DrawImage` commands (and the layout box that carries the image) but does
not draw pixels. Image rendering is **not** complete.

## Decoder dependency

This is the **only third-party dependency** in the workspace:

```toml
image = { version = "0.25", default-features = false, features = ["png", "jpeg"] }
```

`default-features = false` keeps the tree minimal — only the PNG and JPEG codecs
are pulled in (via `png` and `zune-jpeg`), no GIF/WebP/TIFF, no `rayon`. Mocha
does **not** write an image decoder from scratch. The `mocha_image` crate is a
thin wrapper: it detects the format, validates by fully decoding, and returns the
intrinsic dimensions (`DecodedImage { width, height, format }`). Unsupported
formats are `UnsupportedFeature`; corrupt/unreadable data is `MochaError::Image`.

## `<img>` parsing

`<img>` is a **void element** (no end tag). Attributes `src`, `alt`, `width`,
`height` are stored; `src` is required for rendering (a missing `src` is a clear
`Layout` error). Not supported: `srcset`, `sizes`, `<picture>`, `loading`,
`decoding`, `crossorigin`, `referrerpolicy`, responsive images, SVG, animation.

## Loading

`mocha_resources::discover_images` finds each `<img src>` in document order;
`load_image` resolves `src` against the document base URL, loads it through
`mocha_net`, validates the content type (an `image/*` type, or
`application/octet-stream`/missing for the decoder to validate; an explicit
non-image type like `text/plain` is rejected), and decodes it. Local files report
`image/png`/`image/jpeg` from their extension. A failed load or decode aborts the
render with a clear error (no broken-image placeholder yet). HTTPS images are
unsupported (no TLS).

## Sizing

The final content-box size is resolved per axis as **CSS > attribute >
intrinsic**:

- both width and height specified → use both;
- only one specified → preserve the intrinsic aspect ratio for the other;
- neither → use the intrinsic decoded size.

CSS `width`/`height` (px) come from computed style; `width`/`height` attributes
are read from the DOM element; the intrinsic size comes from the decoder.

## Layout

A loaded image becomes a `ReplacedBox { image_id, width, height }` on its styled
node, and a `LayoutBoxKind::Image(image_id)` layout box:

- **inline `<img>`** (the default) participates in the inline formatting context
  as an atom in the line's item stream: it shares a line with adjacent text, is
  placed in document order, and raises the line height when taller than the text.
- **block `<img>`** (`img { display: block }`) is laid out as a block box and
  stacks vertically with its siblings.

Baseline / `vertical-align` is **not** modelled — inline items are top-aligned.
Backgrounds/borders on images are out of scope.

## Paint

`mocha_paint` emits `DrawImage { image_id, x, y, width, height }` for each image
box, in document order with surrounding text/rect/border commands. The image id
indexes the document's decoded-image store. The terminal prints the command; no
pixels are drawn.

## Tests

`mocha_image` decode tests (PNG and JPEG dimensions, unsupported format, corrupt
bytes), `mocha_resources` image discovery/loading tests, and an `image_pipeline`
integration test cover: intrinsic/attribute/CSS sizing, inline image sharing a
line in document order, block images stacking, `DrawImage` emission, HTTP image
load, `text/plain` image rejected, and missing/corrupt image failing the render
clearly. The checked-in `examples/assets/mocha-test.png` is a tiny 16×16 PNG.
No public internet is used.
