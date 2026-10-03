//! Asynchronous native approvals retain exact request identity and authorize only one operation.
use std::collections::HashMap;

use crate::local::opencode::types::{ApprovalRequest, Decision, Invocation};
use crate::local::opencode::types::{ApprovalTarget, Fault, ProtocolError};
use reqwest::Method;
use serde_json::{Value, json};
use tokio_util::task::AbortOnDropHandle;

use super::{
    failure,
    http::{Api, Version, required_string},
    session::valid_id,
};

pub(super) struct Pending {
    tasks: HashMap<String, Task>,
    sender: tokio::sync::mpsc::Sender<Result<(), ProtocolError>>,
    pub(super) receiver: tokio::sync::mpsc::Receiver<Result<(), ProtocolError>>,
}

struct Task {
    handle: AbortOnDropHandle<()>,
    approval: Option<ApprovalRequest>,
}

impl Pending {
    pub(super) fn new() -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(16);
        Self {
            tasks: HashMap::new(),
            sender,
            receiver,
        }
    }

    pub(super) async fn reconcile(
        &mut self,
        api: &Api,
        request: &Invocation,
        session: &str,
    ) -> Result<(), ProtocolError> {
        let path = match api.version {
            Version::V1 => "/permission".to_owned(),
            Version::V2 => api.path(session, "/permission"),
        };
        let response = api.json(Method::GET, &path, None).await?;
        let pending = api.data(&response).as_array().ok_or_else(|| {
            failure(
                Fault::ProviderFailed,
                "invalid OpenCode pending permissions",
            )
        })?;
        for item in pending {
            if item.get("sessionID").and_then(Value::as_str) == Some(session) {
                self.observe(api, request, session, item)?;
            }
        }
        let ids = pending
            .iter()
            .filter_map(|item| item.get("id").and_then(Value::as_str))
            .collect::<std::collections::HashSet<_>>();
        let withdrawn = self
            .tasks
            .keys()
            .filter(|id| !ids.contains(id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        for id in withdrawn {
            if let Some(task) = self.tasks.remove(&id) {
                task.handle.abort();
                if let Some(approval) = task.approval {
                    request.approvals.expire(&approval).await?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn observe(
        &mut self,
        api: &Api,
        request: &Invocation,
        session: &str,
        data: &Value,
    ) -> Result<(), ProtocolError> {
        let id = required_string(data, "id")?;
        if !valid_id(id) {
            return Err(failure(
                Fault::ProviderFailed,
                "invalid OpenCode permission identity",
            ));
        }
        if self.tasks.contains_key(id) {
            return Ok(());
        }
        if self.tasks.len() >= 16 {
            return Err(failure(
                Fault::RunLimitExceeded,
                "too many pending OpenCode approvals",
            ));
        }
        let approval = normalize(api.version, request, session, data);
        let retained_approval = approval.clone();
        let api = api.clone();
        let approvals = request.approvals.clone();
        let cancellation = request.cancellation.clone();
        let id = id.to_owned();
        let session = session.to_owned();
        let sender = self.sender.clone();
        let task = tokio::spawn(async move {
            let result = async {
                let decision = if let Some(approval) = approval {
                    tokio::select! {
                        result=approvals.decide(approval.clone())=>result?,
                        ()=cancellation.cancelled()=>{
                            approvals.expire(&approval).await?;
                            Decision::Cancelled
                        }
                    }
                } else {
                    Decision::Denied
                };
                // Session-wide grants cannot widen the exact reviewed native request.
                let reply = if matches!(decision, Decision::Approved) {
                    "once"
                } else {
                    "reject"
                };
                let (path, body) = match api.version {
                    Version::V1 => (format!("/permission/{id}/reply"), json!({"reply":reply})),
                    Version::V2 => (
                        api.path(&session, &format!("/permission/{id}/reply")),
                        json!({"decision":reply}),
                    ),
                };
                api.json(Method::POST, &path, Some(&body)).await?;
                approvals.resolved(&id).await?;
                Ok(())
            }
            .await;
            let _ = sender.send(result).await;
        });
        self.tasks.insert(
            required_string(data, "id")?.to_owned(),
            Task {
                handle: AbortOnDropHandle::new(task),
                approval: retained_approval,
            },
        );
        Ok(())
    }
}

fn normalize(
    version: Version,
    request: &Invocation,
    _session: &str,
    data: &Value,
) -> Option<ApprovalRequest> {
    let id = data.get("id")?.as_str()?;
    let action = data
        .get(match version {
            Version::V1 => "permission",
            Version::V2 => "action",
        })?
        .as_str()?;
    let resources = data
        .get(match version {
            Version::V1 => "patterns",
            Version::V2 => "resources",
        })?
        .as_array()?;
    let cwd = request.cwd.to_string_lossy().into_owned();
    let target = match action {
        "bash" | "shell" if resources.len() == 1 => {
            let explicit = data.pointer("/metadata/command").and_then(Value::as_str);
            let command = explicit.or_else(|| resources[0].as_str())?;
            if explicit.is_none() && command.contains(['*', '?', '[', ']']) {
                return None;
            }
            let lower = command.to_ascii_lowercase();
            if command.len() > 4096
                || command.chars().any(char::is_control)
                || ["bearer ", "token", "password", "secret", "api_key", "://"]
                    .iter()
                    .any(|key| lower.contains(key))
            {
                return None;
            }
            ApprovalTarget::Command {
                command: command.to_owned(),
                cwd,
            }
        }
        "edit" if !resources.is_empty() && resources.len() <= 64 => {
            let changes = resources
                .iter()
                .map(|value| {
                    let path = value.as_str()?;
                    if path.len() > 4096 || path.contains(['\0', '*', '?', '[', ']']) {
                        return None;
                    }
                    let path = std::path::Path::new(path);
                    let path = if path.is_absolute() {
                        path.to_owned()
                    } else {
                        request.cwd.join(path)
                    };
                    Some(path.to_string_lossy().into_owned())
                })
                .collect::<Option<Vec<_>>>()?;
            ApprovalTarget::Files { paths: changes }
        }
        _ => return None,
    };
    Some(ApprovalRequest {
        id: id.to_owned(),
        target,
    })
}

#[cfg(test)]
mod tests;
