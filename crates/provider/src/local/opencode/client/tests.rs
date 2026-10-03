use super::{AgentPersistenceHandle, AgentSessionError, native_handle};
use serde_json::{Value, json};

#[test]
fn native_handles_accept_encoded_and_legacy_objects_and_reject_invalid_data() {
    let saved = json!({"config":{},"model":"local/model","clients":{}});
    let mut handle = AgentPersistenceHandle {
        provider: "opencode".into(),
        session_id: "ses_one".into(),
        native_handle: Some(saved.clone()),
        metadata: None,
    };
    assert_eq!(native_handle(&handle).unwrap().as_ref(), &saved);
    handle.native_handle = Some(json!(saved.to_string()));
    assert_eq!(native_handle(&handle).unwrap().as_ref(), &saved);
    for invalid in [
        None,
        Some(Value::Null),
        Some(json!(42)),
        Some(json!("[]")),
        Some(json!("invalid")),
        Some(json!(" ".repeat(256 * 1024 + 1))),
    ] {
        handle.native_handle = invalid;
        assert_eq!(
            native_handle(&handle).unwrap_err(),
            AgentSessionError::Rejected
        );
    }
}
