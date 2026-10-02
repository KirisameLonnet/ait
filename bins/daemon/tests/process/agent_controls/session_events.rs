use super::*;

#[tokio::test]
async fn approvals_and_native_children_publish_to_session_subscribers_without_timeline_observers() {
    let fixture = super::super::native::NativeFixture::new();
    let state = fixture.root.path().join("state");
    let log = fixture.root.path().join("server.log");
    let mut process = start_with_path(&state, &log, Some(&fixture.path));
    let address = ready(&mut process, &log).await;
    let mut methods = METHODS.to_vec();
    methods.extend([
        "session.events.set_subscription.request",
        "subscription.release.request",
    ]);
    let mut client = connect(&address, &methods).await;
    let mut observer = connect(&address, &methods).await;
    let subscription = success(
        &mut observer,
        "session.events.set_subscription.request",
        json!({"events":["agent_permission_request","agent_permission_resolved",
            "agent.provider_subagents.update"]}),
    )
    .await;
    let created = success(
        &mut client,
        "agent.create.request",
        json!({"config":{"provider":"codex","cwd":fixture.cwd}}),
    )
    .await;
    let id = &created["agentId"];
    success(
        &mut client,
        "agent.message.send.request",
        json!({"agentId":id,"text":"permit-command"}),
    )
    .await;
    let permission = next_event(&mut observer, "agent_permission_request").await;
    assert_eq!(permission["agentId"], *id);
    assert_eq!(permission["subscriptionId"], subscription["subscriptionId"]);
    success(
        &mut client,
        "agent.permission.resolve.request",
        json!({"agentId":id,"requestId":permission["request"]["id"],
            "response":{"behavior":"allow"}}),
    )
    .await;
    let resolved = next_event(&mut observer, "agent_permission_resolved").await;
    assert_eq!(resolved["requestId"], permission["request"]["id"]);
    assert_eq!(resolved["resolution"]["behavior"], "allow");
    assert_eq!(resolved["subscriptionId"], subscription["subscriptionId"]);
    success(
        &mut client,
        "agent.finish.wait.request",
        json!({"agentId":id}),
    )
    .await;
    success(
        &mut client,
        "agent.message.send.request",
        json!({"agentId":id,"text":"live-subagent"}),
    )
    .await;
    let child = next_event(&mut observer, "agent.provider_subagents.update").await;
    assert_eq!(child["kind"], "upsert");
    assert_eq!(child["subagent"]["parentAgentId"], *id);
    assert_eq!(child["subscriptionId"], subscription["subscriptionId"]);
    success(
        &mut client,
        "agent.finish.wait.request",
        json!({"agentId":id}),
    )
    .await;
    let mut released = request(&mut observer, "subscription.release.request", subscription).await;
    while released["type"] == "event" {
        released = receive(&mut observer).await;
    }
    assert_eq!(released["type"], "response", "{released}");
    observer.close(None).await.unwrap();
    client.close(None).await.unwrap();
    terminate(&mut process).await;
}

async fn next_event(client: &mut Socket, method: &str) -> Value {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let event = receive(client).await;
            if event["method"] == method {
                return event["params"].clone();
            }
        }
    })
    .await
    .unwrap()
}
