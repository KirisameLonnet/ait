use std::path::Path;

use serde_json::{Value, json};
use server_domain::agent_runtime::{StoredAgentConfig, StoredAgentRuntimeInfo};

use super::{PROVIDER, transport::Transport};
use crate::ports::agent_session::{AgentSessionError, AgentSessionSpec};
use crate::protocol::provider::Details;

pub(super) fn validate(config: &StoredAgentConfig) -> Result<(), AgentSessionError> {
    if config.mode_id.is_some()
        || config.system_prompt.is_some()
        || config
            .tool_policy
            .as_ref()
            .is_some_and(|value| !value.is_null())
        || config
            .provider_options
            .as_ref()
            .is_some_and(|options| !options.is_empty())
        || config
            .feature_values
            .as_ref()
            .is_some_and(|features| !features.is_empty())
        || [&config.model, &config.thinking_option_id]
            .into_iter()
            .flatten()
            .any(|value| value.len() > 1024 || value.chars().any(char::is_control))
        || config.model.as_ref().is_some_and(String::is_empty)
    {
        return Err(AgentSessionError::Rejected);
    }
    crate::local::configuration::validate(config, PROVIDER)?;
    for (_, server) in config.mcp_servers.iter().flatten() {
        if server["type"] == "sse"
            || (server["type"] == "stdio"
                && !server["command"]
                    .as_str()
                    .is_some_and(|command| Path::new(command).is_absolute()))
        {
            return Err(AgentSessionError::Rejected);
        }
    }
    Ok(())
}

pub(super) fn validate_spec(spec: &AgentSessionSpec) -> Result<(), AgentSessionError> {
    if spec.provider != PROVIDER
        || !Path::new(&spec.cwd).is_absolute()
        || !Path::new(&spec.cwd).is_dir()
    {
        return Err(AgentSessionError::Rejected);
    }
    validate(&spec.config)
}

pub(super) fn mcp_servers(config: &StoredAgentConfig) -> Vec<Value> {
    config
        .mcp_servers
        .iter()
        .flatten()
        .map(|(name, server)| {
            let pairs = |field: &str| -> Vec<Value> {
                server[field]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(name, value)| json!({"name":name,"value":value}))
                    .collect()
            };
            if server["type"] == "stdio" {
                json!({"name":name,"command":server["command"],
                "args":server.get("args").cloned().unwrap_or_else(|| json!([])),"env":pairs("env")})
            } else {
                json!({"type":"http","name":name,"url":server["url"],"headers":pairs("headers")})
            }
        })
        .collect()
}

pub(super) fn option<'a>(options: &'a Value, category: &str) -> Option<&'a Value> {
    options
        .as_array()?
        .iter()
        .find(|option| option["category"] == category && option["type"] == "select")
}

fn choices(option: &Value) -> Result<Vec<&Value>, AgentSessionError> {
    let options = option["options"]
        .as_array()
        .ok_or(AgentSessionError::Failed)?;
    let mut choices = Vec::new();
    for entry in options {
        if let Some(group) = entry["options"].as_array() {
            choices.extend(group);
        } else {
            choices.push(entry);
        }
        if choices.len() > 4096 {
            return Err(AgentSessionError::Failed);
        }
    }
    for choice in &choices {
        choice["value"]
            .as_str()
            .filter(|value| value.len() <= 1024)
            .ok_or(AgentSessionError::Failed)?;
        text(choice, "name")?;
    }
    Ok(choices)
}

pub(super) fn details(options: &Value) -> Result<Details, AgentSessionError> {
    let Some(models) = option(options, "model") else {
        return Ok(Details::default());
    };
    let thinking = option(options, "thought_level");
    let thinking_options = thinking
        .map(|option| {
            choices(option).map(|choices| {
                choices
                    .into_iter()
                    .map(|choice| {
                        json!({"id":choice["value"],"label":choice["name"],
            "isDefault":choice["value"] == option["currentValue"]})
                    })
                    .collect::<Vec<_>>()
            })
        })
        .transpose()?
        .unwrap_or_default();
    Ok(Details {
        models: choices(models)?
            .into_iter()
            .map(|choice| {
                json!({"provider":PROVIDER,
            "id":choice["value"],"label":choice["name"],"description":choice["description"],
            "isDefault":choice["value"] == models["currentValue"],"isSelectable":true,
            "thinkingOptions":thinking_options,
            "defaultThinkingOptionId":thinking.map(|option| &option["currentValue"])})
            })
            .collect(),
        ..Details::default()
    })
}

pub(super) fn runtime(id: &str, options: &Value) -> StoredAgentRuntimeInfo {
    let selection = |category| {
        option(options, category)
            .and_then(|option| option["currentValue"].as_str())
            .map(str::to_owned)
    };
    StoredAgentRuntimeInfo {
        provider: PROVIDER.to_owned(),
        session_id: Some(id.to_owned()),
        model: selection("model"),
        thinking_option_id: selection("thought_level"),
        mode_id: None,
        extra: None,
    }
}

pub(super) fn state(response: &Value) -> Result<Value, AgentSessionError> {
    let options = response["configOptions"]
        .as_array()
        .filter(|options| options.len() <= 128)
        .ok_or(AgentSessionError::Failed)?;
    for option in options {
        text(option, "id")?;
        if option["type"] == "select" {
            choices(option)?;
            option["currentValue"]
                .as_str()
                .ok_or(AgentSessionError::Failed)?;
        }
    }
    Ok(Value::Array(options.clone()))
}

pub(super) async fn apply(
    transport: &mut Transport,
    id: &str,
    options: &mut Value,
    config: &StoredAgentConfig,
) -> Result<(), AgentSessionError> {
    validate(config)?;
    for (category, selected) in [
        ("model", &config.model),
        ("thought_level", &config.thinking_option_id),
    ] {
        let Some(value) = selected else { continue };
        let option = option(options, category).ok_or(AgentSessionError::Rejected)?;
        if !choices(option)?
            .iter()
            .any(|choice| choice["value"] == *value)
        {
            return Err(AgentSessionError::Rejected);
        }
        if option["currentValue"] == *value {
            continue;
        }
        let result = transport
            .request(
                "session/set_config_option",
                json!({
            "sessionId":id,"configId":text(option, "id")?,"value":value}),
            )
            .await?;
        *options = state(&result)?;
    }
    Ok(())
}

pub(super) fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, AgentSessionError> {
    value[key]
        .as_str()
        .filter(|text| {
            !text.is_empty() && text.len() <= 1024 && !text.chars().any(char::is_control)
        })
        .ok_or(AgentSessionError::Failed)
}

#[cfg(test)]
mod tests;
