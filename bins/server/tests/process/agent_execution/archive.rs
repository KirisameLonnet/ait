//! Metadata and worktree archives must finish native/PTY cleanup before deleting a checkout.

use std::path::Path;
use std::time::Duration;

use super::*;

#[tokio::test]
async fn workspace_project_and_worktree_archives_close_agents_and_terminals_before_returning() {
    for method in [
        "workspace.archive.request",
        "project.remove.request",
        "workspace.worktree.archive.request",
    ] {
        archive_resources(method).await;
    }
}

async fn archive_resources(method: &str) {
    let super::super::native::NativeFixture { root, cwd, path } =
        super::super::native::NativeFixture::new();
    super::super::worktrees::create_repository(&cwd);
    write_process_config(&cwd);
    let state = root.path().join("state");
    let log = root.path().join("server.log");
    let mut process = start_with_path(&state, &log, Some(&path));
    let address = ready(&mut process, &log).await;
    let mut methods = METHODS.to_vec();
    methods.extend([
        "workspace.list.request",
        "terminal.create.request",
        "terminal.list.request",
        "workspace.script.start.request",
        method,
    ]);
    let mut client = connect(&address, &methods).await;
    let created = request(
        &mut client,
        "agent.create.request",
        json!({"config":{"provider":"codex","cwd":cwd},
        "worktree":{"mode":"branch-off","newBranch":"archive-test"},"initialPrompt":"hang"}),
    )
    .await;
    assert_eq!(created["type"], "response", "{created}");
    let id = &created["result"]["agentId"];
    let workspace = &created["result"]["agent"]["workspaceId"];
    let directory = Path::new(created["result"]["agent"]["cwd"].as_str().unwrap());
    let terminal_pid = start_terminal(&mut client, workspace, directory).await;
    let automation_pids = start_automation(&mut client, workspace, directory).await;
    let records = std::fs::read_to_string(directory.join("native-requests.jsonl")).unwrap();
    let listing = request(&mut client, "workspace.list.request", json!({})).await;
    let project = &listing["result"]["entries"][0]["projectId"];
    let params = if method == "project.remove.request" {
        json!({"projectId":project})
    } else {
        json!({"workspaceId":workspace})
    };
    let archived = request(&mut client, method, params.clone()).await;
    assert_eq!(archived["type"], "response", "{archived}");
    assert!(archived["result"]["error"].is_null(), "{archived}");
    if method == "workspace.worktree.archive.request" {
        assert_eq!(archived["result"]["removedAgents"], json!([id]));
    }
    assert!(!directory.exists(), "{method} retained checkout");
    assert!(cwd.join("README.md").exists());
    assert_native_pids_exited(&records);
    assert!(!pid_is_alive(&terminal_pid));
    assert!(automation_pids.iter().all(|pid| !pid_is_alive(pid)));
    let agent = request(&mut client, "agent.get.request", json!({"agentId":id})).await;
    assert!(
        agent["result"]["agent"]["archivedAt"].is_string(),
        "{agent}"
    );
    let terminals = request(&mut client, "terminal.list.request", json!({})).await;
    assert_eq!(terminals["result"]["terminals"], json!([]));
    let repeated = request(&mut client, method, params).await;
    assert!(repeated["result"]["error"].is_null(), "{repeated}");
    client.close(None).await.unwrap();
    terminate(&mut process).await;
}

