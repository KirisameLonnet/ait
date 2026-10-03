use super::*;

#[cfg(unix)]
mod process;

#[tokio::test]
async fn invalid_synthesis_is_rejected_before_preparing_models() {
    let root = tempfile::tempdir().unwrap();
    let voice = Offline::synthesizer(
        root.path().to_owned(),
        root.path().join("missing-worker"),
        "kokoro-en-v0_19",
    )
    .unwrap();
    for text in [
        String::new(),
        " \n".into(),
        "bad\0text".into(),
        "x".repeat(MAX_TEXT_BYTES + 1),
    ] {
        assert_eq!(
            voice
                .synthesize(&text, CancellationToken::new())
                .await
                .unwrap_err(),
            Error::Invalid
        );
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}
