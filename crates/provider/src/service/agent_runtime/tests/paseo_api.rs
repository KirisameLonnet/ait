//! Agent directory cases adapted from Paseo session.workspaces and pagination tests.

use serde_json::json;

use super::*;
use crate::rpc::agent_runtime::execute;

#[test]
fn archive_before_the_history_cursor_does_not_skip_an_active_agent() {
    let (mut service, agents) = service();
    let first = execute(
        &mut service,
        "agent.list.request",
        json!({"page":{"limit":1}}),
    )
    .unwrap();
    assert_eq!(first["entries"][0]["agent"]["id"], "agent-b");
    agents.remove("agent-b").unwrap();
    let cursor = &first["pageInfo"]["nextCursor"];
    let second = execute(
        &mut service,
        "agent.list.request",
        json!({"page":{"limit":1,"cursor":cursor}}),
    )
    .unwrap();
    assert_eq!(second["entries"][0]["agent"]["id"], "agent-a");
    assert_eq!(second["pageInfo"]["prevCursor"], *cursor);
}

#[test]
fn agent_cursor_is_bound_to_the_requested_sort_order() {
    let (mut service, _) = service();
    let first = execute(
        &mut service,
        "agent.history.get.request",
        json!({"page":{"limit":1}}),
    )
    .unwrap();
    assert!(
        execute(
            &mut service,
            "agent.history.get.request",
            json!({
                "sort":[{"key":"title","direction":"asc"}],
                "page":{"limit":1,"cursor":first["pageInfo"]["nextCursor"]}
            })
        )
        .is_err()
    );
}

#[test]
fn equivalent_timestamp_offsets_break_ties_by_identity() {
    let (service, agents) = service();
    for (id, time) in [
        ("agent-a", "2026-09-20T11:00:00.000Z"),
        ("agent-b", "2026-09-20T12:00:00.000+01:00"),
    ] {
        agents
            .update(id, &|current| {
                let mut record = current.clone();
                record.updated_at = time.to_owned();
                record
            })
            .unwrap();
    }
    let page = service.list(&default_query()).unwrap();
    assert_eq!(
        page.entries
            .iter()
            .map(|row| row.agent.id.as_str())
            .collect::<Vec<_>>(),
        ["agent-a", "agent-b"]
    );
}

#[test]
fn history_rpc_matches_typos_and_tokens_across_agent_and_workspace_names() {
    let (mut service, agents) = service();
    agents
        .update("agent-a", &|current| {
            let mut agent = current.clone();
            agent.title = Some("Fix terminal billing".to_owned());
            agent
        })
        .unwrap();
    for search in ["trmnl", "bulling", "Fix billing", "terminal"] {
        let result = execute(
            &mut service,
            "agent.history.get.request",
            json!({"search":search}),
        )
        .unwrap();
        assert_eq!(
            result["entries"].as_array().unwrap().len(),
            1,
            "{search}: {result}"
        );
        assert_eq!(result["entries"][0]["agent"]["id"], "agent-a");
    }
}
