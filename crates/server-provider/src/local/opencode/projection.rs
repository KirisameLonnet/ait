//! Project verified native history into the server's immutable display timeline.
use std::collections::BTreeMap;

use super::types::{Content, Role, Snapshot, ToolStatus};
use crate::{ports::agent_session::AgentSessionError, protocol::timeline::NativeItem};
use serde_json::{Value, json};

pub(super) fn entries(
    snapshot: &Snapshot,
    clients: &BTreeMap<String, String>,
) -> Result<Vec<NativeItem>, AgentSessionError> {
    let results = snapshot
        .messages
        .iter()
        .filter_map(|record| {
            record
                .tool_result
                .as_ref()
                .map(|result| (result.call_id.as_str(), result))
        })
        .collect::<BTreeMap<_, _>>();
    let mut entries = Vec::new();
    let mut turn = None;
    for record in &snapshot.messages {
        if record.tool_result.is_some() || record.role == Role::System {
            continue;
        }
        if record.role == Role::User {
            turn = record.input_id.clone().or_else(|| Some(record.id.clone()));
        }
        let timestamp = chrono::DateTime::from_timestamp_millis(record.created_at)
            .ok_or(AgentSessionError::Failed)?
            .to_rfc3339();
        if record.role == Role::User {
            let texts = record
                .sub_messages
                .iter()
                .filter_map(|part| match part {
                    Content::Text { text } => Some(text.as_str()),
                    Content::ToolCall(_)
                    | Content::NativeContent(_)
                    | Content::StructuredData { .. } => None,
                })
                .collect::<Vec<_>>();
            if !texts.is_empty() {
                let client = record
                    .input_id
                    .as_ref()
                    .and_then(|id| clients.get(id))
                    .map_or(record.id.as_str(), String::as_str);
                entries.push(NativeItem { key: format!("native:opencode:{}", record.id), turn_id: turn.clone(), timestamp: timestamp.clone(), item: json!({"type":"user_message","messageId":record.id,"clientMessageId":client,"text":texts.join("\n")}) });
            }
            continue;
        }
        for (index, part) in record.sub_messages.iter().enumerate() {
            let key = if record.role == Role::User {
                record.id.clone()
            } else {
                record.metadata["part_ids"][index]
                    .as_str()
                    .map_or_else(|| format!("{}:{index}", record.id), str::to_owned)
            };
            let (key, mut item) = match part {
                Content::Text { text } => (
                    key.clone(),
                    json!({"type":if record.role == Role::User {"user_message"} else {"assistant_message"},"messageId":key,"text":text}),
                ),
                Content::ToolCall(call) => {
                    let result = results
                        .get(call.call_id.as_str())
                        .ok_or(AgentSessionError::Failed)?;
                    let input: Value = serde_json::from_str(&call.arguments)
                        .map_err(|_| AgentSessionError::Failed)?;
                    let mut detail = detail(&call.tool_name, &input);
                    detail["output"] = json!(result.output);
                    (
                        format!("tool:{}", call.call_id),
                        json!({"type":"tool_call","callId":call.call_id,"name":call.tool_name,
                        "status":if result.status==ToolStatus::Succeeded {"completed"} else {"failed"},"error":result.error,"detail":detail}),
                    )
                }
                Content::NativeContent(content) if content.item_type == "reasoning" => (
                    key,
                    json!({"type":"reasoning","text":content.payload["text"].as_str().unwrap_or_default()}),
                ),
                Content::NativeContent(_) | Content::StructuredData { .. } => continue,
            };
            if record.role == Role::User
                && let Some(client) = record.input_id.as_ref().and_then(|id| clients.get(id))
            {
                item["clientMessageId"] = json!(client);
            }
            entries.push(NativeItem {
                key: format!("native:opencode:{key}"),
                turn_id: turn.clone(),
                timestamp: timestamp.clone(),
                item,
            });
        }
    }
    Ok(entries)
}

fn detail(name: &str, input: &Value) -> Value {
    match name {
        "bash" | "shell" => json!({"type":"shell","command":input["command"],"cwd":input["cwd"]}),
        "read" => json!({"type":"read","filePath":input["filePath"]}),
        "edit" | "write" => json!({"type":name,"filePath":input["filePath"]}),
        _ => json!({"type":"unknown","input":input,"output":null}),
    }
}
