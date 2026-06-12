# Tabs and Session Model (Milestone 13)

Milestone 13 turned the single-page desktop shell into a **multi-tab** browser
and added an **in-memory session snapshot/restore** model. Everything lives in
`mocha_desktop`; no new crate was needed. Milestone 14 later added persistence
for the same snapshot shape through `mocha_storage`.

## Concepts

| Type | Role |
| --- | --- |
| `TabId(u64)` | Stable, unique tab identifier. Never reused; survivors are not renumbered when a tab closes. |
| `BrowserTab` | One tab: its own page, title, URL, navigation history, scroll, and focus. |
| `TabManager` | Owns the tab list and the active-tab invariant. |
| `BrowserAppState` | The whole browser: a `TabManager` + chrome layout + (app-level) address bar + focus. |
| `BrowserAction` | A high-level command (`Navigate`/`Back`/`Forward`/`Reload`/`NewTab`/`CloseTab`/`SwitchTab`). |
| `SessionSnapshot` / `SessionTab` | A lightweight, metadata-only capture of all tabs (no DOM/layout). |

## Per-tab state

Each `BrowserTab` owns an independent `DesktopPageState` (the "browser page":
document, form state, display list, images, **scroll**, and **focus**) plus its
**own navigation history** (`Vec<Url>` + current index). Nothing page-related is
global: scrolling tab A does not move tab B, form edits in A do not touch B, and
back/forward in A do not affect B. The only shared chrome state is the address
bar (below).

`title` is derived from the URL (last path segment, else host); the internal
new-tab page uses the title `New Tab`. There is **no `<title>` element parsing**
(the HTML subset has no `<head>`/`<title>`), so titles are URL-derived.

## TabManager

Invariants the manager guarantees:

- There is **always at least one tab**.
- `active_id()` always names an existing tab; `active()`/`active_mut()` never panic.
- Tab **order is preserved**; ids are **unique and stable** across closes.

Behavior:

- `new(w, h)` starts with one blank **new-tab page**; `with_loaded(input, w, h)`
  starts with one loaded tab (used at launch).
- `new_tab()` creates a blank tab and **activates** it; `open_in_new_tab(input)`
  loads a URL in a new active tab.
- `switch_tab(id)` activates an existing tab; an unknown id is a clear
  `Navigation` error.
- `close_tab(id)` removes a tab. **Close policy:** closing the active tab
  activates its **right** neighbour, else its **left** neighbour; closing the
  **last** tab opens a fresh blank tab.
- `navigate_active`/`back_active`/`forward_active`/`reload_active` affect the
  **active tab only**.

## New-tab page

`InternalPage::NewTab` renders a fixed HTML string through the normal pipeline as
an **in-memory document** (no base URL), so it **never hits the network**. It uses
only Mocha's supported subset (`doctype`, `html`, `body`, `h1`, `p`):

```html
<!doctype html>
<html><body>
  <h1>Mocha Browser</h1>
  <p>Enter a local path or http:// URL in the address bar.</p>
</body></html>
```

A new tab has title `New Tab`, no external URL, and an empty address bar.

## Tab strip layout and hit testing

The chrome stacks vertically (top → bottom): **tab strip → toolbar
(back/forward/reload) → address bar → page viewport**. Metrics
(`ChromeLayout`): `tab_strip_height = 32`, `tab_min_width = 120`,
`tab_max_width = 180`, `new_tab_button_width = 32`, `tab_close_button_width = 20`.
Tab width is `(window_width - new_tab_button_width) / count`, clamped to
`[120, 180]`. The page viewport begins at
`total_chrome_height = tab_strip + toolbar + address_bar`.

`ChromeLayout::hit_test(x, y, &[TabId])` resolves a click to a `ChromeElement`:
`Tab(id)`, `TabClose(id)` (tested before the tab body, since it sits inside it),
`NewTabButton`, the toolbar buttons, `AddressBar`, or `PageViewport`. The tab-id
slice supplies count + ids so geometry maps back to a `TabId`.

The active tab is drawn distinctly (white vs. grey), each tab shows its title and
a close mark, and the `+` button follows the last tab. The page is rasterized
with `mocha_raster::rasterize_at(..., top_offset = total_chrome_height)` so the
document sits in its real viewport region below the chrome (and click mapping
`page_y = window_y - viewport.y` lines up with what is drawn).

## Address bar with tabs

The address-bar **draft** belongs to the app chrome, not to each tab. Switching
or navigating tabs calls `sync_address_bar()`, which sets the bar to the active
tab's URL and cancels any in-progress edit (returning focus to the page). Typing
edits the draft; Enter navigates the active tab; Escape restores the active URL.
An edit in the bar never mutates an inactive tab.

## In-memory session snapshot

`TabManager::snapshot()` captures a `SessionSnapshot { tabs, active_tab_index }`
where each `SessionTab` holds only `url`, `title`, `scroll_y`, `history`
(normalized URL strings), and `current_history_index`. **No DOM, form state,
layout tree, or display list is captured** — the snapshot is cheap to copy and
persist through the M14 storage DTOs.

`TabManager::restore(&snapshot, w, h)` rebuilds the manager. **Restore policy:**

- Each tab is recreated as an **unloaded metadata tab** backed by the internal
  new-tab placeholder page.
- The **active** tab is reloaded **eagerly**.
- **Inactive** tabs are reloaded **lazily** the first time they are activated
  (`switch_tab`), reapplying the saved scroll offset.
- Tabs whose URL is `None` stay on the new-tab page.
- An empty snapshot degrades to a single fresh tab.

This keeps restore cheap and avoids serializing heavy page state.

## Limitations

- M13 itself was in-memory only; M14 added persistent session DTOs and stores,
  but the interactive shell still does not auto-restore sessions by default.
- Bookmarks/history/downloads/settings exist in `mocha_storage`, but the
  interactive desktop UI does not yet surface them.
- Cookies and origin-keyed localStorage exist at the M15 storage layer, but tab
  loads are not automatically profile-cookie-backed and JS storage is not yet
  tab/profile-backed.
- No tab **drag/reorder**, pinned tabs, or tab groups.
- No private browsing, crash recovery, or multiprocess isolation.
- Titles are URL-derived (no `<title>` parsing); `is_loading` is effectively
  always `false` because loads are **synchronous/blocking**.
