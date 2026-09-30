use super::*;

const TOKEN: &str = "test-token-with-at-least-32-characters";

#[test]
fn equivalent_ipv6_authorities_match_without_accepting_other_hosts_or_ports() {
    assert!(authority_matches("[::1]:7316", "[0:0:0:0:0:0:0:1]:7316"));
    assert!(authority_matches(
        "[::ffff:127.0.0.1]:80",
        "[::ffff:7f00:1]"
    ));
    assert!(authority_matches("localhost:7316", "LOCALHOST:7316"));
    for requested in [
        "[::1]:7317",
        "[::1]:65536",
        "[::2]:7316",
        "evil.test:7316",
        "user@[::1]:7316",
    ] {
        assert!(!authority_matches("[::1]:7316", requested), "{requested}");
    }
}

#[test]
fn credentials_are_bounded_and_never_reported() {
    assert!(validate_token(TOKEN).is_ok());
    for value in [
        String::new(),
        "x".repeat(31),
        "x".repeat(257),
        "has spaces".repeat(8),
        "中".repeat(32),
    ] {
        let error = validate_token(&value).unwrap_err().to_string();
        if !value.is_empty() {
            assert!(!error.contains(&value));
        }
    }
    let mut headers = HeaderMap::new();
    assert_eq!(
        authenticate(&headers, &TOKEN.into()).err().unwrap().0,
        StatusCode::UNAUTHORIZED
    );
    headers.insert("authorization", format!("bEaReR {TOKEN}").parse().unwrap());
    assert!(authenticate(&headers, &TOKEN.into()).is_ok());
    assert!(authenticate(&headers, &"a-different-token-with-32-characters".into()).is_err());
    headers.append("authorization", format!("Bearer {TOKEN}").parse().unwrap());
    assert_eq!(
        authenticate(&headers, &TOKEN.into()).err().unwrap().0,
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn validates_host_and_optional_same_origin() {
    let allowed = vec![
        "127.0.0.1:7316".to_owned(),
        "localhost:7316".to_owned(),
        "[::1]:7316".to_owned(),
    ];
    let mut headers = HeaderMap::new();
    assert!(
        validate_source(
            &headers,
            &allowed,
            &crate::browser_auth::BrowserAuth::default()
        )
        .is_err()
    );
    headers.insert("host", "127.0.0.1:7316".parse().unwrap());
    assert!(
        validate_source(
            &headers,
            &allowed,
            &crate::browser_auth::BrowserAuth::default()
        )
        .is_ok()
    );
    for origin in ["http://localhost:7316", "http://[::1]:7316"] {
        headers.insert("origin", origin.parse().unwrap());
        assert!(
            validate_source(
                &headers,
                &allowed,
                &crate::browser_auth::BrowserAuth::default()
            )
            .is_ok()
        );
    }
    for origin in [
        "null",
        "https://localhost:7316",
        "http://localhost:7317",
        "http://evil.test",
        "http://localhost:7316/path",
        "http://localhost:7316/?token=x",
    ] {
        headers.insert("origin", origin.parse().unwrap());
        assert!(
            validate_source(
                &headers,
                &allowed,
                &crate::browser_auth::BrowserAuth::default()
            )
            .is_err(),
            "{origin}"
        );
    }
    headers.remove("origin");
    headers.insert("host", "evil.test:7316".parse().unwrap());
    assert!(
        validate_source(
            &headers,
            &allowed,
            &crate::browser_auth::BrowserAuth::default()
        )
        .is_err()
    );
    headers.insert("host", "127.0.0.1:7316".parse().unwrap());
    headers.append("host", "localhost:7316".parse().unwrap());
    assert!(
        validate_source(
            &headers,
            &allowed,
            &crate::browser_auth::BrowserAuth::default()
        )
        .is_err()
    );
}
