use std::collections::BTreeMap;

use super::*;

#[test]
fn invalid_http_endpoint_and_missing_local_models_fail_during_configuration() {
    let root = tempfile::tempdir().unwrap();
    for values in [
        BTreeMap::from([
            ("AIT_SPEECH_PROVIDER", "openai".to_owned()),
            ("AIT_SPEECH_BASE_URL", "not a URL".to_owned()),
        ]),
        BTreeMap::from([
            ("AIT_SPEECH_STT_PROVIDER", "local".to_owned()),
            (
                "AIT_SPEECH_WHISPER_MODEL",
                root.path().join("missing").to_string_lossy().into_owned(),
            ),
        ]),
        BTreeMap::from([
            ("AIT_SPEECH_STT_PROVIDER", "disabled".to_owned()),
            ("AIT_SPEECH_TTS_PROVIDER", "local".to_owned()),
            (
                "AIT_SPEECH_PIPER_MODEL",
                root.path().join("missing").to_string_lossy().into_owned(),
            ),
        ]),
    ] {
        assert!(load(|key| values.get(key).cloned(), None).is_err());
    }
}

fn load(
    get: impl Fn(&str) -> Option<String>,
    agents: Option<Arc<dyn Agents>>,
) -> Result<Speech, Error> {
    super::load(
        get,
        agents,
        Path::new("/missing-ait-speech-models"),
        Path::new("server"),
    )
}

#[test]
fn defaults_are_offline_and_stt_tts_can_be_mixed() {
    let default = load(|_| None, None).unwrap();
    assert_eq!(default.availability(), (true, false));
    let models = tempfile::tempdir().unwrap();
    let model = models.path().join("voice.onnx");
    std::fs::write(&model, b"model").unwrap();
    let values = BTreeMap::from([
        ("AIT_SPEECH_STT_PROVIDER", "openai".to_owned()),
        ("AIT_SPEECH_TTS_PROVIDER", "local".to_owned()),
        (
            "AIT_SPEECH_PIPER_MODEL",
            model.to_string_lossy().into_owned(),
        ),
    ]);
    let speech = load(|key| values.get(key).cloned(), None).unwrap();
    assert!(speech.stt.is_some());
    assert!(speech.tts.is_some());
    assert_eq!(speech.availability(), (true, false));
    assert!(
        load(
            |key| (key == "AIT_SPEECH_PROVIDER").then(|| "unknown".to_owned()),
            None
        )
        .is_err()
    );
    assert!(
        load(
            |key| (key == "AIT_SPEECH_PROVIDER").then(|| "local".to_owned()),
            None
        )
        .is_err()
    );
}

#[test]
fn disabled_and_offline_model_overrides_remain_explicit() {
    let disabled = load(
        |key| (key == "AIT_SPEECH_PROVIDER").then(|| "disabled".to_owned()),
        None,
    )
    .unwrap();
    assert_eq!(disabled.availability(), (false, false));
    for (key, id) in [
        ("AIT_SPEECH_OFFLINE_STT_MODEL", "parakeet-tdt-0.6b-v2-int8"),
        ("AIT_SPEECH_OFFLINE_TTS_MODEL", "kokoro-en-v0_19"),
    ] {
        assert!(load(|name| (name == key).then(|| id.to_owned()), None).is_ok());
        assert!(load(|name| (name == key).then(|| "unknown".to_owned()), None).is_err());
    }
}

#[test]
fn provider_configuration_keeps_independent_engines_and_rejects_invalid_tts() {
    let models = tempfile::tempdir().unwrap();
    let model = models.path().join("model");
    std::fs::write(&model, b"fixture").unwrap();
    for (stt, tts) in [
        ("local", "openai"),
        ("openai", "disabled"),
        ("disabled", "local"),
    ] {
        let values = BTreeMap::from([
            ("AIT_SPEECH_STT_PROVIDER", stt.to_owned()),
            ("AIT_SPEECH_TTS_PROVIDER", tts.to_owned()),
            (
                "AIT_SPEECH_WHISPER_MODEL",
                model.to_string_lossy().into_owned(),
            ),
            (
                "AIT_SPEECH_PIPER_MODEL",
                model.to_string_lossy().into_owned(),
            ),
            (
                "AIT_SPEECH_API_KEY",
                "offline-configuration-fixture".to_owned(),
            ),
        ]);
        let speech = load(|key| values.get(key).cloned(), None).unwrap();
        assert_eq!(speech.stt.is_some(), stt != "disabled");
        assert_eq!(speech.tts.is_some(), tts != "disabled");
    }
    let values = BTreeMap::from([
        ("AIT_SPEECH_STT_PROVIDER", "disabled"),
        ("AIT_SPEECH_TTS_PROVIDER", "unknown"),
    ]);
    assert!(matches!(
        load(|key| values.get(key).map(|value| (*value).to_owned()), None),
        Err(Error::Invalid)
    ));
}
