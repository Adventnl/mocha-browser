//! Security policy foundation for Mocha Browser (Milestone 16).
//!
//! This crate defines policy objects and explicit allow/block decisions for
//! origins, URL schemes, file access, mixed content, CSP, permissions,
//! certificate-error representation, and renderer capabilities. It is **not** a
//! sandbox, not complete web security, and not a TLS implementation.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use mocha_error::{MochaError, MochaResult};
use mocha_origin::Origin;
use mocha_url::{Scheme, Url};

/// A policy decision: either allow an action or block it with a reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecurityDecision {
    /// The action is allowed by this policy check.
    Allow,
    /// The action is blocked by this policy check.
    Block(SecurityViolation),
}

impl SecurityDecision {
    /// Whether this decision allows the action.
    pub fn is_allowed(&self) -> bool {
        matches!(self, SecurityDecision::Allow)
    }

    /// Convert a decision into a `MochaResult`.
    pub fn into_result(self) -> MochaResult<()> {
        match self {
            SecurityDecision::Allow => Ok(()),
            SecurityDecision::Block(violation) => Err(violation.into()),
        }
    }
}

/// The structured reason a security policy blocked something.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityViolation {
    /// Machine-readable category.
    pub kind: SecurityViolationKind,
    /// Human-readable explanation.
    pub message: String,
}

impl SecurityViolation {
    fn new(kind: SecurityViolationKind, message: impl Into<String>) -> SecurityViolation {
        SecurityViolation {
            kind,
            message: message.into(),
        }
    }
}

impl From<SecurityViolation> for MochaError {
    fn from(violation: SecurityViolation) -> Self {
        MochaError::Security(violation.message)
    }
}

/// Categories of M16 policy violations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityViolationKind {
    CrossOriginAccess,
    FileUrlAccess,
    MixedContent,
    UnsupportedScheme,
    ContentSecurityPolicy,
    PermissionDenied,
    InsecureDowngrade,
    CertificateError,
}

fn block(kind: SecurityViolationKind, message: impl Into<String>) -> SecurityDecision {
    SecurityDecision::Block(SecurityViolation::new(kind, message))
}

/// Whether two URLs have the same tuple origin.
pub fn same_origin(a: &Url, b: &Url) -> MochaResult<bool> {
    Ok(Origin::from_url(a)?.is_same_origin(&Origin::from_url(b)?))
}

/// Require two URLs to have the same tuple origin.
pub fn require_same_origin(a: &Url, b: &Url) -> MochaResult<()> {
    if same_origin(a, b)? {
        Ok(())
    } else {
        Err(MochaError::Security(format!(
            "cross-origin access blocked: {} is not same-origin with {}",
            a.normalized(),
            b.normalized()
        )))
    }
}

/// The context in which a URL is being used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlUseContext {
    DocumentNavigation,
    Subresource,
    Stylesheet,
    Image,
    Script,
    FormSubmission,
    AnchorNavigation,
    WebStorage,
}

/// Check a parsed URL against M16 scheme/context policy.
pub fn check_url_context(url: &Url, context: UrlUseContext) -> SecurityDecision {
    match context {
        UrlUseContext::DocumentNavigation | UrlUseContext::AnchorNavigation => match url.scheme {
            Scheme::File | Scheme::Http | Scheme::Https => SecurityDecision::Allow,
        },
        UrlUseContext::Subresource | UrlUseContext::Stylesheet | UrlUseContext::Image => {
            match url.scheme {
                Scheme::File | Scheme::Http => SecurityDecision::Allow,
                Scheme::Https => block(
                    SecurityViolationKind::UnsupportedScheme,
                    "https subresources are recognized but not loadable until TLS exists",
                ),
            }
        }
        UrlUseContext::Script => block(
            SecurityViolationKind::UnsupportedScheme,
            "external script loading is blocked in M16",
        ),
        UrlUseContext::FormSubmission => match url.scheme {
            Scheme::Http | Scheme::File => SecurityDecision::Allow,
            Scheme::Https => block(
                SecurityViolationKind::UnsupportedScheme,
                "https form submission is recognized but not loadable until TLS exists",
            ),
        },
        UrlUseContext::WebStorage => match Origin::from_url(url) {
            Ok(_) => SecurityDecision::Allow,
            Err(_) => block(
                SecurityViolationKind::FileUrlAccess,
                "web storage requires an http(s) tuple origin; file:// is opaque",
            ),
        },
    }
}

