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

use mocha_desktop::render::ChromeInput;
use mocha_desktop::{render_browser, BrowserAppState, BrowserTheme, Fonts};
use mocha_error::{MochaError, MochaResult};
use mocha_raster::Surface;

/// Pixels scrolled per mouse-wheel notch.
const SCROLL_STEP: f32 = 40.0;

/// Address-bar caret blink half-period (milliseconds on, then off).
const CARET_BLINK_MS: u128 = 530;

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
    let blink_start = Instant::now();

    while window.is_open() && !window.is_key_down(Key::Escape) {
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

        // Left click on the press edge → route into the browser (chrome or page).
        if mouse_down && !mouse_was_down {
            if let Some((mx, my)) = mouse_pos {
                handle_click(&mut app, mx, my);
            }
        }
        mouse_was_down = mouse_down;

        // Keyboard → text input into the address bar or page.
        let shift = window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift);
        for key in window.get_keys_pressed(KeyRepeat::Yes) {
            match key {
                Key::Escape => app.escape(),
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

        let input = ChromeInput {
            hover: mouse_pos.and_then(|(mx, my)| app.chrome.hit_test(mx, my, &app.tabs.tab_ids())),
            mouse_down,
            caret_visible: (blink_start.elapsed().as_millis() / CARET_BLINK_MS).is_multiple_of(2),
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
    Ok(())
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
