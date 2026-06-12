//! The Milestone 12 desktop browser shell.
//!
//! This crate opens a native window (behind the `gui` feature, see `main.rs`),
//! rasterizes the Mocha display list to it, and provides a minimal browser:
//! toolbar, address bar, back/forward/reload buttons, and a page viewport.
//!
//! All interaction logic is testable **without** opening a window. The window/event loop
//! (`window.rs`) is a thin driver. After any change the shell uses **coarse
//! full-document rerender** ([`mocha_engine::relayout`]): it re-runs style/layout/paint
//! over the whole document rather than invalidating incrementally. See
//! `docs/architecture/desktop-shell.md` and `docs/architecture/browser-chrome.md`.

use mocha_dom::{Document, NodeId};
use mocha_engine::{relayout, render_html, render_url, RenderOptions, RenderedPage, Stylesheet};
use mocha_error::MochaResult;
use mocha_forms::{ControlKind, FormState};
use mocha_image::RasterImage;
use mocha_layout::{hit_test, LayoutBox};
use mocha_paint::DisplayCommand;
use mocha_url::Url;

/// What a click resolved to, so the window driver knows whether to redraw or
/// follow a link/submission.
#[derive(Debug, Clone, PartialEq)]
pub enum DesktopAction {
    /// Nothing changed (click on empty space, or a no-op control).
    None,
    /// State changed and the page was re-rendered; the window should redraw.
    Rerendered,
    /// The click resolved to a navigation (an `<a href>` or a GET form
    /// submission). The window decides whether to follow it.
    Navigate(Url),
}

/// The testable core of the desktop shell: a loaded page plus viewport, scroll,
/// and focus state, with input handlers that drive a coarse rerender.
pub struct DesktopPageState {
    document: Document,
    form_state: FormState,
    stylesheets: Vec<Stylesheet>,
    images: Vec<RasterImage>,
    base: Option<Url>,
    layout_root: LayoutBox,
    display_list: Vec<DisplayCommand>,
    document_height: f32,
    viewport_width: u32,
    viewport_height: u32,
    scroll_y: f32,
    focused: Option<NodeId>,
    console_output: Vec<String>,
}

impl DesktopPageState {
    /// Build state from an already-rendered [`RenderedPage`] at the given
    /// viewport size.
    pub fn from_page(page: RenderedPage, viewport_width: u32, viewport_height: u32) -> Self {
        let base = page.base_url().cloned();
        let mut state = DesktopPageState {
            document: page.document,
            form_state: page.form_state,
            stylesheets: page.stylesheets,
            images: page.images,
            base,
            layout_root: page.layout_root,
            display_list: page.display_list,
            document_height: page.document_height,
            viewport_width: viewport_width.max(1),
            viewport_height: viewport_height.max(1),
            scroll_y: 0.0,
            focused: None,
            console_output: page.console_output,
        };
        state.clamp_scroll();
        state
    }

    /// Load a document (file/http URL or local path) at the given viewport size.
    pub fn load(input: &str, viewport_width: u32, viewport_height: u32) -> MochaResult<Self> {
        let page = render_url(input, &options(viewport_width, viewport_height))?;
        Ok(Self::from_page(page, viewport_width, viewport_height))
    }

    /// Build state from an in-memory HTML string (no base URL: external
    /// subresources, link navigation, and form submission are unavailable).
    pub fn from_html(html: &str, viewport_width: u32, viewport_height: u32) -> MochaResult<Self> {
        let page = render_html(html, &options(viewport_width, viewport_height))?;
        Ok(Self::from_page(page, viewport_width, viewport_height))
    }

    /// Replace the current page by loading `url` (used to follow a navigation).
    pub fn navigate(&mut self, url: &Url) -> MochaResult<()> {
        let page = render_url(
            &url.normalized(),
            &options(self.viewport_width, self.viewport_height),
        )?;
        *self = Self::from_page(page, self.viewport_width, self.viewport_height);
        Ok(())
    }

