//! Native plugins share durable admission, immutable publication and recovery with Codex.
#![allow(clippy::pedantic)]
mod fixtures;
#[path = "opencode_execution/opencode_fixture.rs"]
mod opencode_fixture;
mod support;

use ait_application::LocalControlService;
use ait_contracts::{AgentConfiguration, AgentProvider, Command, CommandResult, ProviderModel};
use ait_domain::{ProviderKind, SessionSource};
use ait_storage_sqlite::SqliteControlStore;
use fixtures::control_fixtures::{ok, save_permission_settings, send_text, setup, view};
use opencode_fixture::Writer;
use std::sync::Arc;

async fn fixture() -> (LocalControlService, Arc<Writer>, tempfile::TempDir) {
    let store = Arc::new(SqliteControlStore::in_memory().unwrap());
    let writer = Arc::new(Writer::new(store.clone()));
    let service = LocalControlService::new(
        Arc::new(ait_workspace_local::LocalProjectWorkspace::default()),
        store,
    )
    .with_native_session_writer(writer.clone());
    ok(
        &service,
        Command::SaveAgentProvider {
            provider: AgentProvider {
                id: "builtin-opencode".into(),
                name: "OpenCode".into(),
                kind: ProviderKind::OpenCode,
                url: None,
                models: vec![ProviderModel {
                    id: "local/model".into(),
                    name: "Local model".into(),
                    reasoning_efforts: vec![],
                }],
            },
            secret: None,
        },
    )
    .await;
    let directory = setup(
        &service,
        AgentConfiguration {
            provider_id: "builtin-opencode".into(),
            model: "local/model".into(),
            reasoning_effort: None,
            system_prompt: None,
        },
    )
    .await;
    save_permission_settings(&service, "full_access", "on_request").await;
    (service, writer, directory)
}

#[tokio::test]
async fn plugin_admits_before_sending_and_resumes_only_new_input() {
    let (service, writer, directory) = fixture().await;
    let CommandResult::Run(first) = ok(&service, send_text("one", "first")).await else {
        panic!()
    };
    assert_eq!(first.status, "completed");
    let before = view(&service).await;
    let CommandResult::Run(second) = ok(&service, send_text("one", "second")).await else {
        panic!()
    };
    assert_eq!(second.status, "completed");
    let after = view(&service).await;
    assert!(
        before
            .messages
            .iter()
            .all(|message| after.messages.contains(message))
    );
    let session = after
        .sessions
        .iter()
        .find(|session| session.id == "one")
        .unwrap();
    assert!(
        matches!(&session.source, SessionSource::NativeSession(source) if source.driver=="opencode")
    );
    assert!(session.active_run_id.is_none());
    let calls = writer.requests.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(calls[0].session_id.is_none());
    assert_eq!(calls[1].session_id.as_deref(), Some("ses_one"));
    assert_eq!(calls[1].prompt, "second");
    assert!(calls[1].instructions.is_none());
    assert_eq!(
        calls[0].cwd,
        directory.path().canonicalize().unwrap().join(".ait/one")
    );
    assert_eq!(calls[0].cwd, calls[1].cwd);
    assert_eq!(
        writer.close_count.load(std::sync::atomic::Ordering::SeqCst),
        2
    );
}

#[tokio::test]
async fn unknown_acceptance_is_reconciled_on_resume_without_replaying() {
    let (service, writer, _directory) = fixture().await;
    let CommandResult::Run(unknown) = ok(&service, send_text("one", "lost response")).await else {
        panic!()
    };
    assert_eq!(unknown.status, "interrupted");
    let CommandResult::Run(next) = ok(&service, send_text("one", "new work")).await else {
        panic!()
    };
    assert_eq!(next.status, "completed");
    let after = view(&service).await;
    assert_eq!(
        after
            .runs
            .iter()
            .find(|run| run.id == unknown.id)
            .unwrap()
            .status,
        "completed"
    );
    let calls = writer.requests.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.prompt == "lost response")
            .count(),
        1
    );
}

