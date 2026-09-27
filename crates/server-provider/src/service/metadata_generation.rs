//! Shared bounded structured generation; auxiliary sessions never enter the Agent registry.

mod candidates;
mod prompts;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use server_domain::agent_runtime::StoredAgentConfig;
use server_metadata::ports::daemon::DaemonConfigStore;
use server_metadata::ports::generation::{
    MetadataError, MetadataFuture, MetadataGenerator, MetadataRequest,
};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::ports::agent_session::{AgentClient, AgentSessionSpec};

/// Model-backed metadata generator shared by all four wording use cases.
#[derive(Debug)]
pub struct Generation {
    config: Arc<dyn DaemonConfigStore>,
    clients: BTreeMap<String, Arc<dyn AgentClient>>,
    permits: Semaphore,
    queue: Semaphore,
    cancel: CancellationToken,
    deadline: Duration,
}

impl Generation {
    /// Use live `config` and registered `clients`, allowing two concurrent 90-second operations.
    /// Clients retain provider credentials in their native authentication stores.
    #[must_use]
    pub fn new(config: Arc<dyn DaemonConfigStore>, clients: Vec<Arc<dyn AgentClient>>) -> Self {
        Self {
            config,
            clients: clients
                .into_iter()
                .map(|client| (client.provider().to_owned(), client))
                .collect(),
            permits: Semaphore::new(2),
            queue: Semaphore::new(32),
            cancel: CancellationToken::new(),
            deadline: Duration::from_secs(90),
        }
    }

    async fn run(&self, request: MetadataRequest) -> Result<Value, MetadataError> {
        let _queued = self
            .queue
            .try_acquire()
            .map_err(|_| MetadataError::Cancelled)?;
        let _permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| MetadataError::Cancelled)?;
        if self.cancel.is_cancelled() || request.context.len() > 1024 * 1024 {
            return Err(MetadataError::Cancelled);
        }
        let config = self.config.clone();
        let input = request.clone();
        let (config, prompt) = tokio::task::spawn_blocking(move || {
            Ok((
                config.get().map_err(|_| MetadataError::Unavailable)?,
                prompts::build(&input),
            ))
        })
        .await
        .map_err(|_| MetadataError::Unavailable)??;
        let candidates = candidates::resolve(&self.clients, &config, &request).await;
        let schema = prompts::schema(request.kind);
        for candidate in candidates {
            let Some(client) = self.clients.get(&candidate.provider) else {
                continue;
            };
            let spec = AgentSessionSpec {
                provider: candidate.provider,
                cwd: request.cwd.clone(),
                config: StoredAgentConfig {
                    model: candidate.model,
                    thinking_option_id: candidate.thinking_option_id,
                    ..StoredAgentConfig::default()
                },
            };
            // Match Paseo's two repair attempts; transport failures move to the next candidate.
            for attempt in 0..3 {
                let text = if attempt == 0 {
                    prompt.clone()
                } else {
                    format!(
                        "{prompt}\n\nThe previous response was invalid. Return only JSON matching the supplied schema."
                    )
                };
                let output = tokio::time::timeout(
                    Duration::from_secs(25),
                    client.generate_metadata(&spec, &text, &schema),
                )
                .await;
                let Ok(Ok(output)) = output else {
                    break;
                };
                if let Some(value) = prompts::parse(request.kind, &output) {
                    return Ok(value);
                }
            }
        }
        Err(MetadataError::Unavailable)
    }
}

impl MetadataGenerator for Generation {
    fn generate(&self, request: MetadataRequest) -> MetadataFuture<'_> {
        Box::pin(async move {
            tokio::select! {
                biased;
                () = self.cancel.cancelled() => Err(MetadataError::Cancelled),
                result = tokio::time::timeout(self.deadline, self.run(request)) => {
                    result.unwrap_or(Err(MetadataError::Unavailable))
                }
            }
        })
    }

    fn shutdown(&self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
mod tests;
