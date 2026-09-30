use std::path::PathBuf;

use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::*;

fn request() -> AgentRunRequest {
    AgentRunRequest {
        request_id: "run-1".to_owned(),
        model: Some("provider/model".to_owned()),
        reasoning_effort: None,
        project_instructions: None,
        prompt: "hello".to_owned(),
        cwd: PathBuf::from("."),
        resume_thread_id: None,
        ephemeral: false,
        sandbox: SandboxMode::DangerFullAccess,
        approval_policy: ApprovalPolicy::Never,
        approval_handler: None,
        output_schema: None,
        cancellation: CancellationToken::new(),
    }
}

#[test]
fn opencode_events_keep_native_identity_and_tool_details() {
    let mut state = ParseState::default();
    let first = parse_event(
        NativeCliKind::OpenCode,
        &json!({"type":"step_start","sessionID":"ses_native"}),
        &request(),
        &mut state,
    )
    .expect("parse session");
    assert!(
        matches!(&first[0], AgentEvent::ThreadStarted { thread_id } if thread_id == "ses_native")
    );
    assert!(matches!(&first[1], AgentEvent::TurnStarted { turn_id } if turn_id == "run-1"));
    let text = parse_event(
        NativeCliKind::OpenCode,
        &json!({"type":"text","sessionID":"ses_native","part":{"text":"answer"}}),
        &request(),
        &mut state,
    )
    .expect("parse text");
    assert!(matches!(&text[0], AgentEvent::MessageDelta { delta, .. } if delta == "answer"));
    let tool = parse_event(
        NativeCliKind::OpenCode,
        &json!({"type":"tool_use","sessionID":"ses_native","part":{"type":"tool","tool":"bash"}}),
        &request(),
        &mut state,
    )
    .expect("parse tool");
    assert!(matches!(&tool[0], AgentEvent::ItemCompleted { item } if item["tool"] == "bash"));
}

#[test]
fn claude_stream_does_not_duplicate_final_text() {
    let mut state = ParseState::default();
    let delta = parse_event(
        NativeCliKind::ClaudeCode,
        &json!({"type":"stream_event","session_id":"session-1","event":{"delta":{"type":"text_delta","text":"hello"}}}),
        &request(),
        &mut state,
    )
    .expect("parse delta");
    assert_eq!(delta.len(), 3);
    let final_result = parse_event(
        NativeCliKind::ClaudeCode,
        &json!({"type":"result","session_id":"session-1","result":"hello"}),
        &request(),
        &mut state,
    )
    .expect("parse result");
    assert!(final_result.is_empty());
}

#[test]
fn resume_identity_and_permissions_fail_closed() {
    let mut resumed = request();
    resumed.resume_thread_id = Some("expected".to_owned());
    assert!(
        parse_event(
            NativeCliKind::OpenCode,
            &json!({"type":"text","sessionID":"unexpected"}),
            &resumed,
            &mut ParseState::default(),
        )
        .is_err()
    );
    let adapter = NativeCliAdapter::new(NativeCliKind::ClaudeCode, "claude".into());
    let mut restricted = request();
    restricted.sandbox = SandboxMode::WorkspaceWrite;
    assert!(adapter.command(&restricted).is_err());
    restricted.sandbox = SandboxMode::DangerFullAccess;
    restricted.approval_policy = ApprovalPolicy::OnRequest;
    assert!(adapter.command(&restricted).is_err());
    let mut open_code = request();
    open_code.ephemeral = true;
    assert!(
        NativeCliAdapter::new(NativeCliKind::OpenCode, "opencode".into())
            .command(&open_code)
            .is_err()
    );
    let mut bounded = request();
    bounded.project_instructions = Some("x".repeat(1_048_576));
    assert!(
        NativeCliAdapter::new(NativeCliKind::ClaudeCode, "claude".into())
            .command(&bounded)
            .is_err()
    );
    let mut empty_session = request();
    empty_session.resume_thread_id = Some(String::new());
    assert!(adapter.command(&empty_session).is_err());
}

#[test]
fn session_identity_must_remain_stable_after_start() {
    let mut state = ParseState::default();
    parse_event(
        NativeCliKind::OpenCode,
        &json!({"type":"step_start","sessionID":"first"}),
        &request(),
        &mut state,
    )
    .expect("first event");
    assert!(
        parse_event(
            NativeCliKind::OpenCode,
            &json!({"type":"text","sessionID":"second","part":{"text":"wrong"}}),
            &request(),
            &mut state,
        )
        .is_err()
    );
}

