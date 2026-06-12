//! Headless DevTools snapshots for Mocha Browser.
//!
//! This crate is a Milestone 19 foundation layer: it exposes deterministic
//! snapshots and logs for tests, shell output, and future UI panels. It is **not**
//! the Chrome DevTools Protocol, does not open a remote debugging socket, and
//! does not provide breakpoints, live editing, heap inspection, or profiling.

use mocha_dom::{Document, NodeId, NodeKind};
use mocha_engine::RenderedPage;
use mocha_error::MochaResult;
use mocha_layout::{LayoutBox, LayoutBoxKind};
use mocha_paint::DisplayCommand;
use mocha_security::SecurityViolation;
use mocha_style::{build_style_tree, ComputedStyle, Display, FontWeight, StyledNode};

/// A complete point-in-time inspector snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct DevToolsSnapshot {
    /// User-facing request target, when known.
    pub request: Option<String>,
    /// Final document URL, when loaded through the URL pipeline.
    pub url: Option<String>,
    /// Document title, when one is present.
    pub title: Option<String>,
    /// DOM tree snapshot.
    pub dom: DomSnapshot,
    /// Computed-style tree snapshot.
    pub styles: StyleSnapshot,
    /// Layout tree snapshot.
    pub layout: LayoutSnapshot,
    /// Paint display-list snapshot.
    pub display_list: DisplayListSnapshot,
    /// Network/resource observations currently available to the engine.
    pub network: Vec<NetworkLogEntry>,
    /// Captured console output and script errors.
    pub console: Vec<ConsoleLogEntry>,
    /// Input/event observations recorded by embedders.
    pub events: Vec<EventLogEntry>,
    /// Cookie/local-storage observations recorded by embedders.
    pub storage: StorageSnapshot,
    /// Security decisions and violations recorded by embedders.
    pub security: Vec<SecurityLogEntry>,
    /// IPC messages recorded by process embedders.
    pub ipc: Vec<IpcLogEntry>,
    /// Renderer/browser process lifecycle observations recorded by embedders.
    pub processes: Vec<ProcessLogEntry>,
}

/// DOM tree root plus a total node count for quick panel summaries.
#[derive(Debug, Clone, PartialEq)]
pub struct DomSnapshot {
    /// Root node.
    pub root: DomNodeSnapshot,
    /// Total nodes in the DOM arena.
    pub node_count: usize,
}

/// A serializable DOM node representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomNodeSnapshot {
    /// DOM arena id.
    pub id: usize,
    /// Node kind: `document`, `element`, `text`, `comment`, or `doctype`.
    pub kind: String,
    /// Tag name or doctype name.
    pub name: Option<String>,
    /// Element attributes in source order.
    pub attributes: Vec<(String, String)>,
    /// Text/comment body.
    pub text: Option<String>,
    /// Child nodes.
    pub children: Vec<DomNodeSnapshot>,
}

/// Computed style tree root.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleSnapshot {
    /// Root style node.
    pub root: StyleNodeSnapshot,
}

/// Computed style for one DOM node.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleNodeSnapshot {
    /// DOM node id.
    pub node_id: usize,
    /// Optional text content for text nodes.
    pub text: Option<String>,
    /// Stable style properties.
    pub style: StylePropertiesSnapshot,
    /// Styled children.
    pub children: Vec<StyleNodeSnapshot>,
}

/// Stable subset of computed style fields useful to inspector panels.
#[derive(Debug, Clone, PartialEq)]
pub struct StylePropertiesSnapshot {
    pub display: String,
    pub color: String,
    pub background_color: String,
    pub font_size: f32,
    pub font_weight: String,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub margin: EdgeSnapshot,
    pub padding: EdgeSnapshot,
    pub border_width: f32,
    pub border_color: String,
}

/// Four-sided box lengths.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeSnapshot {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// Layout tree root.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutSnapshot {
    /// Root layout box.
    pub root: LayoutNodeSnapshot,
}

