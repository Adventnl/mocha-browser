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
//! pipeline, and prints the display list (default), the layout tree
//! (`--dump-layout`), or the form-control state (`--dump-form-state`). Exits
//! non-zero on any error.

use std::process::ExitCode;

use mocha_error::{MochaError, MochaResult};
use mocha_shell::{eval_js, hit_test_file, render_request, RunOptions};

fn main() -> ExitCode {
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mocha: {error}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "usage: mocha_shell [--dump-layout] [--dump-form-state] [--no-cache] [--show-headers] [--hit-test X,Y] <path-or-url>\n       mocha_shell --eval-js \"<javascript>\"\n       (file paths, file:// and http:// URLs; https:// is not implemented)";

fn real_main() -> MochaResult<()> {
    let mut options = RunOptions::default();
    let mut target: Option<String> = None;
    let mut hit_test: Option<(f32, f32)> = None;
    let mut eval_source: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dump-layout" => options.dump_layout = true,
            "--dump-form-state" => options.dump_form_state = true,
            "--no-cache" => options.no_cache = true,
            "--show-headers" => options.show_headers = true,
            "--hit-test" => {
                let coords = args
                    .next()
                    .ok_or_else(|| MochaError::Shell(format!("--hit-test needs X,Y\n{USAGE}")))?;
                hit_test = Some(parse_coords(&coords)?);
            }
            "--eval-js" => {
                eval_source = Some(args.next().ok_or_else(|| {
                    MochaError::Shell(format!("--eval-js needs a source string\n{USAGE}"))
                })?);
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

    // `--eval-js` evaluates standalone JavaScript and never loads a document.
    if let Some(source) = eval_source {
        println!("{}", eval_js(&source)?);
        return Ok(());
    }

    let target = target.ok_or_else(|| MochaError::Shell(USAGE.to_string()))?;

    if let Some((x, y)) = hit_test {
        match hit_test_file(&target, x, y)? {
            Some(node) => println!("Hit node {node:?}"),
            None => println!("No hit at ({x}, {y})"),
        }
        return Ok(());
    }

    println!("{}", render_request(&target, options)?);
    Ok(())
}

fn parse_coords(value: &str) -> MochaResult<(f32, f32)> {
    let (x, y) = value
        .split_once(',')
        .ok_or_else(|| MochaError::Shell(format!("--hit-test expects X,Y, got {value:?}")))?;
    let x = x
        .trim()
        .parse::<f32>()
        .map_err(|_| MochaError::Shell(format!("invalid X coordinate: {x:?}")))?;
    let y = y
        .trim()
        .parse::<f32>()
        .map_err(|_| MochaError::Shell(format!("invalid Y coordinate: {y:?}")))?;
    Ok((x, y))
}