#[test]
fn command_line_never_contains_the_user_prompt() {
    for kind in [NativeCliKind::OpenCode, NativeCliKind::ClaudeCode] {
        let adapter = NativeCliAdapter::new(kind, "native-agent".into());
        let mut input = request();
        input.prompt = "private user input".to_owned();
        let command = adapter.command(&input).expect("valid invocation");
        let args = command
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args.iter().all(|arg| !arg.contains("private user input")));
    }
}

#[test]
fn claude_provider_failure_marks_terminal_state() {
    let mut state = ParseState::default();
    let events = parse_event(
        NativeCliKind::ClaudeCode,
        &json!({"type":"result","session_id":"session-1","is_error":true}),
        &request(),
        &mut state,
    )
    .expect("parse failure");
    assert_eq!(events.len(), 2);
    assert!(state.failed);
    assert!(state.saw_result);
}

#[cfg(unix)]
#[test]
fn claude_missing_terminal_event_rejects_the_turn() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use tokio_stream::StreamExt;

    let directory = tempfile::tempdir().expect("temporary harness");
    let binary = directory.path().join("fake-claude");
    fs::write(
        &binary,
        "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"assistant\",\"session_id\":\"session-1\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"partial\"}]}}'\n",
    )
    .expect("write fake harness");
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))
        .expect("make fake harness executable");
    let runtime = tokio::runtime::Runtime::new().expect("Tokio runtime");
    runtime.block_on(async {
        let adapter = NativeCliAdapter::new(NativeCliKind::ClaudeCode, binary);
        let mut stream = adapter.run(request()).await.expect("start fake harness");
        let mut saw_rejection = false;
        while let Some(event) = stream.next().await {
            if event.is_err() {
                saw_rejection = true;
            }
            assert!(!matches!(event, Ok(AgentEvent::Completed { .. })));
        }
        assert!(saw_rejection);
    });
}

#[cfg(unix)]
#[test]
fn dropping_the_event_stream_reaps_the_native_process() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Stdio};

    let directory = tempfile::tempdir().expect("temporary harness");
    let binary = directory.path().join("fake-agent");
    let pid_file = directory.path().join("pid");
    fs::write(
        &binary,
        "#!/bin/sh\ncat >/dev/null\necho $$ > \"$(dirname \"$0\")/pid\"\nexec sleep 30\n",
    )
    .expect("write fake harness");
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))
        .expect("make fake harness executable");
    let runtime = tokio::runtime::Runtime::new().expect("Tokio runtime");
    runtime.block_on(async {
        let adapter = NativeCliAdapter::new(NativeCliKind::OpenCode, binary);
        let stream = adapter.run(request()).await.expect("start fake harness");
        let pid = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Ok(pid) = fs::read_to_string(&pid_file) {
                    break pid.trim().to_owned();
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("fake harness started");
        drop(stream);
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let status = Command::new("kill")
                    .args(["-0", &pid])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .expect("check process status");
                if !status.success() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("native process reaped");
    });
}

#[cfg(unix)]
#[test]
fn both_native_cli_streams_complete_with_a_supervised_child() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use tokio_stream::StreamExt;

    let directory = tempfile::tempdir().expect("temporary harness");
    let binary = directory.path().join("fake-agent");
    fs::write(
        &binary,
        "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"text\",\"sessionID\":\"ses_1\",\"part\":{\"text\":\"ok\"}}' '{\"type\":\"result\",\"session_id\":\"claude-1\",\"result\":\"ok\"}'\n",
    )
    .expect("write fake harness");
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))
        .expect("make fake harness executable");
    let runtime = tokio::runtime::Runtime::new().expect("Tokio runtime");
    runtime.block_on(async {
        for (kind, session) in [
            (NativeCliKind::OpenCode, "ses_1"),
            (NativeCliKind::ClaudeCode, "claude-1"),
        ] {
            let adapter = NativeCliAdapter::new(kind, binary.clone());
            let mut stream = adapter.run(request()).await.expect("start fake harness");
            let mut events = Vec::new();
            while let Some(event) = stream.next().await {
                events.push(event.expect("valid native event"));
            }
            assert!(events.iter().any(|event| matches!(event, AgentEvent::ThreadStarted { thread_id } if thread_id == session)));
            assert!(events.iter().any(|event| matches!(event, AgentEvent::MessageDelta { delta, .. } if delta == "ok")));
            assert!(events.iter().any(|event| matches!(event, AgentEvent::Completed { status: AgentRunStatus::Completed, .. })));
        }
    });
}
