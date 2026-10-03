use super::*;

#[test]
fn matches_upstream_unicode_and_identifier_ordering_in_three_server_locales() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/paseo-collation.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let collator = collator(case["environment"].as_str());
        let mut values: Vec<_> = fixture["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        values.sort_by(|left, right| collator.compare(left, right));
        assert_eq!(
            serde_json::to_value(values).unwrap(),
            case["expected"],
            "{}",
            case["locale"]
        );
    }
}

#[test]
fn c_posix_missing_and_empty_locales_have_icu_compatible_fallbacks() {
    for name in [None, Some("C"), Some("C.UTF-8"), Some("POSIX"), Some("")] {
        let collator = collator(name);
        assert_eq!(collator.compare("ä", "b"), Ordering::Less);
        assert_eq!(collator.compare("e\u{301}", "é"), Ordering::Equal);
    }
}
