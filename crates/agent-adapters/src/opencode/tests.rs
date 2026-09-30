use super::*;
use ait_domain::{MessageRole, RunPermissionProfile, SandboxAccess, SubMessage};
use ait_ports::{DenyWorkspaceApprovals, NativeSessionOutcome, WorkspaceProgressEvent};
use serde_json::json;

pub(super) mod fixture;

#[derive(Default)]
struct Progress(std::sync::Mutex<Vec<WorkspaceProgressEvent>>);
#[async_trait]
impl WorkspaceProgressReporter for Progress {
    async fn report(&self, event: WorkspaceProgressEvent) {
        self.0.lock().unwrap().push(event);
    }
}

pub(super) fn invocation(cwd: PathBuf) -> NativeSessionInvocation {
    NativeSessionInvocation {
        driver: "opencode".into(),
        request_id: "run-1".into(),
        session_id: None,
        input_id: "input-1".into(),
        prompt: "hello".into(),
        instructions: None,
        cwd,
        model: "local/test-model".into(),
        reasoning_effort: None,
        permission_profile: RunPermissionProfile {
            sandbox: SandboxAccess::FullAccess,
            ..Default::default()
        },
        project_execution: None,
        approvals: Arc::new(DenyWorkspaceApprovals),
        cancellation: tokio_util::sync::CancellationToken::new(),
    }
}

#[test]
fn selects_protocol_and_rejects_unsupported_versions() {
    for value in ["1.14.46", "opencode v1.14.46\n"] {
        assert_eq!(http::Version::parse(value).unwrap(), http::Version::V1);
    }
    for value in ["2.0.10", "v2.1.0-beta.1"] {
        assert_eq!(http::Version::parse(value).unwrap(), http::Version::V2);
    }
    for value in ["2.0.9", "3.0.0", "garbage", "1.1", "0.1.0"] {
        assert!(http::Version::parse(value).is_err());
    }
}

#[test]
fn rejects_remote_loopback_lookalikes_and_credentials() {
    for base in [
        "http://localhost:1234/",
        "https://127.0.0.1:1234/",
        "http://127.0.0.1:1234/path",
        "http://user@127.0.0.1:1234/",
        "http://127.0.0.1:1234/?token=x",
        "http://127.0.0.1/",
    ] {
        assert!(
            http::Api::new(
                http::Version::V1,
                reqwest::Url::parse(base).unwrap(),
                "private".into(),
                "/tmp".into()
            )
            .is_err()
        );
    }
}

#[test]
fn refuses_sandbox_emulation_resume_instructions_and_unsafe_session_ids() {
    let mut request = invocation("/tmp".into());
    for sandbox in [SandboxAccess::ReadOnly, SandboxAccess::WorkspaceWrite] {
        request.permission_profile.sandbox = sandbox;
        assert_eq!(
            session::validate(&request).unwrap_err().code,
            ErrorCode::AgentCapabilityUnsupported
        );
    }
    request.permission_profile.sandbox = SandboxAccess::FullAccess;
    request.session_id = Some("../other".into());
    assert!(session::validate(&request).is_err());
    request.session_id = Some("ses_one".into());
    request.instructions = Some("replace".into());
    assert!(session::validate(&request).is_err());
    request.instructions = None;
    request.cancellation.cancel();
    assert_eq!(
        session::validate(&request).unwrap_err().code,
        ErrorCode::RunCancelled
    );
}

