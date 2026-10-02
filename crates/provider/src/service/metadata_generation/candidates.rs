use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use metadata::ports::generation::{MetadataRequest, MetadataSelection};
use serde_json::Value;

use crate::ports::agent_session::AgentClient;

pub(super) async fn resolve(
    clients: &BTreeMap<String, Arc<dyn AgentClient>>,
    config: &Value,
    request: &MetadataRequest,
) -> Vec<MetadataSelection> {
    let mut models = BTreeMap::new();
    for (provider, client) in clients {
        if config["providers"][provider]["enabled"] == false {
            continue;
        }
        if let Ok(Ok(details)) =
            tokio::time::timeout(Duration::from_secs(5), client.discover(&request.cwd)).await
        {
            models.insert(provider.clone(), details.models);
        }
    }
    ordered(
        clients.keys().map(String::as_str),
        config,
        request.selection.as_ref(),
        &models,
    )
}

fn ordered<'a>(
    providers: impl Iterator<Item = &'a str>,
    config: &Value,
    current: Option<&MetadataSelection>,
    models: &BTreeMap<String, Vec<Value>>,
) -> Vec<MetadataSelection> {
    let available: BTreeSet<_> = providers
        .filter(|provider| config["providers"][*provider]["enabled"] != false)
        .collect();
    let mut result: Vec<MetadataSelection> = config["metadataGeneration"]["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .take(32)
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .filter(|candidate: &MetadataSelection| available.contains(candidate.provider.trim()))
        .map(|candidate| defaults(candidate, models))
        .collect();
    for (needle, effort) in [
        ("haiku", None),
        ("gpt-5.4-mini", Some("low")),
        ("minimax-m3", None),
        ("nemotron-3-super", None),
    ] {
        if let Some((provider, model)) = models
            .iter()
            .filter(|(provider, _)| available.contains(provider.as_str()))
            .flat_map(|(provider, models)| models.iter().map(move |model| (provider, model)))
            .find(|(_, model)| {
                ["id", "label"].iter().any(|key| {
                    model[key]
                        .as_str()
                        .is_some_and(|text| text.to_lowercase().contains(needle))
                })
            })
        {
            result.push(MetadataSelection {
                provider: provider.clone(),
                model: model["id"].as_str().map(str::to_owned),
                thinking_option_id: effort
                    .filter(|effort| supports(model, effort))
                    .map(str::to_owned)
                    .or_else(|| model["defaultThinkingOptionId"].as_str().map(str::to_owned)),
            });
        }
    }
    if let Some(current) = current.filter(|current| available.contains(current.provider.as_str())) {
        result.push(defaults(current.clone(), models));
    }
    let mut seen = BTreeSet::new();
    result.retain(|candidate| seen.insert(candidate.clone()));
    result
}

fn defaults(
    mut candidate: MetadataSelection,
    models: &BTreeMap<String, Vec<Value>>,
) -> MetadataSelection {
    candidate.provider = candidate.provider.trim().to_owned();
    if candidate
        .model
        .as_deref()
        .is_some_and(|model| !model.trim().is_empty())
    {
        return candidate;
    }
    if let Some(models) = models.get(&candidate.provider)
        && let Some(model) = models
            .iter()
            .find(|model| model["isDefault"] == true)
            .or_else(|| models.first())
    {
        candidate.model = model["id"].as_str().map(str::to_owned);
        if !candidate
            .thinking_option_id
            .as_deref()
            .is_some_and(|effort| supports(model, effort))
        {
            candidate.thinking_option_id =
                model["defaultThinkingOptionId"].as_str().map(str::to_owned);
        }
    }
    candidate
}

fn supports(model: &Value, effort: &str) -> bool {
    model["thinkingOptions"]
        .as_array()
        .is_some_and(|options| options.iter().any(|option| option["id"] == effort))
}

#[cfg(test)]
mod tests;
