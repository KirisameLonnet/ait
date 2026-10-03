//! All creation APIs share PR placement and require approval before running fork setup.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use super::*;

#[tokio::test]
async fn modern_legacy_and_workspace_pr_checkout_preserve_fork_trust_and_native_placement() {
    let fixture = super::super::super::native::NativeFixture::new();
    prepare_repository(&fixture.root, &fixture.cwd);
    install_forge(&fixture.root);
    let state = fixture.root.path().join("state");
    let log = fixture.root.path().join("server.log");
    let mut process = start_with_path(&state, &log, Some(&fixture.path));
    let address = ready(&mut process, &log).await;
    let mut methods = METHODS.to_vec();
    methods.extend([
        "workspace.create.request",
        "workspace.worktree.create.request",
        "workspace.setup.status.request",
        "workspace.setup.run.request",
    ]);
    let mut client = connect(&address, &methods).await;
    let source = json!({"kind":"change_request","forge":"github","number":123});
    let mut placements = Vec::new();
    for (method, params) in [
        (
            "agent.create.request",
            json!({"config":{"provider":"codex","cwd":fixture.cwd},
            "worktree":{"mode":"checkout-pr","prNumber":123}}),
        ),
        (
            "agent.create.request",
            json!({"config":{"provider":"codex","cwd":fixture.cwd},
            "git":{"createWorktree":true,"checkoutSource":source,"githubPrNumber":999}}),
        ),
        (
            "workspace.create.request",
            json!({"source":{"kind":"worktree","cwd":fixture.cwd,"checkoutSource":source}}),
        ),
        (
            "workspace.worktree.create.request",
            json!({"cwd":fixture.cwd,"githubPrNumber":123}),
        ),
    ] {
        let result = rpc(&mut client, method, params).await;
        assert!(result["error"].is_null(), "{method}: {result}");
        let (id, directory) = if method == "agent.create.request" {
            (
                result["agent"]["workspaceId"].clone(),
                result["agent"]["cwd"].clone(),
            )
        } else {
            (
                result["workspace"]["id"].clone(),
                result["workspace"]["workspaceDirectory"].clone(),
            )
        };
        let directory = PathBuf::from(directory.as_str().unwrap());
        assert!(branch(&directory).starts_with("forkowner/topic"));
        assert!(directory.join("pr-content.txt").is_file());
        let status = rpc(
            &mut client,
            "workspace.setup.status.request",
            json!({"workspaceId":id}),
        )
        .await;
        assert_eq!(status["snapshot"]["status"], "blocked", "{status}");
        assert_eq!(
            status["snapshot"]["blockedSource"]["headRepository"],
            "ForkOwner/repository"
        );
        assert_eq!(status["snapshot"]["blockedSource"]["number"], 123);
        assert!(!directory.join("setup-ran").exists());
        placements.push((id, directory));
    }
    let (id, directory) = &placements[0];
    let approved = rpc(
        &mut client,
        "workspace.setup.run.request",
        json!({"workspaceId":id}),
    )
    .await;
    assert!(approved["error"].is_null(), "{approved}");
    tokio::time::timeout(Duration::from_secs(5), async {
        while !directory.join("setup-ran").is_file() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        placements[1..]
            .iter()
            .all(|(_, directory)| !directory.join("setup-ran").exists())
    );
    assert_eq!(branch(&fixture.cwd), "main");
    assert!(!fixture.cwd.join("pr-content.txt").exists());
    client.close(None).await.unwrap();
    terminate(&mut process).await;
}

async fn rpc(
    client: &mut super::super::super::transport::Socket,
    method: &str,
    params: Value,
) -> Value {
    let mut value = request(client, method, params).await;
    while value["type"] == "event" {
        value = receive(client).await;
    }
    assert_eq!(value["type"], "response", "{method}: {value}");
    value["result"].clone()
}

fn prepare_repository(root: &tempfile::TempDir, cwd: &Path) {
    create_repository(cwd);
    run(cwd, &["checkout", "-b", "topic"]);
    std::fs::write(cwd.join("pr-content.txt"), "PR checkout").unwrap();
    std::fs::write(
        cwd.join("ait.json"),
        r#"{"worktree":{"setup":["touch setup-ran"]}}"#,
    )
    .unwrap();
    run(cwd, &["add", "."]);
    run(
        cwd,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-m",
            "PR head",
        ],
    );
    let remote = root.path().join("remote.git");
    run(
        root.path(),
        &[
            "clone",
            "--bare",
            cwd.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    );
    run(&remote, &["update-ref", "refs/pull/123/head", "topic"]);
    run(cwd, &["checkout", "main"]);
    run(cwd, &["remote", "add", "origin", remote.to_str().unwrap()]);
    run(
        cwd,
        &[
            "config",
            &format!("url.{}.insteadOf", remote.display()),
            "git@github.com:ForkOwner/repository.git",
        ],
    );
}

fn install_forge(root: &tempfile::TempDir) {
    let executable = root.path().join("gh");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/filesystem/tests/fixtures/gh_checkout.py");
    std::fs::copy(fixture, &executable).unwrap();
    std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(root.path().join("pull-request.json"), json!({"data":{"repository":{"pullRequest":{
        "number":123,"headRefName":"topic","baseRefName":"main","isCrossRepository":true,
        "headRepositoryOwner":{"login":"ForkOwner"},
        "headRepository":{"sshUrl":"git@github.com:ForkOwner/repository.git","url":"https://github.com/ForkOwner/repository"}
    }}}}).to_string()).unwrap();
}
