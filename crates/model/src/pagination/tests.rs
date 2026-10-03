//! Cases adapted from Paseo pagination/cursor.test.ts and sortable-pager.test.ts.

use serde_json::json;

use super::*;

fn sort(direction: Direction) -> Vec<Sort> {
    vec![Sort {
        key: "rank".to_owned(),
        direction,
    }]
}

fn entry(id: &str, value: SortValue) -> Entry<String> {
    Entry {
        id: id.to_owned(),
        values: BTreeMap::from([("rank".to_owned(), value)]),
        value: id.to_owned(),
    }
}

fn rows() -> Vec<Entry<String>> {
    vec![
        entry("b", SortValue::Number(2)),
        entry("a", SortValue::Null),
        entry("c", SortValue::Number(2)),
    ]
}

#[test]
fn orders_primary_values_with_nulls_and_breaks_ties_by_identity() {
    for (direction, expected) in [
        (Direction::Asc, ["a", "b", "c"]),
        (Direction::Desc, ["b", "c", "a"]),
    ] {
        let page = paginate(rows(), &sort(direction), 200, None).unwrap();
        assert_eq!(page.entries, expected);
        assert!(!page.has_more);
        assert!(page.next_cursor.is_none());
    }
}

#[test]
fn roundtrips_and_continues_after_a_removed_cursor_row() {
    let sort = sort(Direction::Asc);
    let first = paginate(rows(), &sort, 1, None).unwrap();
    assert_eq!(first.entries, ["a"]);
    let token = first.next_cursor.unwrap();
    let cursor = decode(&token, &sort).unwrap();
    assert_eq!(cursor.id, "a");
    assert_eq!(cursor.values["rank"], SortValue::Null);
    let second = paginate(
        rows().into_iter().filter(|entry| entry.id != "a").collect(),
        &sort,
        1,
        Some(&token),
    )
    .unwrap();
    assert_eq!(second.entries, ["b"]);
    assert_eq!(second.prev_cursor.as_deref(), Some(token.as_str()));
    let last = paginate(rows(), &sort, 1, second.next_cursor.as_deref()).unwrap();
    assert_eq!(last.entries, ["c"]);
    assert!(!last.has_more);
}

#[test]
fn uses_secondary_fields_and_keeps_first_duplicate_direction() {
    let mut a = entry("a", SortValue::Number(2));
    let mut b = entry("b", SortValue::Number(2));
    a.values
        .insert("name".to_owned(), SortValue::Text("z".to_owned()));
    b.values
        .insert("name".to_owned(), SortValue::Text("a".to_owned()));
    let sort = vec![
        Sort {
            key: "rank".to_owned(),
            direction: Direction::Asc,
        },
        Sort {
            key: "rank".to_owned(),
            direction: Direction::Desc,
        },
        Sort {
            key: "name".to_owned(),
            direction: Direction::Asc,
        },
    ];
    let first = paginate(vec![a, b], &sort, 1, None).unwrap();
    assert_eq!(first.entries, ["b"]);
    let decoded = decode(
        first.next_cursor.as_deref().unwrap(),
        &[sort[0].clone(), sort[2].clone()],
    )
    .unwrap();
    assert_eq!(decoded.values.len(), 2);
}

#[test]
fn only_encodes_requested_sort_values_and_handles_an_empty_tail() {
    let mut row = entry("a", SortValue::Number(1));
    row.values.insert(
        "unrequested".to_owned(),
        SortValue::Text("private".to_owned()),
    );
    let sort = sort(Direction::Asc);
    let token = encode(&row, &sort).unwrap();
    assert_eq!(decode(&token, &sort).unwrap().values.len(), 1);
    let page = paginate(vec![row], &sort, 1, Some(&token)).unwrap();
    assert!(page.entries.is_empty());
    assert!(!page.has_more);
}

#[test]
fn accepts_empty_cursors_as_first_pages_and_missing_values_as_null() {
    let sort = sort(Direction::Asc);
    assert_eq!(paginate(rows(), &sort, 1, Some("")).unwrap().entries, ["a"]);
    let token = URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&json!({"id":"a","sort":sort,"values":{}})).unwrap());
    assert_eq!(
        paginate(rows(), &sort, 1, Some(&token)).unwrap().entries,
        ["b"]
    );
}

#[test]
fn rejects_invalid_limits_and_empty_sort() {
    for limit in [0, 201, usize::MAX] {
        assert_eq!(
            paginate(rows(), &sort(Direction::Asc), limit, None).unwrap_err(),
            ErrorCode::InvalidMessage
        );
    }
    assert_eq!(
        paginate(rows(), &[], 1, None).unwrap_err(),
        ErrorCode::InvalidMessage
    );
}

#[test]
fn rejects_malformed_or_mismatched_cursors() {
    let sort = sort(Direction::Asc);
    for value in [
        json!(null),
        json!([]),
        json!({}),
        json!({"sort":sort,"values":{},"id":4}),
        json!({"sort":sort,"id":"a"}),
        json!({"sort":[],"values":{},"id":"a"}),
        json!({"sort":[{"key":"unknown","direction":"asc"}],"values":{},"id":"a"}),
        json!({"sort":[{"key":"rank","direction":"desc"}],"values":{},"id":"a"}),
        json!({"sort":[{"key":"rank","direction":"bad"}],"values":{},"id":"a"}),
        json!({"sort":sort,"values":{"rank":{}},"id":"a"}),
    ] {
        let token = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value).unwrap());
        assert_eq!(
            decode(&token, &sort).unwrap_err(),
            ErrorCode::InvalidMessage,
            "{value}"
        );
    }
    for token in ["!".to_owned(), "eA".to_owned(), "a".repeat(16 * 1024 + 1)] {
        assert!(decode(&token, &sort).is_err());
    }
}

#[test]
fn mixed_scalar_cursor_values_follow_the_upstream_string_comparison() {
    let sort = sort(Direction::Asc);
    let page = paginate(
        vec![
            entry("n", SortValue::Number(2)),
            entry("s", SortValue::Text("10".to_owned())),
        ],
        &sort,
        200,
        None,
    )
    .unwrap();
    assert_eq!(page.entries, ["s", "n"]);
}
