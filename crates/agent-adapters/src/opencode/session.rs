//! Prepare, submit once, observe and reconcile; cancellation never becomes an input retry.
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use ait_domain::{DomainError, ErrorCode, SandboxAccess, SubMessage};
use ait_ports::{
    NativeSessionInvocation, NativeSessionSnapshot, WorkspaceProgressEvent,
    WorkspaceProgressReporter,
};
use reqwest::Method;
use serde_json::{Value, json};

use super::{
    approvals::Pending,
    failure, history,
    http::{Api, Events, Version, required_string},
    runtime::Runtime,
};

pub(super) struct Connection {
    pub(super) runtime: Runtime,
    pub(super) invocation: NativeSessionInvocation,
    pub(super) prepared: NativeSessionSnapshot,
    pub(super) submitted: bool,
    pub(super) limits: super::OpenCodeExecutionLimits,
}

pub(super) fn validate(request: &NativeSessionInvocation) -> Result<(), DomainError> {
    if request.driver != "opencode"
        || !request.cwd.is_absolute()
        || request.input_id.is_empty()
        || request.request_id.is_empty()
        || request
            .instructions
            .as_ref()
            .is_some_and(|text| text.len() > 1024 * 1024)
        || request.prompt.len() > 1024 * 1024
        || request.model.split_once('/').is_none()
        || request.session_id.as_ref().is_some_and(|id| !valid_id(id))
    {
        return Err(failure(
            ErrorCode::AgentCapabilityUnsupported,
            "invalid OpenCode session invocation",
        ));
    }
    if request.permission_profile.sandbox != SandboxAccess::FullAccess {
        return Err(failure(
            ErrorCode::AgentCapabilityUnsupported,
            "OpenCode native permissions are not an OS sandbox; this adapter requires explicit full access",
        ));
    }
    if request.session_id.is_some() && request.instructions.is_some() {
        return Err(failure(
            ErrorCode::AgentCapabilityUnsupported,
            "OpenCode resume cannot replace session instructions",
        ));
    }
    if request.cancellation.is_cancelled() {
        return Err(failure(
            ErrorCode::RunCancelled,
            "OpenCode admission cancelled",
        ));
    }
    Ok(())
}

pub(super) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 512
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

pub(super) async fn prepare(
    api: &Api,
    request: &NativeSessionInvocation,
) -> Result<NativeSessionSnapshot, DomainError> {
    let models = api.models().await?;
    if !models.iter().any(|model| {
        model.id == request.model
            && request
                .reasoning_effort
                .as_ref()
                .is_none_or(|effort| model.reasoning_efforts.contains(effort))
    }) {
        return Err(failure(
            ErrorCode::AgentCapabilityUnsupported,
            "OpenCode model or variant is unavailable",
        ));
    }
    let permission = permissions(api.version);
    let id = if let Some(id) = &request.session_id {
        if !api.idle(id).await? {
            return Err(failure(
                ErrorCode::SessionBusy,
                "OpenCode session is active elsewhere",
            ));
        }
        let body = match api.version {
            Version::V1 => json!({"permission":permission}),
            Version::V2 => json!({"permissions":permission}),
        };
        api.json(Method::PATCH, &api.path(id, ""), Some(&body))
            .await?;
        if api.version == Version::V2 {
            api.json(
                Method::POST,
                &api.path(id, "/model"),
                Some(&json!({"model":model(request, api.version)})),
            )
            .await?;
        }
        id.clone()
    } else {
        let body = match api.version {
            Version::V1 => json!({"permission":permission}),
            Version::V2 => json!({"location":{"directory":request.cwd}, "agent":"build",
                "model":model(request, api.version), "permissions":permission}),
        };
        let response = api
            .json(
                Method::POST,
                &format!("{}/session", api.version.prefix()),
                Some(&body),
            )
            .await?;
        let id = required_string(api.data(&response), "id")?;
        if !valid_id(id) {
            return Err(failure(
                ErrorCode::ProviderFailed,
                "invalid OpenCode session identity",
            ));
        }
        if api.version == Version::V2
            && let Some(instructions) = &request.instructions
        {
            let path = format!("/api/experimental/session/{id}/instructions/entries/ait");
            api.json(Method::PUT, &path, Some(&json!({"value":instructions})))
                .await?;
        }
        id.to_owned()
    };
    let mut history = snapshot(api, &id, request).await?;
    if api.version == Version::V1 && !history.input_id.starts_with("msg_") {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let random = uuid::Uuid::new_v4().simple().to_string();
        history.input_id = format!(
            "msg_{:012x}{}",
            (timestamp << 12) & 0xffff_ffff_ffff,
            &random[..14]
        );
    }
    Ok(history)
}