/// One layout box with geometry and paint-relevant style.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutNodeSnapshot {
    /// Kind string, including text/image/control detail where relevant.
    pub kind: String,
    /// Source DOM node id, if any.
    pub node_id: Option<usize>,
    /// Border-box geometry.
    pub rect: RectSnapshot,
    /// Children in layout order.
    pub children: Vec<LayoutNodeSnapshot>,
}

/// Rectangle geometry in CSS px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectSnapshot {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Flat paint command snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayListSnapshot {
    /// Commands in paint order.
    pub commands: Vec<DisplayCommandSnapshot>,
}

/// One display-list command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayCommandSnapshot {
    /// Stable command kind.
    pub kind: String,
    /// Existing debug-line form, preserved for shell parity.
    pub debug: String,
}

/// A network/resource observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkLogEntry {
    pub resource_type: String,
    pub request_url: String,
    pub final_url: String,
    pub status: Option<u16>,
    pub content_type: Option<String>,
    pub from_cache: bool,
}

/// A console observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleLogEntry {
    pub level: String,
    pub message: String,
}

/// An input/event observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventLogEntry {
    pub event_type: String,
    pub target: Option<usize>,
    pub detail: Option<String>,
}

/// Cookie/local-storage observations.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StorageSnapshot {
    pub cookies: Vec<CookieSnapshot>,
    pub local_storage: Vec<StorageItemSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieSnapshot {
    pub name: String,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub secure: bool,
    pub http_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageItemSnapshot {
    pub origin: String,
    pub key: String,
    pub value: String,
}

/// A security observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityLogEntry {
    pub policy: String,
    pub blocked_url: String,
    pub reason: String,
}

/// A browser/renderer IPC observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpcLogEntry {
    pub direction: String,
    pub message: String,
    pub result: String,
}

/// A browser/renderer process observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessLogEntry {
    pub process: String,
    pub pid: Option<u32>,
    pub state: String,
    pub detail: Option<String>,
}

/// Build a complete DevTools snapshot from an engine render.
pub fn snapshot_rendered_page(
    page: &RenderedPage,
    request: Option<String>,
) -> MochaResult<DevToolsSnapshot> {
    let style_tree = build_style_tree(&page.document, &page.stylesheets)?;
    let final_url = page.meta.as_ref().map(|meta| meta.final_url.normalized());
    Ok(DevToolsSnapshot {
        request: request.clone(),
        url: final_url.clone(),
        title: document_title(&page.document)?,
        dom: snapshot_dom(&page.document)?,
        styles: snapshot_styles(&style_tree),
        layout: snapshot_layout(&page.layout_root),
        display_list: snapshot_display_list(&page.display_list),
        network: page
            .meta
            .as_ref()
            .map(|meta| {
                vec![NetworkLogEntry {
                    resource_type: "document".to_string(),
                    request_url: request.unwrap_or_else(|| meta.final_url.normalized()),
                    final_url: meta.final_url.normalized(),
                    status: meta.status,
                    content_type: meta.content_type.clone(),
                    from_cache: meta.from_cache,
                }]
            })
            .unwrap_or_default(),
        console: page
            .console_output
            .iter()
            .map(|message| ConsoleLogEntry {
                level: "log".to_string(),
                message: message.clone(),
            })
            .collect(),
        events: Vec::new(),
        storage: StorageSnapshot::default(),
        security: Vec::new(),
        ipc: Vec::new(),
        processes: Vec::new(),
    })
}

/// Build a DOM snapshot from a document.
pub fn snapshot_dom(document: &Document) -> MochaResult<DomSnapshot> {
    Ok(DomSnapshot {
        root: snapshot_dom_node(document, document.root_id())?,
        node_count: document.len(),
    })
}

/// Build a computed-style snapshot.
pub fn snapshot_styles(root: &StyledNode) -> StyleSnapshot {
    StyleSnapshot {
        root: snapshot_style_node(root),
    }
}

