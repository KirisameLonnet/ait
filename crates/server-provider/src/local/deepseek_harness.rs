//! `DeepSeek` Harness sessions over ACP v1 JSON-RPC stdio, following Paseo's ACP adapter.

mod config;
mod permissions;
mod session;
mod streaming;
mod transport;

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};
use server_domain::agent_runtime::{AgentPersistenceHandle, StoredAgentConfig};

use crate::ports::agent_session::{
    AgentClient, AgentResumePurpose, AgentSession, AgentSessionError, AgentSessionFuture,
    AgentSessionSpec,
};
use crate::ports::environment::AgentEnvironment;
use crate::protocol::provider::Details;

/// Stable provider identity used by creation, discovery and durable resume handles.
pub const PROVIDER: &str = "deepseek-harness";

/// Local Harness launcher; the ACP profile owns credentials, tools and native persistence.
#[derive(Debug, Clone)]
pub struct DeepSeekHarnessClient {
    program: PathBuf,
    deadline: Duration,
    environment: AgentEnvironment,
    images: super::images::ImageStore,
}

impl DeepSeekHarnessClient {
    /// Launch `program --profile acp` without a shell, inheriting Harness configuration.
    /// Control operations have a thirty-second deadline; prompts have no duration limit.
    #[must_use]
    pub fn new(program: PathBuf) -> Self {
        Self {
            program,
            deadline: Duration::from_secs(30),
            environment: AgentEnvironment::default(),
            images: super::images::ImageStore::default(),
        }
    }

    /// Materialize native image output in `directory` for live display and saved timelines.
    #[must_use]
    pub fn with_image_directory(mut self, directory: PathBuf) -> Self {
        self.images = super::images::ImageStore::new(directory);
        self
    }

    async fn probe(&self, spec: &AgentSessionSpec) -> Result<Details, AgentSessionError> {
        let mut session = session::open(self, spec, None).await?;
        let details = config::details(&session.options);
        let closed = session.close().await;
        let details = details?;
        closed?;
        Ok(details)
    }
}

impl AgentClient for DeepSeekHarnessClient {
    fn provider(&self) -> &'static str {
        PROVIDER
    }

    fn supports_history_replay(&self) -> bool {
        false
    }

    fn validate_config(&self, config: &StoredAgentConfig) -> Result<(), AgentSessionError> {
        config::validate(config)
    }

    fn settings(&self, _config: &StoredAgentConfig) -> Value {
        json!({"availableModes":[],"features":[],"capabilities":{
            "supportsMcpServers":true,"supportsStreaming":true,"supportsReasoningStream":true,
            "supportsDynamicModes":false,"supportsSessionListing":false,
            "supportsRewindConversation":false,"supportsRewindFiles":false,"supportsRewindBoth":false}})
    }

    fn is_available(&self) -> AgentSessionFuture<'_, bool> {
        Box::pin(async { Ok(super::configuration::executable(&self.program)) })
    }

    fn diagnostic(&self) -> AgentSessionFuture<'_, String> {
        Box::pin(async move {
            if !self.is_available().await? {
                return Ok("DeepSeek Harness executable is unavailable".to_owned());
            }
            let cwd = std::env::current_dir().map_err(|_| AgentSessionError::Failed)?;
            let details = self
                .discover(cwd.to_str().ok_or(AgentSessionError::Failed)?)
                .await?;
            Ok(format!(
                "DeepSeek Harness ACP v1 ready; {} models",
                details.models.len()
            ))
        })
    }

    fn discover<'a>(&'a self, cwd: &'a str) -> AgentSessionFuture<'a, Details> {
        Box::pin(async move {
            self.probe(&AgentSessionSpec {
                provider: PROVIDER.to_owned(),
                cwd: cwd.to_owned(),
                config: StoredAgentConfig::default(),
            })
            .await
        })
    }

    fn validate_selection<'a>(&'a self, spec: &'a AgentSessionSpec) -> AgentSessionFuture<'a, ()> {
        Box::pin(async move { self.probe(spec).await.map(|_| ()) })
    }

    fn create_session<'a>(
        &'a self,
        spec: &'a AgentSessionSpec,
    ) -> AgentSessionFuture<'a, Box<dyn AgentSession>> {
        Box::pin(async move {
            Ok(Box::new(session::open(self, spec, None).await?) as Box<dyn AgentSession>)
        })
    }

    fn create_session_with_environment<'a>(
        &'a self,
        spec: &'a AgentSessionSpec,
        environment: &'a AgentEnvironment,
    ) -> AgentSessionFuture<'a, Box<dyn AgentSession>> {
        let mut client = self.clone();
        client.environment = environment.clone();
        Box::pin(async move { client.create_session(spec).await })
    }

    fn resume_session<'a>(
        &'a self,
        handle: &'a AgentPersistenceHandle,
        spec: &'a AgentSessionSpec,
        purpose: AgentResumePurpose,
    ) -> AgentSessionFuture<'a, Box<dyn AgentSession>> {
        Box::pin(async move {
            if purpose == AgentResumePurpose::History {
                return Err(AgentSessionError::Unavailable);
            }
            Ok(Box::new(session::open(self, spec, Some(handle)).await?) as Box<dyn AgentSession>)
        })
    }
}

#[cfg(all(test, unix))]
mod tests;
