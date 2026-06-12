//! The `mocha_desktop` binary: a minimal desktop browser.
//!
//! ```bash
//! cargo run -p mocha_desktop --features gui -- examples/basic/index.html
//! cargo run -p mocha_desktop -- --dump-display-list examples/forms/basic-form.html
//! ```
//!
//! Loads a local file / `file://` / `http://` document and shows it in a native
//! window (with the `gui` feature). The window includes a toolbar with back/forward/reload
//! buttons, an address bar, and the page viewport. `https://` is unsupported.

use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use mocha_desktop::{DesktopPageState, SessionSnapshot, TabManager};
use mocha_error::{MochaError, MochaResult};
use mocha_storage::{Profile, StoredSession};

#[cfg(feature = "gui")]
mod window;

const DEFAULT_WIDTH: u32 = 800;
const DEFAULT_HEIGHT: u32 = 600;

const USAGE: &str = "usage: mocha_desktop [--width W] [--height H] [--dump-display-list]\n                     [--profile DIR] [--dump-session] <path-or-url>\n       (file paths, file:// and http:// URLs; https:// is not implemented)\n       --profile DIR opens a persistent profile (history + session persistence)\n       --dump-session loads <path>, records history, saves + prints the session\n       the visible window needs the `gui` feature: cargo run -p mocha_desktop --features gui -- <path>";

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
    let mut dump_session = false;
    let mut profile_dir: Option<String> = None;
    let mut target: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--width" => width = parse_dimension(&mut args, "--width")?,
            "--height" => height = parse_dimension(&mut args, "--height")?,
            "--dump-display-list" => dump_display_list = true,
            "--dump-session" => dump_session = true,
            "--profile" => {
                profile_dir = Some(args.next().ok_or_else(|| {
                    MochaError::Shell(format!("--profile needs a directory\n{USAGE}"))
                })?)
            }
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

    if dump_session {
        // Headless: load into a tab, record history + save the session in a
        // persistent profile, then print the session loaded back from storage.
        let dir = profile_dir.ok_or_else(|| {
            MochaError::Shell(format!("--dump-session requires --profile DIR\n{USAGE}"))
        })?;
        return run_dump_session(&dir, &target, width, height);
    }

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

/// The current wall-clock time in epoch milliseconds (for CLI use; the storage
/// stores are otherwise driven by caller-supplied timestamps).
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Minimal Milestone 14 desktop integration: open a persistent profile, load the
/// target into a one-tab session, record the visit in history, save the session,
/// and print the session read back from storage.
fn run_dump_session(profile_dir: &str, target: &str, width: u32, height: u32) -> MochaResult<()> {
    let profile = Profile::persistent(profile_dir)?;
    let manager = TabManager::with_loaded(target, width, height)?;

    // Record the active tab's visit in history.
    if let Some(url) = manager.active().url() {
        profile.history().record_visit(url, None, now_ms())?;
    }

    // Save the in-memory session snapshot to the profile.
    let snapshot = manager.snapshot();
    let stored: StoredSession = (&snapshot).into();
    profile.session().save_session(&stored)?;

    // Read it back and print it.
    let loaded = profile.session().load_session()?;
    print_session(loaded.map(SessionSnapshot::from));
    Ok(())
}

fn print_session(session: Option<SessionSnapshot>) {
    match session {
        None => println!("session: (none)"),
        Some(s) => {
            println!(
                "session: {} tab(s), active={}",
                s.tabs.len(),
                s.active_tab_index
            );
            for (i, tab) in s.tabs.iter().enumerate() {
                println!(
                    "  [{i}] url={:?} title={:?} scroll={} history={} index={:?}",
                    tab.url,
                    tab.title,
                    tab.scroll_y,
                    tab.history.len(),
                    tab.current_history_index,
                );
            }
        }
    }
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
