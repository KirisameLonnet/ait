use super::*;
use serde_json::json;

#[test]
fn configured_automatic_and_current_candidates_are_ordered_and_deduplicated() {
    let models = BTreeMap::from([
        (
            "claude".to_owned(),
            vec![json!({"id":"haiku","isDefault":true})],
        ),
        (
            "codex".to_owned(),
            vec![
                json!({"id":"gpt-5.4-mini","isDefault":true,"thinkingOptions":[{"id":"low"}],"defaultThinkingOptionId":"high"}),
            ],
        ),
    ]);
    let current = MetadataSelection {
        provider: "codex".into(),
        model: Some("large".into()),
        thinking_option_id: None,
    };
    let config = json!({"metadataGeneration":{"providers":[{"provider":" codex ","model":"custom","thinkingOptionId":"high"},{"provider":"claude"},{"provider":"missing"}]}});
    let candidates = ordered(
        ["codex", "claude"].into_iter(),
        &config,
        Some(&current),
        &models,
    );
    assert_eq!(
        candidates
            .iter()
            .map(|c| c.model.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["custom", "haiku", "gpt-5.4-mini", "large"]
    );
    assert_eq!(candidates[2].thinking_option_id.as_deref(), Some("low"));
    assert_eq!(candidates[0].provider, "codex");
}

#[test]
fn disabled_provider_and_unsupported_effort_are_filtered() {
    let models = BTreeMap::from([(
        "claude".into(),
        vec![
            json!({"id":"haiku","isDefault":true,"defaultThinkingOptionId":"low","thinkingOptions":[{"id":"low"}]}),
        ],
    )]);
    let chosen = defaults(
        MetadataSelection {
            provider: "claude".into(),
            model: None,
            thinking_option_id: Some("invalid".into()),
        },
        &models,
    );
    assert_eq!(chosen.thinking_option_id.as_deref(), Some("low"));
    let config = json!({"providers":{"claude":{"enabled":false}},"metadataGeneration":{"providers":[{"provider":"claude"}]}});
    // Disabled providers must be excluded even if stale discovery supplied models.
    assert!(
        ordered(
            ["claude"].into_iter(),
            &config,
            Some(&chosen),
            &BTreeMap::new()
        )
        .is_empty()
    );
}
