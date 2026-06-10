//! The `mocha_shell` binary.
//!
//! Usage:
//!
//! ```bash
//! cargo run -p mocha_shell -- examples/basic/index.html
//! cargo run -p mocha_shell -- http://127.0.0.1:8080/index.html
//! cargo run -p mocha_shell -- --dump-layout examples/layout/inline-wrap.html
//! cargo run -p mocha_shell -- --show-headers --no-cache http://127.0.0.1:8080/
//! ```
//!
//! Loads a local file, `file://`, or `http://` document, runs the rendering
//! pipeline, and prints the display list (default) or the layout tree
//! (`--dump-layout`). Exits non-zero on any error.

use std::process::ExitCode;

use mocha_error::{MochaError, MochaResult};
use mocha_shell::{render_request, RunOptions};

fn main() -> ExitCode {
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mocha: {error}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "usage: mocha_shell [--dump-layout] [--no-cache] [--show-headers] <path-or-url>\n       (file paths, file:// and http:// URLs; https:// is not implemented)";

fn real_main() -> MochaResult<()> {
    let mut options = RunOptions::default();
    let mut target: Option<String> = None;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--dump-layout" => options.dump_layout = true,
            "--no-cache" => options.no_cache = true,
            "--show-headers" => options.show_headers = true,
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
    println!("{}", render_request(&target, options)?);
    Ok(())
}
