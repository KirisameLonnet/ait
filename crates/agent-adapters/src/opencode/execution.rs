//! Durable execution evidence is separate from prompt admission and message completion.
use ait_domain::{DomainError, ErrorCode};
use ait_ports::NativeSessionOutcome;
use serde_json::Value;

use super::{failure, http::Version};

pub(super) fn outcome(
    version: Version,
    info: &Value,
    execution: Option<&Value>,
    history: &[Value],
) -> Result<Option<NativeSessionOutcome>, DomainError> {
    if version == Version::V1 {
        // An earlier assistant cannot settle a newer queued input.
        let Some(last) = history.last().and_then(|message| message.get("info")) else {
            return Ok(None);
        };
        return Ok(
            (last.get("role").and_then(Value::as_str) == Some("assistant")).then(|| {
                if last.get("error").is_some_and(|error| !error.is_null()) {
                    NativeSessionOutcome::Failed
                } else {
                    NativeSessionOutcome::Completed
                }
            }),
        );
    }
    let Some(event) = execution else {
        return Ok(None);
    };
    let created = event
        .get("created")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            failure(
                ErrorCode::ProviderFailed,
                "OpenCode execution timestamp missing",
            )
        })?;
    if history
        .iter()
        .rev()
        .find(|message| message.get("type").and_then(Value::as_str) == Some("user"))
        .and_then(|message| message.pointer("/time/created"))
        .and_then(Value::as_i64)
        .is_some_and(|input| created < input)
    {
        return Ok(None);
    }
    let (outcome, expected) = match event.get("type").and_then(Value::as_str) {
        Some("session.execution.succeeded") => (NativeSessionOutcome::Completed, "succeeded"),
        Some("session.execution.failed") => (NativeSessionOutcome::Failed, "failed"),
        Some("session.execution.interrupted") => {
            if event.pointer("/data/reason").and_then(Value::as_str) == Some("shutdown") {
                return Ok(None);
            }
            (NativeSessionOutcome::Interrupted, "interrupted")
        }
        Some("session.execution.started") => return Ok(None),
        Some(_) | None => {
            return Err(failure(
                ErrorCode::ProviderFailed,
                "invalid OpenCode execution event",
            ));
        }
    };
    if info.get("outcome").and_then(Value::as_str) != Some(expected) {
        return Err(failure(
            ErrorCode::RunRecoveryFailed,
            "OpenCode execution outcome has not reconciled",
        ));
    }
    Ok(Some(outcome))
}

#[cfg(test)]
mod tests;
