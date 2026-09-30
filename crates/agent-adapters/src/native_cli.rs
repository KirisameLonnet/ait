//! Isolated command-line protocol boundary for OpenCode and Claude Code.
//!
//! This adapter only translates one native invocation into events. It does not own AIT
//! Message, Session, Run, or approval state. Callers must enforce their own admission and
//! persistence rules before using it.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    AdapterError, AdapterErrorKind, AgentAdapter, AgentCapabilities, AgentEvent, AgentRunRequest,
    AgentRunStatus, AgentStream, ApprovalPolicy, SandboxMode,
};

const MAX_OUTPUT_BYTES: u64 = 8 * 1024 * 1024;

/// Native CLI wire protocol selected for one local harness installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeCliKind {
    /// `OpenCode`'s `run --format json` event stream.
    OpenCode,
    /// Claude Code's `--print --output-format stream-json` event stream.
    ClaudeCode,
}

impl NativeCliKind {
    const fn driver(self) -> &'static str {
        match self {
            Self::OpenCode => "opencode",
            Self::ClaudeCode => "claude_code",
        }
    }
}

/// A local coding-agent harness invoked through its own JSON event protocol.
#[derive(Debug, Clone)]
pub struct NativeCliAdapter {
    kind: NativeCliKind,
    binary: PathBuf,
}

impl NativeCliAdapter {
    /// Configure a native harness binary without starting a process.
    ///
    /// `binary` is an executable path or command resolved on the worker's PATH.
    #[must_use]
    pub fn new(kind: NativeCliKind, binary: PathBuf) -> Self {
        Self { kind, binary }
    }

    fn command(&self, request: &AgentRunRequest) -> Result<Command, AdapterError> {
        if request.sandbox != SandboxMode::DangerFullAccess
            || request.approval_policy != ApprovalPolicy::Never
        {
            return Err(AdapterError::new(
                AdapterErrorKind::InvalidConfiguration,
                "native CLI invocation requires explicit full access without pending approvals",
                false,
            ));
        }
        let input_len = request
            .project_instructions
            .as_ref()
            .map_or(0, |instructions| instructions.len().saturating_add(2))
            .saturating_add(request.prompt.len());
        if request.prompt.trim().is_empty() || input_len > 1_048_576 {
            return Err(AdapterError::new(
                AdapterErrorKind::InvalidConfiguration,
                "native CLI input is empty or too large",
                false,
            ));
        }
        if request.resume_thread_id.as_deref() == Some("") {
            return Err(AdapterError::new(
                AdapterErrorKind::InvalidConfiguration,
                "native session ID cannot be empty",
                false,
            ));
        }
        if request.output_schema.is_some() && self.kind == NativeCliKind::OpenCode {
            return Err(AdapterError::new(
                AdapterErrorKind::InvalidConfiguration,
                "OpenCode CLI does not provide a validated output schema",
                false,
            ));
        }
        if request.ephemeral && self.kind == NativeCliKind::OpenCode {
            return Err(AdapterError::new(
                AdapterErrorKind::InvalidConfiguration,
                "OpenCode CLI cannot guarantee an ephemeral session",
                false,
            ));
        }
        let mut command = Command::new(&self.binary);
        command.current_dir(&request.cwd);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::null());
        command.kill_on_drop(true);
        match self.kind {
            NativeCliKind::OpenCode => {
                command.args(["run", "--format", "json", "--auto"]);
                if let Some(model) = request.model.as_deref() {
                    command.args(["--model", model]);
                }
                if let Some(effort) = request.reasoning_effort.as_deref() {
                    command.args(["--variant", effort]);
                }
                if let Some(session) = request.resume_thread_id.as_deref() {
                    command.args(["--session", session]);
                }
            }
            NativeCliKind::ClaudeCode => {
                command.args([
                    "--print",
                    "--output-format",
                    "stream-json",
                    "--verbose",
                    "--include-partial-messages",
                    "--dangerously-skip-permissions",
                ]);
                if let Some(model) = request.model.as_deref() {
                    command.args(["--model", model]);
                }
                if let Some(effort) = request.reasoning_effort.as_deref() {
                    command.args(["--effort", effort]);
                }
                if let Some(session) = request.resume_thread_id.as_deref() {
                    command.args(["--resume", session]);
                } else if request.ephemeral {
                    command.arg("--no-session-persistence");
                }
                if let Some(schema) = request.output_schema.as_ref() {
                    command.args([
                        "--json-schema",
                        &serde_json::to_string(schema).map_err(|_| {
                            AdapterError::protocol("output schema could not be encoded")
                        })?,
                    ]);
                }
            }
        }
        Ok(command)
    }
}

