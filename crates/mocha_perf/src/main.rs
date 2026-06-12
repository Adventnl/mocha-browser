//! The `mocha_perf` binary: print a render performance baseline for a document.
//!
//! ```bash
//! cargo run -p mocha_perf -- examples/layout/article.html
//! ```
//!
//! Local files only. Timings are a rough baseline and vary run to run; they are
//! never used as a pass/fail gate.

use std::process::ExitCode;

use mocha_perf::measure_file;

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: mocha_perf <local-file.html>");
        return ExitCode::FAILURE;
    };
    match measure_file(&path) {
        Ok(report) => {
            print!("{}", report.format());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("mocha_perf: {error}");
            ExitCode::FAILURE
        }
    }
}
