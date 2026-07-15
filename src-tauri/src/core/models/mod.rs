use serde::{Deserialize, Serialize};
use std::path::Path;

pub mod download;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelStatus {
    Available,
    Downloading,
    Downloaded,
    NotReady,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrModelInfo {
    pub id: String,
    pub engine_id: String,
    pub name: String,
    pub description: String,
    pub languages: Vec<String>,
    pub best_for: String,
    pub size_mb: Option<u64>,
    pub download_url: Option<String>,
    pub status: ModelStatus,
}

pub fn builtin_model_catalog() -> Vec<AsrModelInfo> {
    vec![
        AsrModelInfo {
            id: "large-v3-turbo".into(),
            engine_id: "whisper-cpp".into(),
            name: "Whisper Large V3 Turbo".into(),
            description: "速度和精度平衡较好的通用多语言模型".into(),
            languages: vec![
                "en".into(),
                "zh".into(),
                "ja".into(),
                "ko".into(),
                "auto".into(),
            ],
            best_for: "general-multilingual".into(),
            size_mb: Some(1500),
            download_url: Some(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin"
                    .into(),
            ),
            status: ModelStatus::NotReady,
        },
        AsrModelInfo {
            id: "large-v3".into(),
            engine_id: "whisper-cpp".into(),
            name: "Whisper Large V3".into(),
            description: "高精度多语言 Whisper 模型，推荐用于中文等高精度场景（推荐·中文）".into(),
            languages: vec![
                "en".into(),
                "zh".into(),
                "ja".into(),
                "ko".into(),
                "auto".into(),
            ],
            best_for: "high-accuracy-multilingual".into(),
            size_mb: Some(3100),
            download_url: Some(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin"
                    .into(),
            ),
            status: ModelStatus::NotReady,
        },
        AsrModelInfo {
            id: "medium".into(),
            engine_id: "whisper-cpp".into(),
            name: "Whisper Medium".into(),
            description: "中等体积，速度快于 large，适合平衡场景".into(),
            languages: vec!["en".into(), "zh".into(), "auto".into()],
            best_for: "balanced".into(),
            size_mb: Some(1500),
            download_url: Some(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin".into(),
            ),
            status: ModelStatus::NotReady,
        },
        AsrModelInfo {
            id: "small".into(),
            engine_id: "whisper-cpp".into(),
            name: "Whisper Small".into(),
            description: "速度较快，占用较低，精度低于大模型".into(),
            languages: vec!["en".into(), "auto".into()],
            best_for: "fast-low-memory".into(),
            size_mb: Some(500),
            download_url: Some(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin".into(),
            ),
            status: ModelStatus::NotReady,
        },
        AsrModelInfo {
            id: "parakeet-tdt-0.6b-v2".into(),
            engine_id: "parakeet-mlx".into(),
            name: "Parakeet TDT 0.6B V2 (Native)".into(),
            description: "英文识别优化，内置 sherpa-onnx 原生运行时，无需 Python 或 uv".into(),
            languages: vec!["en".into()],
            best_for: "english-fast".into(),
            size_mb: Some(650),
            download_url: Some(
                "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8.tar.bz2".into(),
            ),
            status: ModelStatus::Available,
        },
        AsrModelInfo {
            id: crate::core::asr::sensevoice::SENSEVOICE_MODEL_ID.into(),
            engine_id: "sensevoice".into(),
            name: "SenseVoice Small".into(),
            description: "中英日韩粤多语言识别，原生 sherpa-onnx int8 运行时".into(),
            languages: vec![
                "zh".into(),
                "yue".into(),
                "en".into(),
                "ja".into(),
                "ko".into(),
            ],
            best_for: "chinese-cantonese".into(),
            size_mb: Some(158),
            download_url: Some(
                "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09.tar.bz2".into(),
            ),
            status: ModelStatus::Available,
        },
        AsrModelInfo {
            id: crate::core::asr::sherpa_native::PARAFORMER_MODEL_ID.into(),
            engine_id: "paraformer".into(),
            name: "Paraformer Zh Int8".into(),
            description: "中文与川渝方言识别优化，Silero VAD 长音频分段，原生 sherpa-onnx 运行".into(),
            languages: vec!["zh".into(), "四川话".into(), "重庆话".into()],
            best_for: "chinese-dialects-fast".into(),
            size_mb: Some(218),
            download_url: Some(
                "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-paraformer-zh-int8-2025-10-07.tar.bz2".into(),
            ),
            status: ModelStatus::Available,
        },
        AsrModelInfo {
            id: crate::core::asr::sherpa_native::QWEN3_MODEL_ID.into(),
            engine_id: "qwen3-asr".into(),
            name: "Qwen3-ASR 0.6B Int8".into(),
            description: "30 种语言、多种中文方言与歌声识别，Silero VAD 分段，原生 sherpa-onnx 运行".into(),
            languages: vec![
                "zh".into(),
                "en".into(),
                "yue".into(),
                "ja".into(),
                "ko".into(),
                "30 languages".into(),
            ],
            best_for: "multilingual-dialects-music".into(),
            size_mb: Some(838),
            download_url: Some(
                "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25.tar.bz2".into(),
            ),
            status: ModelStatus::Available,
        },
        AsrModelInfo {
            id: crate::core::asr::sherpa_native::FIRERED_MODEL_ID.into(),
            engine_id: "firered-asr".into(),
            name: "FireRedASR2 CTC Int8".into(),
            description: "中英识别与 20 余种中文方言/口音优化，体积小于 AED 版，支持 VAD 长音频".into(),
            languages: vec!["zh".into(), "en".into(), "yue".into(), "20+ dialects".into()],
            best_for: "chinese-english-dialects".into(),
            size_mb: Some(496),
            download_url: Some(
                "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-fire-red-asr2-ctc-zh_en-int8-2026-02-25.tar.bz2".into(),
            ),
            status: ModelStatus::Available,
        },
        AsrModelInfo {
            id: crate::core::asr::cloud::CLOUD_ASR_MODEL_ID.into(),
            engine_id: crate::core::asr::cloud::CLOUD_ASR_ENGINE_ID.into(),
            name: "Cloud ASR".into(),
            description:
                "OpenAI-compatible speech-to-text endpoint with explicit media upload consent"
                    .into(),
            languages: vec!["auto".into(), "multilingual".into()],
            best_for: "managed-cloud-accuracy".into(),
            size_mb: None,
            download_url: None,
            status: ModelStatus::NotReady,
        },
        AsrModelInfo {
            id: "custom-command".into(),
            engine_id: "custom-command".into(),
            name: "Custom Command".into(),
            description: "自定义识别 CLI 命令行（可在设置中配置）".into(),
            languages: vec!["any".into()],
            best_for: "advanced-users".into(),
            size_mb: None,
            download_url: None,
            status: ModelStatus::NotReady,
        },
    ]
}

pub fn whisper_model_path(models_dir: &Path, model_id: &str) -> std::path::PathBuf {
    models_dir.join(whisper_model_file_name(model_id))
}

pub fn whisper_model_file_name(model_id: &str) -> String {
    format!("ggml-{}.bin", normalize_whisper_model_id(model_id))
}

pub fn normalize_whisper_model_id(model_id: &str) -> String {
    model_id
        .trim()
        .strip_prefix("whisper-")
        .unwrap_or_else(|| model_id.trim())
        .to_string()
}

pub(crate) fn validate_whisper_model_id(model_id: &str) -> crate::error::Result<String> {
    let normalized = normalize_whisper_model_id(model_id);
    let valid = !normalized.is_empty()
        && normalized
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !valid {
        return Err(crate::error::FinalSubError::Validation(format!(
            "模型 ID 格式异常，拒绝操作：{model_id}"
        )));
    }
    Ok(normalized)
}

pub fn scan_model_status(catalog: &mut [AsrModelInfo], whisper_dir: &Path, parakeet_dir: &Path) {
    for model in catalog.iter_mut() {
        match model.engine_id.as_str() {
            "whisper-cpp" => {
                let path = whisper_model_path(whisper_dir, &model.id);
                if path.exists() {
                    model.status = ModelStatus::Downloaded;
                } else {
                    model.status = ModelStatus::Available;
                }
            }
            "parakeet-mlx" => {
                let path = parakeet_dir.join(&model.id);
                if crate::core::asr::parakeet::ParakeetNativeEngine::is_model_installed_at(&path) {
                    model.status = ModelStatus::Downloaded;
                } else {
                    model.status = ModelStatus::Available;
                }
            }
            "sensevoice" => {
                let path = whisper_dir.join(crate::core::asr::sensevoice::SENSEVOICE_MODEL_ID);
                if crate::core::asr::sensevoice::SenseVoiceEngine::is_model_installed_at(&path) {
                    model.status = ModelStatus::Downloaded;
                } else {
                    model.status = ModelStatus::Available;
                }
            }
            "paraformer" | "qwen3-asr" | "firered-asr" => {
                let kind = match model.engine_id.as_str() {
                    "paraformer" => crate::core::asr::sherpa_native::SherpaNativeKind::Paraformer,
                    "qwen3-asr" => crate::core::asr::sherpa_native::SherpaNativeKind::Qwen3,
                    _ => crate::core::asr::sherpa_native::SherpaNativeKind::FireRedCtc,
                };
                let path = whisper_dir.join(&model.id);
                if crate::core::asr::sherpa_native::SherpaNativeEngine::is_model_installed_at(
                    kind, &path,
                ) {
                    model.status = ModelStatus::Downloaded;
                } else {
                    model.status = ModelStatus::Available;
                }
            }
            "custom-command" => {
                model.status = ModelStatus::Downloaded;
            }
            _ => {}
        }
    }
}

pub fn delete_whisper_model(models_dir: &Path, model_id: &str) -> crate::error::Result<()> {
    let normalized = validate_whisper_model_id(model_id)?;
    let path = whisper_model_path(models_dir, &normalized);
    if !path.exists() {
        return Err(crate::error::FinalSubError::Validation(format!(
            "模型文件不存在：{}",
            path.display()
        )));
    }
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !file_name.starts_with("ggml-") || !file_name.ends_with(".bin") {
        return Err(crate::error::FinalSubError::Validation(
            "模型文件名格式异常，拒绝删除".into(),
        ));
    }
    std::fs::remove_file(&path)?;
    Ok(())
}

pub fn delete_managed_model(models_dir: &Path, model_id: &str) -> crate::error::Result<()> {
    let normalized = validate_whisper_model_id(model_id)?;
    let model = builtin_model_catalog()
        .into_iter()
        .find(|model| model.id == normalized)
        .ok_or_else(|| {
            crate::error::FinalSubError::Validation(format!("未知模型 ID：{normalized}"))
        })?;
    match model.engine_id.as_str() {
        "whisper-cpp" => delete_whisper_model(models_dir, &normalized),
        "parakeet-mlx" | "sensevoice" | "paraformer" | "qwen3-asr" | "firered-asr" => {
            let path = models_dir.join(&normalized);
            if !path.exists() {
                return Err(crate::error::FinalSubError::Validation(format!(
                    "模型目录不存在：{}",
                    path.display()
                )));
            }
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                std::fs::remove_file(path)?;
            } else if metadata.is_dir() {
                std::fs::remove_dir_all(path)?;
            } else {
                std::fs::remove_file(path)?;
            }
            Ok(())
        }
        _ => Err(crate::error::FinalSubError::Validation(format!(
            "模型 {normalized} 不由 FinalSub 管理，不能从此处删除"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn catalog_has_eleven_models() {
        assert_eq!(builtin_model_catalog().len(), 11);
    }

    #[test]
    fn scan_detects_downloaded_model() {
        let tmp = TempDir::new().unwrap();
        let whisper_dir = tmp.path().join("whisper");
        std::fs::create_dir_all(&whisper_dir).unwrap();
        std::fs::write(whisper_dir.join("ggml-small.bin"), b"fake").unwrap();

        let mut catalog = builtin_model_catalog();
        scan_model_status(&mut catalog, &whisper_dir, &tmp.path().join("parakeet"));

        let small = catalog.iter().find(|m| m.id == "small").unwrap();
        assert!(matches!(small.status, ModelStatus::Downloaded));

        let large = catalog.iter().find(|m| m.id == "large-v3").unwrap();
        assert!(matches!(large.status, ModelStatus::Available));
    }

    #[test]
    fn whisper_model_path_uses_ggml_file_names() {
        let dir = std::path::PathBuf::from("/models");
        assert_eq!(
            whisper_model_path(&dir, "large-v3-turbo"),
            dir.join("ggml-large-v3-turbo.bin")
        );
        assert_eq!(
            whisper_model_path(&dir, "whisper-small"),
            dir.join("ggml-small.bin")
        );
    }

    #[test]
    fn scan_parakeet_native_model_requires_all_files() {
        let tmp = TempDir::new().unwrap();
        let parakeet_dir = tmp.path().join("parakeet");
        std::fs::create_dir_all(&parakeet_dir).unwrap();
        let model_dir = parakeet_dir.join("parakeet-tdt-0.6b-v2");
        std::fs::create_dir_all(&model_dir).unwrap();
        for name in [
            "encoder.int8.onnx",
            "decoder.int8.onnx",
            "joiner.int8.onnx",
            "tokens.txt",
        ] {
            std::fs::write(model_dir.join(name), b"test").unwrap();
        }

        let mut catalog = builtin_model_catalog();
        scan_model_status(&mut catalog, &tmp.path().join("whisper"), &parakeet_dir);

        let parakeet = catalog
            .iter()
            .find(|m| m.id == "parakeet-tdt-0.6b-v2")
            .unwrap();
        assert!(matches!(parakeet.status, ModelStatus::Downloaded));
    }

    #[test]
    fn delete_model_removes_file() {
        let tmp = TempDir::new().unwrap();
        let models_dir = tmp.path();
        std::fs::write(models_dir.join("ggml-test.bin"), b"fake").unwrap();

        delete_whisper_model(models_dir, "test").unwrap();
        assert!(!models_dir.join("ggml-test.bin").exists());
    }

    #[test]
    fn delete_model_accepts_legacy_prefixed_id() {
        let tmp = TempDir::new().unwrap();
        let models_dir = tmp.path();
        std::fs::write(models_dir.join("ggml-small.bin"), b"fake").unwrap();

        delete_whisper_model(models_dir, "whisper-small").unwrap();
        assert!(!models_dir.join("ggml-small.bin").exists());
    }

    #[test]
    fn delete_model_rejects_missing() {
        let tmp = TempDir::new().unwrap();
        let result = delete_whisper_model(tmp.path(), "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn delete_model_rejects_path_escape() {
        let tmp = TempDir::new().unwrap();
        let result = delete_whisper_model(tmp.path(), "../../small");
        assert!(result.is_err());
    }
}
