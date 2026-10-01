//! Metadata ports owned by the independent server.

pub mod daemon;
pub mod generation;
pub mod provisioning;
pub mod registry;
pub mod workspace_automation;
pub mod workspace_git;
pub mod workspace_labels;
pub mod workspace_runtime;
pub mod workspace_state;
pub mod worktrees;

/// Durable push token storage.
pub mod push;
