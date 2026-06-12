//! The `mocha_compat` binary: run a compatibility manifest and report results.
//!
//! ```bash
//! cargo run -p mocha_compat -- tests/compat/manifest.toml
//! MOCHA_BLESS=1 cargo run -p mocha_compat -- tests/compat/manifest.toml
//! ```
//!
//! Exits 0 when there are no unexpected failures, 1 otherwise. Set `MOCHA_BLESS=1`
//! to (re)write every blessed `expect` snapshot file from the current render.

use std::path::PathBuf;
use std::process::ExitCode;

use mocha_compat::run_manifest;

const USAGE: &str = "usage: mocha_compat <manifest.toml>\n       MOCHA_BLESS=1 mocha_compat <manifest.toml>   # rewrite expected snapshots";

fn main() -> ExitCode {
    let Some(manifest) = std::env::args().nth(1) else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    if manifest.starts_with("--") {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    }

    match run_manifest(&PathBuf::from(&manifest)) {
        Ok(summary) => {
            print!("{}", summary.format());
            if summary.has_unexpected_failures() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("mocha_compat: {error}");
            ExitCode::FAILURE
        }
    }
}
