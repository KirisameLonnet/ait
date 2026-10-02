//! Structured wording generation shared by metadata, filesystem and provider use cases.

use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The four independently configurable Paseo metadata styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataKind {
    /// A conversation's display title.
    Title,
    /// A workspace title and independently generated Git branch.
    BranchName,
    /// A Git commit subject.
    CommitMessage,
    /// A pull request title and Markdown body.
    PullRequest,
}

/// An ordered provider/model selection without credentials or foreground permissions.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataSelection {
    /// Registered provider identity.
    pub provider: String,
    /// Explicit model, or the provider default.
    pub model: Option<String>,
    /// Provider reasoning option, when selected.
    pub thinking_option_id: Option<String>,
}

/// Owned input to a bounded, non-persisted metadata generation operation.
#[derive(Debug, Clone)]
pub struct MetadataRequest {
    /// Artifact to generate and validate.
    pub kind: MetadataKind,
    /// Existing working directory for model discovery and project styles.
    pub cwd: String,
    /// Prompt/attachments or a bounded Git diff, treated as source material.
    pub context: String,
    /// Current foreground selection, tried after configured and automatic candidates.
    pub selection: Option<MetadataSelection>,
}

/// Safe metadata failures; raw model output and diagnostics never become public errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MetadataError {
    /// No candidate produced valid output within the operation's budget.
    #[error("metadata generation unavailable")]
    Unavailable,
    /// A bounded queue is full or the server is shutting down.
    #[error("metadata generation busy or cancelled")]
    Cancelled,
}

/// Sendable generation future with validated JSON output for the requested artifact.
pub type MetadataFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Value, MetadataError>> + Send + 'a>>;

/// Outbound model boundary. Implementations own budgets, cancellation and native cleanup.
pub trait MetadataGenerator: Debug + Send + Sync {
    /// Generate the artifact in `request`, or return a safe budget/provider failure.
    fn generate(&self, request: MetadataRequest) -> MetadataFuture<'_>;

    /// Cancel outstanding auxiliary work when the server drains. Does not cancel user turns.
    fn shutdown(&self);
}

/// Blocking Git boundary for renaming a still-eligible managed placeholder branch.
pub trait WorkspaceBranchNamer: Debug + Send + Sync {
    /// Rename only when `cwd` remains managed and its current branch equals `expected`.
    /// Returns the chosen collision-free branch, or none when no safe rename is possible.
    fn rename(&self, cwd: &str, expected: &str, desired: &str) -> Option<String>;
}
