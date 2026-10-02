use crate::Error;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub(super) enum Model {
    SenseVoice,
    Parakeet,
    Kokoro,
    KokoroEnglish,
}

impl Model {
    pub(super) fn stt(id: &str) -> Result<Self, Error> {
        match id {
            "sensevoice-int8" => Ok(Self::SenseVoice),
            "parakeet-tdt-0.6b-v2-int8" => Ok(Self::Parakeet),
            _ => Err(Error::Invalid),
        }
    }

    pub(super) fn tts(id: &str) -> Result<Self, Error> {
        match id {
            "kokoro-multi-lang-v1_0" => Ok(Self::Kokoro),
            "kokoro-en-v0_19" => Ok(Self::KokoroEnglish),
            _ => Err(Error::Invalid),
        }
    }

    pub(super) fn directory(self) -> &'static str {
        match self {
            Self::SenseVoice => "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17",
            Self::Parakeet => "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8",
            Self::Kokoro => "kokoro-multi-lang-v1_0",
            Self::KokoroEnglish => "kokoro-en-v0_19",
        }
    }

    pub(super) fn url(self) -> String {
        let group = match self {
            Self::SenseVoice | Self::Parakeet => "asr-models",
            Self::Kokoro | Self::KokoroEnglish => "tts-models",
        };
        format!(
            "https://github.com/k2-fsa/sherpa-onnx/releases/download/{group}/{}.tar.bz2",
            self.directory()
        )
    }

    pub(super) fn files(self) -> &'static [&'static str] {
        match self {
            Self::SenseVoice => &["model.int8.onnx", "tokens.txt"],
            Self::Parakeet => &[
                "encoder.int8.onnx",
                "decoder.int8.onnx",
                "joiner.int8.onnx",
                "tokens.txt",
            ],
            Self::Kokoro => &[
                "model.onnx",
                "voices.bin",
                "tokens.txt",
                "espeak-ng-data",
                "dict",
                "lexicon-us-en.txt",
                "lexicon-zh.txt",
                "date-zh.fst",
                "number-zh.fst",
                "phone-zh.fst",
            ],
            Self::KokoroEnglish => &["model.onnx", "voices.bin", "tokens.txt", "espeak-ng-data"],
        }
    }
}
