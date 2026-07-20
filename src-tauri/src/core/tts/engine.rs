use serde::{Deserialize, Serialize};
use sherpa_onnx::{
    GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig,
    OfflineTtsModelConfig, OfflineTtsVitsModelConfig, OfflineTtsZipvoiceModelConfig, Wave,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::core::tts::models::{ReadyTtsModel, TtsModelFamily};
use crate::error::{FinalSubError, Result};

const MAX_TEXT_BYTES: usize = 20_000;
const MAX_REFERENCE_TEXT_BYTES: usize = 4_000;
const MAX_REFERENCE_AUDIO_BYTES: u64 = 64 * 1024 * 1024;
const MAX_REFERENCE_DURATION_MS: u64 = 30_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalTtsSynthesisRequest {
    pub model_id: String,
    pub text: String,
    pub voice_id: Option<String>,
    pub speed: Option<f32>,
    pub output_path: String,
    pub reference_audio_path: Option<String>,
    pub reference_text: Option<String>,
    pub num_steps: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsSynthesisResult {
    pub output_path: String,
    pub sample_rate: u32,
    pub duration_ms: u64,
}

pub type TtsEngineCache = Arc<Mutex<HashMap<String, Arc<OfflineTts>>>>;

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn join_existing(model: &ReadyTtsModel, relative: &str) -> String {
    path_string(&model.path.join(relative))
}

fn build_config(model: &ReadyTtsModel) -> OfflineTtsConfig {
    let mut config = OfflineTtsConfig {
        max_num_sentences: 1,
        ..Default::default()
    };
    config.model = OfflineTtsModelConfig {
        num_threads: 2,
        provider: Some("cpu".into()),
        ..Default::default()
    };
    match model.spec.family {
        TtsModelFamily::Kokoro => {
            config.model.kokoro = OfflineTtsKokoroModelConfig {
                model: Some(join_existing(model, "model.int8.onnx")),
                voices: Some(join_existing(model, "voices.bin")),
                tokens: Some(join_existing(model, "tokens.txt")),
                data_dir: Some(join_existing(model, "espeak-ng-data")),
                lexicon: Some(format!(
                    "{},{}",
                    join_existing(model, "lexicon-us-en.txt"),
                    join_existing(model, "lexicon-zh.txt")
                )),
                ..Default::default()
            };
            let rule_fsts = ["phone-zh.fst", "date-zh.fst", "number-zh.fst"]
                .into_iter()
                .map(|name| model.path.join(name))
                .filter(|path| path.is_file())
                .map(|path| path_string(&path))
                .collect::<Vec<_>>();
            if !rule_fsts.is_empty() {
                config.rule_fsts = Some(rule_fsts.join(","));
            }
        }
        TtsModelFamily::Vits => {
            config.model.vits = OfflineTtsVitsModelConfig {
                model: Some(join_existing(model, "model.onnx")),
                tokens: Some(join_existing(model, "tokens.txt")),
                lexicon: Some(join_existing(model, "lexicon.txt")),
                ..Default::default()
            };
            let rule_fsts = ["phone.fst", "date.fst", "number.fst"]
                .into_iter()
                .map(|name| model.path.join(name))
                .filter(|path| path.is_file())
                .map(|path| path_string(&path))
                .collect::<Vec<_>>();
            if !rule_fsts.is_empty() {
                config.rule_fsts = Some(rule_fsts.join(","));
            }
        }
        TtsModelFamily::Zipvoice => {
            config.model.zipvoice = OfflineTtsZipvoiceModelConfig {
                tokens: Some(join_existing(model, "tokens.txt")),
                encoder: Some(join_existing(model, "encoder.int8.onnx")),
                decoder: Some(join_existing(model, "decoder.int8.onnx")),
                vocoder: Some(join_existing(model, "vocos_24khz.onnx")),
                data_dir: Some(join_existing(model, "espeak-ng-data")),
                lexicon: Some(join_existing(model, "lexicon.txt")),
                ..Default::default()
            };
        }
    }
    config
}

fn validate_text(text: &str) -> Result<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(FinalSubError::Validation("配音文本不能为空".into()));
    }
    if trimmed.len() > MAX_TEXT_BYTES {
        return Err(FinalSubError::Validation(format!(
            "单次配音文本不能超过 {MAX_TEXT_BYTES} 字节"
        )));
    }
    if trimmed.contains('\0') {
        return Err(FinalSubError::Validation("配音文本包含非法空字符".into()));
    }
    Ok(trimmed.to_string())
}