fn model(request: &NativeSessionInvocation, version: Version) -> Value {
    let (provider, model) = request
        .model
        .split_once('/')
        .expect("validated provider/model identifier");
    let mut selected = match version {
        Version::V1 => json!({"providerID":provider,"modelID":model}),
        Version::V2 => json!({"providerID":provider,"id":model}),
    };
    if version == Version::V2
        && let Some(variant) = &request.reasoning_effort
    {
        selected["variant"] = json!(variant);
    }
    selected
}

fn permissions(version: Version) -> Value {
    let rules = [
        ("*", "deny"),
        ("read", "allow"),
        ("glob", "allow"),
        ("grep", "allow"),
        ("list", "allow"),
        ("skill", "allow"),
        ("todowrite", "allow"),
        ("edit", "ask"),
        (
            if version == Version::V1 {
                "bash"
            } else {
                "shell"
            },
            "ask",
        ),
    ];
    Value::Array(
        rules
            .into_iter()
            .map(|(name, action)| match version {
                Version::V1 => json!({"permission":name,"pattern":"*","action":action}),
                Version::V2 => json!({"action":name,"resource":"*","effect":action}),
            })
            .collect(),
    )
}

pub(super) async fn snapshot(
    api: &Api,
    id: &str,
    request: &NativeSessionInvocation,
) -> Result<NativeSessionSnapshot, DomainError> {
    if !api.idle(id).await? {
        return Err(failure(
            ErrorCode::SessionBusy,
            "OpenCode execution is not idle",
        ));
    }
    let response = api.json(Method::GET, &api.path(id, ""), None).await?;
    let info = api.data(&response);
    let cwd = match api.version {
        Version::V1 => info.get("directory"),
        Version::V2 => info.pointer("/location/directory"),
    }
    .and_then(Value::as_str)
    .ok_or_else(|| failure(ErrorCode::ProviderFailed, "OpenCode cwd missing"))?;
    let effective_permissions = match api.version {
        Version::V1 => info.get("permission"),
        Version::V2 => info.get("permissions"),
    };
    if required_string(info, "id")? != id
        || std::path::Path::new(cwd) != request.cwd
        || effective_permissions != Some(&permissions(api.version))
    {
        return Err(failure(
            ErrorCode::AgentCapabilityUnsupported,
            "OpenCode session identity, cwd or permissions differ from admission",
        ));
    }
    if api.version == Version::V2
        && (info.pointer("/model/providerID").and_then(Value::as_str)
            != request.model.split_once('/').map(|(provider, _)| provider)
            || info.pointer("/model/id").and_then(Value::as_str)
                != request.model.split_once('/').map(|(_, model)| model)
            || request.reasoning_effort.as_deref().is_some_and(|effort| {
                info.pointer("/model/variant").and_then(Value::as_str) != Some(effort)
            }))
    {
        return Err(failure(
            ErrorCode::AgentCapabilityUnsupported,
            "OpenCode effective model differs from admission",
        ));
    }
    let execution = if api.version == Version::V2 {
        api.execution(id).await?
    } else {
        None
    };
    let raw = api.history(id).await?;
    let outcome = super::execution::outcome(api.version, info, execution.as_ref(), &raw)?;
    if api.version == Version::V2 && api.execution(id).await? != execution {
        return Err(failure(
            ErrorCode::SessionBusy,
            "OpenCode execution changed during reconciliation",
        ));
    }
    if api.version == Version::V2 && execution.is_some() && outcome.is_none() {
        return Err(failure(
            ErrorCode::SessionBusy,
            "OpenCode durable execution has not drained",
        ));
    }
    let messages = history::normalize(api.version, id, &raw)?;
    if !api.idle(id).await? {
        return Err(failure(
            ErrorCode::SessionBusy,
            "OpenCode became active during history reconciliation",
        ));
    }
    Ok(NativeSessionSnapshot {
        driver: "opencode".into(),
        id: id.into(),
        input_id: request.input_id.clone(),
        cwd: request.cwd.clone(),
        model: request.model.clone(),
        reasoning_effort: request.reasoning_effort.clone(),
        messages,
        outcome,
    })
}