    // --- read-only accessors (for the window driver) ------------------------

    /// The display list to rasterize.
    pub fn display_list(&self) -> &[DisplayCommand] {
        &self.display_list
    }

    /// The decoded images (indexed by `image_id`).
    pub fn images(&self) -> &[RasterImage] {
        &self.images
    }

    /// The current vertical scroll offset in px.
    pub fn scroll_y(&self) -> f32 {
        self.scroll_y
    }

    /// The total document height in px.
    pub fn document_height(&self) -> f32 {
        self.document_height
    }

    /// The viewport size in px.
    pub fn viewport(&self) -> (u32, u32) {
        (self.viewport_width, self.viewport_height)
    }

    /// The currently focused text-editable control, if any.
    pub fn focused(&self) -> Option<NodeId> {
        self.focused
    }

    /// Captured `console.log` output from the page's scripts.
    pub fn console_output(&self) -> &[String] {
        &self.console_output
    }

    /// The document base URL/final load URL, if this page came from file/http.
    pub fn base_url(&self) -> Option<&Url> {
        self.base.as_ref()
    }

    // --- scrolling ----------------------------------------------------------

    /// The maximum scroll offset (0 when the document fits the viewport).
    pub fn max_scroll(&self) -> f32 {
        (self.document_height - self.viewport_height as f32).max(0.0)
    }

    /// Scroll by `delta_y` px (positive = down), clamped to the document.
    pub fn scroll_by(&mut self, delta_y: f32) {
        self.scroll_y = (self.scroll_y + delta_y).clamp(0.0, self.max_scroll());
    }

    /// Set the absolute scroll offset (clamped to the document). Used when
    /// restoring a tab's scroll position from a session snapshot.
    pub fn set_scroll(&mut self, scroll_y: f32) {
        self.scroll_y = scroll_y.clamp(0.0, self.max_scroll());
    }

    fn clamp_scroll(&mut self) {
        self.scroll_y = self.scroll_y.clamp(0.0, self.max_scroll());
    }

    /// Resize the viewport and rerender at the new width.
    pub fn resize(&mut self, width: u32, height: u32) -> MochaResult<()> {
        self.viewport_width = width.max(1);
        self.viewport_height = height.max(1);
        self.rerender()
    }

    // --- input --------------------------------------------------------------

    /// Handle a click at window coordinates `(x, y)`. Maps to document
    /// coordinates via the scroll offset, hit-tests, and runs the form/anchor
    /// default actions. A POST form submission is a clear error (propagated).
    pub fn click(&mut self, x: f32, y: f32) -> MochaResult<DesktopAction> {
        let doc_y = y + self.scroll_y;
        let Some(hit) = hit_test(&self.layout_root, x, doc_y) else {
            self.focused = None;
            return Ok(DesktopAction::None);
        };

        // Form default action first (checkbox / radio / reset / submit).
        let form_action =
            mocha_forms::click_default_action(&self.document, &mut self.form_state, hit)?;
        self.update_focus(hit);
        match form_action {
            mocha_forms::FormDefaultAction::ToggleCheckbox(_)
            | mocha_forms::FormDefaultAction::SelectRadio(_)
            | mocha_forms::FormDefaultAction::Reset(_) => {
                self.rerender()?;
                return Ok(DesktopAction::Rerendered);
            }
            mocha_forms::FormDefaultAction::Submit { form, submitter } => {
                // A GET submission resolves to a URL the window can follow; with
                // no base URL (in-memory page) it cannot navigate. POST errors.
                if let Some(base) = self.base.clone() {
                    let submission = mocha_forms::build_submission(
                        &self.document,
                        &mut self.form_state,
                        form,
                        Some(submitter),
                        &base,
                    )?;
                    return Ok(DesktopAction::Navigate(submission.action));
                }
                return Ok(DesktopAction::None);
            }
            mocha_forms::FormDefaultAction::None => {}
        }

        // Anchor navigation default action.
        let event = mocha_events::Event::click(hit, x, doc_y);
        match mocha_nav::default_action_for_event(&self.document, &event, self.base.as_ref())? {
            mocha_nav::DefaultAction::Navigate(url) => Ok(DesktopAction::Navigate(url)),
            mocha_nav::DefaultAction::None => Ok(DesktopAction::None),
        }
    }

