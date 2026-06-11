//! The native window + event loop, driven by `minifb` (compiled only with the
//! `gui` feature).
//!
//! This is the deliberately thin, untestable layer: it owns the OS window and
//! pumps events into [`BrowserAppState`], which holds all the real logic. Each
//! frame it rasterizes the page and chrome (via `mocha_raster`) into a CPU buffer
//! and hands it to `minifb`. There is no GPU, no compositor; chrome is rendered
//! as simple rectangles.

use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};

use mocha_desktop::BrowserAppState;
use mocha_error::{MochaError, MochaResult};
use mocha_layout::Color;
use mocha_raster::Surface;

/// Pixels scrolled per mouse-wheel notch.
const SCROLL_STEP: f32 = 40.0;

/// Chrome colors.
const CHROME_BG: Color = Color {
    r: 220,
    g: 220,
    b: 220,
    a: 255,
};
const CHROME_BUTTON: Color = Color {
    r: 200,
    g: 200,
    b: 200,
    a: 255,
};
const CHROME_BUTTON_DISABLED: Color = Color {
    r: 230,
    g: 230,
    b: 230,
    a: 255,
};

/// Open a window showing `target`, pumping input until it is closed (or Escape).
pub fn run(target: &str, width: u32, height: u32) -> MochaResult<()> {
    let mut app = BrowserAppState::load(target, width, height)?;
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

    let mut surface = Surface::new(width, height);
    let mut last_size = (width, height);
    let mut mouse_was_down = false;

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

        // Left click on the press edge → route into the browser (chrome or page).
        let mouse_down = window.get_mouse_down(MouseButton::Left);
        if mouse_down && !mouse_was_down {
            if let Some((mx, my)) = window.get_mouse_pos(MouseMode::Clamp) {
                handle_click(&mut app, mx, my);
            }
        }
        mouse_was_down = mouse_down;

        // Keyboard → text input into the address bar or page.
        let shift = window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift);
        for key in window.get_keys_pressed(KeyRepeat::Yes) {
            match key {
                Key::Escape => app.escape(),
                Key::Backspace => {
                    let _ = app.backspace();
                }
                _ if let Some(c) = key_to_char(key, shift) => {
                    let _ = app.input_char(c);
                    // Special handling for Enter in address bar.
                    if c == '\n' {
                        let _ = app.address_bar_submit();
                    }
                }
                _ => {}
            }
        }

        render_browser(&mut surface, &app);
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

/// Render the browser: page + chrome.
fn render_browser(surface: &mut Surface, app: &BrowserAppState) {
    mocha_raster::rasterize(
        surface,
        app.display_list(),
        app.images(),
        app.scroll_y(),
    );

    render_chrome(surface, app);
}

/// Render chrome on top of the page.
fn render_chrome(surface: &mut Surface, app: &BrowserAppState) {
    let back_rect = app.chrome.back_button();
    let forward_rect = app.chrome.forward_button();
    let reload_rect = app.chrome.reload_button();
    let addr_rect = app.chrome.address_bar();

    let toolbar_color = CHROME_BG;
    let back_color = if app.can_go_back() {
        CHROME_BUTTON
    } else {
        CHROME_BUTTON_DISABLED
    };
    let forward_color = if app.can_go_forward() {
        CHROME_BUTTON
    } else {
        CHROME_BUTTON_DISABLED
    };

    let toolbar_height = (app.chrome.toolbar_height + app.chrome.address_bar_height) as i32;
    surface.draw_rect(0, 0, surface.width() as i32, toolbar_height, toolbar_color);

    surface.draw_rect(
        back_rect.x as i32,
        back_rect.y as i32,
        back_rect.width as i32,
        back_rect.height as i32,
        back_color,
    );
    surface.draw_rect_outline(
        back_rect.x as i32,
        back_rect.y as i32,
        back_rect.width as i32,
        back_rect.height as i32,
        1,
        Color { r: 100, g: 100, b: 100, a: 255 },
    );

    surface.draw_rect(
        forward_rect.x as i32,
        forward_rect.y as i32,
        forward_rect.width as i32,
        forward_rect.height as i32,
        forward_color,
    );
    surface.draw_rect_outline(
        forward_rect.x as i32,
        forward_rect.y as i32,
        forward_rect.width as i32,
        forward_rect.height as i32,
        1,
        Color { r: 100, g: 100, b: 100, a: 255 },
    );

    surface.draw_rect(
        reload_rect.x as i32,
        reload_rect.y as i32,
        reload_rect.width as i32,
        reload_rect.height as i32,
        CHROME_BUTTON,
    );
    surface.draw_rect_outline(
        reload_rect.x as i32,
        reload_rect.y as i32,
        reload_rect.width as i32,
        reload_rect.height as i32,
        1,
        Color { r: 100, g: 100, b: 100, a: 255 },
    );

    surface.draw_rect(
        addr_rect.x as i32,
        addr_rect.y as i32,
        addr_rect.width as i32,
        addr_rect.height as i32,
        Color { r: 255, g: 255, b: 255, a: 255 },
    );
    surface.draw_rect_outline(
        addr_rect.x as i32,
        addr_rect.y as i32,
        addr_rect.width as i32,
        addr_rect.height as i32,
        1,
        Color { r: 100, g: 100, b: 100, a: 255 },
    );

    let text_color = Color { r: 0, g: 0, b: 0, a: 255 };
    let addr_text = app.address_bar.draft_text.as_str();
    surface.draw_text_at(
        addr_text,
        addr_rect.x as i32 + 2,
        addr_rect.y as i32 + 2,
        1,
        text_color,
    );
}

/// Route a click; follow a resulting navigation by loading the new document.
fn handle_click(app: &mut BrowserAppState, x: f32, y: f32) {
    if let Err(error) = app.click(x, y) {
        eprintln!("mocha: click error: {error}");
    }
}

fn drain_console(app: &BrowserAppState) {
    for line in app.page.console_output() {
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
        Key::Enter => Some('\n'),
        _ => None,
    }
}
