use super::*;

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
