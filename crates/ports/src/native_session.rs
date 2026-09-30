//! Persistent coding-agent sessions, independent of provider transport and Ait Message identity.
use std::{path::PathBuf, sync::Arc};

use ait_domain::{DomainError, MessageRole, RunPermissionProfile, SubMessage, ToolResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::{ProjectExecution, WorkspaceApproval, WorkspaceProgressReporter};

/// Admission configuration; opening a connection must never submit this input.
#[derive(Clone)]
pub struct NativeSessionInvocation {
    /// Stable plugin identifier.
    pub driver: String,
    /// Stable Ait Run identity used only for host approval routing.
    pub request_id: String,
    /// Existing provider session; absence creates a persistent session.
    pub session_id: Option<String>,
    /// Provider-compatible input identity persisted before submission.
    pub input_id: String,
    /// New user input, without replayed conversation history.
    pub prompt: String,
    /// Instructions installed only on a new session.
    pub instructions: Option<String>,
    /// Canonical session working directory.
    pub cwd: PathBuf,
    /// Provider-specific model identifier.
    pub model: String,
    /// Provider-specific reasoning variant.
    pub reasoning_effort: Option<String>,
    /// Frozen host permission ceiling.
    pub permission_profile: RunPermissionProfile,
    /// Worker process ownership evidence.
    pub project_execution: Option<Arc<dyn ProjectExecution>>,
    /// Run-scoped durable approval boundary.
    pub approvals: Arc<dyn WorkspaceApproval>,
    /// Explicit cooperative cancellation.
    pub cancellation: CancellationToken,
}

/// One terminal provider message; identities remain in the provider namespace.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NativeHistoryMessage {
    /// Provider message identity, including a suffix for synthetic tool results.
    pub id: String,
    /// Domain-compatible role.
    pub role: MessageRole,
    /// Ordered content normalized by the plugin.
    pub sub_messages: Vec<SubMessage>,
    /// A terminal result for an assistant `ToolUse`, when present.
    pub tool_result: Option<ToolResult>,
    /// Provider-owned input correlation, never an Ait Message identity.
    pub input_id: Option<String>,
    /// Native timestamp in milliseconds.
    pub created_at: i64,
    /// Bounded, non-secret native record for audit and reconciliation.
    pub metadata: Value,
}

/// Provider execution outcome observed after all native work becomes idle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeSessionOutcome {
    /// Provider drained its execution and queue successfully.
    Completed,
    /// Provider reported an execution failure.
    Failed,
    /// Provider confirmed interruption.
    Interrupted,
}

/// Complete, idle history read through the owned provider runtime.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NativeSessionSnapshot {
    /// Stable plugin identifier.
    pub driver: String,
    /// Provider session identity.
    pub id: String,
    /// Input identity selected by this runtime and persisted before submission.
    pub input_id: String,
    /// Effective native working directory.
    pub cwd: PathBuf,
    /// Effective model verified before admission.
    pub model: String,
    /// Effective variant verified before admission.
    pub reasoning_effort: Option<String>,
    /// Full history, including native-generated inputs and compaction records.
    pub messages: Vec<NativeHistoryMessage>,
    /// Latest native execution outcome, absent for a new empty session.
    pub outcome: Option<NativeSessionOutcome>,
}

/// Owned persistent provider runtime, prepared before durable input admission.
#[async_trait]
pub trait NativeSessionConnection: Send {
    /// Verified idle history; no input has been submitted.
    fn prepared(&self) -> &NativeSessionSnapshot;
    /// Submits the admitted input at most once and reconciles complete idle history.
    ///
    /// # Errors
    /// Transport ambiguity must never trigger input replay.
    async fn start(
        &mut self,
        progress: Arc<dyn WorkspaceProgressReporter>,
    ) -> Result<NativeSessionSnapshot, DomainError>;
    /// Reads authoritative idle history without submitting input.
    ///
    /// # Errors
    /// Active or incomplete histories cannot be published.
    async fn read(&mut self) -> Result<NativeSessionSnapshot, DomainError>;
    /// Stops and reaps the owned runtime before releasing the host execution lease.
    async fn close(&mut self);
}

/// Plugin registry implemented by the supervised worker boundary.
#[async_trait]
pub trait NativeSessionWriter: Send + Sync {
    /// Opens one owned runtime and verifies session identity, configuration and idle history.
    ///
    /// # Errors
    /// Unsupported plugins or permission profiles fail before submission.
    async fn open(
        &self,
        invocation: NativeSessionInvocation,
    ) -> Result<Box<dyn NativeSessionConnection>, DomainError>;
}
