//! Cases ported from Paseo directory-sync and versioned-collection tests.

use super::*;
use serde_json::json;

fn rows(values: &[(&str, &str)]) -> Vec<(String, Value)> {
    values
        .iter()
        .map(|(id, title)| (id.to_string(), json!({"id":id,"title":title})))
        .collect()
}

fn cursor(seq: u64) -> Cursor {
    Cursor {
        generation: Some("generation".to_owned()),
        after_seq: Some(seq),
    }
}

#[test]
fn changes_return_only_the_latest_projection_for_each_entity() {
    let sync = DirectorySync::new("generation".to_owned());
    let first = sync.synchronize("agents", rows(&[("one", "first")]), &Cursor::default());
    assert_eq!(first.sync.reason, Some(Reason::NoCursor));
    assert_eq!(first.values[0]["syncSeq"], 1);
    let _ = sync.synchronize("agents", rows(&[("one", "second")]), &cursor(1));
    let read = sync.synchronize(
        "agents",
        rows(&[("one", "latest"), ("two", "other")]),
        &cursor(1),
    );
    assert_eq!(read.sync.mode, Mode::Changes);
    assert_eq!(read.sync.head_seq, 4);
    assert_eq!(
        read.values,
        [
            json!({"id":"one","title":"latest","syncSeq":3}),
            json!({"id":"two","title":"other","syncSeq":4})
        ]
    );
    assert!(read.sync.removals.is_empty());
}

#[test]
fn repeated_observations_share_one_sequence_and_do_not_repeat_removals() {
    let sync = DirectorySync::new("generation".to_owned());
    let _ = sync.synchronize("projects", rows(&[("one", "same")]), &Cursor::default());
    let clone = sync.clone();
    let same = clone.synchronize("projects", rows(&[("one", "same")]), &cursor(1));
    assert!(same.values.is_empty());
    assert_eq!(same.sync.head_seq, 1);
    let removed = clone.synchronize("projects", [], &cursor(1));
    assert_eq!(
        removed.sync.removals,
        [Removal {
            id: "one".to_owned(),
            seq: 2
        }]
    );
    let repeated = sync.synchronize("projects", [], &cursor(2));
    assert!(repeated.sync.removals.is_empty());
    assert_eq!(repeated.sync.head_seq, 2);
}

#[test]
fn reappearing_value_replaces_its_tombstone() {
    let sync = DirectorySync::new("generation".to_owned());
    let _ = sync.synchronize("workspaces", rows(&[("one", "before")]), &Cursor::default());
    let _ = sync.synchronize("workspaces", [], &cursor(1));
    let read = sync.synchronize("workspaces", rows(&[("one", "after")]), &cursor(1));
    assert_eq!(read.values[0]["syncSeq"], 3);
    assert!(read.sync.removals.is_empty());
}

#[test]
fn expired_tombstones_and_future_checkpoints_require_a_snapshot() {
    let mut collection = Collection::default();
    collection.replace_all(rows(&[("one", "one"), ("two", "two")]), 1);
    collection.replace_all(rows(&[("two", "two")]), 1);
    collection.replace_all([], 1);
    let expired = collection.read("generation", &cursor(2));
    assert_eq!(expired.sync.mode, Mode::Snapshot);
    assert_eq!(expired.sync.reason, Some(Reason::CursorExpired));
    assert!(expired.sync.removals.is_empty());
    assert_eq!(
        collection.read("generation", &cursor(3)).sync.mode,
        Mode::Changes
    );
    assert_eq!(
        collection.read("generation", &cursor(5)).sync.reason,
        Some(Reason::CursorExpired)
    );
}

#[test]
fn missing_checkpoint_and_changed_generation_have_distinct_reasons() {
    let sync = DirectorySync::new("generation".to_owned());
    for (checkpoint, expected) in [
        (Cursor::default(), Reason::NoCursor),
        (
            Cursor {
                generation: Some("old".to_owned()),
                after_seq: Some(1),
            },
            Reason::GenerationChanged,
        ),
        (
            Cursor {
                generation: Some("generation".to_owned()),
                after_seq: None,
            },
            Reason::NoCursor,
        ),
    ] {
        let read = sync.synchronize("projects", rows(&[("one", "one")]), &checkpoint);
        assert_eq!(read.sync.reason, Some(expected));
        assert_eq!(read.values.len(), 1);
    }
}

#[test]
fn collections_have_independent_sequences_in_one_generation() {
    let sync = DirectorySync::new("generation".to_owned());
    let projects = sync.synchronize(
        "projects",
        rows(&[("one", "one"), ("two", "two")]),
        &Cursor::default(),
    );
    let agents = sync.synchronize("agents", rows(&[("one", "one")]), &Cursor::default());
    assert_eq!(projects.sync.generation, agents.sync.generation);
    assert_eq!(projects.sync.head_seq, 2);
    assert_eq!(agents.sync.head_seq, 1);
}

#[test]
fn changing_an_icon_only_sequences_the_affected_project() {
    let sync = DirectorySync::new("generation".to_owned());
    let one = json!({"projectId":"one","projectIconRevision":"a"});
    let two = json!({"projectId":"two","projectIconRevision":"a"});
    let _ = sync.synchronize(
        "projects",
        [("one".into(), one.clone()), ("two".into(), two.clone())],
        &Cursor::default(),
    );
    let mut changed = one;
    changed["projectIconRevision"] = json!("b");
    let read = sync.synchronize(
        "projects",
        [("one".into(), changed), ("two".into(), two)],
        &cursor(2),
    );
    assert_eq!(read.values.len(), 1);
    assert_eq!(read.values[0]["projectId"], "one");
    assert_eq!(read.values[0]["syncSeq"], 3);
}
