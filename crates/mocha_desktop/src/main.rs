//! The `mocha_desktop` binary: a minimal desktop document viewer.
//!
//! ```bash
//! cargo run -p mocha_desktop --features gui -- examples/desktop/interactive-form.html
//! cargo run -p mocha_desktop -- --dump-display-list examples/forms/basic-form.html
//! ```
//!
//! Loads a local file / `file://` / `http://` document and shows it in a native
//! window (with the `gui` feature) where it can be scrolled and clicked.
//! Without `gui`, only `--dump-display-list` works (headless). There is **no**
//! address bar, tabs, or browser chrome. `https://` is unsupported.

use std::process::ExitCode;

use mocha_desktop::DesktopPageState;
use mocha_error::{MochaError, MochaResult};

#[cfg(feature = "gui")]
mod window;

const DEFAULT_WIDTH: u32 = 800;
const DEFAULT_HEIGHT: u32 = 600;

const USAGE: &str = "usage: mocha_desktop [--width W] [--height H] [--dump-display-list] <path-or-url>\n       (file paths, file:// and http:// URLs; https:// is not implemented)\n       the visible window needs the `gui` feature: cargo run -p mocha_desktop --features gui -- <path>";

fn main() -> ExitCode {
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mocha: {error}");
            ExitCode::FAILURE
        }
    }
}

fn real_main() -> MochaResult<()> {
    let mut width = DEFAULT_WIDTH;
    let mut height = DEFAULT_HEIGHT;
    let mut dump_display_list = false;
    let mut target: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--width" => width = parse_dimension(&mut args, "--width")?,
            "--height" => height = parse_dimension(&mut args, "--height")?,
            "--dump-display-list" => dump_display_list = true,
            flag if flag.starts_with("--") => {
                return Err(MochaError::Shell(format!("unknown flag '{flag}'\n{USAGE}")));
            }
            _ if target.is_none() => target = Some(arg),
            _ => {
                return Err(MochaError::Shell(format!(
                    "expected one path or URL argument\n{USAGE}"
                )))
            }
        }
    }

    let target = target.ok_or_else(|| MochaError::Shell(USAGE.to_string()))?;

    if dump_display_list {
        // Headless: render and print the display list (works without `gui`).
        let state = DesktopPageState::load(&target, width, height)?;
        for line in state.console_output() {
            eprintln!("{line}");
        }
        println!("{}", mocha_paint::format_display_list(state.display_list()));
        return Ok(());
    }

    run_window(&target, width, height)
}

fn parse_dimension(args: &mut impl Iterator<Item = String>, flag: &str) -> MochaResult<u32> {
    let value = args
        .next()
        .ok_or_else(|| MochaError::Shell(format!("{flag} needs a number\n{USAGE}")))?;
    value
        .parse::<u32>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| MochaError::Shell(format!("invalid {flag} value: {value:?}")))
}

#[cfg(feature = "gui")]
fn run_window(target: &str, width: u32, height: u32) -> MochaResult<()> {
    window::run(target, width, height)
}

#[cfg(not(feature = "gui"))]
fn run_window(_target: &str, _width: u32, _height: u32) -> MochaResult<()> {
    Err(MochaError::Shell(
        "the desktop window needs the `gui` feature; rebuild with \
         `cargo run -p mocha_desktop --features gui -- <path>` \
         (or use --dump-display-list for headless output)"
            .to_string(),
    ))
}
