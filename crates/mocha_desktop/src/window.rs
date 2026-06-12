//! The native window + event loop, driven by `minifb` (compiled only with the
//! `gui` feature).
//!
//! This is the deliberately thin, untestable layer: it owns the OS window and
//! pumps events into [`BrowserAppState`], which holds all the real logic. Each
//! frame it asks the (headlessly testable) [`render_browser`] to rasterize the
//! page and chrome into a CPU buffer and hands it to `minifb`. There is no GPU
//! and no compositor; the chrome is drawn with system fonts, vector icons, and
//! the [`BrowserTheme`] palette.

use std::time::Instant;

use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};

use mocha_desktop::anim::{Easing, Tween};
use mocha_desktop::browser_app::BrowserAction;
use mocha_desktop::chrome::ChromeElement;
use mocha_desktop::render::ChromeInput;
use mocha_desktop::tab::TabId;
use mocha_desktop::{render_browser, BrowserAppState, BrowserTheme, Fonts};
use mocha_error::{MochaError, MochaResult};
use mocha_raster::Surface;

/// Pixels scrolled per mouse-wheel notch.
const SCROLL_STEP: f32 = 40.0;

/// Address-bar caret blink half-period (milliseconds on, then off).
const CARET_BLINK_MS: u128 = 530;

/// Horizontal pixels the pointer must travel before a tab press becomes a drag.
const DRAG_THRESHOLD: f32 = 8.0;

/// Open a window showing the prepared browser state, pumping input until the
/// window is closed (or Escape). The caller decides what the first tab shows:
/// a loaded document, the home page, or an internal error page.
pub fn run(mut app: BrowserAppState, width: u32, height: u32) -> MochaResult<()> {
    drain_console(&app);

    let mut window = Window::new(
        "Mocha Browser",
        width as usize,
        height as usize,
        WindowOptions {
            resize: true,
            ..WindowOptions::default()
        },
    )
    .map_err(|error| MochaError::Shell(format!("could not open a window: {error}")))?;
    window.set_target_fps(60);

    let theme = BrowserTheme::default();
    let mut fonts = Fonts::load();
    let mut surface = Surface::new(width, height);
    let mut last_size = (width, height);
    let mut mouse_was_down = false;
    let mut press_origin: Option<(f32, f32)> = None;
    let mut drag_tab: Option<TabId> = None;
    let clock = Instant::now();

    // Animation + change-tracking state.
    let mut last_url = active_url(&app);
    let mut nav_tween: Option<Tween> = None;
    let mut last_tab_count = app.tabs.len();
    let mut tab_tween: Option<Tween> = None;
    let mut escape_was_down = false;

    while window.is_open() {
        let now = clock.elapsed().as_millis();

        // Resize → re-render at the new viewport and rebuild the surface.
        let (w, h) = window.get_size();
        let (w, h) = (w as u32, h as u32);
        if (w, h) != last_size && w > 0 && h > 0 {
            last_size = (w, h);
            if let Err(error) = app.resize(w, h) {
                eprintln!("mocha: {error}");
            }
            surface = Surface::new(w, h);
        }

        // Mouse wheel → scroll (wheel up is positive; scrolling down advances).
        if let Some((_, scroll_y)) = window.get_scroll_wheel() {
            if scroll_y != 0.0 {
                app.scroll(-scroll_y * SCROLL_STEP);
            }
        }

        // Pointer position drives chrome hover styling and click routing.
        let mouse_pos = window.get_mouse_pos(MouseMode::Clamp);
        let mouse_down = window.get_mouse_down(MouseButton::Left);

        if mouse_down && !mouse_was_down {
            // Press edge → click + remember the origin for drag detection.
            if let Some((mx, my)) = mouse_pos {
                press_origin = Some((mx, my));
                handle_click(&mut app, mx, my);
                if let Some(ChromeElement::Tab(id)) =
                    app.chrome.hit_test(mx, my, &app.tabs.tab_ids())
                {
                    drag_tab = Some(id);
                }
            }
        } else if mouse_down {
            // Hold → drag-to-reorder once past the threshold.
            if let (Some(id), Some((mx, _)), Some((ox, _))) = (drag_tab, mouse_pos, press_origin) {
                if (mx - ox).abs() > DRAG_THRESHOLD {
                    reorder_drag(&mut app, id, mx);
                }
            }
        } else {
            if drag_tab.take().is_some() {
                app.persist_session();
            }
            press_origin = None;
        }
        mouse_was_down = mouse_down;

        // Keyboard → shortcuts first, then text into the address bar or page.
        let shift = down(&window, Key::LeftShift, Key::RightShift);
        let ctrl = down(&window, Key::LeftCtrl, Key::RightCtrl);
        let alt = down(&window, Key::LeftAlt, Key::RightAlt);
        // Escape on its own edge (closes menu/suggestions/edit; never quits).
        let escape_down = window.is_key_down(Key::Escape);
        if escape_down && !escape_was_down {
            app.escape();
        }
        escape_was_down = escape_down;
        for key in window.get_keys_pressed(KeyRepeat::Yes) {
            if (ctrl || alt) && handle_shortcut(&mut app, key, shift, ctrl, alt) {
                continue;
            }
            match key {
                Key::Enter => {
                    if let Err(error) = app.address_bar_submit() {
                        eprintln!("mocha: {error}");
                    }
                }
                Key::Backspace => {
                    let _ = app.backspace();
                }
                _ if let Some(c) = key_to_char(key, shift) => {
                    let _ = app.input_char(c);
                }
                _ => {}
            }
        }

        // Start a loading-progress sweep whenever the active URL changes, and a
        // tab-strip settle whenever the tab count changes.
        let url_now = active_url(&app);
        if url_now != last_url {
            last_url = url_now;
            nav_tween = Some(Tween::new(now, 0.0, 1.0, 360, Easing::EaseOut));
            drain_console(&app);
        }
        if app.tabs.len() != last_tab_count {
            last_tab_count = app.tabs.len();
            tab_tween = Some(Tween::new(now, 0.0, 1.0, 180, Easing::EaseOut));
        }
        let progress = nav_tween.and_then(|t| (!t.is_done(now)).then(|| t.value(now)));
        if nav_tween.is_some_and(|t| t.is_done(now)) {
            nav_tween = None;
        }
        let tab_anim = tab_tween.map_or(1.0, |t| t.value(now));
        if tab_tween.is_some_and(|t| t.is_done(now)) {
            tab_tween = None;
        }

        let input = ChromeInput {
            hover: mouse_pos.and_then(|(mx, my)| app.chrome.hit_test(mx, my, &app.tabs.tab_ids())),
            mouse_down,
            caret_visible: (now / CARET_BLINK_MS).is_multiple_of(2),
            progress,
            tab_anim,
        };

        render_browser(&mut surface, &app, &mut fonts, &theme, input);
        window
            .update_with_buffer(
                surface.buffer(),
                surface.width() as usize,
                surface.height() as usize,
            )
            .map_err(|error| MochaError::Shell(format!("window update failed: {error}")))?;
    }
    app.persist_session();
    Ok(())
}

