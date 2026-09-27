//! Adapter from metadata's unified creation port to the shared worktree service.

use std::sync::{Arc, Mutex};

use server_metadata::ports::worktrees::{
    CreatedWorktreeWorkspace, WorktreeAction, WorktreeCreation, WorktreeCreationError,
    WorktreeProvisioning,
};

use super::{CreateAction, CreateWorktree, Worktrees, WorktreesError, is_active_project};
use crate::ports::worktrees::WorktreeError;

/// Metadata adapter sharing the same serialized service as the worktree RPCs.
#[derive(Debug, Clone)]
pub struct WorkspaceWorktrees {
    worktrees: Arc<Mutex<Worktrees>>,
}

impl WorkspaceWorktrees {
    /// Bind unified workspace creation to an existing shared worktree service.
    #[must_use]
    pub fn new(worktrees: Arc<Mutex<Worktrees>>) -> Self {
        Self { worktrees }
    }
}

impl WorktreeProvisioning for WorkspaceWorktrees {
    fn create(
        &self,
        input: &WorktreeCreation,
        timestamp: &str,
    ) -> Result<CreatedWorktreeWorkspace, WorktreeCreationError> {
        let worktrees = self
            .worktrees
            .lock()
            .map_err(|_| failure(&WorktreesError::Registry))?;
        let project = input
            .project_id
            .as_deref()
            .map(|id| {
                let project = worktrees
                    .projects
                    .get(id)
                    .map_err(|_| WorktreesError::Registry)?
                    .ok_or_else(|| WorktreesError::UnknownProject(id.to_owned()))?;
                if !is_active_project(&project) {
                    return Err(WorktreesError::ArchivedProject(id.to_owned()));
                }
                Ok(project)
            })
            .transpose()
            .map_err(|error| failure(&error))?;
        let cwd = input
            .cwd
            .as_deref()
            .filter(|cwd| !cwd.trim().is_empty())
            .or_else(|| project.as_ref().map(|project| project.root_path.as_str()))
            .ok_or_else(|| WorktreeCreationError {
                code: "source_required",
                message: "cwd or projectId is required for a worktree-backed workspace".to_owned(),
            })?;
        let created = worktrees
            .create(
                &CreateWorktree {
                    cwd: cwd.to_owned(),
                    project_id: input.project_id.clone(),
                    workspace_id: input.workspace_id.clone(),
                    title: input.title.clone(),
                    branch_name: input.branch_name.clone(),
                    base_branch: input.base_branch.clone(),
                    worktree_slug: input.worktree_slug.clone(),
                    ref_name: input.ref_name.clone(),
                    action: match input.action {
                        WorktreeAction::BranchOff => CreateAction::BranchOff,
                        WorktreeAction::Checkout => CreateAction::Checkout,
                    },
                    has_change_request_source: input.has_change_request_source,
                    first_agent_prompt: input.first_agent_prompt.clone(),
                    expects_initial_agent: input.expects_initial_agent,
                },
                timestamp,
            )
            .map_err(|error| failure(&error))?;
        Ok(CreatedWorktreeWorkspace {
            workspace: created.workspace,
            project: created.project,
        })
    }
}

fn failure(error: &WorktreesError) -> WorktreeCreationError {
    let code = match error {
        WorktreesError::Worktree(error) => match error {
            WorktreeError::NotGitRepository => "not_git_repository",
            WorktreeError::NotAllowed => "not_allowed",
            WorktreeError::BranchAlreadyCheckedOut(_) => "branch_already_checked_out",
            WorktreeError::MissingCheckoutTarget => "missing_checkout_target",
            WorktreeError::UnknownBranch(_) => "unknown_branch",
            WorktreeError::ForgeUnavailable => "unsupported_capability",
            WorktreeError::Invalid(_) => "invalid_request",
            WorktreeError::Io(_) => "unknown",
        },
        WorktreesError::Registry => "registry_io",
        WorktreesError::UnknownProject(_) => "unknown_project",
        WorktreesError::ArchivedProject(_) => "archived_project",
        WorktreesError::Rollback { cause, .. } => failure(cause).code,
    };
    WorktreeCreationError {
        code,
        message: error.to_string(),
    }
}
