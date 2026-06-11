# Milestone 12: Browser Chrome

Milestone 12 adds minimal browser UI to the desktop shell: a toolbar with navigation buttons, an address bar, and a page viewport. The crate remains testable without opening a window.

## Architecture

### State Machine

All logic lives in [`BrowserAppState`](../../crates/mocha_desktop/src/browser_app.rs), a plain state machine:

```rust
pub struct BrowserAppState {
    pub page: DesktopPageState,          // The loaded page (M11)
    pub chrome: ChromeLayout,            // Chrome layout computation
    pub address_bar: AddressBarState,    // Address bar editing state
    pub history: Vec<Url>,               // Simple back/forward history
    pub history_index: Option<usize>,    // Current position in history
    pub focus: BrowserFocus,             // Address bar or page focus
}
```

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

The rasterizer (`mocha_raster`) does not yet draw chrome; chrome rendering is not implemented in M12. The display list from the page is clipped below the chrome offset during rasterization, but buttons/address/toolbar pixels are not drawn.

**This is a limitation documented below.**

## Limitations — M12 Does NOT Provide

- Actual rasterization of chrome UI (buttons, address bar visuals, toolbar background)
- Error page rendering for failed navigation
- Loading state indicator
- Page title display
- Tab support (see M13)
- Bookmarks, history database, settings
- HTTPS support (unsupported since M4; returns clear error)
- Cookies
- Search suggestions
- Keyboard shortcuts (Ctrl+T, Ctrl+L, etc.)
- Drag/reorder UI elements
- Full text field editing (IME, caret, selection)
- Form validation
- Button/address bar visual feedback on hover/press

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

## Next Steps (M13)

M13 builds on M12 by replacing single-page state with a tab manager:

- Multiple tabs, each with own page/scroll/history
- Tab strip UI
- New tab / close tab / switch tab
- Session snapshot and restore

## Current Honest Status

**What works:**
- Browser chrome state machine (testable)
- Address bar input/navigation
- Back/forward/reload logic
- Chrome layout computation
- Hit testing for buttons/address/page
- All existing M1-M11 features (display list, page rendering, forms, etc.)

**What doesn't work:**
- Chrome UI is not rasterized to the window (buttons/toolbar/address not drawn)
- No error page for failed navigation
- No page title in window or chrome
- No loading indicator
- Terminal shell unaffected (still works, no chrome)