    /// Append printable characters to the focused text/password/textarea control
    /// (control characters are ignored). Returns whether anything changed.
    pub fn input_text(&mut self, text: &str) -> MochaResult<bool> {
        let Some(node) = self.focused else {
            return Ok(false);
        };
        let printable: String = text.chars().filter(|c| !c.is_control()).collect();
        if printable.is_empty() {
            return Ok(false);
        }
        {
            let Some(control) = self.form_state.control_mut(node) else {
                return Ok(false);
            };
            if !is_text_editable(control.kind) {
                return Ok(false);
            }
            control.value.push_str(&printable);
        }
        self.rerender()?;
        Ok(true)
    }

    /// Delete the last character of the focused control. Returns whether
    /// anything changed.
    pub fn backspace(&mut self) -> MochaResult<bool> {
        let Some(node) = self.focused else {
            return Ok(false);
        };
        {
            let Some(control) = self.form_state.control_mut(node) else {
                return Ok(false);
            };
            if !is_text_editable(control.kind) || control.value.pop().is_none() {
                return Ok(false);
            }
        }
        self.rerender()?;
        Ok(true)
    }

    // --- internals ----------------------------------------------------------

    /// Set focus to the nearest text-editable control ancestor of `hit`, or
    /// clear it when the click landed elsewhere.
    fn update_focus(&mut self, hit: NodeId) {
        self.focused = self.text_editable_ancestor(hit);
    }

    fn text_editable_ancestor(&mut self, hit: NodeId) -> Option<NodeId> {
        let mut chain = vec![hit];
        if let Ok(ancestors) = self.document.ancestors(hit) {
            chain.extend(ancestors);
        }
        for node in chain {
            if let Ok(Some(control)) = self.form_state.ensure_control(&self.document, node) {
                return is_text_editable(control.kind).then_some(node);
            }
        }
        None
    }

    /// Coarse rerender: re-run style/layout/paint over the whole document with
    /// the (possibly mutated) form state. Stylesheets and images are reused.
    fn rerender(&mut self) -> MochaResult<()> {
        let opts = options(self.viewport_width, self.viewport_height);
        let result = relayout(
            &self.document,
            &mut self.form_state,
            &self.stylesheets,
            &self.images,
            &opts,
        )?;
        self.layout_root = result.layout_root;
        self.display_list = result.display_list;
        self.document_height = result.document_height;
        self.clamp_scroll();
        Ok(())
    }
}

fn options(viewport_width: u32, viewport_height: u32) -> RenderOptions {
    RenderOptions {
        viewport_width: viewport_width as f32,
        viewport_height: viewport_height as f32,
        no_cache: false,
    }
}

