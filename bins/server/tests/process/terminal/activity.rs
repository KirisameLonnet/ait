//! Paseo HTTP activity, heartbeat and Workspace rollup through a real terminal process.

use super::*;

#[tokio::test]
async fn terminal_activity_hook_token_updates_streams_workspace_and_visible_focus() {
    let root = tempfile::tempdir().unwrap();
    let cwd = root.path().join("workspace");
    std::fs::create_dir(&cwd).unwrap();
    let log = root.path().join("server.log");
    let mut server = start(&root.path().join("state"), &log);
    let address = ready(&mut server, &log).await;
    let mut client = Client::connect(&address).await;
    client
        .request("workspace.open.request", json!({"cwd":cwd}))
        .await;
    let terminal = create_hook_process(&mut client, &cwd).await;
    subscribe_attention(&mut client, &terminal).await;
    let subscribed = client
        .request("terminal.list.subscribe.request", json!({"cwd":cwd}))
        .await;
    assert!(
        subscribed["result"]["subscriptionId"].is_string(),
        "{subscribed}"
    );
    let activity_env: Value =
        serde_json::from_slice(&std::fs::read(cwd.join("hook.json")).unwrap()).unwrap();
    assert_eq!(activity_env["terminalId"], terminal);
    let http = reqwest::Client::new();
    let url = activity_env["url"].as_str().unwrap();
    let token = &activity_env["token"];
    assert_rejected_reports(&http, url, &terminal, token).await;
    for (state, workspace, reason) in [
        ("running", "running", Value::Null),
        ("idle", "attention", json!("finished")),
        ("idle", "attention", json!("finished")),
        ("needs-input", "needs_input", json!("needs_input")),
    ] {
        let response = http
            .post(url)
            .json(&json!({"terminalId":terminal,"token":token,"state":state}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 204);
        assert_workspace(&mut client, workspace).await;
        let listed = client.request("terminal.list.request", json!({})).await;
        assert_eq!(
            listed["result"]["terminals"][0]["activity"]["attentionReason"],
            reason
        );
        assert!(!listed.to_string().contains(token.as_str().unwrap()));
    }
    let attention: Vec<_> = client
        .events
        .iter()
        .filter(|event| event["method"] == "terminal_attention_required")
        .collect();
    assert_eq!(attention.len(), 2);
    assert_eq!(attention[0]["params"]["terminalId"], terminal);
    assert_eq!(attention[0]["params"]["title"], "Terminal finished");
    assert_eq!(attention[1]["params"]["title"], "Terminal needs input");
    assert_eq!(attention[0]["params"]["shouldNotify"], true);
    send_heartbeat(&mut client, &terminal, false, "2026-09-29T00:00:00Z").await;
    assert_workspace(&mut client, "needs_input").await;
    send_heartbeat(&mut client, &terminal, true, "invalid").await;
    assert_workspace(&mut client, "needs_input").await;
    send_heartbeat(&mut client, &terminal, true, "2026-09-29T00:00:00Z").await;
    assert_workspace(&mut client, "done").await;
    await_idle_list_event(&mut client).await;
    client
        .request("terminal.kill.request", json!({"terminalId":terminal}))
        .await;
    let response = http
        .post(url)
        .json(&json!({"terminalId":terminal,"token":token,"state":"running"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 403);
    client.socket.close(None).await.unwrap();
    terminate(&mut server).await;
    assert!(
        !std::fs::read_to_string(log)
            .unwrap()
            .contains(token.as_str().unwrap())
    );
}

async fn subscribe_attention(client: &mut Client, terminal: &Value) {
    let response = client
        .request(
            "session.events.set_subscription.request",
            json!({"events":["terminal_attention_required"],"notifications":true}),
        )
        .await;
    assert!(
        response["result"]["subscriptionId"].is_string(),
        "{response}"
    );
    send_heartbeat(client, terminal, false, &chrono::Utc::now().to_rfc3339()).await;
}

async fn create_hook_process(client: &mut Client, cwd: &std::path::Path) -> Value {
    let script = "import os,json,time; json.dump({'terminalId':os.environ.get('PASEO_TERMINAL_ID'),'token':os.environ.get('PASEO_ACTIVITY_TOKEN'),'url':os.environ.get('PASEO_TERMINAL_ACTIVITY_URL')},open('hook.json','w')); print('HOOK_READY',flush=True); time.sleep(300)";
    let response = client
        .request(
            "terminal.create.request",
            json!({"cwd":cwd,"command":"/usr/bin/python3","args":["-c",script]}),
        )
        .await;
    assert!(response["result"]["error"].is_null(), "{response}");
    let terminal = response["result"]["terminal"]["id"].clone();
    client.capture(&terminal, "HOOK_READY").await;
    terminal
}

async fn assert_rejected_reports(
    http: &reqwest::Client,
    url: &str,
    terminal: &Value,
    token: &Value,
) {
    let mut rejected = Vec::new();
    for id in [terminal.clone(), json!("unknown")] {
        let response = http
            .post(url)
            .json(&json!({"terminalId":id,"token":"wrong","state":"running"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 403);
        rejected.push(response.json::<Value>().await.unwrap());
    }
    assert_eq!(rejected[0], rejected[1]);
    let response = http
        .post(url)
        .header("Origin", "https://untrusted.example")
        .json(&json!({"terminalId":terminal,"token":token,"state":"running"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 403);
    let response = http
        .post(url)
        .json(&json!({"terminalId":terminal,"token":token,"state":"working"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 400);
}

async fn assert_workspace(client: &mut Client, expected: &str) {
    let listed = client.request("workspace.list.request", json!({})).await;
    assert_eq!(
        listed["result"]["entries"][0]["status"], expected,
        "{listed}"
    );
}

async fn send_heartbeat(client: &mut Client, terminal: &Value, visible: bool, timestamp: &str) {
    client.send(json!({"type":"event","method":"session.heartbeat","params":{"deviceType":"web","focusedAgentId":null,"focusedTerminalId":terminal,"lastActivityAt":timestamp,"appVisible":visible}})).await;
    client
        .request("connection.ping", json!({"nonce":"heartbeat-fence"}))
        .await;
}

async fn await_idle_list_event(client: &mut Client) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        client
            .request("connection.ping", json!({"nonce":"activity-event-fence"}))
            .await;
        if client.events.iter().any(|event| {
            event["method"] == "terminal.list.changed"
                && event["params"]["terminals"][0]["activity"]["state"] == "idle"
                && event["params"]["terminals"][0]["activity"]["attentionReason"].is_null()
        }) {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "missing terminal activity event"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