fn validate_speed(speed: Option<f32>) -> Result<f32> {
    let speed = speed.unwrap_or(1.0);
    if !speed.is_finite() || !(0.3..=3.0).contains(&speed) {
        return Err(FinalSubError::Validation(
            "本地配音速度必须在 0.3-3.0 之间".into(),
        ));
    }
    Ok(speed)
}

fn resolve_sid(model: &ReadyTtsModel, voice_id: Option<&str>) -> i32 {
    let requested = voice_id.unwrap_or(model.spec.default_voice_id);
    model
        .spec
        .voices
        .iter()
        .find(|voice| voice.id == requested)
        .or_else(|| {
            model
                .spec
                .voices
                .iter()
                .find(|voice| voice.id == model.spec.default_voice_id)
        })
        .map(|voice| voice.sid)
        .unwrap_or(0)
}

fn engine_key(model: &ReadyTtsModel) -> String {
    format!("{}|{}", model.spec.id, model.path.to_string_lossy())
}

fn load_engine(cache: &TtsEngineCache, model: &ReadyTtsModel) -> Result<Arc<OfflineTts>> {
    let key = engine_key(model);
    if let Some(engine) = cache
        .lock()
        .map_err(|_| FinalSubError::Validation("TTS 引擎缓存不可用".into()))?
        .get(&key)
        .cloned()
    {
        return Ok(engine);
    }
    let created = OfflineTts::create(&build_config(model)).ok_or_else(|| {
        FinalSubError::Validation(format!(
            "无法加载 {}，请检查模型文件是否与 sherpa-onnx 1.13.3 兼容",
            model.spec.name
        ))
    })?;
    let engine = Arc::new(created);
    let mut locked = cache
        .lock()
        .map_err(|_| FinalSubError::Validation("TTS 引擎缓存不可用".into()))?;
    if locked.len() >= 2 {
        if let Some(first) = locked.keys().next().cloned() {
            locked.remove(&first);
        }
    }
    Ok(locked.entry(key).or_insert_with(|| engine.clone()).clone())
}

fn output_paths(output_path: &str) -> Result<(PathBuf, PathBuf)> {
    let output = PathBuf::from(output_path.trim());
    if output.as_os_str().is_empty() || !output.is_absolute() {
        return Err(FinalSubError::Validation("配音输出必须使用绝对路径".into()));
    }
    if output.extension().and_then(|value| value.to_str()) != Some("wav") {
        return Err(FinalSubError::Validation(
            "本地配音输出必须是 .wav 文件".into(),
        ));
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = output.with_extension("wav.generating");
    Ok((output, temporary))
}

fn generation_config(
    model: &ReadyTtsModel,
    request: &LocalTtsSynthesisRequest,
    speed: f32,
) -> Result<GenerationConfig> {
    let mut config = GenerationConfig {
        speed,
        sid: resolve_sid(model, request.voice_id.as_deref()),
        num_steps: request.num_steps.unwrap_or(4).clamp(1, 20),
        ..Default::default()
    };
    if model.spec.family == TtsModelFamily::Zipvoice {
        let reference_path = request
            .reference_audio_path
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| FinalSubError::Validation("ZipVoice 需要参考音频".into()))?;
        let reference_text = request
            .reference_text
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| FinalSubError::Validation("ZipVoice 需要逐字核对的参考文本".into()))?;
        if reference_text.len() > MAX_REFERENCE_TEXT_BYTES || reference_text.contains('\0') {
            return Err(FinalSubError::Validation(format!(
                "ZipVoice 参考文本不能包含空字符，且不能超过 {MAX_REFERENCE_TEXT_BYTES} 字节"
            )));
        }
        let reference_path = PathBuf::from(reference_path.trim());
        if !reference_path.is_absolute() || !reference_path.is_file() {
            return Err(FinalSubError::Validation(
                "ZipVoice 参考音频必须是存在的绝对路径".into(),
            ));
        }
        if !reference_path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("wav"))
        {
            return Err(FinalSubError::Validation(
                "ZipVoice 参考音频必须是 WAV 文件".into(),
            ));
        }
        if std::fs::metadata(&reference_path)?.len() > MAX_REFERENCE_AUDIO_BYTES {
            return Err(FinalSubError::Validation(
                "ZipVoice 参考音频不能超过 64 MB".into(),
            ));
        }
        let wave = Wave::read(&path_string(&reference_path)).ok_or_else(|| {
            FinalSubError::Validation(format!(
                "无法读取 ZipVoice 参考音频：{}",
                reference_path.display()
            ))
        })?;
        if wave.samples().is_empty() || wave.sample_rate() <= 0 {
            return Err(FinalSubError::Validation(
                "ZipVoice 参考音频为空或格式不受支持".into(),
            ));
        }
        let duration_ms =
            (wave.samples().len() as u128 * 1_000 / wave.sample_rate() as u128) as u64;
        if duration_ms > MAX_REFERENCE_DURATION_MS {
            return Err(FinalSubError::Validation(
                "ZipVoice 参考音频不能超过 30 秒，请先裁剪有效人声片段".into(),
            ));
        }
        config.reference_audio = Some(wave.samples().to_vec());
        config.reference_sample_rate = wave.sample_rate();
        config.reference_text = Some(reference_text.to_string());
    }
    Ok(config)
}

