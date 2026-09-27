use std::time::Duration;

use serde_json::Value;

use super::*;

const METHODS: &[&str] = &[
    "workspace.create.request",
    "workspace.worktree.list.request",
    "workspace.setup.status.request",
    "project.add.request",
    "workspace.list.request",
    "creation.subscribe.request",
];

async fn call(client: &mut Socket, method: &str, params: Value) -> Value {
    let mut reply = request(client, method, params).await;
    while reply["type"] == "event" {
        reply = receive(client).await;
    }
    reply
}

#[tokio::test]
async fn unified_worktree_creation_preserves_nested_placement_and_replays_after_restart() {
    let root = tempfile::tempdir().unwrap();
    let repository = root.path().join("repo");
    create_repository(&repository);
    let nested = repository.join("packages/app");
    std::fs::write(
        nested.join("paseo.json"),
        r#"{"worktree":{"setup":["printf x >> setup-count.txt"]}}"#,
    )
    .unwrap();
    let state = root.path().join("server");
    let log = root.path().join("server.log");
    let mut process = start(&state, &log);
    let address = ready(&mut process, &log).await;
    let mut client = connect(&address, METHODS).await;
    let intent = json!({
        "workspaceId":"wks_0123456789abcdef", "idempotencyKey":"unified-worktree", "subscribe":true,
        "title":" Review workspace ", "firstAgentContext":{"prompt":"Do the work","attachments":[]},
        "source":{"kind":"worktree", "cwd":nested, "worktreeSlug":"review-dir",
            "branchName":"feature/review", "baseBranch":"main"}
    });
    let created = call(&mut client, "workspace.create.request", intent.clone()).await;
    assert_eq!(created["type"], "response", "{created}");
    assert!(created["result"]["error"].is_null(), "{created}");
    assert!(created["result"]["subscriptionId"].is_string());
    assert_eq!(created["result"]["creation"]["phase"], "completed");
    assert_eq!(
        created["result"]["creation"]["workspaceId"],
        "wks_0123456789abcdef"
    );
    let workspace = &created["result"]["workspace"];
    assert_eq!(workspace["id"], "wks_0123456789abcdef");
    assert_eq!(workspace["title"], "Review workspace");
    assert_eq!(workspace["workspaceKind"], "worktree");
    assert_eq!(workspace["worktreeSlug"], "review-dir");
    let cwd = PathBuf::from(workspace["workspaceDirectory"].as_str().unwrap());
    assert!(cwd.ends_with("review-dir/packages/app"));
    assert_eq!(branch(&cwd), "feature/review");
    wait_for_setup(&mut client).await;
    assert_eq!(
        std::fs::read_to_string(cwd.join("setup-count.txt")).unwrap(),
        "x"
    );
    let replay = call(&mut client, "workspace.create.request", intent.clone()).await;
    assert_eq!(replay["result"]["workspace"], *workspace);
    let mut conflict = intent.clone();
    conflict["source"]["branchName"] = json!("feature/other");
    assert_eq!(
        call(&mut client, "workspace.create.request", conflict).await["code"],
        "idempotency_conflict"
    );
    let mut duplicate = intent.clone();
    duplicate["idempotencyKey"] = json!("duplicate-id");
    let rejected = call(&mut client, "workspace.create.request", duplicate).await;
    assert_eq!(
        rejected["result"]["errorCode"], "invalid_request",
        "{rejected}"
    );
    terminate(&mut process).await;

    let mut process = start(&state, &log);
    let address = ready(&mut process, &log).await;
    let mut client = connect(&address, METHODS).await;
    let replay = call(&mut client, "workspace.create.request", intent).await;
    assert_eq!(replay["result"]["workspace"], *workspace);
    let listed = call(
        &mut client,
        "workspace.worktree.list.request",
        json!({"cwd":repository}),
    )
    .await;
    assert_eq!(listed["result"]["worktrees"].as_array().unwrap().len(), 1);
    assert_eq!(
        std::fs::read_to_string(cwd.join("setup-count.txt")).unwrap(),
        "x"
    );
    terminate(&mut process).await;
}

async fn wait_for_setup(client: &mut Socket) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let status = call(
            client,
            "workspace.setup.status.request",
            json!({"workspaceId":"wks_0123456789abcdef"}),
        )
        .await;
        if status["result"]["snapshot"]["status"] == "completed" {
            return;
        }
        assert!(tokio::time::Instant::now() < deadline, "{status}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn unified_worktree_creation_supports_project_only_checkout_and_reports_errors() {
    let root = tempfile::tempdir().unwrap();
    let repository = root.path().join("repo");
    create_repository(&repository);
    run(&repository, &["branch", "existing"]);
    let state = root.path().join("server");
    let log = root.path().join("server.log");
    let mut process = start(&state, &log);
    let address = ready(&mut process, &log).await;
    let mut client = connect(&address, METHODS).await;
    let project = call(
        &mut client,
        "project.add.request",
        json!({"cwd":repository}),
    )
    .await;
    let project_id = &project["result"]["project"]["projectId"];
    assert!(project_id.is_string(), "{project}");
    let created = call(&mut client, "workspace.create.request", json!({
        "source":{"kind":"worktree", "projectId":project_id, "action":"checkout", "refName":"existing"}
    })).await;
    let workspace = &created["result"]["workspace"];
    assert!(created["result"]["error"].is_null(), "{created}");
    assert_eq!(workspace["projectId"], *project_id);
    assert_eq!(
        branch(Path::new(workspace["workspaceDirectory"].as_str().unwrap())),
        "existing"
    );
    for (source, code) in [
        (json!({"kind":"worktree"}), "source_required"),
        (
            json!({"kind":"worktree","projectId":"missing"}),
            "unknown_project",
        ),
        (
            json!({"kind":"worktree","cwd":repository,"action":"checkout"}),
            "missing_checkout_target",
        ),
        (
            json!({"kind":"worktree","cwd":repository,"action":"checkout","refName":"missing"}),
            "unknown_branch",
        ),
        (
            json!({"kind":"worktree","cwd":repository,"branchName":"bad..name"}),
            "invalid_request",
        ),
        (
            json!({"kind":"worktree","cwd":repository,"githubPrNumber":42}),
            "unsupported_capability",
        ),
    ] {
        let failed = call(
            &mut client,
            "workspace.create.request",
            json!({"source":source}),
        )
        .await;
        assert_eq!(failed["result"]["errorCode"], code, "{failed}");
        assert!(failed["result"]["workspace"].is_null());
        assert_eq!(failed["result"]["creation"]["phase"], "failed");
    }
    let listed = call(
        &mut client,
        "workspace.worktree.list.request",
        json!({"cwd":repository}),
    )
    .await;
    assert_eq!(listed["result"]["worktrees"].as_array().unwrap().len(), 1);
    terminate(&mut process).await;
}