#[async_trait]
impl AgentAdapter for NativeCliAdapter {
    fn driver(&self) -> &'static str {
        self.kind.driver()
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            streaming: true,
            thread_resume: true,
            approvals: false,
            command_execution: true,
            file_changes: true,
            usage: false,
        }
    }

    async fn run(&self, request: AgentRunRequest) -> Result<AgentStream, AdapterError> {
        let mut child = self.command(&request)?.spawn().map_err(|_| {
            AdapterError::new(
                AdapterErrorKind::ProcessSpawn,
                "native coding-agent executable could not be started",
                false,
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AdapterError::new(
                AdapterErrorKind::ProcessSpawn,
                "native coding-agent stdout is unavailable",
                false,
            )
        })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            AdapterError::new(
                AdapterErrorKind::ProcessSpawn,
                "native coding-agent stdin is unavailable",
                false,
            )
        })?;
        let (sender, receiver) = mpsc::channel(64);
        let kind = self.kind;
        tokio::spawn(async move {
            let result = drive(kind, &request, child, stdin, stdout, &sender).await;
            if let Err(error) = result {
                let _ = sender.send(Err(error)).await;
            }
        });
        Ok(Box::pin(ReceiverStream::new(receiver)))
    }
}

async fn drive(
    kind: NativeCliKind,
    request: &AgentRunRequest,
    mut child: Child,
    stdin: tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
    sender: &mpsc::Sender<Result<AgentEvent, AdapterError>>,
) -> Result<(), AdapterError> {
    let result = stream_events(kind, request, &mut child, stdin, stdout, sender).await;
    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
    result
}

async fn stream_events(
    kind: NativeCliKind,
    request: &AgentRunRequest,
    child: &mut Child,
    mut stdin: tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
    sender: &mpsc::Sender<Result<AgentEvent, AdapterError>>,
) -> Result<(), AdapterError> {
    let prompt = request.project_instructions.as_deref().map_or_else(
        || request.prompt.clone(),
        |instructions| format!("{instructions}\n\n{}", request.prompt),
    );
    tokio::select! {
        () = request.cancellation.cancelled() => return Err(AdapterError::cancelled()),
        () = sender.closed() => return Ok(()),
        result = stdin.write_all(prompt.as_bytes()) => {
            result.map_err(|_| AdapterError::new(
                AdapterErrorKind::ProcessExited,
                "native input was not accepted",
                false,
            ))?;
        }
    }
    drop(stdin);
    let mut output = BufReader::new(stdout.take(MAX_OUTPUT_BYTES + 1));
    let mut line_buffer = String::new();
    let mut state = ParseState::default();
    loop {
        let line = tokio::select! {
            () = request.cancellation.cancelled() => return Err(AdapterError::cancelled()),
            () = sender.closed() => return Ok(()),
            line = output.read_line(&mut line_buffer) => line.map_err(|_| AdapterError::protocol("native event stream could not be read"))?,
        };
        if line == 0 {
            break;
        }
        if output.get_ref().limit() == 0 {
            return Err(AdapterError::protocol("native event stream exceeded 8 MiB"));
        }
        let event: Value = serde_json::from_str(&line_buffer)
            .map_err(|_| AdapterError::protocol("native event stream contains invalid JSON"))?;
        line_buffer.clear();
        for normalized in parse_event(kind, &event, request, &mut state)? {
            tokio::select! {
                () = request.cancellation.cancelled() => return Err(AdapterError::cancelled()),
                result = sender.send(Ok(normalized)) => {
                    if result.is_err() {
                        return Ok(());
                    }
                }
            }
        }
    }
    if output.get_ref().limit() == 0 {
        return Err(AdapterError::protocol("native event stream exceeded 8 MiB"));
    }
    let status = tokio::select! {
        () = request.cancellation.cancelled() => return Err(AdapterError::cancelled()),
        () = sender.closed() => return Ok(()),
        result = child.wait() => result.map_err(|_| AdapterError::new(
            AdapterErrorKind::ProcessExited,
            "native process could not be reaped",
            false,
        ))?,
    };
    if state.session_id.is_none() || (kind == NativeCliKind::ClaudeCode && !state.saw_result) {
        return Err(AdapterError::protocol("native session did not complete"));
    }
    let result = if status.success() && !state.failed {
        AgentRunStatus::Completed
    } else {
        AgentRunStatus::Failed
    };
    tokio::select! {
        () = request.cancellation.cancelled() => return Err(AdapterError::cancelled()),
        result = sender.send(Ok(AgentEvent::Completed {
            turn_id: request.request_id.clone(),
            status: result,
            error: (result == AgentRunStatus::Failed)
                .then(|| "native coding-agent turn failed".to_owned()),
        })) => { let _ = result; }
    }
    Ok(())
}