pub(crate) fn synthesize_local(
    cache: &TtsEngineCache,
    model: ReadyTtsModel,
    request: LocalTtsSynthesisRequest,
    cancelled: Arc<AtomicBool>,
) -> Result<TtsSynthesisResult> {
    let text = validate_text(&request.text)?;
    let speed = validate_speed(request.speed)?;
    let (output, temporary) = output_paths(&request.output_path)?;
    let config = generation_config(&model, &request, speed)?;
    if cancelled.load(Ordering::Relaxed) {
        return Err(FinalSubError::Validation("配音已取消".into()));
    }
    let engine = load_engine(cache, &model)?;
    let callback_cancelled = cancelled.clone();
    let audio = engine
        .generate_with_config(
            &text,
            &config,
            Some(move |_samples: &[f32], _progress: f32| {
                !callback_cancelled.load(Ordering::Relaxed)
            }),
        )
        .ok_or_else(|| FinalSubError::Validation("本地 TTS 合成失败".into()))?;
    if cancelled.load(Ordering::Relaxed) {
        let _ = std::fs::remove_file(&temporary);
        return Err(FinalSubError::Validation("配音已取消".into()));
    }
    if audio.samples().is_empty() || audio.sample_rate() <= 0 {
        return Err(FinalSubError::Validation("本地 TTS 返回了空音频".into()));
    }
    if !audio.save(&path_string(&temporary)) {
        let _ = std::fs::remove_file(&temporary);
        return Err(FinalSubError::Validation("写入配音 WAV 失败".into()));
    }
    // 临时文件与目标位于同一目录；macOS 的 rename 会原子替换旧产物。
    std::fs::rename(&temporary, &output)?;
    let sample_rate = audio.sample_rate() as u32;
    let duration_ms = ((audio.samples().len() as u128 * 1000) / sample_rate as u128) as u64;
    Ok(TtsSynthesisResult {
        output_path: path_string(&output),
        sample_rate,
        duration_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesis_request_guards_text_and_speed() {
        assert!(validate_text("  ").is_err());
        assert!(validate_text("hello\0world").is_err());
        assert_eq!(validate_text("  hello  ").unwrap(), "hello");
        assert!(validate_speed(Some(0.2)).is_err());
        assert!(validate_speed(Some(f32::NAN)).is_err());
        assert_eq!(validate_speed(Some(1.25)).unwrap(), 1.25);
    }

    #[test]
    fn output_must_be_absolute_wav() {
        assert!(output_paths("relative.wav").is_err());
        assert!(output_paths("/tmp/demo.mp3").is_err());
        let (output, temporary) = output_paths("/tmp/finalsub-tts-test.wav").unwrap();
        assert_eq!(output, PathBuf::from("/tmp/finalsub-tts-test.wav"));
        assert_eq!(
            temporary,
            PathBuf::from("/tmp/finalsub-tts-test.wav.generating")
        );
    }
}
