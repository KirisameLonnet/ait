use super::*;

fn progress(stream: &mut Stream, method: &str, params: &Value) -> Vec<NativeItem> {
    let first = stream.progress(method, params).unwrap();
    first
        .into_iter()
        .chain(stream.events.drain(..))
        .map(|event| item(Some(event)))
        .collect()
}

#[test]
fn live_actions_keep_the_same_keys_and_details_as_completed_history() {
    let mut stream = Stream::default();
    let mut native = json!({"type":"commandExecution","id":"tool","status":"inProgress",
    "command":"/bin/zsh -lc 'cat a.rs; rg TODO src; echo done'","cwd":"/project",
    "commandActions":[
        {"type":"read","command":"cat a.rs","path":"/project/a.rs"},
        {"type":"search","command":"rg TODO src","query":"TODO","path":"src"},
        {"type":"unknown","command":"echo done"}
    ]});
    let started = json!({"turnId":"turn","item":native});
    let running = progress(&mut stream, "item/started", &started);
    assert_eq!(running.len(), 3);
    assert!(
        running
            .iter()
            .all(|entry| entry.item["status"] == "running")
    );
    assert!(progress(&mut stream, "item/started", &started).is_empty());
    for delta in ["hello", " world"] {
        let updates = progress(
            &mut stream,
            "item/commandExecution/outputDelta",
            &params("tool", delta),
        );
        assert_eq!(updates.len(), 3);
        assert_eq!(
            updates.iter().map(|entry| &entry.key).collect::<Vec<_>>(),
            running.iter().map(|entry| &entry.key).collect::<Vec<_>>()
        );
    }
    let updates = progress(
        &mut stream,
        "item/commandExecution/outputDelta",
        &params("tool", "!"),
    );
    native["status"] = json!("completed");
    native["aggregatedOutput"] = json!("hello world!");
    let completed = discovery::timeline_items(
        &native,
        "turn",
        "time",
        &crate::local::images::ImageStore::default(),
    )
    .unwrap();
    for (live, finished) in updates.iter().zip(&completed) {
        assert_eq!(live.key, finished.key);
        assert_eq!(live.item["detail"], finished.item["detail"]);
        assert_eq!(finished.item["status"], "completed");
    }
    assert!(stream.complete(&native).unwrap());
    assert!(!stream.complete(&native).unwrap());
    assert!(
        progress(
            &mut stream,
            "item/commandExecution/outputDelta",
            &params("tool", "late")
        )
        .is_empty()
    );
}

#[test]
fn every_action_receives_the_same_bounded_unicode_output_tail() {
    let mut stream = Stream::default();
    progress(
        &mut stream,
        "item/started",
        &json!({"turnId":"turn","item":{
        "type":"commandExecution","id":"tool","command":"wrapped",
        "commandActions":[
            {"type":"listFiles","command":"ls","path":null},
            {"type":"unknown","command":"echo done"}
        ]}}),
    );
    let output = "界".repeat(MAX_OUTPUT);
    let updates = progress(
        &mut stream,
        "item/commandExecution/outputDelta",
        &params("tool", &output),
    );
    assert_eq!(updates.len(), 2);
    let content = updates[0].item["detail"]["content"].as_str().unwrap();
    assert!(content.len() <= MAX_OUTPUT);
    assert!(output.ends_with(content));
    assert_eq!(updates[1].item["detail"]["output"], content);
}

#[test]
fn action_rows_share_the_existing_stream_capacity_and_release_it_on_completion() {
    let mut stream = Stream::default();
    for index in 0..MAX_ITEMS - 1 {
        progress(
            &mut stream,
            "item/started",
            &json!({"turnId":"turn","item":{
                "type":"commandExecution","id":index.to_string(),"command":"echo"
            }}),
        );
    }
    let multiple = json!({"turnId":"turn","item":{
    "type":"commandExecution","id":"multiple","command":"wrapped",
    "commandActions":[
        {"type":"unknown","command":"first"},
        {"type":"unknown","command":"second"}
    ]}});
    assert_eq!(
        stream.progress("item/started", &multiple),
        Err(AgentSessionError::Failed)
    );
    assert!(stream.events.is_empty());
    assert!(stream.complete(&json!({"id":"0"})).unwrap());
    assert_eq!(progress(&mut stream, "item/started", &multiple).len(), 2);
    assert!(stream.complete(&multiple["item"]).unwrap());
    assert!(!stream.complete(&multiple["item"]).unwrap());
    assert_eq!(stream.tool_rows, MAX_ITEMS - 2);
}
