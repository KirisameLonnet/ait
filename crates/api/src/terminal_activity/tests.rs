use super::*;
use crate::{Api, Services};
use axum::body::Body;

#[test]
fn activity_urls_resolve_wildcard_binds_and_bracket_ipv6() {
    for (address, expected) in [
        (
            "0.0.0.0:1234",
            "http://127.0.0.1:1234/api/terminal-activity",
        ),
        ("[::]:1234", "http://[::1]:1234/api/terminal-activity"),
        (
            "127.0.0.1:1234",
            "http://127.0.0.1:1234/api/terminal-activity",
        ),
    ] {
        assert_eq!(url(address.parse().unwrap()), expected);
    }
}

#[tokio::test]
async fn route_rejects_remote_peers_before_body_or_token_validation() {
    let api = api();
    for peer in [None, Some("192.168.1.5:60000"), Some("127.0.0.2:60000")] {
        let response = report(State(api.shared.clone()), request(peer, "not json")).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    api.begin_shutdown();
    api.wait_closed().await;
}

#[tokio::test]
async fn route_validates_the_paseo_report_shape_on_each_loopback_spelling() {
    let api = api();
    for peer in ["127.0.0.1:60000", "[::1]:60000", "[::ffff:127.0.0.1]:60000"] {
        for body in [
            "null",
            "{}",
            "not json",
            r#"{"terminalId":"one","token":"secret","state":"working"}"#,
            r#"{"terminalId":"","token":"secret","state":"running"}"#,
            r#"{"terminalId":"one","token":"","state":"idle"}"#,
        ] {
            let response = report(State(api.shared.clone()), request(Some(peer), body)).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }
        for state in ["running", "idle", "needs-input"] {
            let body = serde_json::json!({"terminalId":"unknown","token":"wrong","state":state});
            let response = report(
                State(api.shared.clone()),
                request(Some(peer), &body.to_string()),
            )
            .await;
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            let bytes = axum::body::to_bytes(response.into_body(), 1000)
                .await
                .unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
                serde_json::json!({"error":"Forbidden"})
            );
        }
    }
    let response = report(
        State(api.shared.clone()),
        request(Some("127.0.0.1:1"), &"x".repeat(4097)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    api.begin_shutdown();
    api.wait_closed().await;
}

fn api() -> Api {
    Api::new(
        "127.0.0.1:1234".parse().unwrap(),
        "server".into(),
        "instance".into(),
        "test-server-activity-token-32-characters".into(),
        Services::default(),
    )
    .unwrap()
}

fn request(peer: Option<&str>, body: &str) -> Request {
    let mut request = Request::new(Body::from(body.to_owned()));
    if let Some(peer) = peer {
        request.extensions_mut().insert(ConnectInfo(LocalAddress(
            "127.0.0.1:1234".parse().unwrap(),
            peer.parse().unwrap(),
        )));
    }
    request
}
