use std::{cell::Cell, path::Path, rc::Rc};

use sherpa_onnx::{
    GenerationConfig, OfflineRecognizer, OfflineRecognizerConfig, OfflineTts, OfflineTtsConfig,
};

use super::{catalog::Model, worker::Init};
use crate::{
    Error,
    audio::{Audio, Format, MAX_AUDIO_BYTES, MAX_TEXT_BYTES},
    ports::Transcript,
};

pub(super) enum Engine {
    Recognizer(OfflineRecognizer),
    Synthesizer(OfflineTts, i32),
}

impl Engine {
    pub(super) fn new(init: &Init) -> Result<Self, Error> {
        match init.model {
            Model::SenseVoice | Model::Parakeet => {
                OfflineRecognizer::create(&recognizer_config(init)?)
                    .map(Self::Recognizer)
                    .ok_or(Error::Unavailable)
            }
            Model::Kokoro | Model::KokoroEnglish => OfflineTts::create(&tts_config(init)?)
                .map(|tts| Self::Synthesizer(tts, if init.model == Model::Kokoro { 45 } else { 0 }))
                .ok_or(Error::Unavailable),
        }
    }
}

fn file(directory: &Path, name: &str) -> Result<String, Error> {
    let path = directory.join(name);
    let text = path
        .to_str()
        .filter(|text| !text.contains('\0'))
        .ok_or(Error::Invalid)?;
    if !path.exists() {
        return Err(Error::Unavailable);
    }
    Ok(text.to_owned())
}

fn recognizer_config(init: &Init) -> Result<OfflineRecognizerConfig, Error> {
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.tokens = Some(file(&init.directory, "tokens.txt")?);
    config.model_config.num_threads = 2;
    config.model_config.provider = Some("cpu".to_owned());
    match init.model {
        Model::SenseVoice => {
            config.model_config.sense_voice.model = Some(file(&init.directory, "model.int8.onnx")?);
            config.model_config.sense_voice.language = Some("auto".to_owned());
            config.model_config.sense_voice.use_itn = true;
        }
        Model::Parakeet => {
            config.model_config.model_type = Some("nemo_transducer".to_owned());
            config.model_config.transducer.encoder =
                Some(file(&init.directory, "encoder.int8.onnx")?);
            config.model_config.transducer.decoder =
                Some(file(&init.directory, "decoder.int8.onnx")?);
            config.model_config.transducer.joiner =
                Some(file(&init.directory, "joiner.int8.onnx")?);
        }
        Model::Kokoro | Model::KokoroEnglish => return Err(Error::Invalid),
    }
    Ok(config)
}

fn tts_config(init: &Init) -> Result<OfflineTtsConfig, Error> {
    let mut config = OfflineTtsConfig {
        max_num_sentences: 1,
        ..Default::default()
    };
    config.model.num_threads = 2;
    config.model.provider = Some("cpu".to_owned());
    let kokoro = &mut config.model.kokoro;
    kokoro.model = Some(file(&init.directory, "model.onnx")?);
    kokoro.voices = Some(file(&init.directory, "voices.bin")?);
    kokoro.tokens = Some(file(&init.directory, "tokens.txt")?);
    kokoro.data_dir = Some(file(&init.directory, "espeak-ng-data")?);
    if init.model == Model::Kokoro {
        kokoro.dict_dir = Some(file(&init.directory, "dict")?);
        kokoro.lexicon = Some(format!(
            "{},{}",
            file(&init.directory, "lexicon-us-en.txt")?,
            file(&init.directory, "lexicon-zh.txt")?
        ));
        config.rule_fsts = Some(
            ["date-zh.fst", "number-zh.fst", "phone-zh.fst"]
                .iter()
                .map(|name| file(&init.directory, name))
                .collect::<Result<Vec<_>, _>>()?
                .join(","),
        );
    }
    Ok(config)
}

pub(super) fn transcribe(
    recognizer: &OfflineRecognizer,
    audio: Audio,
) -> Result<Transcript, Error> {
    let audio = audio.pcm()?;
    let Format::Pcm(rate) = audio.format else {
        return Err(Error::Invalid);
    };
    let samples: Vec<f32> = audio
        .bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|sample| f32::from(i16::from_le_bytes([sample[0], sample[1]])) / 32768.0)
        .collect();
    let stream = recognizer.create_stream();
    stream.accept_waveform(i32::try_from(rate).map_err(|_| Error::Invalid)?, &samples);
    recognizer.decode(&stream);
    let result = stream.get_result().ok_or(Error::Provider)?;
    if result.text.len() > MAX_TEXT_BYTES {
        return Err(Error::Capacity);
    }
    Ok(Transcript {
        text: result.text.trim().to_owned(),
        language: None,
    })
}

pub(super) fn synthesize(tts: &OfflineTts, speaker: i32, text: &str) -> Result<Audio, Error> {
    if text.trim().is_empty() || text.len() > MAX_TEXT_BYTES || text.contains('\0') {
        return Err(Error::Invalid);
    }
    let total = Rc::new(Cell::new(0_usize));
    let callback_total = total.clone();
    let generated = tts
        .generate_with_config(
            text,
            &GenerationConfig {
                sid: speaker,
                ..Default::default()
            },
            Some(move |samples: &[f32], _: f32| {
                callback_total.set(callback_total.get().saturating_add(samples.len()));
                callback_total.get() <= MAX_AUDIO_BYTES / 2
            }),
        )
        .ok_or(Error::Provider)?;
    if total.get() > MAX_AUDIO_BYTES / 2 {
        return Err(Error::Capacity);
    }
    samples_to_audio(generated.samples(), generated.sample_rate())
}

fn samples_to_audio(samples: &[f32], rate: i32) -> Result<Audio, Error> {
    if samples.is_empty()
        || samples.len() > MAX_AUDIO_BYTES / 2
        || samples.iter().any(|sample| !sample.is_finite())
    {
        return Err(Error::Capacity);
    }
    let rate = u32::try_from(rate).map_err(|_| Error::Provider)?;
    Format::parse(&format!("pcm;rate={rate}"))?;
    let bytes = samples
        .iter()
        .flat_map(|sample| {
            // Clamp before the intentional float-to-PCM quantization.
            #[allow(clippy::cast_possible_truncation)]
            let value = (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            value.to_le_bytes()
        })
        .collect();
    Ok(Audio {
        bytes,
        format: Format::Pcm(rate),
    })
}

#[cfg(test)]
mod tests;
