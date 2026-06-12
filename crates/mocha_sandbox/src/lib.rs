//! Capability-based renderer sandbox prototype (Milestone 18).
//!
//! This crate models the renderer sandbox boundary Mocha wants, but it is not a
//! production OS sandbox. The portable M18 implementation is
//! capability-restricted only: the browser process owns privileged I/O and the
//! renderer receives prepared document data.

use mocha_error::{MochaError, MochaResult};
use mocha_ipc::PreparedDocument;
use mocha_origin::Origin;
use mocha_security::{Capability, CapabilitySet, SecurityDecision, SecurityViolationKind};
use mocha_url::Url;

/// Renderer sandbox mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxMode {
    UnsandboxedLegacy,
    CapabilityRestricted,
    PlatformSandboxed,
}

/// Status reported after applying platform/capability sandbox setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxStatus {
    NotApplied,
    CapabilityRestrictedOnly,
    PlatformApplied,
}

/// Renderer sandbox policy.
#[derive(Debug, Clone)]
pub struct RendererSandboxPolicy {
    pub mode: SandboxMode,
    pub allowed_capabilities: CapabilitySet,
    pub document_origin: Option<Origin>,
    pub allow_file_reads: bool,
    pub allow_network_loads: bool,
    pub allow_profile_storage: bool,
    pub allow_process_spawn: bool,
}

impl RendererSandboxPolicy {
    /// Legacy M17 policy. This is explicitly unsandboxed.
    pub fn legacy_unsandboxed() -> RendererSandboxPolicy {
        RendererSandboxPolicy {
            mode: SandboxMode::UnsandboxedLegacy,
            allowed_capabilities: CapabilitySet::browser_process_default(),
            document_origin: None,
            allow_file_reads: true,
            allow_network_loads: true,
            allow_profile_storage: true,
            allow_process_spawn: true,
        }
    }

    /// Default M18 renderer policy: no direct file/network/profile/spawn.
    pub fn default_renderer() -> RendererSandboxPolicy {
        RendererSandboxPolicy {
            mode: SandboxMode::CapabilityRestricted,
            allowed_capabilities: CapabilitySet::renderer_process_default(),
            document_origin: None,
            allow_file_reads: false,
            allow_network_loads: false,
            allow_profile_storage: false,
            allow_process_spawn: false,
        }
    }

    /// Browser-process policy for privileged operations.
    pub fn browser_process() -> RendererSandboxPolicy {
        RendererSandboxPolicy {
            mode: SandboxMode::UnsandboxedLegacy,
            allowed_capabilities: CapabilitySet::browser_process_default(),
            document_origin: None,
            allow_file_reads: true,
            allow_network_loads: true,
            allow_profile_storage: true,
            allow_process_spawn: true,
        }
    }

    pub fn check_capability(&self, capability: Capability) -> SecurityDecision {
        match capability {
            Capability::ReadFile if !self.allow_file_reads => denied("direct file reads"),
            Capability::LoadNetwork if !self.allow_network_loads => denied("direct network loads"),
            Capability::AccessProfileStorage if !self.allow_profile_storage => {
                denied("direct profile storage access")
            }
            Capability::SpawnProcess if !self.allow_process_spawn => denied("process spawning"),
            _ => self.allowed_capabilities.require(capability),
        }
    }

    pub fn require_capability(&self, capability: Capability) -> MochaResult<()> {
        self.check_capability(capability).into_result()
    }

    pub fn allows_direct_document_load(&self) -> bool {
        self.allow_file_reads || self.allow_network_loads
    }
}

fn denied(what: &str) -> SecurityDecision {
    SecurityDecision::Block(mocha_security::SecurityViolation {
        kind: SecurityViolationKind::PermissionDenied,
        message: format!("sandbox denied {what}"),
    })
}

/// Platform sandbox hook. M18's portable implementation is capability-only.
pub trait PlatformSandbox {
    fn apply(policy: &RendererSandboxPolicy) -> MochaResult<SandboxStatus>;
}

/// No-op platform hook used until real OS sandboxing is implemented.
pub struct NoopPlatformSandbox;

impl PlatformSandbox for NoopPlatformSandbox {
    fn apply(policy: &RendererSandboxPolicy) -> MochaResult<SandboxStatus> {
        Ok(match policy.mode {
            SandboxMode::UnsandboxedLegacy => SandboxStatus::NotApplied,
            SandboxMode::CapabilityRestricted | SandboxMode::PlatformSandboxed => {
                SandboxStatus::CapabilityRestrictedOnly
            }
        })
    }
}

/// Prepare already-loaded HTML for the sandboxed renderer path.
pub fn prepare_document(final_url: Option<&Url>, html: impl Into<String>) -> PreparedDocument {
    PreparedDocument {
        final_url: final_url.map(Url::normalized),
        html: html.into(),
    }
}

/// Return a clear sandbox violation.
pub fn sandbox_violation(message: impl Into<String>) -> MochaError {
    MochaError::Security(format!("sandbox violation: {}", message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_renderer_policy_denies_privileged_capabilities() {
        let policy = RendererSandboxPolicy::default_renderer();
        assert!(!policy.check_capability(Capability::ReadFile).is_allowed());
        assert!(!policy
            .check_capability(Capability::LoadNetwork)
            .is_allowed());
        assert!(!policy
            .check_capability(Capability::AccessProfileStorage)
            .is_allowed());
        assert!(!policy
            .check_capability(Capability::SpawnProcess)
            .is_allowed());
    }

    #[test]
    fn browser_policy_allows_privileged_capabilities() {
        let policy = RendererSandboxPolicy::browser_process();
        assert!(policy.check_capability(Capability::ReadFile).is_allowed());
        assert!(policy
            .check_capability(Capability::LoadNetwork)
            .is_allowed());
        assert!(policy
            .check_capability(Capability::AccessProfileStorage)
            .is_allowed());
        assert!(policy
            .check_capability(Capability::SpawnProcess)
            .is_allowed());
    }

    #[test]
    fn platform_hook_is_honest_capability_only() {
        let status =
            NoopPlatformSandbox::apply(&RendererSandboxPolicy::default_renderer()).unwrap();
        assert_eq!(status, SandboxStatus::CapabilityRestrictedOnly);
    }

    #[test]
    fn prepared_document_carries_html_and_final_url() {
        let url = Url::parse("http://example.com/index.html").unwrap();
        let prepared = prepare_document(Some(&url), "<html><body>ok</body></html>");
        assert_eq!(
            prepared.final_url.as_deref(),
            Some("http://example.com/index.html")
        );
        assert!(prepared.html.contains("ok"));
    }
}
