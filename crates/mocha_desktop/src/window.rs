//! The native window + event loop, driven by `minifb` (compiled only with the
//! `gui` feature).
//!
//! This is the deliberately thin, untestable layer: it owns the OS window and
//! pumps events into [`DesktopPageState`], which holds all the real logic. Each
//! frame it rasterizes the page (via `mocha_raster`) into a CPU buffer and hands
//! it to `minifb`. There is no GPU, no compositor, and no browser chrome.

use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};

use mocha_desktop::{DesktopAction, DesktopPageState};
use mocha_error::{MochaError, MochaResult};
use mocha_raster::Surface;

/// Pixels scrolled per mouse-wheel notch.
const SCROLL_STEP: f32 = 40.0;

/// Open a window showing `target`, pumping input until it is closed (or Escape).
pub fn run(target: &str, width: u32, height: u32) -> MochaResult<()> {
    let mut state = DesktopPageState::load(target, width, height)?;
    drain_console(&state);

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

    let mut surface = Surface::new(width, height);
    let mut last_size = (width, height);
    let mut mouse_was_down = false;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        // Resize → re-render at the new viewport and rebuild the surface.
        let (w, h) = window.get_size();
        let (w, h) = (w as u32, h as u32);
        if (w, h) != last_size && w > 0 && h > 0 {
            last_size = (w, h);
            if let Err(error) = state.resize(w, h) {
                eprintln!("mocha: {error}");
            }
            surface = Surface::new(w, h);
        }

        // Mouse wheel → scroll (wheel up is positive; scrolling down advances).
        if let Some((_, scroll_y)) = window.get_scroll_wheel() {
            if scroll_y != 0.0 {
                state.scroll_by(-scroll_y * SCROLL_STEP);
            }
        }

        // Left click on the press edge → route into the page.
        let mouse_down = window.get_mouse_down(MouseButton::Left);
        if mouse_down && !mouse_was_down {
            if let Some((mx, my)) = window.get_mouse_pos(MouseMode::Clamp) {
                handle_click(&mut state, &mut surface, mx, my);
            }
        }
        mouse_was_down = mouse_down;

        // Keyboard → text input into the focused control.
        let shift = window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift);
        for key in window.get_keys_pressed(KeyRepeat::Yes) {
            if key == Key::Backspace {
                let _ = state.backspace();
            } else if let Some(c) = key_to_char(key, shift) {
                let _ = state.input_text(&c.to_string());
            }
        }

        mocha_raster::rasterize(
            &mut surface,
            state.display_list(),
            state.images(),
            state.scroll_y(),
        );
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
fn handle_click(state: &mut DesktopPageState, surface: &mut Surface, x: f32, y: f32) {
    match state.click(x, y) {
        Ok(DesktopAction::Navigate(url)) => match state.navigate(&url) {
            Ok(()) => {
                drain_console(state);
                let (w, h) = state.viewport();
                *surface = Surface::new(w, h);
            }
            Err(error) => eprintln!("mocha: {error}"),
        },
        Ok(_) => {}
        Err(error) => eprintln!("mocha: {error}"),
    }
}

fn drain_console(state: &DesktopPageState) {
    for line in state.console_output() {
        eprintln!("{line}");
    }
}

/// Map a `minifb` key to a printable character (basic: lowercase letters unless
/// Shift is held, digits, and space). Text input is intentionally crude.
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
        Key::Key0 => Some('0'),
        Key::Key1 => Some('1'),
        Key::Key2 => Some('2'),
        Key::Key3 => Some('3'),
        Key::Key4 => Some('4'),
        Key::Key5 => Some('5'),
        Key::Key6 => Some('6'),
        Key::Key7 => Some('7'),
        Key::Key8 => Some('8'),
        Key::Key9 => Some('9'),
        Key::Space => Some(' '),
        _ => None,
    }
}
