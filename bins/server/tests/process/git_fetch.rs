use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde_json::{Value, json};

use super::transport::{Socket, connect, receive, request};
use super::{ready, start, terminate};

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

async fn rpc(socket: &mut Socket, method: &str, params: Value) -> Value {
    let mut reply = request(socket, method, params).await;
    while reply["type"] == "event" {
        reply = receive(socket).await;
    }
    assert_eq!(reply["type"], "response", "{reply}");
    reply["result"].clone()
}

async fn checkout_update(socket: &mut Socket) -> Value {
    loop {
        let event = receive(socket).await;
        if event["method"] == "checkout.status.update" {
            return event["params"].clone();
        }
    }
}

#[tokio::test]
async fn active_directory_fetches_remote_main_pushes_status_and_stops_when_released() {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init", "--bare", "remote"]);
    git(root.path(), &["init", "-b", "main", "source"]);
    let source = root.path().join("source");
    git(&source, &["config", "user.name", "Test"]);
    git(&source, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(source.join("tracked"), "initial\n").unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "initial"]);
    git(&source, &["remote", "add", "origin", "../remote"]);
    git(&source, &["push", "origin", "main"]);
    git(
        root.path(),
        &["clone", "--branch", "main", "remote", "checkout"],
    );
    let checkout = root.path().join("checkout");
    git(&checkout, &["checkout", "-b", "feature"]);
    let original = git(&checkout, &["rev-parse", "HEAD"]);
    let state = root.path().join("server");
    let log = root.path().join("server.log");
    let mut process = start(&state, &log);
    let address = ready(&mut process, &log).await;
    let mut client = connect(
        &address,
        &[
            "workspace.open.request",
            "workspace.list.request",
            "subscription.release.request",
            "session.events.set_subscription.request",
            "checkout.merge_from_base.request",
        ],
    )
    .await;
    rpc(
        &mut client,
        "workspace.open.request",
        json!({"cwd":checkout}),
    )
    .await;
    let events = rpc(
        &mut client,
        "session.events.set_subscription.request",
        json!({"events":["checkout_status_update"]}),
    )
    .await;
    std::fs::write(source.join("tracked"), "remote change\n").unwrap();
    git(&source, &["commit", "-am", "remote change"]);
    git(&source, &["push", "origin", "main"]);
    let latest = git(&source, &["rev-parse", "HEAD"]);
    assert_eq!(git(&checkout, &["rev-parse", "origin/main"]), original);
    let directory = rpc(
        &mut client,
        "workspace.list.request",
        json!({"subscribe":{}}),
    )
    .await;
    let update = checkout_update(&mut client).await;
    assert_eq!(update["subscriptionId"], events["subscriptionId"]);
    assert_eq!(update["cwd"], checkout.to_str().unwrap());
    assert_eq!(update["aheadBehind"]["behind"], 1);
    assert_eq!(git(&checkout, &["rev-parse", "origin/main"]), latest);
    assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), original);
    assert_eq!(
        std::fs::read_to_string(checkout.join("tracked")).unwrap(),
        "initial\n"
    );
    rpc(
        &mut client,
        "subscription.release.request",
        json!({"subscriptionId":directory["subscriptionId"]}),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    std::fs::write(source.join("tracked"), "next change\n").unwrap();
    git(&source, &["commit", "-am", "next change"]);
    git(&source, &["push", "origin", "main"]);
    let next = git(&source, &["rev-parse", "HEAD"]);
    assert_eq!(git(&checkout, &["rev-parse", "origin/main"]), latest);
    rpc(
        &mut client,
        "workspace.list.request",
        json!({"subscribe":{}}),
    )
    .await;
    let update = checkout_update(&mut client).await;
    assert_eq!(update["aheadBehind"]["behind"], 2);
    assert_eq!(git(&checkout, &["rev-parse", "origin/main"]), next);
    let merged = rpc(
        &mut client,
        "checkout.merge_from_base.request",
        json!({"cwd":checkout,"baseRef":"main","requireCleanTarget":true}),
    )
    .await;
    assert_eq!(merged["success"], true);
    assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), next);
    assert_eq!(git(&checkout, &["rev-parse", "main"]), original);
    terminate(&mut process).await;
}
