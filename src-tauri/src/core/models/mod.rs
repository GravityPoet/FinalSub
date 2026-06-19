use serde::{Deserialize, Serialize};
use std::path::Path;

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
            description: "高精度多语言 Whisper 模型，适合质量优先的任务".into(),
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
            name: "Parakeet TDT 0.6B V2".into(),
            description: "英文识别优化，Apple Silicon 快速路径（自动缓存）".into(),
            languages: vec!["en".into()],
            best_for: "english-fast".into(),
            size_mb: Some(600),
            download_url: None,
            status: ModelStatus::NotReady,
        },
        AsrModelInfo {
            id: "sensevoice-small".into(),
            engine_id: "sensevoice".into(),
            name: "SenseVoice Small".into(),
            description: "中文和粤语识别候选模型，等待运行时验证接入".into(),
            languages: vec![
                "zh".into(),
                "yue".into(),
                "en".into(),
                "ja".into(),
                "ko".into(),
            ],
            best_for: "chinese-cantonese".into(),
            size_mb: Some(800),
            download_url: Some("https://huggingface.co/FunAudioLLM/SenseVoiceSmall".into()),
            status: ModelStatus::NotReady,
        },
        AsrModelInfo {
            id: "custom-command".into(),
            engine_id: "custom-command".into(),
            name: "Custom Command".into(),
            description: "高级用户自定义识别命令，等待权限方案设计".into(),
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

fn validate_whisper_model_id(model_id: &str) -> crate::error::Result<String> {
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
                if path.exists() {
                    model.status = ModelStatus::Downloaded;
                } else {
                    model.status = ModelStatus::NotReady;
                }
            }
            "sensevoice" | "custom-command" => {
                model.status = ModelStatus::NotReady;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn catalog_has_seven_models() {
        assert_eq!(builtin_model_catalog().len(), 7);
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
    fn scan_parakeet_auto_cache() {
        let tmp = TempDir::new().unwrap();
        let parakeet_dir = tmp.path().join("parakeet");
        std::fs::create_dir_all(&parakeet_dir).unwrap();
        std::fs::create_dir_all(parakeet_dir.join("parakeet-tdt-0.6b-v2")).unwrap();

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