#[test]
fn maps_terminal_native_tools_to_domain_messages_and_redacts_secret_fields() {
    let records = json!([
        {"id":"u1","type":"user","text":"hello","metadata":{"aitInputId":"input-1"},"time":{"created":1}},
        {"id":"a1","type":"assistant","time":{"created":2,"completed":3},"content":[
            {"type":"text","text":"working"},
            {"id":"call1","type":"tool","name":"shell","state":{"status":"completed","input":{"command":"pwd","token":"secret"},"content":[{"type":"text","text":"/tmp"}]}},
            {"type":"reasoning","text":"thought","providerState":{"token":"secret"}}
        ]}
    ]);
    let mapped =
        history::normalize(http::Version::V2, "ses_one", records.as_array().unwrap()).unwrap();
    assert_eq!(mapped.len(), 3);
    assert_eq!(mapped[0].input_id.as_deref(), Some("input-1"));
    assert!(
        matches!(&mapped[1].sub_messages[1],SubMessage::ToolUse(tool) if tool.call_id=="call1" && !tool.arguments.contains("secret"))
    );
    assert_eq!(mapped[2].role, MessageRole::User);
    assert!(mapped[2].tool_result.is_some());
    assert!(!serde_json::to_string(&mapped).unwrap().contains("secret"));
}

#[test]
fn rejects_duplicate_and_unfinished_history() {
    let assistant = json!({"id":"a1","type":"assistant","time":{"created":2},"content":[]});
    assert!(
        history::normalize(
            http::Version::V2,
            "ses_one",
            std::slice::from_ref(&assistant)
        )
        .is_err()
    );
    let mut complete = assistant;
    complete["time"]["completed"] = json!(3);
    assert!(
        history::normalize(http::Version::V2, "ses_one", &[complete.clone(), complete]).is_err()
    );
    let wrong = json!({"info":{"id":"u1","role":"user","sessionID":"other","time":{"created":1}},"parts":[]});
    assert!(history::normalize(http::Version::V1, "ses_one", &[wrong]).is_err());
    let running = json!({"id":"a1","type":"assistant","time":{"created":1,"completed":2},"content":[{"type":"tool","id":"c","name":"shell","state":{"status":"running","input":{}}}]});
    assert!(history::normalize(http::Version::V2, "ses_one", &[running]).is_err());
}

#[tokio::test]
async fn native_v1_and_v2_prepare_send_once_read_and_resume() {
    for version in [http::Version::V1, http::Version::V2] {
        let fixture = fixture::Fixture::start(version).await;
        let adapter = OpenCodeAdapter::new(fixture.binary.clone());
        let request = invocation(fixture.cwd.clone());
        let mut connection = adapter.open(request.clone()).await.unwrap();
        assert_eq!(fixture.state.lock().unwrap().submissions, 0);
        assert!(connection.prepared().messages.is_empty());
        if version == http::Version::V1 {
            assert!(connection.prepared().input_id.starts_with("msg_"));
        }
        let snapshot = connection
            .start(Arc::new(Progress::default()))
            .await
            .unwrap();
        assert_eq!(snapshot.outcome, Some(NativeSessionOutcome::Completed));
        assert_eq!(snapshot.messages.len(), 2);
        assert_eq!(fixture.state.lock().unwrap().submissions, 1);
        assert!(
            connection
                .start(Arc::new(Progress::default()))
                .await
                .is_err()
        );
        assert_eq!(connection.read().await.unwrap().messages, snapshot.messages);
        connection.close().await;
        let mut resumed = request;
        resumed.session_id = Some(snapshot.id);
        resumed.input_id = snapshot.input_id;
        let mut connection = adapter.open(resumed).await.unwrap();
        assert_eq!(connection.prepared().messages, snapshot.messages);
        connection.close().await;
        assert_eq!(fixture.state.lock().unwrap().submissions, 1);
    }
}

#[tokio::test]
async fn model_catalog_uses_connected_native_models_and_variants() {
    for version in [http::Version::V1, http::Version::V2] {
        let fixture = fixture::Fixture::start(version).await;
        let models = OpenCodeAdapter::new(fixture.binary.clone())
            .discover_models(fixture.cwd.clone())
            .await
            .unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "local/test-model");
        assert_eq!(models[0].reasoning_efforts, vec!["high"]);
    }
}

