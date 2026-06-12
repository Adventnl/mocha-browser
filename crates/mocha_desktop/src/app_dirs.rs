//! Default per-user application data directories for the desktop app.
//!
//! When Mocha runs as a packaged Windows app (`Mocha.exe`), it must not write
//! into the repository or the current directory by default. The profile and
//! logs directories live under `%APPDATA%\MochaBrowser`; if `APPDATA` is
//! unset (or empty), they fall back to `.\profile` / `.\logs` relative to the
//! working directory — the packaged `dist\MochaBrowser` folder ships those
//! directories for exactly that case. `--profile <dir>` always wins over the
//! default.

use std::ffi::OsString;
use std::path::PathBuf;

/// The root directory for Mocha's per-user application data
/// (`%APPDATA%\MochaBrowser`, or `.` when `APPDATA` is unavailable).
pub fn default_app_data_root() -> PathBuf {
    app_data_root_from(std::env::var_os("APPDATA"))
}

/// The default profile directory (`<root>/profile`).
pub fn default_profile_dir() -> PathBuf {
    default_app_data_root().join("profile")
}

/// The default logs directory (`<root>/logs`). Nothing writes log files yet;
/// the directory exists so future logging has a stable, documented home.
pub fn default_logs_dir() -> PathBuf {
    default_app_data_root().join("logs")
}

/// Pure core of [`default_app_data_root`], testable without mutating the
/// process environment.
fn app_data_root_from(appdata: Option<OsString>) -> PathBuf {
    match appdata {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir).join("MochaBrowser"),
        _ => PathBuf::from("."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn appdata_set_uses_mocha_browser_subdirectory() {
        let root = app_data_root_from(Some(OsString::from(r"C:\Users\test\AppData\Roaming")));
        assert_eq!(
            root,
            Path::new(r"C:\Users\test\AppData\Roaming").join("MochaBrowser")
        );
    }

    #[test]
    fn appdata_missing_falls_back_to_working_directory() {
        assert_eq!(app_data_root_from(None), PathBuf::from("."));
    }

    #[test]
    fn appdata_empty_falls_back_to_working_directory() {
        assert_eq!(
            app_data_root_from(Some(OsString::new())),
            PathBuf::from(".")
        );
    }

    #[test]
    fn profile_and_logs_are_siblings_under_the_root() {
        // The env-reading wrappers compose the pure core with fixed names.
        let profile = default_profile_dir();
        let logs = default_logs_dir();
        assert!(profile.ends_with("profile"));
        assert!(logs.ends_with("logs"));
        assert_eq!(profile.parent(), logs.parent());
    }
}
