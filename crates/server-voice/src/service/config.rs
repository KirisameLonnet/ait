use std::{path::Path, sync::Arc};

use crate::{
    Error,
    local::{Piper, Whisper},
    offline::Offline,
    openai::{Config, OpenAi},
    ports::Agents,
};

use super::Speech;

pub(super) fn load(
    get: impl Fn(&str) -> Option<String>,
    agents: Option<Arc<dyn Agents>>,
    models: &Path,
    program: &Path,
) -> Result<Speech, Error> {
    let value = |key: &str, default: &str| {
        get(key)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| default.to_owned())
    };
    let provider = value("AIT_SPEECH_PROVIDER", "offline");
    let stt = value("AIT_SPEECH_STT_PROVIDER", &provider);
    let tts = value("AIT_SPEECH_TTS_PROVIDER", &provider);
    let mut speech = Speech::new(None, None, agents);
    let http = if stt == "openai" || tts == "openai" {
        Some(Arc::new(OpenAi::new(Config {
            base_url: value("AIT_SPEECH_BASE_URL", "https://api.openai.com/v1"),
            api_key: get("AIT_SPEECH_API_KEY")
                .or_else(|| get("OPENAI_API_KEY"))
                .filter(|key| !key.trim().is_empty())
                .map(Into::into),
            stt_model: value("AIT_SPEECH_STT_MODEL", "whisper-1"),
            tts_model: value("AIT_SPEECH_TTS_MODEL", "tts-1"),
            voice: value("AIT_SPEECH_VOICE", "alloy"),
        })?))
    } else {
        None
    };
    speech.stt = match stt.as_str() {
        "disabled" => None,
        "offline" => Some(Offline::transcriber(
            get("AIT_SPEECH_MODELS_DIR").map_or_else(|| models.to_owned(), Into::into),
            program.to_owned(),
            &value("AIT_SPEECH_OFFLINE_STT_MODEL", "sensevoice-int8"),
        )?),
        "openai" => http
            .as_ref()
            .map(|http| http.clone() as Arc<dyn crate::ports::Transcriber>),
        "local" => Some(Arc::new(Whisper::new(
            value("AIT_SPEECH_WHISPER_BIN", "whisper-cli").into(),
            get("AIT_SPEECH_WHISPER_MODEL")
                .ok_or(Error::Unavailable)?
                .into(),
        )?)),
        _ => return Err(Error::Invalid),
    };
    speech.tts = match tts.as_str() {
        "disabled" => None,
        "offline" => Some(Offline::synthesizer(
            get("AIT_SPEECH_MODELS_DIR").map_or_else(|| models.to_owned(), Into::into),
            program.to_owned(),
            &value("AIT_SPEECH_OFFLINE_TTS_MODEL", "kokoro-multi-lang-v1_0"),
        )?),
        "openai" => http.map(|http| http as Arc<dyn crate::ports::Synthesizer>),
        "local" => Some(Arc::new(Piper::new(
            value("AIT_SPEECH_PIPER_BIN", "piper").into(),
            get("AIT_SPEECH_PIPER_MODEL")
                .ok_or(Error::Unavailable)?
                .into(),
        )?)),
        _ => return Err(Error::Invalid),
    };
    Ok(speech)
}

#[cfg(test)]
mod tests;
