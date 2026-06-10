//! The `mocha_shell` binary.
//!
//! Usage:
//!
//! ```bash
//! cargo run -p mocha_shell -- examples/basic/index.html
//! ```
//!
//! Reads a local HTML file, runs the Milestone 1 pipeline, and prints the
//! resulting display list to stdout. Exits non-zero on any error.

use std::process::ExitCode;

use mocha_error::{MochaError, MochaResult};
use mocha_shell::{format_display_list, run_file};

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
    // Skip argv[0]; expect exactly one positional path argument.
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or_else(|| {
        MochaError::Shell(
            "usage: mocha_shell <path-to-html-file>\n       (Milestone 1 loads local files only)"
                .to_string(),
        )
    })?;
    if args.next().is_some() {
        return Err(MochaError::Shell(
            "expected exactly one path argument".to_string(),
        ));
    }

    let display_list = run_file(&path)?;
    println!("{}", format_display_list(&display_list));
    Ok(())
}