#[tokio::test]
async fn response_ambiguity_reconciles_without_replaying_input() {
    let fixture = fixture::Fixture::start(http::Version::V2).await;
    fixture.state.lock().unwrap().reject_ack = true;
    let mut connection = OpenCodeAdapter::new(fixture.binary.clone())
        .open(invocation(fixture.cwd.clone()))
        .await
        .unwrap();
    let history = connection
        .start(Arc::new(Progress::default()))
        .await
        .unwrap();
    assert_eq!(history.outcome, Some(NativeSessionOutcome::Completed));
    assert_eq!(fixture.state.lock().unwrap().submissions, 1);
    connection.close().await;
}

#[tokio::test]
async fn active_session_and_unknown_model_fail_before_input() {
    let fixture = fixture::Fixture::start(http::Version::V2).await;
    let adapter = OpenCodeAdapter::new(fixture.binary.clone());
    let mut request = invocation(fixture.cwd.clone());
    request.model = "local/missing".into();
    assert!(adapter.open(request).await.is_err());
    let mut connection = adapter.open(invocation(fixture.cwd.clone())).await.unwrap();
    let id = connection.prepared().id.clone();
    connection.close().await;
    fixture.state.lock().unwrap().busy = true;
    let mut request = invocation(fixture.cwd.clone());
    request.session_id = Some(id);
    assert_eq!(
        adapter.open(request).await.err().unwrap().code,
        ErrorCode::SessionBusy
    );
    assert_eq!(fixture.state.lock().unwrap().submissions, 0);
}

#[tokio::test]
async fn history_cursor_cycles_fail_closed() {
    let fixture = fixture::Fixture::start(http::Version::V2).await;
    fixture.state.lock().unwrap().cursor_cycle = true;
    let result = OpenCodeAdapter::new(fixture.binary.clone())
        .open(invocation(fixture.cwd.clone()))
        .await;
    assert_eq!(result.err().unwrap().code, ErrorCode::ProviderFailed);
    assert_eq!(fixture.state.lock().unwrap().submissions, 0);
}

#[tokio::test]
async fn cancelling_an_active_execution_interrupts_without_replaying() {
    let fixture = fixture::Fixture::start(http::Version::V2).await;
    let request = invocation(fixture.cwd.clone());
    let cancellation = request.cancellation.clone();
    let mut connection = OpenCodeAdapter::new(fixture.binary.clone())
        .open(request)
        .await
        .unwrap();
    fixture.state.lock().unwrap().busy = true;
    let task = tokio::spawn(async move {
        let result = connection.start(Arc::new(Progress::default())).await;
        connection.close().await;
        result
    });
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while fixture.state.lock().unwrap().submissions == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    cancellation.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.unwrap_err().code, ErrorCode::RunCancelled);
    let state = fixture.state.lock().unwrap();
    assert_eq!(state.submissions, 1);
    assert!(!state.busy);
}

#[tokio::test]
async fn native_failure_without_assistant_content_is_terminal() {
    let fixture = fixture::Fixture::start(http::Version::V2).await;
    fixture.state.lock().unwrap().early_failure = true;
    let mut connection = OpenCodeAdapter::new(fixture.binary.clone())
        .open(invocation(fixture.cwd.clone()))
        .await
        .unwrap();
    let history = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        connection.start(Arc::new(Progress::default())),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(history.outcome, Some(NativeSessionOutcome::Failed));
    assert_eq!(history.messages.len(), 1);
    connection.close().await;
}

#[tokio::test]
async fn native_budget_overrun_interrupts_execution() {
    let fixture = fixture::Fixture::start(http::Version::V2).await;
    let adapter = OpenCodeAdapter::new(fixture.binary.clone())
        .with_execution_limits(OpenCodeExecutionLimits {
            max_output_bytes: 1,
            ..Default::default()
        })
        .unwrap();
    let mut connection = adapter.open(invocation(fixture.cwd.clone())).await.unwrap();
    let result = connection.start(Arc::new(Progress::default())).await;
    assert_eq!(result.unwrap_err().code, ErrorCode::RunLimitExceeded);
    assert_eq!(fixture.state.lock().unwrap().submissions, 1);
    connection.close().await;
}