/// Check a user/reference string against M16 scheme/context policy.
pub fn check_url_input(input: &str, context: UrlUseContext) -> SecurityDecision {
    match Url::parse(input) {
        Ok(url) => check_url_context(&url, context),
        Err(error) => block(
            SecurityViolationKind::UnsupportedScheme,
            format!("URL is not allowed in this context: {error}"),
        ),
    }
}

/// Conservative file access policy for local documents.
pub struct FileAccessPolicy;

impl FileAccessPolicy {
    /// A `file://` document may load itself, siblings, or descendants only.
    pub fn check(document_url: &Url, resource_url: &Url) -> SecurityDecision {
        if document_url.scheme != Scheme::File || resource_url.scheme != Scheme::File {
            return block(
                SecurityViolationKind::FileUrlAccess,
                "file access policy only allows file:// document to file:// resource checks",
            );
        }
        let Some(base_dir) = parent_dir(&document_url.path) else {
            return block(
                SecurityViolationKind::FileUrlAccess,
                "file document has no parent directory for file access policy",
            );
        };
        let resource = normalize_path(&resource_url.path);
        if resource == normalize_path(&document_url.path) || resource.starts_with(&base_dir) {
            SecurityDecision::Allow
        } else {
            block(
                SecurityViolationKind::FileUrlAccess,
                "file document may only load same-directory or descendant resources",
            )
        }
    }
}

fn normalize_path(path: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn parent_dir(path: &str) -> Option<PathBuf> {
    normalize_path(path).parent().map(Path::to_path_buf)
}

/// Mixed-content resource category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixedContentKind {
    Active,
    Passive,
}

/// Check future-facing mixed-content policy.
pub fn check_mixed_content(
    document_url: &Url,
    resource_url: &Url,
    kind: MixedContentKind,
) -> SecurityDecision {
    if document_url.scheme == Scheme::Https && resource_url.scheme == Scheme::Http {
        let label = match kind {
            MixedContentKind::Active => "active",
            MixedContentKind::Passive => "passive",
        };
        return block(
            SecurityViolationKind::MixedContent,
            format!("https document may not load http {label} content"),
        );
    }
    SecurityDecision::Allow
}

/// CSP directives supported by the tiny M16 parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CspDirective {
    DefaultSrc,
    ScriptSrc,
    StyleSrc,
    ImgSrc,
    ConnectSrc,
    FormAction,
}

impl CspDirective {
    fn parse(name: &str) -> Option<CspDirective> {
        match name {
            "default-src" => Some(CspDirective::DefaultSrc),
            "script-src" => Some(CspDirective::ScriptSrc),
            "style-src" => Some(CspDirective::StyleSrc),
            "img-src" => Some(CspDirective::ImgSrc),
            "connect-src" => Some(CspDirective::ConnectSrc),
            "form-action" => Some(CspDirective::FormAction),
            _ => None,
        }
    }
}

/// A tiny subset of CSP source expressions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceExpression {
    Self_,
    None_,
    Any,
    Scheme(String),
    Origin(Origin),
}

/// A parsed CSP policy for one protected origin.
#[derive(Debug, Clone)]
pub struct ContentSecurityPolicy {
    protected_origin: Origin,
    directives: HashMap<CspDirective, Vec<SourceExpression>>,
}