/// Build a layout snapshot.
pub fn snapshot_layout(root: &LayoutBox) -> LayoutSnapshot {
    LayoutSnapshot {
        root: snapshot_layout_node(root),
    }
}

/// Build a display-list snapshot.
pub fn snapshot_display_list(commands: &[DisplayCommand]) -> DisplayListSnapshot {
    DisplayListSnapshot {
        commands: commands
            .iter()
            .map(|command| DisplayCommandSnapshot {
                kind: display_command_kind(command).to_string(),
                debug: command.to_debug_line(),
            })
            .collect(),
    }
}

/// Convert a security violation into an inspector log entry.
pub fn security_log_from_violation(violation: &SecurityViolation) -> SecurityLogEntry {
    SecurityLogEntry {
        policy: format!("{:?}", violation.kind),
        blocked_url: String::new(),
        reason: violation.message.clone(),
    }
}

/// Format a complete snapshot as deterministic text for headless tooling.
pub fn format_snapshot(snapshot: &DevToolsSnapshot) -> String {
    let mut out = String::new();
    out.push_str("DevToolsSnapshot\n");
    push_opt(&mut out, "request", snapshot.request.as_deref());
    push_opt(&mut out, "url", snapshot.url.as_deref());
    push_opt(&mut out, "title", snapshot.title.as_deref());
    out.push_str(&format!("dom.nodes: {}\n", snapshot.dom.node_count));
    out.push_str("DOM\n");
    format_dom_node(&snapshot.dom.root, 0, &mut out);
    out.push_str("Styles\n");
    format_style_node(&snapshot.styles.root, 0, &mut out);
    out.push_str("Layout\n");
    format_layout_node(&snapshot.layout.root, 0, &mut out);
    out.push_str("DisplayList\n");
    for (index, command) in snapshot.display_list.commands.iter().enumerate() {
        out.push_str(&format!("  {index}: {} {}\n", command.kind, command.debug));
    }
    format_network(&snapshot.network, &mut out);
    format_console(&snapshot.console, &mut out);
    format_events(&snapshot.events, &mut out);
    format_storage(&snapshot.storage, &mut out);
    format_security(&snapshot.security, &mut out);
    format_ipc(&snapshot.ipc, &mut out);
    format_processes(&snapshot.processes, &mut out);
    out
}

fn snapshot_dom_node(document: &Document, id: NodeId) -> MochaResult<DomNodeSnapshot> {
    let node = document.node(id)?;
    let (kind, name, attributes, text) = match &node.kind {
        NodeKind::Document => ("document".to_string(), None, Vec::new(), None),
        NodeKind::Element(data) => (
            "element".to_string(),
            Some(data.tag_name.clone()),
            data.attributes
                .iter()
                .map(|attribute| (attribute.name.clone(), attribute.value.clone()))
                .collect(),
            None,
        ),
        NodeKind::Text(data) => (
            "text".to_string(),
            None,
            Vec::new(),
            Some(data.text.clone()),
        ),
        NodeKind::Comment(comment) => (
            "comment".to_string(),
            None,
            Vec::new(),
            Some(comment.clone()),
        ),
        NodeKind::Doctype(doctype) => (
            "doctype".to_string(),
            Some(doctype.clone()),
            Vec::new(),
            None,
        ),
    };
    let children = document
        .children(id)?
        .iter()
        .map(|&child| snapshot_dom_node(document, child))
        .collect::<MochaResult<Vec<_>>>()?;
    Ok(DomNodeSnapshot {
        id: id.0,
        kind,
        name,
        attributes,
        text,
        children,
    })
}

fn snapshot_style_node(node: &StyledNode) -> StyleNodeSnapshot {
    StyleNodeSnapshot {
        node_id: node.node_id.0,
        text: node.text.clone(),
        style: style_properties(&node.style),
        children: node.children.iter().map(snapshot_style_node).collect(),
    }
}

