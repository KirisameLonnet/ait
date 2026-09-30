use std::os::unix::fs::PermissionsExt;

use serde_json::json;

use super::transport::{connect, request};
use super::{ready, start_with_path, terminate};

const METHODS: &[&str] = &[
    "workspace.open.request",
    "agent.create.request",
    "agent.message.send.request",
    "agent.finish.wait.request",
    "agent.timeline.get.request",
    "provider.models.list.request",
    "provider.snapshot.get.request",
    "provider.available.list.request",
    "agent.resume.request",
    "agent.get.request",
];

#[tokio::test]
async fn opencode_is_discovered_executed_and_restored_through_the_real_server() {
    let fixture = fixture();
    let cwd = fixture.cwd.canonicalize().unwrap();
    let state = fixture.root.path().join("state");
    let log = fixture.root.path().join("server.log");
    let mut process = start_with_path(&state, &log, Some(&fixture.path));
    let address = ready(&mut process, &log).await;
    let mut socket = connect(&address, METHODS).await;
    let models = request(
        &mut socket,
        "provider.models.list.request",
        json!({"provider":"opencode","cwd":cwd}),
    )
    .await;
    assert_eq!(
        models["result"]["models"][0]["id"], "local/model",
        "{models}"
    );
    let available = request(&mut socket, "provider.available.list.request", json!({})).await;
    assert!(available.to_string().contains("opencode"), "{available}");
    request(&mut socket, "workspace.open.request", json!({"cwd":cwd})).await;
    let created = request(
        &mut socket,
        "agent.create.request",
        json!({"config":{"provider":"opencode","cwd":cwd,"model":"local/model","modeId":"build"}}),
    )
    .await;
    let id = created["result"]["agentId"]
        .as_str()
        .unwrap_or_else(|| {
            panic!(
                "{created}; providers: {available}; log: {}",
                std::fs::read_to_string(&log).unwrap()
            )
        })
        .to_owned();
    assert_eq!(
        created["result"]["agent"]["persistence"]["provider"],
        "opencode"
    );
    for index in 0..2 {
        let sent = request(&mut socket,"agent.message.send.request",json!({"agentId":id,"text":format!("hello {index}"),"messageId":format!("client-{index}")})).await;
        assert_eq!(sent["result"]["accepted"], true, "{sent}");
        let finished = request(
            &mut socket,
            "agent.finish.wait.request",
            json!({"agentId":id}),
        )
        .await;
        assert_eq!(
            finished["result"]["lastMessage"], "authoritative answer",
            "{finished}"
        );
    }
    terminate(&mut process).await;
    let mut process = start_with_path(&state, &log, Some(&fixture.path));
    let address = ready(&mut process, &log).await;
    let mut socket = connect(&address, METHODS).await;
    let resumed = request(
        &mut socket,
        "agent.resume.request",
        json!({"handle":created["result"]["agent"]["persistence"]}),
    )
    .await;
    assert_eq!(resumed["type"], "response", "{resumed}");
    let sent = request(
        &mut socket,
        "agent.message.send.request",
        json!({"agentId":id,"text":"after restart"}),
    )
    .await;
    assert_eq!(sent["result"]["accepted"], true, "{sent}");
    let finished = request(
        &mut socket,
        "agent.finish.wait.request",
        json!({"agentId":id}),
    )
    .await;
    assert_eq!(
        finished["result"]["lastMessage"], "authoritative answer",
        "{finished}"
    );
    let timeline = request(
        &mut socket,
        "agent.timeline.get.request",
        json!({"agentId":id}),
    )
    .await;
    assert_eq!(timeline["type"], "response", "{timeline}");
    terminate(&mut process).await;
    let native: serde_json::Value = serde_json::from_slice(
        &std::fs::read(fixture.root.path().join("native-fixture.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(native["seq"], 3);
    assert_eq!(native["history"].as_array().unwrap().len(), 6);
    assert_helpers_reaped(fixture.root.path());
}

fn fixture() -> super::native::NativeFixture {
    let fixture = super::native::NativeFixture::new();
    let program = fixture.root.path().join("opencode");
    std::fs::write(
        &program,
        include_str!("../../../../crates/server-provider/tests/fixtures/opencode_http.py"),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    fixture
}

fn assert_helpers_reaped(root: &std::path::Path) {
    for pid in std::fs::read_to_string(root.join("pids.txt"))
        .unwrap()
        .lines()
    {
        assert!(
            !std::process::Command::new("/bin/kill")
                .args(["-0", pid])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success(),
            "OpenCode helper {pid} survived shutdown"
        );
    }
}