impl Connection {
    pub(super) async fn execute(
        &mut self,
        progress: Arc<dyn WorkspaceProgressReporter>,
    ) -> Result<NativeSessionSnapshot, DomainError> {
        if self.submitted {
            return Err(failure(
                ErrorCode::RunRecoveryFailed,
                "OpenCode input must not be replayed",
            ));
        }
        if self.invocation.cancellation.is_cancelled() {
            return Err(failure(
                ErrorCode::RunCancelled,
                "OpenCode input cancelled before submission",
            ));
        }
        let events = self.ready_events().await?;
        self.invocation.input_id.clone_from(&self.prepared.input_id);
        let mut body = match self.runtime.api.version {
            Version::V1 => {
                json!({"messageID":self.invocation.input_id,"parts":[{"type":"text","text":self.invocation.prompt}],
                "model":model(&self.invocation, Version::V1)})
            }
            Version::V2 => json!({"text":self.invocation.prompt,"files":[],
                "metadata":{"aitInputId":self.invocation.input_id}}),
        };
        if self.runtime.api.version == Version::V1 {
            if let Some(variant) = &self.invocation.reasoning_effort {
                body["variant"] = json!(variant);
            }
            if let Some(instructions) = &self.invocation.instructions {
                body["system"] = json!(instructions);
            }
        }
        let suffix = if self.runtime.api.version == Version::V1 {
            "/prompt_async"
        } else {
            "/prompt"
        };
        // Mark before polling the future: even a lost admission response may have produced effects.
        self.submitted = true;
        let submission = self
            .runtime
            .api
            .json(
                Method::POST,
                &self.runtime.api.path(&self.prepared.id, suffix),
                Some(&body),
            )
            .await;
        if submission.is_err() {
            // Reconcile once; never dispatch again after transport ambiguity.
            if let Ok(history) =
                snapshot(&self.runtime.api, &self.prepared.id, &self.invocation).await
                && accepted(&history)
            {
                self.check_budget().await?;
                return Ok(history);
            }
            return Err(failure(
                ErrorCode::RunRecoveryFailed,
                "OpenCode input outcome unknown; reconcile history without replay",
            ));
        }
        self.observe(events, progress).await
    }

    async fn interrupt(&self) {
        let suffix = if self.runtime.api.version == Version::V1 {
            "/abort"
        } else {
            "/interrupt"
        };
        let _ = tokio::time::timeout(
            Duration::from_secs(5),
            self.runtime.api.json(
                Method::POST,
                &self.runtime.api.path(&self.prepared.id, suffix),
                Some(&json!({})),
            ),
        )
        .await;
    }

    async fn check_budget(&self) -> Result<(), DomainError> {
        let raw = self.runtime.api.history(&self.prepared.id).await?;
        if let Err(error) =
            super::budget::validate(self.runtime.api.version, &raw, &self.prepared, self.limits)
        {
            self.interrupt().await;
            return Err(error);
        }
        Ok(())
    }

    async fn ready_events(&self) -> Result<Events, DomainError> {
        tokio::select! {
            () = self.invocation.cancellation.cancelled() => Err(failure(ErrorCode::RunCancelled, "OpenCode observation cancelled")),
            result = tokio::time::timeout(Duration::from_secs(30), async {
                let mut events = self.runtime.api.events().await?;
                connected(&mut events).await?;
                Ok(events)
            }) => result.unwrap_or_else(|_| Err(failure(ErrorCode::RunRecoveryFailed, "OpenCode event stream was not ready; input was not sent"))),
        }
    }