#[tokio::test]
async fn executor_panic_closes_connection_and_releases_session() {
    let (service, writer, _directory) = fixture().await;
    let CommandResult::Run(run) = ok(&service, send_text("one", "panic")).await else {
        panic!()
    };
    assert_eq!(run.status, "interrupted");
    assert!(
        view(&service)
            .await
            .sessions
            .iter()
            .find(|session| session.id == "one")
            .unwrap()
            .active_run_id
            .is_none()
    );
    assert_eq!(
        writer.close_count.load(std::sync::atomic::Ordering::SeqCst),
        1
    );
}

#[tokio::test]
async fn native_failure_can_publish_its_accepted_input_without_an_assistant() {
    let (service, _writer, _directory) = fixture().await;
    let CommandResult::Run(run) = ok(&service, send_text("one", "early failure")).await else {
        panic!()
    };
    assert_eq!(run.status, "failed");
    let state = view(&service).await;
    assert!(
        state
            .messages
            .iter()
            .any(|message| message.text.as_deref() == Some("early failure"))
    );
    assert!(
        state
            .sessions
            .iter()
            .find(|session| session.id == "one")
            .unwrap()
            .active_run_id
            .is_none()
    );
}

#[tokio::test]
async fn startup_recovery_reads_native_history_without_resubmission() {
    use support::ControlStoreTestExt;
    let (service, writer, _directory) = fixture().await;
    let CommandResult::Run(run) = ok(&service, send_text("one", "accepted before crash")).await
    else {
        panic!()
    };
    let before = view(&service).await.messages;
    let snapshot = writer.store.load().await.unwrap();
    let mut state = snapshot.value;
    state["runs"][0]["status"] = serde_json::json!("running");
    state["runs"][0]["phase"] = serde_json::json!("calling_agent");
    state["runs"][0]["harness_input"]["state"] = serde_json::json!("send_unknown");
    state["sessions"][0]["active_run_id"] = serde_json::json!(run.id);
    writer
        .store
        .commit(snapshot.revision, state, vec![])
        .await
        .unwrap();
    let restarted = LocalControlService::new(
        Arc::new(ait_workspace_local::LocalProjectWorkspace::default()),
        writer.store.clone(),
    )
    .with_native_session_writer(writer.clone());
    let recovered = restarted.recover_interrupted_runs().await.unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].status, "completed");
    assert_eq!(
        writer.start_count.load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(view(&restarted).await.messages, before);
    assert!(
        view(&restarted)
            .await
            .sessions
            .iter()
            .find(|session| session.id == "one")
            .unwrap()
            .active_run_id
            .is_none()
    );
}

#[tokio::test]
async fn native_tools_are_assistant_parts_and_results_are_user_messages() {
    let (service, writer, _directory) = fixture().await;
    let CommandResult::Run(run) = ok(&service, send_text("one", "tools")).await else {
        panic!()
    };
    assert_eq!(run.status, "completed");
    use ait_ports::{ControlFilter, ControlRecordKind};
    let records = writer
        .store
        .read(&[ControlFilter::all(ControlRecordKind::Message)])
        .await
        .unwrap()
        .records;
    let results = records
        .iter()
        .filter(|record| record.value["kind"] == "tool_result")
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].value["role"], "user");
    let encoded = serde_json::to_string(&records).unwrap();
    assert!(encoded.contains("tool_use"));
    assert!(encoded.contains("call1"));
    assert!(encoded.contains(&run.id));
}

#[tokio::test]
async fn native_history_branching_fails_before_another_input_is_sent() {
    let (service, writer, _directory) = fixture().await;
    let CommandResult::Run(run) = ok(&service, send_text("one", "first")).await else {
        panic!()
    };
    ok(
        &service,
        Command::CreateSession {
            id: "branch".into(),
            project_id: "p".into(),
            agent_id: "preset".into(),
            at_message_id: run.last_message_id,
        },
    )
    .await;
    let before = view(&service).await;
    let result = service.execute(send_text("branch", "branch input")).await;
    assert_eq!(
        result.error.unwrap().code,
        ait_domain::ErrorCode::AgentCapabilityUnsupported
    );
    assert_eq!(view(&service).await, before);
    assert_eq!(
        writer.start_count.load(std::sync::atomic::Ordering::SeqCst),
        1
    );
}
