//! The `mocha_desktop` binary: a minimal desktop browser (packaged for
//! Windows release as `Mocha.exe`; see `docs/release/windows-exe.md`).
//!
//! ```bash
//! cargo run -p mocha_desktop --features gui                                  # home page
//! cargo run -p mocha_desktop --features gui -- examples/basic/index.html
//! cargo run -p mocha_desktop -- --dump-display-list examples/forms/basic-form.html
//! ```
//!
//! With no argument the window opens on the internal home/new-tab page. With a
//! local file / `file://` / `http://` / `https://` argument it loads that
//! document; if the load fails, the window opens on an internal error page
//! instead of exiting. The window (behind the `gui` feature) includes a
//! toolbar with back/forward/reload buttons, an address bar, and the page
//! viewport.

#[cfg(feature = "gui")]
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use mocha_desktop::{DesktopPageState, SessionSnapshot, TabManager};
use mocha_error::{MochaError, MochaResult};
use mocha_storage::{Profile, StoredSession};

#[cfg(feature = "gui")]
mod window;

const DEFAULT_WIDTH: u32 = 800;
const DEFAULT_HEIGHT: u32 = 600;

const USAGE: &str = "usage: mocha_desktop [--width W] [--height H] [--dump-display-list]\n                     [--profile DIR] [--dump-session] [path-or-url]\n       (file paths, file://, http://, and https:// URLs; with no argument the\n        window opens on the internal home page)\n       --profile DIR uses DIR as the profile directory (default:\n                     %APPDATA%\\MochaBrowser\\profile, or .\\profile without APPDATA)\n       --dump-session loads <path>, records history, saves + prints the session\n                      (requires --profile DIR and a path)\n       the visible window needs the `gui` feature: cargo run -p mocha_desktop --features gui";

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

    if dump_session {
        // Headless: load into a tab, record history + save the session in a
        // persistent profile, then print the session loaded back from storage.
        let dir = profile_dir.ok_or_else(|| {
            MochaError::Shell(format!("--dump-session requires --profile DIR\n{USAGE}"))
        })?;
        let target = target.ok_or_else(|| {
            MochaError::Shell(format!("--dump-session needs a path or URL\n{USAGE}"))
        })?;
        return run_dump_session(&dir, &target, width, height);
    }

    if dump_display_list {
        // Headless: render and print the display list (works without `gui`).
        let target = target.ok_or_else(|| {
            MochaError::Shell(format!("--dump-display-list needs a path or URL\n{USAGE}"))
        })?;
        let state = DesktopPageState::load(&target, width, height)?;
        for line in state.console_output() {
            eprintln!("{line}");
        }
        println!("{}", mocha_paint::format_display_list(state.display_list()));
        return Ok(());
    }

    run_window(target.as_deref(), profile_dir.as_deref(), width, height)
}

/// Prepare the app-data directories (profile + logs) and open the persistent
/// profile store. Best-effort by design: when Mocha runs as a desktop app, a
/// failure here (for example a read-only location) must not stop the browser
/// from opening, so problems are reported as warnings and the browser falls
/// back to a profile-less (no-persistence) session.
#[cfg(feature = "gui")]
fn prepare_app_data(profile_flag: Option<&str>) -> mocha_desktop::BrowserProfile {
    let profile_dir = profile_flag
        .map(PathBuf::from)
        .unwrap_or_else(mocha_desktop::default_profile_dir);
    let logs_dir = mocha_desktop::default_logs_dir();
    for dir in [&profile_dir, &logs_dir] {
        if let Err(error) = std::fs::create_dir_all(dir) {
            eprintln!("mocha: warning: cannot create {}: {error}", dir.display());
        }
    }
    match Profile::persistent(&profile_dir) {
        Ok(profile) => mocha_desktop::BrowserProfile::new(profile),
        Err(error) => {
            eprintln!(
                "mocha: warning: cannot open the profile at {} ({error}); \
                 history, bookmarks and tabs will not be saved",
                profile_dir.display()
            );
            mocha_desktop::BrowserProfile::none()
        }
    }
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
fn run_window(
    target: Option<&str>,
    profile_dir: Option<&str>,
    width: u32,
    height: u32,
) -> MochaResult<()> {
    use mocha_desktop::BrowserAppState;

    let profile = prepare_app_data(profile_dir);
    // Restores the previous session (when enabled) or loads `target`; a failed
    // load opens the browser on an internal error page rather than exiting.
    let app = BrowserAppState::launch(profile, target, width, height)?;
    window::run(app, width, height)
}

#[cfg(not(feature = "gui"))]
fn run_window(
    _target: Option<&str>,
    _profile_dir: Option<&str>,
    _width: u32,
    _height: u32,
) -> MochaResult<()> {
    Err(MochaError::Shell(
        "the desktop window needs the `gui` feature; rebuild with \
         `cargo run -p mocha_desktop --features gui -- [path]` \
         (or use --dump-display-list for headless output)"
            .to_string(),
    ))
}
