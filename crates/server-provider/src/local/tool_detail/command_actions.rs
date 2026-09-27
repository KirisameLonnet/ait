use serde_json::{Value, json};

use super::preview;

// Each action receives the shared output preview. Bound that amplification without
// dropping the command: excessive or malformed action lists use the raw shell row.
const MAX_ACTIONS: usize = 256;

/// Return display details for all valid actions, or `None` to retain the raw command.
///
/// `native` is a Codex thread item. Non-command items, declined commands, and absent,
/// malformed, or excessive action lists return `None` without partially projecting.
pub(super) fn details(native: &Value) -> Option<Vec<Value>> {
    if native["type"] != "commandExecution" || native["status"] == "declined" {
        return None;
    }
    let actions = native["commandActions"].as_array()?;
    if actions.is_empty() || actions.len() > MAX_ACTIONS {
        return None;
    }
    actions
        .iter()
        .map(|action| detail(native, action))
        .collect()
}

fn detail(native: &Value, action: &Value) -> Option<Value> {
    let command = action["command"].as_str()?.trim();
    let mut detail = match action["type"].as_str()? {
        "read" => json!({"type":"read","filePath":action["path"].as_str()?}),
        "listFiles" => json!({"type":"search","toolName":"glob",
            "query":action["path"].as_str().unwrap_or(command)}),
        "search" => {
            let mut detail = json!({"type":"search","toolName":"grep",
                "query":action["query"].as_str().unwrap_or(command)});
            if let Some(path) = action["path"].as_str() {
                detail["filePaths"] = json!([path]);
            }
            detail
        }
        _ => return Some(shell(native, command)),
    };
    detail["content"] = json!(preview(native["aggregatedOutput"].as_str().unwrap_or("")));
    Some(detail)
}

/// Keep the selected command together with its native working directory and output.
///
/// `native` supplies optional cwd, output, and exit status; `command` is either the
/// authoritative parsed action or the raw fallback. Returns a shell detail object.
pub(super) fn shell(native: &Value, command: &str) -> Value {
    let mut detail = json!({"type":"shell","command":command,
        "output":preview(native["aggregatedOutput"].as_str().unwrap_or(""))});
    if let Some(cwd) = native["cwd"].as_str() {
        detail["cwd"] = json!(cwd);
    }
    if let Some(exit) = native["exitCode"].as_i64() {
        detail["exitCode"] = json!(exit);
    }
    detail
}
