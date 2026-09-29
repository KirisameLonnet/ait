//! Wait ownership retains completion text without keeping a closed native session alive.

use super::{AgentExecution, Command, Duration, ErrorCode, Value, json, mpsc, oneshot};
use crate::protocol::agent_execution::WaitRequest;
use crate::rpc::agent_execution::only;

/// Resolved identity and optional live-turn text retained only for this wait's lifetime.
pub(crate) struct WaitObservation {
    /// Stable public Agent identity.
    pub(crate) id: String,
    /// Missing when the request observes only a stored Agent.
    pub(crate) last_message: Option<tokio::sync::watch::Receiver<Option<String>>>,
}

impl AgentExecution {
    pub(super) async fn wait(&self, params: Value) -> Result<Value, ErrorCode> {
        only(&params, &["agentId", "timeoutMs"])?;
        let request: WaitRequest =
            serde_json::from_value(params).map_err(|_| ErrorCode::InvalidMessage)?;
        if request
            .timeout_ms
            .is_some_and(|timeout| timeout == 0 || timeout > 9_007_199_254_740_991)
        {
            return Err(ErrorCode::InvalidMessage);
        }
        let deadline = request
            .timeout_ms
            .map(|timeout| {
                tokio::time::Instant::now()
                    .checked_add(Duration::from_millis(timeout))
                    .ok_or(ErrorCode::InvalidMessage)
            })
            .transpose()?;
        let observation = match self.observe_wait(request.agent_id).await {
            Ok(observation) => observation,
            Err(ErrorCode::AgentNotFound) => {
                return Ok(json!({"status":"error","final":null,
                    "error":"Agent not found","lastMessage":null}));
            }
            Err(error) => return Err(error),
        };
        loop {
            let mut result = self
                .call(
                    "agent.finish.wait.request",
                    json!({"agentId":observation.id}),
                )
                .await?;
            if result["status"] != "running" {
                if result["lastMessage"].is_null()
                    && result["final"]["archivedAt"].is_string()
                    && let Some(message) = &observation.last_message
                {
                    result["lastMessage"] = json!(message.borrow().clone());
                }
                return Ok(result);
            }
            if deadline.is_some_and(|deadline| tokio::time::Instant::now() >= deadline) {
                result["status"] = json!("timeout");
                result["lastMessage"] = Value::Null;
                result["error"] = Value::Null;
                return Ok(result);
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    async fn observe_wait(&self, identifier: String) -> Result<WaitObservation, ErrorCode> {
        let (reply, receiver) = oneshot::channel();
        self.0
            .sender
            .try_send(Command::ObserveWait { identifier, reply })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => ErrorCode::CatalogBusy,
                mpsc::error::TrySendError::Closed(_) => ErrorCode::AgentIo,
            })?;
        receiver.await.map_err(|_| ErrorCode::AgentIo)?
    }
}