fn style_properties(style: &ComputedStyle) -> StylePropertiesSnapshot {
    StylePropertiesSnapshot {
        display: match style.display {
            Display::Block => "block",
            Display::Inline => "inline",
            Display::None => "none",
        }
        .to_string(),
        color: style.color.to_string(),
        background_color: style.background_color.to_string(),
        font_size: style.font_size,
        font_weight: match style.font_weight {
            FontWeight::Normal => "normal",
            FontWeight::Bold => "bold",
        }
        .to_string(),
        width: style.width,
        height: style.height,
        margin: EdgeSnapshot {
            top: style.margin.top,
            right: style.margin.right,
            bottom: style.margin.bottom,
            left: style.margin.left,
        },
        padding: EdgeSnapshot {
            top: style.padding.top,
            right: style.padding.right,
            bottom: style.padding.bottom,
            left: style.padding.left,
        },
        border_width: style.border_width,
        border_color: style.border_color.to_string(),
    }
}

fn snapshot_layout_node(node: &LayoutBox) -> LayoutNodeSnapshot {
    LayoutNodeSnapshot {
        kind: layout_kind(&node.kind),
        node_id: node.node_id.map(|id| id.0),
        rect: RectSnapshot {
            x: node.rect.x,
            y: node.rect.y,
            width: node.rect.width,
            height: node.rect.height,
        },
        children: node.children.iter().map(snapshot_layout_node).collect(),
    }
}

fn layout_kind(kind: &LayoutBoxKind) -> String {
    match kind {
        LayoutBoxKind::Block => "Block".to_string(),
        LayoutBoxKind::Inline => "Inline".to_string(),
        LayoutBoxKind::AnonymousBlock => "AnonymousBlock".to_string(),
        LayoutBoxKind::LineBox => "LineBox".to_string(),
        LayoutBoxKind::TextRun(text) => format!("TextRun({text:?})"),
        LayoutBoxKind::Image(id) => format!("Image({id})"),
        LayoutBoxKind::Control(control) => format!("Control({})", control.control_type),
    }
}

fn display_command_kind(command: &DisplayCommand) -> &'static str {
    match command {
        DisplayCommand::DrawRect { .. } => "DrawRect",
        DisplayCommand::DrawBorder { .. } => "DrawBorder",
        DisplayCommand::DrawText { .. } => "DrawText",
        DisplayCommand::DrawImage { .. } => "DrawImage",
        DisplayCommand::DrawControl { .. } => "DrawControl",
    }
}

fn document_title(document: &Document) -> MochaResult<Option<String>> {
    for id in document.traverse_depth_first(document.root_id())? {
        let NodeKind::Element(element) = &document.node(id)?.kind else {
            continue;
        };
        if element.tag_name != "title" {
            continue;
        }
        let mut title = String::new();
        for &child in document.children(id)? {
            if let NodeKind::Text(text) = &document.node(child)?.kind {
                title.push_str(&text.text);
            }
        }
        let title = title.trim();
        return Ok((!title.is_empty()).then(|| title.to_string()));
    }
    Ok(None)
}

