use super::*;
use ait_domain::{AgentId, NativeSessionSource, SessionReference, SessionStatus, SubMessage};
use ait_ports::NativeHistoryMessage;
use serde_json::json;

fn state() -> ConversationContext {
    let root = MessageId::from_u128(1);
    let mut state = ConversationContext::default();
    state.projects.push(serde_json::from_value(json!({"id":"p","name":"Project","workdir":"/tmp","root_message_id":root,"base_commit":"a".repeat(40),"default_agent_id":null,"revision":1})).unwrap());
    state
        .sessions
        .push(crate::control::conversation::SessionRecord {
            reference: SessionReference::new(root, AgentId::new("agent")),
            id: "session".into(),
            project_id: "p".into(),
            workdir: "/tmp/.ait/session".into(),
            source: SessionSource::NativeSession(Box::new(NativeSessionSource {
                driver: "opencode".into(),
                provider_id: "provider".into(),
                native_session_id: "ses_one".into(),
                sync_state: ProviderSyncState::Synced,
            })),
            name: String::new(),
            title: None,
            description: String::new(),
            title_generation_started: false,
            status: SessionStatus::Active,
        });
    state
}

fn history() -> NativeSessionSnapshot {
    NativeSessionSnapshot {
        driver: "opencode".into(),
        id: "ses_one".into(),
        input_id: "input".into(),
        cwd: "/tmp/.ait/session".into(),
        model: "local/model".into(),
        reasoning_effort: None,
        outcome: Some(NativeSessionOutcome::Completed),
        messages: vec![
            NativeHistoryMessage {
                id: "u1".into(),
                role: MessageRole::User,
                sub_messages: vec![SubMessage::Text {
                    text: "hello".into(),
                }],
                tool_result: None,
                input_id: Some("input".into()),
                created_at: 1,
                metadata: json!({}),
            },
            NativeHistoryMessage {
                id: "a1".into(),
                role: MessageRole::Assistant,
                sub_messages: vec![SubMessage::Text {
                    text: "answer".into(),
                }],
                tool_result: None,
                input_id: None,
                created_at: 2,
                metadata: json!({}),
            },
        ],
    }
}

#[test]
fn complete_history_is_idempotent_and_corrections_create_an_immutable_suffix() {
    let mut state = state();
    let mut snapshot = history();
    publish(&snapshot, &mut state, 0).unwrap();
    let before = state.messages.clone();
    let first = state.sessions[0].current_message_id();
    publish(&snapshot, &mut state, 0).unwrap();
    assert_eq!(state.messages, before);
    snapshot.messages[1].sub_messages = vec![SubMessage::Text {
        text: "corrected".into(),
    }];
    publish(&snapshot, &mut state, 0).unwrap();
    assert_eq!(state.messages.len(), 3);
    assert_eq!(&state.messages[..2], before.as_slice());
    assert_ne!(state.sessions[0].current_message_id(), first);
}

#[test]
fn incorrect_binding_and_ambiguous_input_leave_state_untouched() {
    let mut state = state();
    let before = state.clone();
    let mut snapshot = history();
    snapshot.id = "other".into();
    assert!(publish(&snapshot, &mut state, 0).is_err());
    assert_eq!(state, before);
    snapshot.id = "ses_one".into();
    snapshot.messages.push(snapshot.messages[0].clone());
    assert!(publish(&snapshot, &mut state, 0).is_err());
    assert_eq!(state, before);
}