impl ContentSecurityPolicy {
    /// Evaluate whether `url` is allowed for `directive`.
    pub fn allows(&self, directive: CspDirective, url: &Url) -> SecurityDecision {
        let sources = self
            .directives
            .get(&directive)
            .or_else(|| self.directives.get(&CspDirective::DefaultSrc));
        let Some(sources) = sources else {
            return SecurityDecision::Allow;
        };
        if sources.contains(&SourceExpression::None_) {
            return block(
                SecurityViolationKind::ContentSecurityPolicy,
                "CSP blocks this resource with 'none'",
            );
        }
        if sources
            .iter()
            .any(|source| source_matches(source, &self.protected_origin, url))
        {
            SecurityDecision::Allow
        } else {
            block(
                SecurityViolationKind::ContentSecurityPolicy,
                format!("CSP blocks {} for {:?}", url.normalized(), directive),
            )
        }
    }
}

fn source_matches(source: &SourceExpression, protected_origin: &Origin, url: &Url) -> bool {
    match source {
        SourceExpression::Self_ => Origin::from_url(url)
            .map(|origin| origin == *protected_origin)
            .unwrap_or(false),
        SourceExpression::None_ => false,
        SourceExpression::Any => true,
        SourceExpression::Scheme(scheme) => url.scheme.as_str() == scheme,
        SourceExpression::Origin(origin) => Origin::from_url(url)
            .map(|candidate| candidate == *origin)
            .unwrap_or(false),
    }
}

/// Parse the tiny M16 CSP subset. Unknown directives are ignored.
pub fn parse_csp(header: &str, protected_origin: &Origin) -> MochaResult<ContentSecurityPolicy> {
    let mut directives = HashMap::new();
    for directive_text in header.split(';') {
        let mut parts = directive_text.split_whitespace();
        let Some(name) = parts.next() else {
            continue;
        };
        let Some(directive) = CspDirective::parse(name) else {
            continue;
        };
        let sources = parts
            .map(parse_source_expression)
            .collect::<MochaResult<Vec<_>>>()?;
        if sources.is_empty() {
            return Err(MochaError::Security(format!(
                "CSP directive {name} has no source expressions"
            )));
        }
        directives.insert(directive, sources);
    }
    Ok(ContentSecurityPolicy {
        protected_origin: protected_origin.clone(),
        directives,
    })
}

fn parse_source_expression(source: &str) -> MochaResult<SourceExpression> {
    match source {
        "'self'" => Ok(SourceExpression::Self_),
        "'none'" => Ok(SourceExpression::None_),
        "*" => Ok(SourceExpression::Any),
        "http:" | "https:" => Ok(SourceExpression::Scheme(
            source.trim_end_matches(':').to_string(),
        )),
        other if other.starts_with("http://") || other.starts_with("https://") => Ok(
            SourceExpression::Origin(Origin::from_url(&Url::parse(other)?)?),
        ),
        other => Err(MochaError::Security(format!(
            "unsupported CSP source expression: {other}"
        ))),
    }
}

/// Permission names reserved for current/future web APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PermissionName {
    ClipboardRead,
    ClipboardWrite,
    Geolocation,
    Notifications,
    Camera,
    Microphone,
}

/// Permission state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionState {
    Prompt,
    Granted,
    Denied,
}

/// Minimal origin-keyed permission state. There is no prompt UI in M16.
#[derive(Debug, Default)]
pub struct PermissionManager {
    states: HashMap<(Origin, PermissionName), PermissionState>,
}

impl PermissionManager {
    pub fn new() -> PermissionManager {
        PermissionManager::default()
    }

    pub fn query(&self, origin: &Origin, permission: PermissionName) -> PermissionState {
        self.states
            .get(&(origin.clone(), permission))
            .copied()
            .unwrap_or(PermissionState::Prompt)
    }

    pub fn set(&mut self, origin: Origin, permission: PermissionName, state: PermissionState) {
        self.states.insert((origin, permission), state);
    }

    pub fn clear_origin(&mut self, origin: &Origin) {
        self.states.retain(|(candidate, _), _| candidate != origin);
    }
}

/// Future-facing certificate error categories. TLS is not implemented in M16.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertificateErrorKind {
    Expired,
    NameMismatch,
    UntrustedIssuer,
    Revoked,
    Unknown,
}