fn is_text_editable(kind: ControlKind) -> bool {
    matches!(
        kind,
        ControlKind::Text | ControlKind::Password | ControlKind::TextArea
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control_value(state: &DesktopPageState, id: &str) -> String {
        let node = state.document.get_element_by_id(id).unwrap().unwrap();
        state.form_state.control(node).unwrap().value.clone()
    }

    fn is_checked(state: &DesktopPageState, id: &str) -> bool {
        let node = state.document.get_element_by_id(id).unwrap().unwrap();
        state.form_state.control(node).unwrap().checked
    }

    /// The device rect of the first control of `control_type` in the display list.
    fn control_rect(state: &DesktopPageState, control_type: &str) -> (f32, f32, f32, f32) {
        state
            .display_list()
            .iter()
            .find_map(|c| match c {
                DisplayCommand::DrawControl {
                    control_type: t,
                    x,
                    y,
                    width,
                    height,
                    ..
                } if t == control_type => Some((*x, *y, *width, *height)),
                _ => None,
            })
            .expect("control painted")
    }

    const FORM_HTML: &str = r#"<html><body>
        <input id="t" name="t" type="text" value="ab">
        <input id="c" name="c" type="checkbox">
        <input id="s" name="size" type="radio" value="s">
        <input id="l" name="size" type="radio" value="l" checked>
    </body></html>"#;

    fn form_state() -> DesktopPageState {
        DesktopPageState::from_html(FORM_HTML, 800, 600).unwrap()
    }

    #[test]
    fn scroll_clamps_to_document_bounds() {
        // A tall document in a short viewport.
        let html = r#"<html><body><div style="height: 2000px;">x</div></body></html>"#;
        let mut state = DesktopPageState::from_html(html, 400, 300).unwrap();
        assert!(state.max_scroll() > 0.0);
        state.scroll_by(-50.0);
        assert_eq!(state.scroll_y(), 0.0, "cannot scroll above the top");
        state.scroll_by(100_000.0);
        assert_eq!(
            state.scroll_y(),
            state.max_scroll(),
            "clamped to the bottom"
        );
    }

    #[test]
    fn short_document_cannot_scroll() {
        let mut state =
            DesktopPageState::from_html("<html><body><p>x</p></body></html>", 400, 600).unwrap();
        assert_eq!(state.max_scroll(), 0.0);
        state.scroll_by(200.0);
        assert_eq!(state.scroll_y(), 0.0);
    }

    #[test]
    fn click_maps_viewport_to_document_coords_via_scroll() {
        // Hit the checkbox after scrolling: the click must add scroll_y to find it.
        let html = r#"<html><body><div style="height: 1000px;">spacer</div>
            <input id="c" name="c" type="checkbox"></body></html>"#;
        let mut state = DesktopPageState::from_html(html, 400, 300).unwrap();
        let (cx, cy, cw, ch) = control_rect(&state, "checkbox");
        // Scroll so the checkbox is near the top of the viewport.
        state.scroll_by(cy);
        let window_y = cy - state.scroll_y() + ch / 2.0;
        let action = state.click(cx + cw / 2.0, window_y).unwrap();
        assert_eq!(action, DesktopAction::Rerendered);
        assert!(is_checked(&state, "c"));
    }

    #[test]
    fn click_checkbox_toggles_and_rerenders() {
        let mut state = form_state();
        let (cx, cy, cw, ch) = control_rect(&state, "checkbox");
        assert_eq!(
            state.click(cx + cw / 2.0, cy + ch / 2.0).unwrap(),
            DesktopAction::Rerendered
        );
        assert!(is_checked(&state, "c"));
        // The display list reflects the new checked state.
        assert!(state.display_list().iter().any(|c| matches!(c,
            DisplayCommand::DrawControl { control_type, checked: Some(true), .. }
                if control_type == "checkbox")));
    }

    #[test]
    fn click_radio_selects_one_of_the_group() {
        let mut state = form_state();
        assert!(is_checked(&state, "l"));
        // Click the first radio (id "s").
        let rect = state
            .display_list()
            .iter()
            .filter_map(|c| match c {
                DisplayCommand::DrawControl {
                    control_type,
                    x,
                    y,
                    width,
                    height,
                    ..
                } if control_type == "radio" => Some((*x, *y, *width, *height)),
                _ => None,
            })
            .next()
            .unwrap();
        state
            .click(rect.0 + rect.2 / 2.0, rect.1 + rect.3 / 2.0)
            .unwrap();
        assert!(is_checked(&state, "s"));
        assert!(!is_checked(&state, "l"));
    }

    #[test]
    fn click_text_input_focuses_it_and_typing_appends() {
        let mut state = form_state();
        let (tx, ty, _tw, th) = control_rect(&state, "text");
        state.click(tx + 2.0, ty + th / 2.0).unwrap();
        assert!(state.focused().is_some(), "text input focused");
        assert!(state.input_text("cd").unwrap());
        assert_eq!(control_value(&state, "t"), "abcd");
        // Control characters are ignored.
        assert!(!state.input_text("\n").unwrap());
        assert_eq!(control_value(&state, "t"), "abcd");
    }

    #[test]
    fn backspace_edits_the_focused_input() {
        let mut state = form_state();
        let (tx, ty, _tw, th) = control_rect(&state, "text");
        state.click(tx + 2.0, ty + th / 2.0).unwrap();
        assert!(state.backspace().unwrap());
        assert_eq!(control_value(&state, "t"), "a");
    }

    #[test]
    fn typing_without_focus_does_nothing() {
        let mut state = form_state();
        assert!(state.focused().is_none());
        assert!(!state.input_text("x").unwrap());
        assert!(!state.backspace().unwrap());
    }

    #[test]
    fn clicking_a_non_text_control_clears_focus() {
        let mut state = form_state();
        let (tx, ty, _tw, th) = control_rect(&state, "text");
        state.click(tx + 2.0, ty + th / 2.0).unwrap();
        assert!(state.focused().is_some());
        // Now click the checkbox: focus clears (no caret on a checkbox).
        let (cx, cy, cw, ch) = control_rect(&state, "checkbox");
        state.click(cx + cw / 2.0, cy + ch / 2.0).unwrap();
        assert!(state.focused().is_none());
    }

    #[test]
    fn click_on_empty_space_clears_focus_and_does_nothing() {
        let mut state = form_state();
        let (tx, ty, _tw, th) = control_rect(&state, "text");
        state.click(tx + 2.0, ty + th / 2.0).unwrap();
        assert!(state.focused().is_some());
        // Far below everything.
        assert_eq!(state.click(5.0, 5000.0).unwrap(), DesktopAction::None);
        assert!(state.focused().is_none());
    }

    #[test]
    fn resize_rerenders_at_new_width() {
        let html =
            "<html><body><p>alpha beta gamma delta epsilon zeta eta theta iota</p></body></html>";
        let mut state = DesktopPageState::from_html(html, 800, 600).unwrap();
        let tall_before = state.document_height();
        state.resize(120, 600).unwrap();
        assert_eq!(state.viewport(), (120, 600));
        assert!(
            state.document_height() > tall_before,
            "narrower viewport wraps to more lines"
        );
    }

    #[test]
    fn in_memory_submit_cannot_navigate() {
        // A submit click on an in-memory page (no base URL) is a no-op.
        let html = r#"<html><body><form action="/go"><input type="submit" value="Go"></form></body></html>"#;
        let mut state = DesktopPageState::from_html(html, 800, 600).unwrap();
        let (sx, sy, sw, sh) = control_rect(&state, "submit");
        assert_eq!(
            state.click(sx + sw / 2.0, sy + sh / 2.0).unwrap(),
            DesktopAction::None
        );
    }
}

pub mod address_bar;
pub mod anim;
pub mod app_dirs;
pub mod browser_app;
pub mod chrome;
pub mod icons;
pub mod new_tab;
pub mod profile;
pub mod render;
pub mod session;
pub mod tab;
pub mod text;
pub mod theme;
pub mod views;

pub use address_bar::AddressBarState;
pub use anim::{Easing, Tween};
pub use app_dirs::{default_app_data_root, default_logs_dir, default_profile_dir};
pub use browser_app::{BrowserAction, BrowserAppState, BrowserFocus};
pub use chrome::{ChromeElement, ChromeLayout, ChromeMetrics, Rect};
pub use new_tab::InternalPage;
pub use profile::{BrowserProfile, Suggestion};
pub use render::render_browser;
pub use session::{SessionSnapshot, SessionTab};
pub use tab::{BrowserTab, InternalView, ListRow, TabId, TabManager};
pub use text::Fonts;
pub use theme::BrowserTheme;
