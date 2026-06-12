# Milestone 12: Browser Chrome

Milestone 12 adds minimal browser UI to the desktop shell: a toolbar with navigation buttons, an address bar, and a page viewport. The crate remains testable without opening a window.

## Architecture

### State Machine

All logic lives in [`BrowserAppState`](../../crates/mocha_desktop/src/browser_app.rs), a plain state machine:

As of M13, `BrowserAppState` owns a `TabManager` rather than a single page; the
per-tab page, history, and scroll moved into `BrowserTab`:

```rust
pub struct BrowserAppState {
    pub tabs: TabManager,                // Open tabs + active-tab invariant (M13)
    pub chrome: ChromeLayout,            // Chrome layout computation
    pub address_bar: AddressBarState,    // Address bar editing state (app-level draft)
    pub focus: BrowserFocus,             // Address bar or page focus
}
```

(In M12 this struct held a single `page`/`history`/`history_index` directly; see
[tabs-and-session.md](tabs-and-session.md) for the M13 tab model.)

The window driver (`window.rs`) is a thin untestable layer that calls `BrowserAppState` methods and hands the result to the rasterizer.

### Chrome Layout

Chrome is native/raster UI, not HTML/CSS:

- **Toolbar** (40px): Back/Forward/Reload buttons (28x28 each), 6px spacing
- **Address bar** (28px): Below toolbar, spans window width minus margins
- **Page viewport**: Below chrome, clipped during rasterization

Hit testing is separate from document hit testing (`ChromeLayout::hit_test`).

### Address Bar

The address bar can be focused, edited, and submitted:

```rust
pub struct AddressBarState {
    pub current_url: Option<Url>,   // Last navigated URL
    pub draft_text: String,         // Currently edited text
    pub focused: bool,              // Editing enabled
}
```

Behavior:

- Click address bar → focus and enable editing
- Type → append to draft
- Backspace → delete last char
- Enter → parse draft as URL and navigate
- Escape → blur and discard edits

Invalid URLs are not navigated to; the browser stays on the current page.

### Navigation

History is a simple stack with back/forward support:

```
history: [url1, url2, url3]
                  ↑
            history_index
```

- **Back**: Decrement index, reload page
- **Forward**: Increment index, reload page
- **Reload**: Reload current history entry
- **Navigate**: Truncate forward stack, append new URL

Button disable state:
- Back disabled when `history_index == 0` or None
- Forward disabled when `history_index >= history.len() - 1`

### Focus Model

Two focus contexts:

- **AddressBar**: Keyboard input edits the address bar
- **Page**: Keyboard input goes to the page (form controls, etc.)

Focus changes:
- Click address bar → AddressBar
- Click page → Page
- Escape while in AddressBar → blur and focus Page

### Rendering

Browser chrome is rasterized by `mocha_desktop::window::render_chrome` after the
page content is rasterized. Chrome is drawn as simple rectangles/text via
`Surface` (the same drawing surface used for the page). The chrome layer is drawn
on top of the page to avoid clipping the page's vertical space at the full
viewport height. The page viewport is positioned below the chrome for hit testing.

## Limitations — M12 Does NOT Provide

- Error page rendering for failed navigation
- Loading state indicator / spinner
- Page title display in window title or chrome
- Tab support (see M13)
- Bookmarks, history database, settings UI
- HTTPS support (unsupported since M4; returns clear error)
- Cookies
- Search / search suggestions / address autocomplete
- Keyboard shortcuts (Ctrl+T, Ctrl+L, Ctrl+R, Ctrl+W, etc.)
- Fullscreen / zoom controls
- Full address bar text field editing (IME, caret, selection, copy/paste)
- Form validation UI
- Favicon display
- Favicon caching

## Testing

`BrowserAppState` is fully testable without a window:

```bash
cargo test -p mocha_desktop
```

Tests verify:

- Chrome layout computes correct rects
- Address bar focus/editing/submit
- Back/forward/reload button behavior
- Navigation history stack
- Viewport clipping offset

## Files

- `crates/mocha_desktop/src/browser_app.rs`: Main state machine
- `crates/mocha_desktop/src/chrome.rs`: Layout and hit testing
- `crates/mocha_desktop/src/address_bar.rs`: Address bar state
- `crates/mocha_desktop/src/lib.rs`: DesktopPageState (M11) + new exports
- `crates/mocha_desktop/src/window.rs`: Window loop (unchanged from M11)

## M13 (implemented)

M13 replaced the single-page state with a tab manager:

- Multiple tabs, each with its own page/scroll/history/focus
- Tab strip UI (above the toolbar) + new-tab and per-tab close buttons
- New tab / close tab / switch tab
- In-memory session snapshot and restore (not persisted)

See [tabs-and-session.md](tabs-and-session.md). M14 and M15 later added profile
storage, cookies, and origin-aware web storage below the desktop shell.

## Current Honest Status

**What works:**
- Browser chrome state machine (fully testable without a window)
- Address bar input/editing/submit/navigation
- Back/forward/reload button logic and disable state
- Chrome layout computation and hit testing
- Chrome rasterization to the display surface (buttons, toolbar, address bar)
- Window event routing (clicks and keyboard) to chrome or page
- All existing M1-M11 features (display list, page rendering, forms, JS, etc.)
- Desktop window via `minifb` (with `gui` feature)
- Terminal shell still works (renders page only, no chrome)

**What doesn't work:**
- No error page for failed navigation
- No page title in window title
- No loading indicator / spinner
- No favicon display
- Address bar text field has no caret/selection/copy-paste/IME
- Tabs exist (M13), and session persistence exists in `mocha_storage` (M14), but
  the interactive shell does not yet auto-restore sessions
- Terminal mode has no chrome (intentional; address bar can't exist without a window)
