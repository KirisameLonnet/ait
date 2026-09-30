//! OpenCode session plugin: owned HTTP runtime, native history and input reconciliation.
//!
//! The application admits input durably between `open` and `start`. HTTP acknowledgement
//! and SSE notifications are observations; only an idle, complete history can be published.
use std::{path::PathBuf, sync::Arc};

use ait_domain::{DomainError, ErrorCode, ProviderModel};
use ait_ports::{
    NativeSessionConnection, NativeSessionInvocation, NativeSessionSnapshot, NativeSessionWriter,
    WorkspaceProgressReporter,
};
use async_trait::async_trait;

mod approvals;
mod budget;
mod execution;
mod history;
mod http;
mod runtime;
mod session;

/// Dedicated `OpenCode` runtime factory; provider authentication stays with `OpenCode`.
#[derive(Clone, Debug)]
pub struct OpenCodeAdapter {
    binary: PathBuf,
    limits: OpenCodeExecutionLimits,
}

/// Observation ceilings for native execution; history before this input is excluded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpenCodeExecutionLimits {
    /// Maximum newly generated native content items.
    pub max_steps: u64,
    /// Maximum observed input, output and reasoning tokens.
    pub max_tokens: u64,
    /// Maximum response and newly generated text bytes, up to 8 MiB.
    pub max_output_bytes: usize,
}

impl Default for OpenCodeExecutionLimits {
    fn default() -> Self {
        Self {
            max_steps: 128,
            max_tokens: 1_000_000,
            max_output_bytes: 8 * 1024 * 1024,
        }
    }
}

impl OpenCodeAdapter {
    /// Uses an installed executable directly, without a shell or provider token.
    #[must_use]
    pub fn new(binary: PathBuf) -> Self {
        Self {
            binary,
            limits: OpenCodeExecutionLimits::default(),
        }
    }

    /// Applies host ceilings before starting a runtime.
    ///
    /// # Errors
    /// Rejects zero ceilings and output limits exceeding the bounded HTTP transport.
    pub fn with_execution_limits(
        mut self,
        limits: OpenCodeExecutionLimits,
    ) -> Result<Self, DomainError> {
        if limits.max_steps == 0
            || limits.max_tokens == 0
            || limits.max_output_bytes == 0
            || limits.max_output_bytes > http::MAX_BODY
        {
            return Err(failure(
                ErrorCode::AgentCapabilityUnsupported,
                "invalid OpenCode execution limits",
            ));
        }
        self.limits = limits;
        Ok(self)
    }

    /// Discovers connected models and reasoning variants through the native HTTP API.
    ///
    /// # Errors
    /// Returns a bounded, non-secret error for unavailable or incompatible runtimes.
    pub async fn discover_models(&self, cwd: PathBuf) -> Result<Vec<ProviderModel>, DomainError> {
        let cancel = tokio_util::sync::CancellationToken::new();
        let mut runtime = runtime::Runtime::spawn(&self.binary, &cwd, &cancel).await?;
        let result = runtime.api.models().await;
        runtime.close().await;
        result
    }
}

fn failure(code: ErrorCode, message: &str) -> DomainError {
    DomainError::invariant(code, message)
}

#[async_trait]
impl NativeSessionWriter for OpenCodeAdapter {
    async fn open(
        &self,
        mut invocation: NativeSessionInvocation,
    ) -> Result<Box<dyn NativeSessionConnection>, DomainError> {
        session::validate(&invocation)?;
        let mut runtime =
            runtime::Runtime::spawn(&self.binary, &invocation.cwd, &invocation.cancellation)
                .await?;
        let prepared = session::prepare(&runtime.api, &invocation).await;
        match prepared {
            Ok(prepared) => {
                invocation.input_id.clone_from(&prepared.input_id);
                Ok(Box::new(session::Connection {
                    runtime,
                    invocation,
                    prepared,
                    submitted: false,
                    limits: self.limits,
                }))
            }
            Err(error) => {
                runtime.close().await;
                Err(error)
            }
        }
    }
}

#[async_trait]
impl NativeSessionConnection for session::Connection {
    fn prepared(&self) -> &NativeSessionSnapshot {
        &self.prepared
    }

    async fn start(
        &mut self,
        progress: Arc<dyn WorkspaceProgressReporter>,
    ) -> Result<NativeSessionSnapshot, DomainError> {
        self.execute(progress).await
    }

    async fn read(&mut self) -> Result<NativeSessionSnapshot, DomainError> {
        session::snapshot(&self.runtime.api, &self.prepared.id, &self.invocation).await
    }

    async fn close(&mut self) {
        self.runtime.close().await;
    }
}

#[cfg(test)]
mod tests;
