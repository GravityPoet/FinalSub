use serde::{Deserialize, Serialize};

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
            id: "whisper-large-v3-turbo".into(),
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
            id: "whisper-large-v3".into(),
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
            id: "whisper-medium".into(),
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
            id: "whisper-small".into(),
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
            description: "英文识别优化，计划用于 Apple Silicon 快速路径".into(),
            languages: vec!["en".into()],
            best_for: "english-fast".into(),
            size_mb: Some(600),
            download_url: Some("https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2".into()),
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
