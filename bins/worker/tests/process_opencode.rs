//! The native plugin traverses the real worker IPC and owns its HTTP helper process.
#![cfg(unix)]
#![allow(clippy::pedantic)]
use ait_domain::{RunPermissionProfile, SandboxAccess};
use ait_ports::{
    DenyWorkspaceApprovals, NativeSessionInvocation, NativeSessionWriter, WorkspaceProgressEvent,
    WorkspaceProgressReporter,
};
use async_trait::async_trait;
use std::{os::unix::fs::PermissionsExt, path::PathBuf, sync::Arc};

struct Progress;
#[async_trait]
impl WorkspaceProgressReporter for Progress {
    async fn report(&self, _: WorkspaceProgressEvent) {}
}

fn fixture() -> (
    tempfile::TempDir,
    ait_ipc::supervisor::WorkerSupervisor,
    NativeSessionInvocation,
) {
    let directory = tempfile::tempdir().unwrap();
    let cwd = directory.path().canonicalize().unwrap();
    let binary = cwd.join("opencode-fixture.py");
    std::fs::write(
        &binary,
        format!(
            "#!/usr/bin/env python3\n{}",
            include_str!("fixtures/opencode_http.py")
        ),
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    let worker =
        ait_ipc::supervisor::WorkerSupervisor::new(PathBuf::from(env!("CARGO_BIN_EXE_ait-worker")))
            .with_opencode_binary(binary);
    let request = NativeSessionInvocation {
        driver: "opencode".into(),
        request_id: "run-input".into(),
        session_id: None,
        input_id: "input1".into(),
        prompt: "new input only".into(),
        instructions: Some("Project instructions".into()),
        cwd,
        model: "local/model".into(),
        reasoning_effort: None,
        permission_profile: RunPermissionProfile {
            sandbox: SandboxAccess::FullAccess,
            ..Default::default()
        },
        project_execution: None,
        approvals: Arc::new(DenyWorkspaceApprovals),
        cancellation: tokio_util::sync::CancellationToken::new(),
    };
    (directory, worker, request)
}

fn assert_reaped(cwd: &std::path::Path) {
    for pid in std::fs::read_to_string(cwd.join("pids.txt"))
        .unwrap()
        .lines()
    {
        assert!(
            !std::process::Command::new("kill")
                .args(["-0", pid])
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}

#[tokio::test]
async fn prepare_send_read_resume_and_close_traverse_real_worker() {
    let (_directory, worker, request) = fixture();
    let mut connection = worker.open(request.clone()).await.unwrap();
    assert!(connection.prepared().messages.is_empty());
    assert!(!request.cwd.join("native-fixture.json").exists());
    let history = connection.start(Arc::new(Progress)).await.unwrap();
    assert_eq!(history.messages.len(), 2);
    assert_eq!(history.messages[0].input_id.as_deref(), Some("input1"));
    assert!(connection.start(Arc::new(Progress)).await.is_err());
    assert_eq!(connection.read().await.unwrap(), history);
    connection.close().await;
    assert_reaped(&request.cwd);
    let mut resumed = request.clone();
    resumed.session_id = Some(history.id);
    resumed.input_id = "input2".into();
    resumed.prompt = "second input only".into();
    resumed.instructions = None;
    let mut connection = worker.open(resumed).await.unwrap();
    assert_eq!(connection.prepared().messages, history.messages);
    let second = connection.start(Arc::new(Progress)).await.unwrap();
    assert_eq!(second.messages.len(), 4);
    assert_eq!(second.messages[2].input_id.as_deref(), Some("input2"));
    connection.close().await;
    assert_reaped(&request.cwd);
}

#[tokio::test]
async fn closing_prepared_plugin_sends_nothing_and_reaps_helper() {
    let (_directory, worker, request) = fixture();
    let mut connection = worker.open(request.clone()).await.unwrap();
    connection.close().await;
    assert!(!request.cwd.join("native-fixture.json").exists());
    assert_reaped(&request.cwd);
}

#[tokio::test]
async fn native_model_catalog_uses_an_auxiliary_owned_worker() {
    use ait_ports::HostProviderModelCatalog;
    let (_directory, worker, request) = fixture();
    let provider = ait_domain::AgentProvider {
        id: "opencode".into(),
        name: "OpenCode".into(),
        kind: ait_domain::ProviderKind::OpenCode,
        url: None,
        models: vec![],
    };
    let models = worker.discover_models(&provider).await.unwrap();
    assert_eq!(models[0].id, "local/model");
    assert_reaped(&request.cwd);
    assert!(!request.cwd.join("native-fixture.json").exists());
}
