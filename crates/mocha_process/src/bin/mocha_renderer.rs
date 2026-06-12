use std::io::{self, BufReader};
use std::process::ExitCode;

use mocha_engine::{render_html, render_url, RenderOptions, RenderedPage};
use mocha_error::MochaResult;
use mocha_ipc::{
    read_browser_message, write_renderer_message, BrowserToRenderer, RendererPageSnapshot,
    RendererToBrowser,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> MochaResult<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();

    while let Some(message) = read_browser_message(&mut reader)? {
        match message {
            BrowserToRenderer::Ping { id } => {
                write_renderer_message(&mut writer, &RendererToBrowser::Pong { id })?;
            }
            BrowserToRenderer::RenderDocument {
                id,
                input,
                viewport_width,
                viewport_height,
            } => {
                let result =
                    render_url(&input, &options(viewport_width, viewport_height)).map(snapshot);
                write_render_result(&mut writer, id, result)?;
            }
            BrowserToRenderer::RenderHtml {
                id,
                html,
                base_url,
                viewport_width,
                viewport_height,
            } => {
                let result = if base_url.is_some() {
                    Err(mocha_error::MochaError::UnsupportedFeature(
                        "renderer RenderHtml with base_url is not implemented in M17".to_string(),
                    ))
                } else {
                    render_html(&html, &options(viewport_width, viewport_height)).map(snapshot)
                };
                write_render_result(&mut writer, id, result)?;
            }
            BrowserToRenderer::Shutdown => {
                write_renderer_message(&mut writer, &RendererToBrowser::Goodbye)?;
                return Ok(());
            }
            BrowserToRenderer::CrashForTest => {
                std::process::exit(70);
            }
        }
    }
    Ok(())
}

fn write_render_result<W: std::io::Write>(
    writer: &mut W,
    id: u64,
    result: MochaResult<RendererPageSnapshot>,
) -> MochaResult<()> {
    match result {
        Ok(page) => write_renderer_message(writer, &RendererToBrowser::Rendered { id, page }),
        Err(error) => write_renderer_message(
            writer,
            &RendererToBrowser::Error {
                id,
                message: error.to_string(),
            },
        ),
    }
}

fn options(width: u32, height: u32) -> RenderOptions {
    RenderOptions {
        viewport_width: width.max(1) as f32,
        viewport_height: height.max(1) as f32,
        no_cache: false,
    }
}

fn snapshot(page: RenderedPage) -> RendererPageSnapshot {
    RendererPageSnapshot {
        final_url: page.base_url().map(|url| url.normalized()),
        document_height: page.document_height,
        display_list_len: page.display_list.len(),
        console_output: page.console_output,
    }
}
