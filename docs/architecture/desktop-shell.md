# Milestone 11–12: Desktop Shell

**Milestone 11** introduced the first real browser window: a desktop shell that
displays the document rendering pipeline result in a native window with input
routing and pixel rasterization. **Milestone 12** added browser chrome (address
bar, navigation buttons) on top of the shell.

## Purpose

The desktop shell is the visual and interactive component of the browser:
- Opens a native window (via `minifb` with the optional `gui` feature)
- Rasterizes the display list into pixels
- Draws browser chrome (address bar, buttons)
- Routes user input (clicks, keyboard) to the browser state

All state and logic is isolated in the `BrowserAppState` state machine (M12),
which remains fully testable without opening a window. The window layer (`window.rs`)
is intentionally thin and untestable — it is only a pump for events and a
display surface.

## Architecture

### Crates and Modules

**`mocha_desktop`**: Desktop shell and browser app state

- `browser_app.rs`: `BrowserAppState` — the core state machine (navigation,
  address bar, focus, history)
- `chrome.rs`: `ChromeLayout` — toolbar/button/address-bar layout and hit testing
- `address_bar.rs`: `AddressBarState` — address bar editing state
- `lib.rs`: `DesktopPageState` — document loading/rendering (M11), shared with shell
- `window.rs`: Native window event loop and rendering (M11/M12, untestable)

**`mocha_engine`**: High-level document loading/rendering orchestration

- Shared pipeline used by both `mocha_shell` (terminal) and `mocha_desktop` (window)

**`mocha_raster`**: Display list rasterization

- `rasterize()` — converts display list + images to pixel buffer
- `Surface` — the drawing surface with helper methods

### State Hierarchy

```
BrowserAppState (M12 browser state)
├── DesktopPageState (M11 page loading/rendering)
│   └── mocha_engine::Engine (M1–M10 document pipeline)
├── ChromeLayout (chrome positioning)
├── AddressBarState (address bar editing)
├── history: Vec<Url> (back/forward stack)
├── history_index: Option<usize>
└── focus: BrowserFocus (address bar or page)
```

All state is plain Rust structs with `impl` methods; no trait objects, no async,
fully testable. The window driver (`window.rs`) is a thin untestable layer that
calls `BrowserAppState` methods and pumps the result to the display.

### Event Flow

**Mouse click:**
1. `window.rs` captures click position
2. Calls `BrowserAppState::click(x, y)`
3. Chrome hit test (`ChromeLayout::hit_test`) determines if chrome or page
4. If chrome: address bar focus, button action (back/forward/reload)
5. If page: layout hit test, DOM event dispatch, form actions, link navigation
6. `BrowserAppState` updates internal state (page content, focus, address bar)

**Keyboard input:**
1. `window.rs` captures key
2. Calls `BrowserAppState::input_char(c)`, `backspace()`, `address_bar_submit()`, `escape()`
3. If `focus == AddressBar`: edit address bar text
4. If `focus == Page`: forward to page (form input, JS events)
5. `BrowserAppState` updates state and navigates if needed

**Rendering:**
1. `window.rs` calls `render_browser(surface, app)`
2. `mocha_raster::rasterize()` draws page content
3. `render_chrome()` draws buttons, toolbar, address bar on top
4. `minifb` updates the window with the pixel buffer

## Responsibilities

### BrowserAppState (Testable)

- Load a page from a URL or path
- Track navigation history (back/forward/reload)
- Route clicks to chrome or page
- Route keyboard input to address bar or page
- Manage focus (address bar vs. page)
- Expose display list and images to the rasterizer
- Check if back/forward buttons are enabled

### ChromeLayout (Testable)

- Compute rects for toolbar, buttons, address bar, page viewport
- Determine if a click point hits a button or address bar
- Provide draw-time rects for the rasterizer

### DesktopPageState (Testable)

- Load a document (file or HTTP)
- Execute the rendering pipeline (HTML, CSS, layout, paint)
- Track scroll position
- Perform layout hit testing
- Dispatch DOM events and navigate on link clicks

### window.rs (Untestable)

- Create/manage the `minifb` window
- Pump events from the OS
- Call `BrowserAppState` methods
- Call the rasterizer and window update

## Limitations

### No Persistent State

- Back/forward history is per-session (in-memory)
- No bookmarks or history database
- Closing the window loses all state

### No Mature Input

- Address bar has no caret, selection, copy/paste, or IME
- No tab completion or search suggestions
- No keyboard shortcuts (Ctrl+T, Ctrl+L, etc.)
- Page input (forms, links) uses the same simplified hit-test model

### No Error Handling

- Failed navigation (bad URL, network error) leaves the browser on the current page
- No error page or error message display
- No loading indicator or spinner

### Tabs but No Profiles

- **Tabs (M13):** multiple tabs with per-tab page/history/scroll/focus, a tab
  strip, and an in-memory session snapshot/restore. See
  [tabs-and-session.md](tabs-and-session.md).
- No **persistent** profile/storage/session management yet (M14); no tab
  drag/reorder, pinned tabs, tab groups, or crash recovery.

### No Window Features

- No window title from page title
- No favicon display
- No right-click context menu
- No fullscreen or zoom controls
- No OS integration (e.g. file picker, print)

### Rendering

- Chrome is simple rectangles and text (no gradients, shadows, or icons)
- Text is rendered with a debug bitmap font (no font loading or shaping)
- Images are rasterized with basic scaling (no antialiasing)
- No subpixel rendering or color management

## Testing

`BrowserAppState` is fully testable:

```bash
cargo test -p mocha_desktop
```

Tests verify:
- Loading a page initializes state
- Address bar focus/editing
- Navigation and history
- Chrome layout
- Click routing

Tests do NOT require:
- A window
- `minifb`
- GUI features
- Any external process

The desktop window is not tested in the unit test suite (it is untestable without
a display or automated screenshot comparison, which is not in scope).

## Files

- `crates/mocha_desktop/src/browser_app.rs` — `BrowserAppState`
- `crates/mocha_desktop/src/chrome.rs` — `ChromeLayout`
- `crates/mocha_desktop/src/address_bar.rs` — `AddressBarState`
- `crates/mocha_desktop/src/lib.rs` — `DesktopPageState`
- `crates/mocha_desktop/src/window.rs` — window event loop (M11/M12)

## Integration with Other Layers

**Upstream (M1–M10 document pipeline):**

`DesktopPageState` uses `mocha_engine::Engine` to load and render documents.
The engine produces a display list (same as `mocha_shell`), which flows to the
rasterizer.

**Downstream (rasterization):**

`mocha_raster::rasterize()` consumes the display list and writes to a `Surface`.
Chrome is drawn on the same surface after page rasterization.

**Terminal path:**

`mocha_shell` also uses `mocha_engine::Engine` and prints the display list as
text. It does not use `mocha_desktop` or `mocha_raster`.

## Tabs (M13, implemented)

`BrowserAppState` now owns a `TabManager` (a `Vec<BrowserTab>` + active-tab
invariant) instead of a single page:

- Each `BrowserTab` keeps its own page, navigation history, scroll, and focus.
- Tab strip rendering + hit testing (`ChromeElement::Tab`/`TabClose`/`NewTabButton`).
- New tab / close tab (right-then-left neighbour policy) / switch tab.
- An in-memory `SessionSnapshot`/`restore` (metadata only — **not** persisted).

See [tabs-and-session.md](tabs-and-session.md).

## Next Steps (M14)

Persistent profile storage: a profile directory, schema migrations, and
history/bookmarks/settings/downloads/session persistence on disk.