/// The active tab's URL string (for change detection), or "" for native pages.
fn active_url(app: &BrowserAppState) -> String {
    app.tabs
        .active()
        .url()
        .map(|u| u.normalized())
        .unwrap_or_default()
}

/// Whether either of two keys is held.
fn down(window: &Window, a: Key, b: Key) -> bool {
    window.is_key_down(a) || window.is_key_down(b)
}

/// Reorder the dragged tab so it follows the pointer along the strip.
fn reorder_drag(app: &mut BrowserAppState, id: TabId, mouse_x: f32) {
    let count = app.tabs.len();
    let Some(from) = app.tabs.index_of_id(id) else {
        return;
    };
    let mut target = from;
    for i in 0..count {
        let rect = app.chrome.tab_rect(i, count);
        if mouse_x >= rect.x && mouse_x < rect.x + rect.width {
            target = i;
            break;
        }
        if mouse_x >= rect.x + rect.width {
            target = i;
        }
    }
    if target != from {
        app.tabs.move_tab(from, target);
    }
}

/// Dispatch a Ctrl/Alt keyboard shortcut. Returns whether it was handled.
fn handle_shortcut(
    app: &mut BrowserAppState,
    key: Key,
    shift: bool,
    ctrl: bool,
    alt: bool,
) -> bool {
    let run = |app: &mut BrowserAppState, action: BrowserAction| {
        if let Err(error) = app.dispatch(action) {
            eprintln!("mocha: {error}");
        }
    };
    if alt && !ctrl {
        match key {
            Key::Left => run(app, BrowserAction::Back),
            Key::Right => run(app, BrowserAction::Forward),
            _ => return false,
        }
        return true;
    }
    if !ctrl {
        return false;
    }
    match key {
        Key::T => run(app, BrowserAction::NewTab),
        Key::W => {
            let id = app.tabs.active_id();
            run(app, BrowserAction::CloseTab(id));
        }
        Key::R => run(app, BrowserAction::Reload),
        Key::L => app.focus_address_bar(),
        Key::D => run(app, BrowserAction::ToggleBookmark),
        Key::H => run(app, BrowserAction::ShowHistory),
        Key::J => run(app, BrowserAction::ShowDownloads),
        Key::B => run(app, BrowserAction::ToggleBookmarksBar),
        Key::Comma => run(app, BrowserAction::ShowSettings),
        Key::Tab => switch_relative(app, if shift { -1 } else { 1 }),
        Key::Key1 => switch_index(app, 0),
        Key::Key2 => switch_index(app, 1),
        Key::Key3 => switch_index(app, 2),
        Key::Key4 => switch_index(app, 3),
        Key::Key5 => switch_index(app, 4),
        Key::Key6 => switch_index(app, 5),
        Key::Key7 => switch_index(app, 6),
        Key::Key8 => switch_index(app, 7),
        Key::Key9 => switch_index(app, app.tabs.len().saturating_sub(1)),
        _ => return false,
    }
    true
}

