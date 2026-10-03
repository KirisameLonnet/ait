use super::*;

#[test]
fn missing_native_model_files_fail_before_entering_inference() {
    let root = tempfile::tempdir().unwrap();
    for model in [
        Model::SenseVoice,
        Model::Parakeet,
        Model::Kokoro,
        Model::KokoroEnglish,
    ] {
        assert!(matches!(
            Engine::new(&Init {
                model,
                directory: root.path().to_owned()
            }),
            Err(Error::Unavailable)
        ));
    }
}

#[test]
fn pcm_conversion_bounds_and_clamps_native_output() {
    let audio = samples_to_audio(&[-2.0, 0.0, 2.0], 24000).unwrap();
    assert_eq!(audio.format, Format::Pcm(24000));
    assert_eq!(
        audio.bytes,
        [-32767_i16, 0, 32767]
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect::<Vec<_>>()
    );
    assert!(samples_to_audio(&[], 24000).is_err());
    assert!(samples_to_audio(&[f32::NAN], 24000).is_err());
    assert!(samples_to_audio(&[0.0], -1).is_err());
    assert!(samples_to_audio(&[0.0], 1).is_err());
}

#[test]
fn model_configs_support_bilingual_defaults_and_paseo_english_models() {
    let directory = tempfile::tempdir().unwrap();
    for model in [
        Model::SenseVoice,
        Model::Parakeet,
        Model::Kokoro,
        Model::KokoroEnglish,
    ] {
        for name in model.files() {
            std::fs::write(directory.path().join(name), b"fixture").unwrap();
        }
        let init = Init {
            model,
            directory: directory.path().to_owned(),
        };
        match model {
            Model::SenseVoice | Model::Parakeet => {
                let config = recognizer_config(&init).unwrap();
                assert_eq!(config.model_config.provider.as_deref(), Some("cpu"));
                assert_eq!(config.model_config.num_threads, 2);
                if model == Model::SenseVoice {
                    assert_eq!(
                        config.model_config.sense_voice.language.as_deref(),
                        Some("auto")
                    );
                } else {
                    assert_eq!(
                        config.model_config.model_type.as_deref(),
                        Some("nemo_transducer")
                    );
                }
            }
            Model::Kokoro | Model::KokoroEnglish => {
                let config = tts_config(&init).unwrap();
                assert_eq!(
                    config.model.kokoro.dict_dir.is_some(),
                    model == Model::Kokoro
                );
                assert_eq!(config.rule_fsts.is_some(), model == Model::Kokoro);
                assert!(recognizer_config(&init).is_err());
            }
        }
    }
    assert!(file(directory.path(), "missing").is_err());
    assert!(file(directory.path(), "bad\0path").is_err());
}

#[test]
fn recognition_normalizes_audio_and_limits_native_text() {
    let audio = || Audio {
        bytes: [-32768_i16, 0, 16384]
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect(),
        format: Format::Pcm(16000),
    };
    for input in [
        audio(),
        Audio {
            bytes: audio().wav().unwrap(),
            format: Format::Wav,
        },
    ] {
        let transcript = transcribe_with(input, |rate, samples| {
            assert_eq!(rate, 16000);
            assert_eq!(samples, [-1.0, 0.0, 0.5]);
            Ok(" recognized \n".into())
        })
        .unwrap();
        assert_eq!(transcript.text, "recognized");
        assert!(transcript.language.is_none());
    }
    assert_eq!(
        transcribe_with(audio(), |_, _| Err(Error::Provider)).unwrap_err(),
        Error::Provider
    );
    assert_eq!(
        transcribe_with(audio(), |_, _| Ok("x".repeat(MAX_TEXT_BYTES + 1))).unwrap_err(),
        Error::Capacity
    );
    assert_eq!(
        transcribe_with(
            Audio {
                bytes: vec![0],
                format: Format::Pcm(16000)
            },
            |_, _| panic!("invalid audio must not reach inference")
        )
        .unwrap_err(),
        Error::Invalid
    );
}

#[test]
fn native_synthesis_preserves_capacity_errors_even_when_generation_aborts() {
    for text in ["", " \n", "bad\0text"] {
        assert_eq!(
            synthesize_with(text, |_| panic!("invalid text must not reach inference")).unwrap_err(),
            Error::Invalid
        );
    }
    assert_eq!(
        synthesize_with("text", |_| Err(Error::Provider)).unwrap_err(),
        Error::Provider
    );
    assert_eq!(
        synthesize_with("text", |total| {
            total.set(MAX_AUDIO_BYTES / 2 + 1);
            Err(Error::Provider)
        })
        .unwrap_err(),
        Error::Capacity
    );
    let audio = synthesize_with("text", |total| {
        total.set(2);
        samples_to_audio(&[0.0, 0.5], 24000)
    })
    .unwrap();
    assert_eq!(audio.bytes, [0, 0, 0, 64]);
}