    async fn observe(
        &mut self,
        mut events: Events,
        progress: Arc<dyn WorkspaceProgressReporter>,
    ) -> Result<NativeSessionSnapshot, DomainError> {
        let mut timer = tokio::time::interval(Duration::from_secs(5));
        let mut displayed = HashMap::<String, String>::new();
        let known = self
            .prepared
            .messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<HashSet<_>>();
        let mut approvals = Pending::new();
        loop {
            tokio::select! {
                () = self.invocation.cancellation.cancelled() => {
                    let suffix = if self.runtime.api.version == Version::V1 {"/abort"} else {"/interrupt"};
                    let _ = tokio::time::timeout(Duration::from_secs(5), self.runtime.api.json(
                        Method::POST, &self.runtime.api.path(&self.prepared.id, suffix), Some(&json!({})))).await;
                    return Err(failure(ErrorCode::RunCancelled, "OpenCode execution cancelled"));
                }
                event = events.next() => {
                    if let Err(error) = &event && error.code == ErrorCode::RunLimitExceeded {
                        self.interrupt().await;
                        return Err(error.clone());
                    }
                    if let Ok(Some(event)) = event {
                        let event = event.get("payload").unwrap_or(&event);
                        let data = event.get("properties").or_else(||event.get("data")).unwrap_or(&Value::Null);
                        if event.get("type").and_then(Value::as_str)==Some("permission.asked")
                            && data.get("sessionID").and_then(Value::as_str)==Some(&self.prepared.id) {
                            approvals.observe(&self.runtime.api,&self.invocation,&self.prepared.id,data)?;
                        }
                        self.progress(event, &progress, &mut displayed).await?;
                    } else {
                        // Lost events require state reconciliation; they never imply execution failure.
                        if let Ok(history) = snapshot(&self.runtime.api, &self.prepared.id, &self.invocation).await
                            && accepted(&history) { self.check_budget().await?; return Ok(history); }
                        events = self.ready_events().await?;
                    }
                }
                _ = timer.tick() => {
                    let raw = self.runtime.api.history(&self.prepared.id).await?;
                    if let Err(error) = super::budget::validate(self.runtime.api.version, &raw, &self.prepared, self.limits) {
                        self.interrupt().await;
                        return Err(error);
                    }
                    approvals.reconcile(&self.runtime.api,&self.invocation,&self.prepared.id).await?;
                    if let Ok(history) = snapshot(&self.runtime.api, &self.prepared.id, &self.invocation).await
                        && accepted(&history) {
                        for message in history.messages.iter().filter(|message| !known.contains(message.id.as_str())) {
                            if message.role == ait_domain::MessageRole::Assistant {
                                let text = message.sub_messages.iter().filter_map(|part| match part {
                                    SubMessage::Text { text } => Some(text.as_str()),
                                    SubMessage::FileRef {..} | SubMessage::ToolUse(_) | SubMessage::StructuredData {..} | SubMessage::ProviderItem(_) => None,
                                }).collect::<Vec<_>>().join("\n");
                                progress.report(WorkspaceProgressEvent::MessageCompleted {id:message.id.clone(),phase:None,text}).await;
                            }
                        }
                        return Ok(history);
                    }
                }
                result = approvals.receiver.recv() => {
                    if let Some(result) = result {result?;}
                }
            }
        }
    }

    async fn progress(
        &self,
        raw: &Value,
        progress: &Arc<dyn WorkspaceProgressReporter>,
        displayed: &mut HashMap<String, String>,
    ) -> Result<(), DomainError> {
        let event = raw.get("payload").unwrap_or(raw);
        let data = event
            .get("properties")
            .or_else(|| event.get("data"))
            .unwrap_or(&Value::Null);
        let owner = data
            .get("sessionID")
            .or_else(|| data.pointer("/part/sessionID"))
            .and_then(Value::as_str);
        if owner != Some(&self.prepared.id) {
            return Ok(());
        }
        match event.get("type").and_then(Value::as_str) {
            Some("message.part.updated")
                if data.pointer("/part/type").and_then(Value::as_str) == Some("text") =>
            {
                let part = &data["part"];
                let id = required_string(part, "id")?;
                let text = required_string(part, "text")?;
                let previous = displayed.entry(id.to_owned()).or_default();
                if let Some(delta) = text.strip_prefix(previous.as_str())
                    && !delta.is_empty()
                {
                    progress
                        .report(WorkspaceProgressEvent::TextDelta {
                            id: id.into(),
                            delta: delta.into(),
                        })
                        .await;
                }
                previous.clone_from(&text.to_owned());
            }
            Some("session.text.delta") => {
                let id = required_string(data, "assistantMessageID")?;
                progress
                    .report(WorkspaceProgressEvent::TextDelta {
                        id: id.into(),
                        delta: required_string(data, "delta")?.into(),
                    })
                    .await;
            }
            Some("session.status")
                if data.pointer("/status/type").and_then(Value::as_str) == Some("retry") =>
            {
                progress
                    .report(WorkspaceProgressEvent::Warning {
                        message: "OpenCode is retrying the provider request".into(),
                        retrying: true,
                        code: None,
                    })
                    .await;
            }
            Some(_) | None => {}
        }
        Ok(())
    }
}

async fn connected(events: &mut Events) -> Result<(), DomainError> {
    loop {
        let event = events.next().await?.ok_or_else(|| {
            failure(
                ErrorCode::RunRecoveryFailed,
                "OpenCode event stream ended before readiness",
            )
        })?;
        let event = event.get("payload").unwrap_or(&event);
        if event.get("type").and_then(Value::as_str) == Some("server.connected") {
            return Ok(());
        }
    }
}

fn accepted(snapshot: &NativeSessionSnapshot) -> bool {
    let Some(index) = snapshot
        .messages
        .iter()
        .position(|message| message.input_id.as_ref() == Some(&snapshot.input_id))
    else {
        return false;
    };
    snapshot.outcome.is_some_and(|outcome| {
        outcome != ait_ports::NativeSessionOutcome::Completed
            || snapshot.messages[index + 1..]
                .iter()
                .any(|message| message.role == ait_domain::MessageRole::Assistant)
    })
}