fn switch_index(app: &mut BrowserAppState, index: usize) {
    let ids = app.tabs.tab_ids();
    if let Some(&id) = ids.get(index) {
        let _ = app.dispatch(BrowserAction::SwitchTab(id));
    }
}

fn switch_relative(app: &mut BrowserAppState, delta: isize) {
    let ids = app.tabs.tab_ids();
    if ids.is_empty() {
        return;
    }
    let cur = ids
        .iter()
        .position(|&id| id == app.tabs.active_id())
        .unwrap_or(0) as isize;
    let next = (cur + delta).rem_euclid(ids.len() as isize) as usize;
    let _ = app.dispatch(BrowserAction::SwitchTab(ids[next]));
}

/// Route a click; follow a resulting navigation by loading the new document.
fn handle_click(app: &mut BrowserAppState, x: f32, y: f32) {
    if let Err(error) = app.click(x, y) {
        eprintln!("mocha: click error: {error}");
    }
}

fn drain_console(app: &BrowserAppState) {
    for line in app.active_page().console_output() {
        eprintln!("{line}");
    }
}

/// Map a `minifb` key to a printable character (basic: lowercase letters unless
/// Shift is held, digits, space, and the common URL/search punctuation). Text
/// input is intentionally crude.
fn key_to_char(key: Key, shift: bool) -> Option<char> {
    let letter = |lower: char, upper: char| Some(if shift { upper } else { lower });
    match key {
        Key::A => letter('a', 'A'),
        Key::B => letter('b', 'B'),
        Key::C => letter('c', 'C'),
        Key::D => letter('d', 'D'),
        Key::E => letter('e', 'E'),
        Key::F => letter('f', 'F'),
        Key::G => letter('g', 'G'),
        Key::H => letter('h', 'H'),
        Key::I => letter('i', 'I'),
        Key::J => letter('j', 'J'),
        Key::K => letter('k', 'K'),
        Key::L => letter('l', 'L'),
        Key::M => letter('m', 'M'),
        Key::N => letter('n', 'N'),
        Key::O => letter('o', 'O'),
        Key::P => letter('p', 'P'),
        Key::Q => letter('q', 'Q'),
        Key::R => letter('r', 'R'),
        Key::S => letter('s', 'S'),
        Key::T => letter('t', 'T'),
        Key::U => letter('u', 'U'),
        Key::V => letter('v', 'V'),
        Key::W => letter('w', 'W'),
        Key::X => letter('x', 'X'),
        Key::Y => letter('y', 'Y'),
        Key::Z => letter('z', 'Z'),
        Key::Key0 => Some(if shift { ')' } else { '0' }),
        Key::Key1 => Some(if shift { '!' } else { '1' }),
        Key::Key2 => Some(if shift { '@' } else { '2' }),
        Key::Key3 => Some(if shift { '#' } else { '3' }),
        Key::Key4 => Some(if shift { '$' } else { '4' }),
        Key::Key5 => Some(if shift { '%' } else { '5' }),
        Key::Key6 => Some(if shift { '^' } else { '6' }),
        Key::Key7 => Some(if shift { '&' } else { '7' }),
        Key::Key8 => Some(if shift { '*' } else { '8' }),
        Key::Key9 => Some(if shift { '(' } else { '9' }),
        Key::Space => Some(' '),
        Key::Minus => Some(if shift { '_' } else { '-' }),
        Key::Equal => Some(if shift { '+' } else { '=' }),
        Key::Slash => Some(if shift { '?' } else { '/' }),
        Key::Period => Some(if shift { '>' } else { '.' }),
        Key::Comma => Some(if shift { '<' } else { ',' }),
        Key::Semicolon => Some(if shift { ':' } else { ';' }),
        _ => None,
    }
}