fn push_opt(out: &mut String, label: &str, value: Option<&str>) {
    if let Some(value) = value {
        out.push_str(&format!("{label}: {value}\n"));
    }
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn format_dom_node(node: &DomNodeSnapshot, depth: usize, out: &mut String) {
    indent(out, depth);
    out.push_str(&format!("{}#{}", node.kind, node.id));
    if let Some(name) = &node.name {
        out.push_str(&format!(" <{name}>"));
    }
    for (name, value) in &node.attributes {
        out.push_str(&format!(" {name}={value:?}"));
    }
    if let Some(text) = &node.text {
        out.push_str(&format!(" {text:?}"));
    }
    out.push('\n');
    for child in &node.children {
        format_dom_node(child, depth + 1, out);
    }
}

fn format_style_node(node: &StyleNodeSnapshot, depth: usize, out: &mut String) {
    indent(out, depth);
    out.push_str(&format!(
        "node#{} display={} color={} background={} font-size={} font-weight={}\n",
        node.node_id,
        node.style.display,
        node.style.color,
        node.style.background_color,
        node.style.font_size,
        node.style.font_weight
    ));
    for child in &node.children {
        format_style_node(child, depth + 1, out);
    }
}

fn format_layout_node(node: &LayoutNodeSnapshot, depth: usize, out: &mut String) {
    indent(out, depth);
    out.push_str(&format!(
        "{} node={:?} rect=({}, {}, {}, {})\n",
        node.kind, node.node_id, node.rect.x, node.rect.y, node.rect.width, node.rect.height
    ));
    for child in &node.children {
        format_layout_node(child, depth + 1, out);
    }
}

fn format_network(entries: &[NetworkLogEntry], out: &mut String) {
    out.push_str("Network\n");
    for entry in entries {
        out.push_str(&format!(
            "  {} request={} final={} status={:?} content-type={:?} from-cache={}\n",
            entry.resource_type,
            entry.request_url,
            entry.final_url,
            entry.status,
            entry.content_type,
            entry.from_cache
        ));
    }
}

fn format_console(entries: &[ConsoleLogEntry], out: &mut String) {
    out.push_str("Console\n");
    for entry in entries {
        out.push_str(&format!("  {} {}\n", entry.level, entry.message));
    }
}

fn format_events(entries: &[EventLogEntry], out: &mut String) {
    out.push_str("Events\n");
    for entry in entries {
        out.push_str(&format!(
            "  {} target={:?} detail={:?}\n",
            entry.event_type, entry.target, entry.detail
        ));
    }
}

fn format_storage(storage: &StorageSnapshot, out: &mut String) {
    out.push_str("Storage\n");
    for cookie in &storage.cookies {
        out.push_str(&format!(
            "  cookie {} domain={:?} path={:?} secure={} http-only={}\n",
            cookie.name, cookie.domain, cookie.path, cookie.secure, cookie.http_only
        ));
    }
    for item in &storage.local_storage {
        out.push_str(&format!(
            "  local-storage origin={} key={} value={:?}\n",
            item.origin, item.key, item.value
        ));
    }
}

fn format_security(entries: &[SecurityLogEntry], out: &mut String) {
    out.push_str("Security\n");
    for entry in entries {
        out.push_str(&format!(
            "  policy={} blocked={} reason={}\n",
            entry.policy, entry.blocked_url, entry.reason
        ));
    }
}

fn format_ipc(entries: &[IpcLogEntry], out: &mut String) {
    out.push_str("IPC\n");
    for entry in entries {
        out.push_str(&format!(
            "  {} {} result={}\n",
            entry.direction, entry.message, entry.result
        ));
    }
}

fn format_processes(entries: &[ProcessLogEntry], out: &mut String) {
    out.push_str("Processes\n");
    for entry in entries {
        out.push_str(&format!(
            "  {} pid={:?} state={} detail={:?}\n",
            entry.process, entry.pid, entry.state, entry.detail
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_engine::{render_html, RenderOptions};
    use mocha_origin::Origin;
    use mocha_security::{parse_csp, CspDirective, SecurityDecision};
    use mocha_url::Url;

    fn sample_page() -> RenderedPage {
        render_html(
            r#"
            <html>
              <body><style>p { color: red; font-weight: bold; }</style><p id="greeting">Hi</p><script>console.log("ready")</script></body>
            </html>
            "#,
            &RenderOptions::default(),
        )
        .unwrap()
    }

    #[test]
    fn snapshot_includes_dom_style_layout_display_and_console() {
        let page = sample_page();
        let snapshot = snapshot_rendered_page(&page, Some("memory:test".to_string())).unwrap();
        assert_eq!(snapshot.title, None);
        assert!(snapshot.dom.node_count > 1);
        assert!(snapshot
            .dom
            .root
            .children
            .iter()
            .any(|node| node.name.as_deref() == Some("html")));
        assert!(contains_style_color(&snapshot.styles.root, "#ff0000"));
        assert!(contains_layout_kind(
            &snapshot.layout.root,
            "TextRun(\"Hi\")"
        ));
        assert!(snapshot
            .display_list
            .commands
            .iter()
            .any(|command| command.kind == "DrawText" && command.debug.contains("Hi")));
        assert_eq!(
            snapshot.console,
            vec![ConsoleLogEntry {
                level: "log".to_string(),
                message: "ready".to_string()
            }]
        );
    }

    #[test]
    fn formatter_is_stable_and_panel_oriented() {
        let page = sample_page();
        let snapshot = snapshot_rendered_page(&page, Some("memory:test".to_string())).unwrap();
        let text = format_snapshot(&snapshot);
        assert!(text.starts_with("DevToolsSnapshot\n"));
        assert!(text.contains("DOM\n"));
        assert!(text.contains("Styles\n"));
        assert!(text.contains("Layout\n"));
        assert!(text.contains("DisplayList\n"));
        assert!(text.contains("Console\n  log ready\n"));
    }

    #[test]
    fn network_log_records_document_load_metadata() {
        let mut page = sample_page();
        page.meta = Some(mocha_engine::ResponseMeta {
            final_url: Url::parse("http://example.test/index.html").unwrap(),
            status: Some(200),
            content_type: Some("text/html".to_string()),
            from_cache: false,
        });
        let snapshot =
            snapshot_rendered_page(&page, Some("http://example.test/".to_string())).unwrap();
        assert_eq!(snapshot.network.len(), 1);
        assert_eq!(snapshot.network[0].resource_type, "document");
        assert_eq!(snapshot.network[0].status, Some(200));
    }

    #[test]
    fn log_models_cover_events_storage_security_ipc_and_processes() {
        let blocked = Url::parse("http://blocked.test/script.js").unwrap();
        let protected =
            Origin::from_url(&Url::parse("http://example.test/index.html").unwrap()).unwrap();
        let csp = parse_csp("default-src 'none'", &protected).unwrap();
        let SecurityDecision::Block(violation) = csp.allows(CspDirective::ScriptSrc, &blocked)
        else {
            panic!("expected CSP block");
        };
        let mut snapshot = snapshot_rendered_page(&sample_page(), None).unwrap();
        snapshot.events.push(EventLogEntry {
            event_type: "click".to_string(),
            target: Some(7),
            detail: Some("button=primary".to_string()),
        });
        snapshot.storage.cookies.push(CookieSnapshot {
            name: "session".to_string(),
            domain: Some("example.test".to_string()),
            path: Some("/".to_string()),
            secure: true,
            http_only: true,
        });
        snapshot.storage.local_storage.push(StorageItemSnapshot {
            origin: "http://example.test".to_string(),
            key: "theme".to_string(),
            value: "dark".to_string(),
        });
        snapshot
            .security
            .push(security_log_from_violation(&violation));
        snapshot.ipc.push(IpcLogEntry {
            direction: "browser->renderer".to_string(),
            message: "RenderPreparedDocument".to_string(),
            result: "ok".to_string(),
        });
        snapshot.processes.push(ProcessLogEntry {
            process: "renderer".to_string(),
            pid: Some(42),
            state: "spawned".to_string(),
            detail: Some("capability-restricted".to_string()),
        });
        let text = format_snapshot(&snapshot);
        assert!(text.contains("Events\n  click target=Some(7)"));
        assert!(text.contains("cookie session"));
        assert!(text.contains("local-storage origin=http://example.test"));
        assert!(text.contains("policy=ContentSecurityPolicy"));
        assert!(text.contains("browser->renderer RenderPreparedDocument"));
        assert!(text.contains("renderer pid=Some(42)"));
    }

    fn contains_style_color(node: &StyleNodeSnapshot, color: &str) -> bool {
        node.style.color == color
            || node
                .children
                .iter()
                .any(|child| contains_style_color(child, color))
    }

    fn contains_layout_kind(node: &LayoutNodeSnapshot, kind: &str) -> bool {
        node.kind == kind
            || node
                .children
                .iter()
                .any(|child| contains_layout_kind(child, kind))
    }
}
