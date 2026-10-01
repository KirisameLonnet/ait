//! Filesystem local boundary.

pub mod checkout;
pub mod files;
pub mod forge;
pub(crate) mod git;
pub mod github_projects;
pub mod provisioning;
pub mod workspace_runtime;
pub mod worktrees;

/// Orchestration skill installation and selection.
pub mod skills;
