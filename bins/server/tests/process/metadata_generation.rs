use super::native::NativeFixture;
use super::transport::{Socket, connect, request};
use super::{ready, start_with_path, terminate};
use serde_json::{Value, json};

#[tokio::test]
async fn metadata_generation_applies_live_config_workspace_title_session_title_and_commit() {
    let NativeFixture { root, cwd, path } = NativeFixture::new();
    let state = root.path().join("state");
    let log = root.path().join("server.log");
    let mut process = start_with_path(&state, &log, Some(&path));
    let address = ready(&mut process, &log).await;
    let mut socket = connect(
        &address,
        &[
            "daemon.config.set.request",
            "workspace.create.request",
            "workspace.list.request",
            "agent.create.request",
            "agent.message.send.request",
            "agent.get.request",
            "agent.finish.wait.request",
            "checkout.commit.request",
        ],
    )
    .await;
    let configured=request(&mut socket,"daemon.config.set.request",json!({"config":{"metadataGeneration":{"providers":[{"provider":"codex","model":"metadata-only"}]}}})).await;
    assert_eq!(configured["type"], "response", "{configured}");
    std::fs::write(
        cwd.join("metadata-response.json"),
        json!({"title":"Generated workspace","branch":"fix/metadata"}).to_string(),
    )
    .unwrap();
    let workspace=request(&mut socket,"workspace.create.request",json!({"source":{"kind":"directory","path":cwd},"firstAgentContext":{"prompt":"Fix metadata"}})).await;
    assert_eq!(workspace["type"], "response", "{workspace}");
    wait_for(&mut socket, "workspace.list.request", json!({}), |reply| {
        reply["result"]["entries"][0]["title"] == "Generated workspace"
    })
    .await;
    std::fs::write(
        cwd.join("metadata-response.json"),
        json!({"title":"Generated session"}).to_string(),
    )
    .unwrap();
    let created = request(
        &mut socket,
        "agent.create.request",
        json!({"config":{"provider":"codex","cwd":cwd}}),
    )
    .await;
    let id = created["result"]["agentId"].as_str().unwrap();
    let sent = request(
        &mut socket,
        "agent.message.send.request",
        json!({"agentId":id,"text":"Fix metadata"}),
    )
    .await;
    assert_eq!(sent["result"]["accepted"], true, "{sent}");
    wait_for(
        &mut socket,
        "agent.get.request",
        json!({"agentId":id}),
        |reply| reply["result"]["agent"]["title"] == "Generated session",
    )
    .await;
    let finished = request(
        &mut socket,
        "agent.finish.wait.request",
        json!({"agentId":id}),
    )
    .await;
    assert_eq!(finished["result"]["lastMessage"], "Echo: Fix metadata");
    git(&cwd, &["init", "-b", "main"]);
    git(&cwd, &["config", "user.email", "test@example.test"]);
    git(&cwd, &["config", "user.name", "Metadata Test"]);
    std::fs::write(cwd.join(".gitignore"), "metadata-response.json\nnative-*\n").unwrap();
    std::fs::write(
        cwd.join("metadata-response.json"),
        json!({"message":"Generate commit from diff"}).to_string(),
    )
    .unwrap();
    std::fs::write(cwd.join("change.txt"), "new behavior\n").unwrap();
    let committed = request(&mut socket, "checkout.commit.request", json!({"cwd":cwd})).await;
    assert_eq!(committed["result"]["error"], Value::Null, "{committed}");
    assert_eq!(
        git(&cwd, &["log", "-1", "--format=%s"]),
        "Generate commit from diff"
    );
    let native: Vec<Value> = std::fs::read_to_string(cwd.join("native-requests.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        native
            .iter()
            .any(|request| request["method"] == "thread/start"
                && request["params"]["ephemeral"] == true
                && request["params"]["model"] == "metadata-only")
    );
    terminate(&mut process).await;
}

async fn wait_for(
    socket: &mut Socket,
    method: &str,
    params: Value,
    matches: impl Fn(&Value) -> bool,
) {
    let mut last = Value::Null;
    for _ in 0..100 {
        last = request(socket, method, params.clone()).await;
        if matches(&last) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("metadata result did not arrive: {last}");
}

fn git(cwd: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[tokio::test]
async fn metadata_generation_names_new_managed_worktree_and_preserves_explicit_branch() {
    let NativeFixture { root, cwd, path } = NativeFixture::new();
    git(&cwd, &["init", "-b", "main"]);
    git(&cwd, &["config", "user.email", "test@example.test"]);
    git(&cwd, &["config", "user.name", "Metadata Test"]);
    std::fs::write(
        cwd.join("metadata-response.json"),
        json!({"title":"Generated worktree","branch":"fix/metadata"}).to_string(),
    )
    .unwrap();
    git(&cwd, &["add", "metadata-response.json"]);
    git(&cwd, &["commit", "-m", "Base"]);
    let state = root.path().join("state");
    let log = root.path().join("server.log");
    let mut process = start_with_path(&state, &log, Some(&path));
    let address = ready(&mut process, &log).await;
    let mut socket = connect(
        &address,
        &[
            "daemon.config.set.request",
            "workspace.worktree.create.request",
            "workspace.list.request",
        ],
    )
    .await;
    request(&mut socket,"daemon.config.set.request",json!({"config":{"metadataGeneration":{"providers":[{"provider":"codex","model":"metadata-only"}]}}})).await;
    for slug in [None, Some("explicit-branch")] {
        let created = request(
            &mut socket,
            "workspace.worktree.create.request",
            json!({"cwd":cwd,"worktreeSlug":slug,"firstAgentContext":{"prompt":"Fix metadata"}}),
        )
        .await;
        assert_eq!(created["result"]["error"], Value::Null, "{created}");
        let workspace = &created["result"]["workspace"];
        let id = workspace["id"].as_str().unwrap();
        // Worktree creation emits its normal update after the response.
        let event = super::transport::receive(&mut socket).await;
        assert_eq!(event["method"], "workspace.update");
        wait_for(&mut socket, "workspace.list.request", json!({}), |reply| {
            reply["result"]["entries"]
                .as_array()
                .is_some_and(|entries| {
                    entries
                        .iter()
                        .any(|entry| entry["id"] == id && entry["title"] == "Generated worktree")
                })
        })
        .await;
        let working_directory = workspace["workspaceDirectory"].as_str().unwrap();
        assert_eq!(
            git(
                std::path::Path::new(working_directory),
                &["branch", "--show-current"]
            ),
            slug.unwrap_or("fix/metadata")
        );
    }
    terminate(&mut process).await;
}
