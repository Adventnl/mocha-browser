//! Render performance baseline for Mocha Browser.
//!
//! [`measure`] times the core CPU phases of rendering one in-memory document —
//! parse, inline-JS, style, layout, paint, raster — plus an end-to-end total, and
//! reports node / layout-box / display-command counts and a rough memory
//! estimate. It is a **baseline**, not a benchmark harness: timings vary run to
//! run and are never asserted in CI (the only CI check is that the command runs).
//!
//! To keep timings pure CPU and deterministic in shape, [`measure`] works on an
//! in-memory document and does **not** load subresources (external CSS, images).
//! Only inline `<style>` and inline `<script>` participate. The end-to-end
//! `total_ms` uses the full [`mocha_engine`] in-memory pipeline for an honest
//! whole-render number.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use mocha_engine::{render_html, RenderOptions};
use mocha_error::MochaResult;
use mocha_layout::{build_layout_tree, LayoutBox, LayoutViewport};

/// A render-phase timing and size report.
#[derive(Debug, Clone, PartialEq)]
pub struct PerfReport {
    pub parse_html_ms: f64,
    pub js_ms: f64,
    pub style_ms: f64,
    pub layout_ms: f64,
    pub paint_ms: f64,
    pub raster_ms: f64,
    pub total_ms: f64,
    pub nodes: usize,
    pub layout_boxes: usize,
    pub display_commands: usize,
    pub mem_estimate_bytes: usize,
}

impl PerfReport {
    /// Deterministic-shape, key=value report (timings rounded to 3 places).
    pub fn format(&self) -> String {
        let mut out = String::new();
        out.push_str("Mocha Perf Report\n");
        out.push_str(&format!("parse_html_ms={:.3}\n", self.parse_html_ms));
        out.push_str(&format!("js_ms={:.3}\n", self.js_ms));
        out.push_str(&format!("style_ms={:.3}\n", self.style_ms));
        out.push_str(&format!("layout_ms={:.3}\n", self.layout_ms));
        out.push_str(&format!("paint_ms={:.3}\n", self.paint_ms));
        out.push_str(&format!("raster_ms={:.3}\n", self.raster_ms));
        out.push_str(&format!("total_ms={:.3}\n", self.total_ms));
        out.push_str(&format!("nodes={}\n", self.nodes));
        out.push_str(&format!("layout_boxes={}\n", self.layout_boxes));
        out.push_str(&format!("display_commands={}\n", self.display_commands));
        out.push_str(&format!("mem_estimate_bytes={}\n", self.mem_estimate_bytes));
        out
    }
}

/// Read a local file and measure it. (Local files only — no network, so timings
/// reflect CPU work rather than connection variance.)
pub fn measure_file(path: &str) -> MochaResult<PerfReport> {
    let html = std::fs::read_to_string(path)?;
    measure(&html)
}

/// Time the render phases of an in-memory HTML document.
pub fn measure(html: &str) -> MochaResult<PerfReport> {
    let viewport = LayoutViewport {
        width: mocha_layout::DEFAULT_VIEWPORT_WIDTH,
        height: mocha_layout::DEFAULT_VIEWPORT_HEIGHT,
    };

    // parse
    let start = Instant::now();
    let document = mocha_html::parse_html(html)?;
    let parse_html_ms = ms(start);

    // inline JS (mirrors the engine: run scripts before style/layout)
    let scripts = mocha_js_dom::collect_inline_scripts(&document)?;
    let (document, js_ms) = if scripts.is_empty() {
        (document, 0.0)
    } else {
        let shared = Rc::new(RefCell::new(document));
        let mut runtime = mocha_js_dom::DomRuntime::with_url(shared.clone(), None);
        runtime.init_form_state()?;
        let start = Instant::now();
        for source in &scripts {
            runtime.run_script(source)?;
        }
        runtime.run_pending_timers()?;
        let js_ms = ms(start);
        let document = shared.borrow().clone();
        (document, js_ms)
    };

    // style (inline stylesheets only)
    let stylesheets = mocha_resources::collect_inline_stylesheets(&document)?;
    let start = Instant::now();
    let styled = mocha_style::build_style_tree(&document, &stylesheets)?;
    let style_ms = ms(start);

    // layout
    let start = Instant::now();
    let layout_root = build_layout_tree(&styled, viewport)?;
    let layout_ms = ms(start);

    // paint
    let start = Instant::now();
    let display_list = mocha_paint::build_display_list(&layout_root)?;
    let paint_ms = ms(start);

    // raster (debug font, no images)
    let start = Instant::now();
    let mut surface = mocha_raster::Surface::new(
        viewport.width as u32,
        layout_root.rect.height.max(1.0).ceil() as u32,
    );
    mocha_raster::rasterize(&mut surface, &display_list, &[], 0.0);
    let raster_ms = ms(start);

    // end-to-end total through the real engine
    let start = Instant::now();
    let _ = render_html(html, &RenderOptions::default())?;
    let total_ms = ms(start);

    let nodes = document.len();
    let layout_boxes = count_boxes(&layout_root);
    let display_commands = display_list.len();
    let mem_estimate_bytes = nodes * 256 + layout_boxes * 128 + display_commands * 64;

    Ok(PerfReport {
        parse_html_ms,
        js_ms,
        style_ms,
        layout_ms,
        paint_ms,
        raster_ms,
        total_ms,
        nodes,
        layout_boxes,
        display_commands,
        mem_estimate_bytes,
    })
}

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn count_boxes(box_: &LayoutBox) -> usize {
    1 + box_.children.iter().map(count_boxes).sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_populates_counts_for_a_real_document() {
        let report = measure(
            "<html><body><h1>Title</h1><p>alpha beta gamma delta epsilon</p></body></html>",
        )
        .unwrap();
        assert!(report.nodes > 1, "nodes counted");
        assert!(report.layout_boxes > 1, "layout boxes counted");
        assert!(report.display_commands > 0, "display commands counted");
        assert!(report.mem_estimate_bytes > 0);
        assert_eq!(report.js_ms, 0.0, "no scripts means zero js time");
    }

    #[test]
    fn report_format_includes_every_field() {
        let report = measure("<html><body><p>Hi</p></body></html>").unwrap();
        let text = report.format();
        for field in [
            "parse_html_ms=",
            "js_ms=",
            "style_ms=",
            "layout_ms=",
            "paint_ms=",
            "raster_ms=",
            "total_ms=",
            "nodes=",
            "layout_boxes=",
            "display_commands=",
            "mem_estimate_bytes=",
        ] {
            assert!(text.contains(field), "missing field {field} in:\n{text}");
        }
    }

    #[test]
    fn inline_script_is_timed() {
        let report = measure(
            "<html><body><p id=\"t\">a</p><script>document.getElementById(\"t\").textContent=\"b\";</script></body></html>",
        )
        .unwrap();
        assert!(report.js_ms >= 0.0);
    }
}