/// Data needed to present a future certificate error page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateErrorPage {
    pub url: Url,
    pub kind: CertificateErrorKind,
    pub message: String,
}

impl CertificateErrorPage {
    pub fn new(url: Url, kind: CertificateErrorKind) -> CertificateErrorPage {
        let message = format!("certificate error for {}: {:?}", url.normalized(), kind);
        CertificateErrorPage { url, kind, message }
    }
}

/// Coarse capabilities for future process separation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    ReadFile,
    LoadNetwork,
    AccessProfileStorage,
    AccessCookies,
    AccessLocalStorage,
    PresentSurface,
    SpawnProcess,
}

/// A set of process/actor capabilities.
#[derive(Debug, Clone, Default)]
pub struct CapabilitySet {
    capabilities: HashSet<Capability>,
}

impl CapabilitySet {
    pub fn browser_process_default() -> CapabilitySet {
        CapabilitySet {
            capabilities: [
                Capability::ReadFile,
                Capability::LoadNetwork,
                Capability::AccessProfileStorage,
                Capability::AccessCookies,
                Capability::AccessLocalStorage,
                Capability::PresentSurface,
                Capability::SpawnProcess,
            ]
            .into_iter()
            .collect(),
        }
    }

    pub fn renderer_process_default() -> CapabilitySet {
        CapabilitySet {
            capabilities: [Capability::PresentSurface].into_iter().collect(),
        }
    }

    pub fn network_process_default() -> CapabilitySet {
        CapabilitySet {
            capabilities: [Capability::LoadNetwork].into_iter().collect(),
        }
    }

    pub fn contains(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }

