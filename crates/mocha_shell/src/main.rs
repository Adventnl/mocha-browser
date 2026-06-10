//! The `mocha_shell` binary.
//!
//! Usage:
//!
//! ```bash
//! cargo run -p mocha_shell -- examples/basic/index.html
//! cargo run -p mocha_shell -- --dump-layout examples/basic/index.html
//! ```
//!
//! Reads a local HTML file, runs the rendering pipeline, and prints either the
//! resulting display list (default) or the formatted layout tree
//! (`--dump-layout`) to stdout. Exits non-zero on any error.

use std::process::ExitCode;

use mocha_error::{MochaError, MochaResult};
use mocha_shell::{dump_layout_file, format_display_list, run_file};

fn main() -> ExitCode {
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mocha: {error}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str =
    "usage: mocha_shell [--dump-layout] <path-to-html-file>\n       (loads local files only)";

fn real_main() -> MochaResult<()> {
    let mut dump_layout = false;
    let mut path: Option<String> = None;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--dump-layout" => dump_layout = true,
            flag if flag.starts_with("--") => {
                return Err(MochaError::Shell(format!("unknown flag '{flag}'\n{USAGE}")));
            }
            _ if path.is_none() => path = Some(arg),
            _ => {
                return Err(MochaError::Shell(format!(
                    "expected one path argument\n{USAGE}"
                )))
            }
        }
    }

    let path = path.ok_or_else(|| MochaError::Shell(USAGE.to_string()))?;

    if dump_layout {
        print!("{}", dump_layout_file(&path)?);
    } else {
        println!("{}", format_display_list(&run_file(&path)?));
    }
    Ok(())
}
