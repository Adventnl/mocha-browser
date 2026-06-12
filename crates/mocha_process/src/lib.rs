//! Renderer process lifecycle manager for Mocha's M17 prototype.
//!
//! This is process separation, not sandboxing. The renderer binary still has the
//! parent process' OS privileges and, for M17, may call `mocha_engine::render_url`
//! directly.

use std::io::BufReader;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use mocha_error::{MochaError, MochaResult};
use mocha_ipc::{
    read_renderer_message, write_browser_message, BrowserToRenderer, RendererPageSnapshot,
    RendererToBrowser,
};

/// A spawned renderer child process.
pub struct RendererProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl RendererProcess {
    /// Spawn the renderer binary.
    pub fn spawn() -> MochaResult<RendererProcess> {
        Self::spawn_with_path(renderer_binary()?)
    }

    /// Spawn the renderer binary at an explicit path.
    pub fn spawn_with_path(path: impl Into<PathBuf>) -> MochaResult<RendererProcess> {
        let mut child = Command::new(path.into())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| MochaError::Network(format!("failed to spawn renderer: {e}")))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| MochaError::Network("renderer stdin was not piped".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| MochaError::Network("renderer stdout was not piped".to_string()))?;
        Ok(RendererProcess {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        })
    }

    /// Send ping and wait for pong.
    pub fn ping(&mut self) -> MochaResult<()> {
        let id = self.alloc_id();
        self.send(&BrowserToRenderer::Ping { id })?;
        match self.recv()? {
            RendererToBrowser::Pong { id: got } if got == id => Ok(()),
            other => Err(MochaError::Network(format!(
                "unexpected renderer response to ping: {other:?}"
            ))),
        }
    }

    /// Render a URL/local path in the child process.
    pub fn render_document(
        &mut self,
        input: &str,
        width: u32,
        height: u32,
    ) -> MochaResult<RendererPageSnapshot> {
        let id = self.alloc_id();
        self.send(&BrowserToRenderer::RenderDocument {
            id,
            input: input.to_string(),
            viewport_width: width,
            viewport_height: height,
        })?;
        self.expect_rendered(id)
    }

    /// Render in-memory HTML in the child process.
    pub fn render_html(
        &mut self,
        html: &str,
        width: u32,
        height: u32,
    ) -> MochaResult<RendererPageSnapshot> {
        let id = self.alloc_id();
        self.send(&BrowserToRenderer::RenderHtml {
            id,
            html: html.to_string(),
            base_url: None,
            viewport_width: width,
            viewport_height: height,
        })?;
        self.expect_rendered(id)
    }

    /// Ask the renderer to exit cleanly.
    pub fn shutdown(&mut self) -> MochaResult<()> {
        self.send(&BrowserToRenderer::Shutdown)?;
        match self.recv()? {
            RendererToBrowser::Goodbye => {
                let _ = self.child.wait();
                Ok(())
            }
            other => Err(MochaError::Network(format!(
                "unexpected renderer response to shutdown: {other:?}"
            ))),
        }
    }

    /// Ask the renderer to crash for tests.
    pub fn crash_for_test(&mut self) -> MochaResult<()> {
        self.send(&BrowserToRenderer::CrashForTest)
    }

    /// Best-effort liveness check.
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn expect_rendered(&mut self, id: u64) -> MochaResult<RendererPageSnapshot> {
        match self.recv()? {
            RendererToBrowser::Rendered { id: got, page } if got == id => Ok(page),
            RendererToBrowser::Error { id: got, message } if got == id => {
                Err(MochaError::Network(format!("renderer error: {message}")))
            }
            other => Err(MochaError::Network(format!(
                "unexpected renderer response: {other:?}"
            ))),
        }
    }

    fn send(&mut self, message: &BrowserToRenderer) -> MochaResult<()> {
        write_browser_message(&mut self.stdin, message)
    }

    fn recv(&mut self) -> MochaResult<RendererToBrowser> {
        read_renderer_message(&mut self.stdout)?
            .ok_or_else(|| MochaError::Network("renderer closed IPC channel".to_string()))
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

impl Drop for RendererProcess {
    fn drop(&mut self) {
        if self.is_alive() {
            let _ = write_browser_message(&mut self.stdin, &BrowserToRenderer::Shutdown);
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// Manager that can recover by spawning a replacement renderer.
pub struct RendererManager {
    renderer_path: PathBuf,
    renderer: RendererProcess,
}

impl RendererManager {
    pub fn spawn() -> MochaResult<RendererManager> {
        Self::spawn_with_path(renderer_binary()?)
    }

    pub fn spawn_with_path(path: impl Into<PathBuf>) -> MochaResult<RendererManager> {
        let renderer_path = path.into();
        let renderer = RendererProcess::spawn_with_path(renderer_path.clone())?;
        Ok(RendererManager {
            renderer_path,
            renderer,
        })
    }

    pub fn renderer_mut(&mut self) -> &mut RendererProcess {
        &mut self.renderer
    }

    pub fn respawn(&mut self) -> MochaResult<()> {
        self.renderer = RendererProcess::spawn_with_path(self.renderer_path.clone())?;
        Ok(())
    }
}

fn renderer_binary() -> MochaResult<PathBuf> {
    if let Ok(path) = std::env::var("MOCHA_RENDERER_BIN") {
        return Ok(PathBuf::from(path));
    }
    let exe = std::env::current_exe()?;
    let Some(dir) = exe.parent() else {
        return Err(MochaError::Network(
            "could not locate current executable directory".to_string(),
        ));
    };
    Ok(dir.join(format!("mocha_renderer{}", std::env::consts::EXE_SUFFIX)))
}