pub(super) async fn start_terminal(
    client: &mut super::super::transport::Socket,
    workspace: &Value,
    directory: &Path,
) -> String {
    let _ = std::fs::remove_file(directory.join("terminal.pid"));
    let created = request(
        client,
        "terminal.create.request",
        json!({"workspaceId":workspace,"cwd":directory,
        "command":"/bin/sh","args":["-c","printf '%s' $$ > terminal.pid; sleep 300"]}),
    )
    .await;
    assert!(created["result"]["error"].is_null(), "{created}");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(pid) = std::fs::read_to_string(directory.join("terminal.pid"))
                && !pid.is_empty()
            {
                return pid;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn same_checkout_workspaces_keep_each_others_agents_terminals_and_files() {
    let super::super::native::NativeFixture { root, cwd, path } =
        super::super::native::NativeFixture::new();
    super::super::worktrees::create_repository(&cwd);
    let state = root.path().join("state");
    let log = root.path().join("server.log");
    let mut process = start_with_path(&state, &log, Some(&path));
    let address = ready(&mut process, &log).await;
    let mut methods = METHODS.to_vec();
    methods.extend([
        "workspace.create.request",
        "workspace.archive.request",
        "terminal.create.request",
        "terminal.list.request",
    ]);
    let mut client = connect(&address, &methods).await;
    let first = request(
        &mut client,
        "agent.create.request",
        json!({"config":{"provider":"codex","cwd":cwd},
        "worktree":{"mode":"branch-off","newBranch":"shared-checkout"},
        "initialPrompt":"hang"}),
    )
    .await;
    assert_eq!(first["type"], "response", "{first}");
    let first_workspace = &first["result"]["agent"]["workspaceId"];
    let directory = Path::new(first["result"]["agent"]["cwd"].as_str().unwrap());
    let first_pid = start_terminal(&mut client, first_workspace, directory).await;
    let created = request(
        &mut client,
        "workspace.create.request",
        json!({"source":{"kind":"directory","path":directory}}),
    )
    .await;
    assert_eq!(created["type"], "response", "{created}");
    let second_workspace = &created["result"]["workspace"]["id"];
    assert_ne!(first_workspace, second_workspace);
    let second = request(
        &mut client,
        "agent.create.request",
        json!({"workspaceId":second_workspace,"config":{"provider":"codex","cwd":directory},
        "initialPrompt":"hang"}),
    )
    .await;
    assert_eq!(second["type"], "response", "{second}");
    let second_pid = start_terminal(&mut client, second_workspace, directory).await;
    let archived = request(
        &mut client,
        "workspace.archive.request",
        json!({"workspaceId":first_workspace}),
    )
    .await;
    assert!(archived["result"]["error"].is_null(), "{archived}");
    assert!(directory.exists());
    assert!(!pid_is_alive(&first_pid));
    assert!(pid_is_alive(&second_pid));
    let listed = request(&mut client, "terminal.list.request", json!({})).await;
    assert_eq!(listed["result"]["terminals"].as_array().unwrap().len(), 1);
    assert_eq!(
        listed["result"]["terminals"][0]["workspaceId"],
        *second_workspace
    );
    let agent = request(
        &mut client,
        "agent.get.request",
        json!({"agentId":second["result"]["agentId"]}),
    )
    .await;
    assert!(agent["result"]["agent"]["archivedAt"].is_null(), "{agent}");
    let archived = request(
        &mut client,
        "workspace.archive.request",
        json!({"workspaceId":second_workspace}),
    )
    .await;
    assert!(archived["result"]["error"].is_null(), "{archived}");
    assert!(!directory.exists());
    assert!(!pid_is_alive(&second_pid));
    client.close(None).await.unwrap();
    terminate(&mut process).await;
}

#[tokio::test]
async fn auto_archive_closes_terminal_before_removing_the_owned_checkout() {
    let super::super::native::NativeFixture { root, cwd, path } =
        super::super::native::NativeFixture::new();
    super::super::worktrees::create_repository(&cwd);
    write_process_config(&cwd);
    let state = root.path().join("state");
    let log = root.path().join("server.log");
    let mut process = start_with_path(&state, &log, Some(&path));
    let address = ready(&mut process, &log).await;
    let mut methods = METHODS.to_vec();
    methods.push("terminal.create.request");
    methods.push("workspace.script.start.request");
    let mut client = connect(&address, &methods).await;
    let created = request(
        &mut client,
        "agent.create.request",
        json!({"config":{"provider":"codex","cwd":cwd}, "autoArchive":true,
        "worktree":{"mode":"branch-off","newBranch":"automatic-cleanup"}}),
    )
    .await;
    assert_eq!(created["type"], "response", "{created}");
    let directory = Path::new(created["result"]["agent"]["cwd"].as_str().unwrap());
    let pid = start_terminal(
        &mut client,
        &created["result"]["agent"]["workspaceId"],
        directory,
    )
    .await;
    let automation_pids = start_automation(
        &mut client,
        &created["result"]["agent"]["workspaceId"],
        directory,
    )
    .await;
    let result = request(
        &mut client,
        "agent.message.send.request",
        json!({"agentId":created["result"]["agentId"],"text":"finish"}),
    )
    .await;
    assert_eq!(result["result"]["accepted"], true, "{result}");
    tokio::time::timeout(Duration::from_secs(10), async {
        while directory.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(!pid_is_alive(&pid));
    assert!(automation_pids.iter().all(|pid| !pid_is_alive(pid)));
    assert!(cwd.join("README.md").exists());
    client.close(None).await.unwrap();
    terminate(&mut process).await;
}

fn pid_is_alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

fn write_process_config(directory: &Path) {
    std::fs::write(
        directory.join("ait.json"),
        json!({
            "worktree":{"setup":["printf '%s' $$ > setup.pid; sleep 300"]},
            "scripts":{"background":{"command":"printf '%s' $$ > script.pid; sleep 300"}},
        })
        .to_string(),
    )
    .unwrap();
}

async fn start_automation(
    client: &mut super::super::transport::Socket,
    workspace: &Value,
    directory: &Path,
) -> Vec<String> {
    let started = request(
        client,
        "workspace.script.start.request",
        json!({"workspaceId":workspace,"scriptName":"background"}),
    )
    .await;
    assert!(started["result"]["error"].is_null(), "{started}");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let pids: Vec<_> = ["setup.pid", "script.pid"]
                .iter()
                .filter_map(|file| std::fs::read_to_string(directory.join(file)).ok())
                .filter(|pid| !pid.is_empty())
                .collect();
            if pids.len() == 2 {
                return pids;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap()
}