    pub fn require(&self, capability: Capability) -> SecurityDecision {
        if self.contains(capability) {
            SecurityDecision::Allow
        } else {
            block(
                SecurityViolationKind::PermissionDenied,
                format!("capability denied: {capability:?}"),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(input: &str) -> Url {
        Url::parse(input).unwrap()
    }

    fn origin(input: &str) -> Origin {
        Origin::from_url(&url(input)).unwrap()
    }

    #[test]
    fn same_origin_rules_match_tuple_origin() {
        assert!(same_origin(&url("http://example.com"), &url("http://example.com:80")).unwrap());
        assert!(!same_origin(&url("http://example.com"), &url("https://example.com")).unwrap());
        assert!(!same_origin(&url("http://a.com"), &url("http://b.com")).unwrap());
        assert!(!same_origin(&url("http://example.com:8080"), &url("http://example.com")).unwrap());
        assert!(same_origin(&url("file:///tmp/a"), &url("file:///tmp/b")).is_err());
    }

    #[test]
    fn require_same_origin_blocks_cross_origin() {
        assert!(require_same_origin(&url("http://a.com"), &url("http://b.com")).is_err());
    }

    #[test]
    fn url_context_blocks_unsupported_inputs_and_external_script() {
        assert!(
            !check_url_input("javascript:alert(1)", UrlUseContext::AnchorNavigation).is_allowed()
        );
        assert!(!check_url_input("data:text/plain,hi", UrlUseContext::Image).is_allowed());
        assert!(
            !check_url_input("mailto:a@example.com", UrlUseContext::AnchorNavigation).is_allowed()
        );
        assert!(
            !check_url_context(&url("http://example.com/app.js"), UrlUseContext::Script)
                .is_allowed()
        );
        assert!(
            !check_url_context(&url("file:///tmp/a.html"), UrlUseContext::WebStorage).is_allowed()
        );
    }

    #[test]
    fn file_policy_allows_sibling_and_descendant_only() {
        let doc = url("file:///tmp/site/index.html");
        assert!(FileAccessPolicy::check(&doc, &url("file:///tmp/site/style.css")).is_allowed());
        assert!(FileAccessPolicy::check(&doc, &url("file:///tmp/site/img/a.png")).is_allowed());
        assert!(!FileAccessPolicy::check(&doc, &url("file:///tmp/secret.txt")).is_allowed());
    }

    #[test]
    fn mixed_content_blocks_https_to_http() {
        assert!(!check_mixed_content(
            &url("https://example.com"),
            &url("http://example.com/app.js"),
            MixedContentKind::Active,
        )
        .is_allowed());
        assert!(!check_mixed_content(
            &url("https://example.com"),
            &url("http://example.com/a.png"),
            MixedContentKind::Passive,
        )
        .is_allowed());
        assert!(check_mixed_content(
            &url("http://example.com"),
            &url("http://example.com/a.png"),
            MixedContentKind::Passive,
        )
        .is_allowed());
    }

    #[test]
    fn csp_default_self_allows_same_and_blocks_cross_origin() {
        let policy = parse_csp("default-src 'self'", &origin("http://example.com")).unwrap();
        assert!(policy
            .allows(CspDirective::ImgSrc, &url("http://example.com/a.png"))
            .is_allowed());
        assert!(!policy
            .allows(CspDirective::ImgSrc, &url("http://other.com/a.png"))
            .is_allowed());
    }

    #[test]
    fn csp_directives_cover_none_any_scheme_origin_and_unknown() {
        let protected = origin("http://example.com");
        let policy = parse_csp(
            "unknown-src 'none'; script-src 'none'; img-src *; style-src http:; form-action http://example.com",
            &protected,
        )
        .unwrap();
        assert!(!policy
            .allows(CspDirective::ScriptSrc, &url("http://example.com/app.js"))
            .is_allowed());
        assert!(policy
            .allows(CspDirective::ImgSrc, &url("http://other.com/a.png"))
            .is_allowed());
        assert!(policy
            .allows(CspDirective::StyleSrc, &url("http://cdn.com/a.css"))
            .is_allowed());
        assert!(policy
            .allows(CspDirective::FormAction, &url("http://example.com/post"))
            .is_allowed());
        assert!(!policy
            .allows(CspDirective::FormAction, &url("http://other.com/post"))
            .is_allowed());
    }

    #[test]
    fn malformed_csp_source_errors() {
        assert!(parse_csp("default-src nonce-abc", &origin("http://example.com")).is_err());
    }

    #[test]
    fn permission_manager_tracks_origin_state() {
        let a = origin("http://a.com");
        let b = origin("http://b.com");
        let mut manager = PermissionManager::new();
        assert_eq!(
            manager.query(&a, PermissionName::Geolocation),
            PermissionState::Prompt
        );
        manager.set(
            a.clone(),
            PermissionName::Geolocation,
            PermissionState::Granted,
        );
        manager.set(
            b.clone(),
            PermissionName::Geolocation,
            PermissionState::Denied,
        );
        assert_eq!(
            manager.query(&a, PermissionName::Geolocation),
            PermissionState::Granted
        );
        assert_eq!(
            manager.query(&b, PermissionName::Geolocation),
            PermissionState::Denied
        );
        manager.clear_origin(&a);
        assert_eq!(
            manager.query(&a, PermissionName::Geolocation),
            PermissionState::Prompt
        );
        assert_eq!(
            manager.query(&b, PermissionName::Geolocation),
            PermissionState::Denied
        );
    }

    #[test]
    fn certificate_error_page_formats_message() {
        let page =
            CertificateErrorPage::new(url("https://example.com"), CertificateErrorKind::Expired);
        assert!(page.message.contains("https://example.com/"));
        assert!(page.message.contains("Expired"));
    }

    #[test]
    fn capability_defaults_are_distinct() {
        let browser = CapabilitySet::browser_process_default();
        assert!(browser.contains(Capability::ReadFile));
        assert!(browser.contains(Capability::LoadNetwork));

        let renderer = CapabilitySet::renderer_process_default();
        assert!(!renderer.contains(Capability::ReadFile));
        assert!(!renderer.contains(Capability::LoadNetwork));
        assert!(!renderer.contains(Capability::AccessProfileStorage));
        assert!(!renderer.require(Capability::AccessCookies).is_allowed());
    }
}
