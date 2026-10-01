use super::{AgentTurnEvent, Fixture, OpenCodeClient, Version, drain, spec};
use crate::{
    ports::agent_session::AgentClient,
    storage::timeline::{Row, Timeline},
};
use serde_json::{Value, json};
use std::time::Duration;

fn message(role: &str, id: &str) -> Value {
    json!({"type":"message.updated","properties":{"info":{"id":id,"sessionID":"ses_one","role":role}}})
}

fn part(id: &str, message: &str, text: &str) -> Value {
    json!({"type":"message.part.updated","properties":{"part":{"id":id,"sessionID":"ses_one","messageID":message,"type":"text","text":text}}})
}

fn delta(text: &str) -> Value {
    json!({"type":"message.part.delta","properties":{"sessionID":"ses_one","messageID":"msg_0123456789abABCDEFGHIJKLM1","partID":"prt_0123456789abABCDEFGHIJKLM1","field":"text","delta":text}})
}

async fn run(events: Vec<Value>) -> Vec<Row> {
    let fixture = Fixture::start(Version::V1).await;
    let client = OpenCodeClient::new(fixture.binary.clone());
    let spec = spec(&fixture);
    let mut session = client.create_session(&spec).await.unwrap();
    {
        let mut state = fixture.state.lock().unwrap();
        state.busy = true;
        state.stream_events = events;
    }
    session.start_turn("hello", &spec.config).await.unwrap();
    let timeline = Timeline::memory().unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut assistant = String::new();
        loop {
            match session.poll_turn().unwrap() {
                Some(AgentTurnEvent::Progress { observation, entry }) => {
                    if entry.item["messageId"] == "prt_0123456789abABCDEFGHIJKLM1" {
                        assistant.push_str(entry.item["text"].as_str().unwrap());
                    }
                    timeline
                        .progress("agent", "opencode", &observation, &entry)
                        .unwrap();
                    if assistant == "answer" {
                        break;
                    }
                }
                Some(AgentTurnEvent::Failed) => {
                    panic!("legal native text events failed the execution")
                }
                Some(_) | None => tokio::time::sleep(Duration::from_millis(5)).await,
            }
        }
    })
    .await
    .unwrap();
    fixture.state.lock().unwrap().busy = false;
    let finished = drain(session.as_mut()).await;
    assert!(matches!(
        finished.last(),
        Some(AgentTurnEvent::Completed(_))
    ));
    for event in finished {
        if let AgentTurnEvent::Timeline(entry) = event {
            timeline.append("agent", "opencode", &[entry]).unwrap();
        }
    }
    assert_eq!(fixture.state.lock().unwrap().submissions, 1);
    session.close().await.unwrap();
    timeline.read("agent").unwrap().1
}

#[tokio::test]
async fn empty_placeholder_and_real_deltas_complete_without_repeating_final_text() {
    let id = "prt_0123456789abABCDEFGHIJKLM1";
    let msg = "msg_0123456789abABCDEFGHIJKLM1";
    let rows = run(vec![
        message("assistant", msg),
        part(id, msg, ""),
        delta("ans"),
        delta(""),
        delta("wer"),
        part(id, msg, "answer"),
    ])
    .await;
    let texts = rows
        .iter()
        .filter(|row| row.entry.item["type"] == "assistant_message")
        .map(|row| row.entry.item["text"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(texts, ["ans", "wer", ""]);
}

#[tokio::test]
async fn user_parts_are_never_persisted_as_assistant_messages() {
    let id = "prt_0123456789abABCDEFGHIJKLM1";
    let msg = "msg_0123456789abABCDEFGHIJKLM1";
    let rows = run(vec![
        message("user", "msg_user"),
        part("prt_user", "msg_user", "hello"),
        message("assistant", msg),
        part(id, msg, "answer"),
    ])
    .await;
    assert_eq!(
        rows.iter()
            .filter(|row| row.entry.item["type"] == "user_message")
            .count(),
        1
    );
    let text = rows
        .iter()
        .filter(|row| row.entry.item["type"] == "assistant_message")
        .map(|row| row.entry.item["text"].as_str().unwrap())
        .collect::<String>();
    assert_eq!(text, "answer");
    assert!(!rows.iter().any(|row| row.entry.key.ends_with("prt_user")));
}
