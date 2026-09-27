//! Worktree provisioning consumed by the unified Workspace creation workflow.

use std::fmt::Debug;

use crate::model::registry::{PersistedProjectRecord, PersistedWorkspaceRecord};

/// Branch selection independent of the wire protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeAction {
    /// Create a branch from a selected base.
    BranchOff,
    /// Check out an existing branch.
    Checkout,
}

/// Creation intent, including the identity reserved by the creation coordinator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeCreation {
    /// Source directory, or the selected project's root when omitted.
    pub cwd: Option<String>,
    /// Explicit active owning project.
    pub project_id: Option<String>,
    /// Reserved workspace identity.
    pub workspace_id: Option<String>,
    /// Explicit workspace title.
    pub title: Option<String>,
    /// Managed directory name seed.
    pub worktree_slug: Option<String>,
    /// Base or checkout reference.
    pub ref_name: Option<String>,
    /// Default base override when no reference is selected.
    pub base_branch: Option<String>,
    /// Explicit new branch name, independent of the directory name.
    pub branch_name: Option<String>,
    /// Branch creation or checkout.
    pub action: WorktreeAction,
    /// Whether unsupported forge checkout input was supplied.
    pub has_change_request_source: bool,
    /// Provisional title source when no explicit title was supplied.
    pub first_agent_prompt: Option<String>,
    /// Whether an initial Agent will follow creation.
    pub expects_initial_agent: bool,
}

/// Registered workspace and its owning project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedWorktreeWorkspace {
    /// Persisted workspace placement.
    pub workspace: PersistedWorkspaceRecord,
    /// Project used for descriptor projection.
    pub project: PersistedProjectRecord,
}

/// Safe provisioning failure returned to the creation workflow.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct WorktreeCreationError {
    /// Stable wire error category.
    pub code: &'static str,
    /// Safe business error description.
    pub message: String,
}

/// Blocking adapter that creates Git placement and registers it atomically with rollback.
pub trait WorktreeProvisioning: Debug + Send + Sync {
    /// Create the requested workspace using the reserved identity and timestamp.
    ///
    /// # Errors
    /// Returns validation, Git, registration, or rollback failures.
    fn create(
        &self,
        input: &WorktreeCreation,
        timestamp: &str,
    ) -> Result<CreatedWorktreeWorkspace, WorktreeCreationError>;
}