#[derive(Default)]
struct ParseState {
    session_id: Option<String>,
    text: TextState,
    saw_result: bool,
    failed: bool,
}

#[derive(Default)]
struct TextState {
    partial: bool,
    any: bool,
}

fn parse_event(
    kind: NativeCliKind,
    event: &Value,
    request: &AgentRunRequest,
    state: &mut ParseState,
) -> Result<Vec<AgentEvent>, AdapterError> {
    let mut events = Vec::new();
    let session = match kind {
        NativeCliKind::OpenCode => event.get("sessionID").and_then(Value::as_str),
        NativeCliKind::ClaudeCode => event.get("session_id").and_then(Value::as_str),
    };
    if let (Some(known), Some(observed)) = (&state.session_id, session)
        && known != observed
    {
        return Err(AdapterError::protocol(
            "native session ID changed during execution",
        ));
    }
    if state.session_id.is_none()
        && let Some(session) = session.filter(|value| !value.is_empty())
    {
        if let Some(expected) = request.resume_thread_id.as_deref()
            && expected != session
        {
            return Err(AdapterError::protocol(
                "native session ID changed during resume",
            ));
        }
        state.session_id = Some(session.to_owned());
        events.push(AgentEvent::ThreadStarted {
            thread_id: session.to_owned(),
        });
        events.push(AgentEvent::TurnStarted {
            turn_id: request.request_id.clone(),
        });
    }
    if state.session_id.is_none() {
        return Ok(events);
    }
    match kind {
        NativeCliKind::OpenCode => match event.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = event.pointer("/part/text").and_then(Value::as_str) {
                    events.push(AgentEvent::MessageDelta {
                        item_id: request.request_id.clone(),
                        delta: text.to_owned(),
                    });
                }
            }
            Some("tool_use") => events.push(AgentEvent::ItemCompleted {
                item: event.get("part").cloned().unwrap_or(Value::Null),
            }),
            Some("error") => state.failed = true,
            Some(_) | None => {}
        },
        NativeCliKind::ClaudeCode => match event.get("type").and_then(Value::as_str) {
            Some("stream_event") => {
                if let Some(text) = event.pointer("/event/delta/text").and_then(Value::as_str) {
                    state.text.partial = true;
                    state.text.any = true;
                    events.push(AgentEvent::MessageDelta {
                        item_id: request.request_id.clone(),
                        delta: text.to_owned(),
                    });
                }
            }
            Some("assistant") => {
                if let Some(content) = event.pointer("/message/content").and_then(Value::as_array) {
                    for part in content {
                        if !state.text.partial
                            && let Some(text) = part.get("text").and_then(Value::as_str)
                        {
                            state.text.any = true;
                            events.push(AgentEvent::MessageDelta {
                                item_id: request.request_id.clone(),
                                delta: text.to_owned(),
                            });
                        } else if part.get("type").and_then(Value::as_str) == Some("tool_use") {
                            events.push(AgentEvent::ItemCompleted { item: part.clone() });
                        }
                    }
                }
                state.text.partial = false;
            }
            Some("result") => {
                state.saw_result = true;
                state.failed |= event
                    .get("is_error")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if !state.text.any
                    && let Some(text) = event.get("result").and_then(Value::as_str)
                {
                    events.push(AgentEvent::MessageDelta {
                        item_id: request.request_id.clone(),
                        delta: text.to_owned(),
                    });
                    state.text.any = true;
                }
            }
            Some(_) | None => {}
        },
    }
    Ok(events)
}

#[cfg(test)]
mod tests;
