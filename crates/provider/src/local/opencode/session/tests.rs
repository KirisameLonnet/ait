use super::{Version, permissions, permissions_match};
use serde_json::{Value, json};

#[test]
fn v1_accepts_only_complete_ordered_copies_of_the_creation_policy() {
    let policy = permissions(Version::V1);
    let rules = policy.as_array().unwrap();
    for copies in 1..=3 {
        let repeated = Value::Array(
            rules
                .iter()
                .cycle()
                .take(rules.len() * copies)
                .cloned()
                .collect(),
        );
        assert!(permissions_match(Version::V1, Some(&repeated)));
    }
    for invalid in [Value::Null, json!([]), json!({}), json!(rules[..8])] {
        assert!(!permissions_match(Version::V1, Some(&invalid)));
    }
    assert!(!permissions_match(Version::V1, None));
    let mut reordered = policy.clone();
    reordered.as_array_mut().unwrap().swap(0, 1);
    assert!(!permissions_match(Version::V1, Some(&reordered)));
    let mut additional = policy.clone();
    additional
        .as_array_mut()
        .unwrap()
        .push(json!({"permission":"bash","pattern":"*","action":"allow"}));
    assert!(!permissions_match(Version::V1, Some(&additional)));
    let mut changed = Value::Array(rules.iter().chain(rules).cloned().collect());
    changed[17]["action"] = json!("allow");
    assert!(!permissions_match(Version::V1, Some(&changed)));
}

#[test]
fn v2_requires_exact_replacement_policy() {
    let policy = permissions(Version::V2);
    assert!(permissions_match(Version::V2, Some(&policy)));
    assert!(!permissions_match(Version::V2, None));
    let rules = policy.as_array().unwrap();
    let repeated = Value::Array(rules.iter().chain(rules).cloned().collect());
    assert!(!permissions_match(Version::V2, Some(&repeated)));
}
