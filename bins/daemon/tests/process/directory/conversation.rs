use serde_json::{Value, json};

use super::super::native::NativeFixture;
use super::super::transport::{Socket, connect, request};
use super::super::{ready, start_with_path, terminate};

const METHODS: &[&str] = &[
    "project.create_directory.request",
    "workspace.create.request",
    "workspace.list.request",
    "agent.create.request",
    "agent.finish.wait.request",
    "agent.timeline.get.request",
];

#[tokio::test]
async fn new_directory_project_accepts_first_conversation_context() {
    let fixture = NativeFixture::new();
    let state = fixture.root.path().join("server");
    let log = fixture.root.path().join("server.log");
    let mut process = start_with_path(&state, &log, Some(&fixture.path));
    let address = ready(&mut process, &log).await;
    let mut client = connect(&address, METHODS).await;
    let project = request(
        &mut client,
        "project.create_directory.request",
        json!({"parentPath":fixture.cwd,"name":"new-conversation"}),
    )
    .await;
    assert!(project["result"]["error"].is_null(), "{project}");
    let cwd = project["result"]["directoryPath"].as_str().unwrap();
    let prompt = "Start the first conversation";
    let intent = json!({
        "source":{"kind":"directory","path":cwd,"projectId":project["result"]["project"]["projectId"]},
        "firstAgentContext":{"prompt":prompt,"attachments":[]},
        "idempotencyKey":"first-conversation",
        "title":"New conversation"
    });
    let created = request(&mut client, "workspace.create.request", intent.clone()).await;
    assert_eq!(created["type"], "response", "{created}");
    assert!(created["result"]["error"].is_null(), "{created}");
    assert_eq!(created["result"]["creation"]["phase"], "completed");
    assert_eq!(created["result"]["workspace"]["title"], "New conversation");
    assert!(
        !std::path::Path::new(cwd)
            .join("native-requests.jsonl")
            .exists()
    );
    let replay = request(&mut client, "workspace.create.request", intent.clone()).await;
    assert_eq!(
        replay["result"]["workspace"],
        created["result"]["workspace"]
    );
    let mut conflict = intent;
    conflict["firstAgentContext"]["prompt"] = json!("Different conversation");
    assert_eq!(
        request(&mut client, "workspace.create.request", conflict).await["code"],
        "idempotency_conflict"
    );
    let listed = request(&mut client, "workspace.list.request", json!({})).await;
    assert_eq!(listed["result"]["entries"].as_array().unwrap().len(), 1);
    assert_first_turn(
        &mut client,
        cwd,
        &created["result"]["workspace"]["id"],
        prompt,
    )
    .await;
    terminate(&mut process).await;
}

async fn assert_first_turn(client: &mut Socket, cwd: &str, workspace_id: &Value, prompt: &str) {
    let created = request(
        client,
        "agent.create.request",
        json!({
            "config":{"provider":"codex","cwd":cwd},
            "workspaceId":workspace_id,
            "initialPrompt":prompt
        }),
    )
    .await;
    assert_eq!(created["type"], "response", "{created}");
    let agent_id = &created["result"]["agentId"];
    assert!(agent_id.is_string(), "{created}");
    let finished = request(
        client,
        "agent.finish.wait.request",
        json!({"agentId":agent_id}),
    )
    .await;
    assert_eq!(finished["result"]["status"], "idle", "{finished}");
    assert_eq!(finished["result"]["lastMessage"], format!("Echo: {prompt}"));
    let timeline = request(
        client,
        "agent.timeline.get.request",
        json!({"agentId":agent_id}),
    )
    .await;
    let entries = timeline["result"]["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2, "{timeline}");
    assert_eq!(entries[0]["item"]["text"], prompt);
    assert_eq!(entries[1]["item"]["text"], format!("Echo: {prompt}"));
}
