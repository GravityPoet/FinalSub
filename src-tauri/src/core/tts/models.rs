use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::error::{FinalSubError, Result};

const REGISTRY_VERSION: u32 = 1;
const MAX_SCAN_DEPTH: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TtsModelFamily {
    Kokoro,
    Vits,
    Zipvoice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TtsModelLocation {
    Managed,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TtsModelStatus {
    Ready,
    NotInstalled,
    Incomplete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TtsVoice {
    pub id: String,
    pub sid: i32,
    pub label: String,
    pub label_en: String,
    pub language: String,
    pub gender: String,
}

#[derive(Debug, Clone)]
pub struct TtsModelSpec {
    pub id: &'static str,
    pub family: TtsModelFamily,
    pub name: &'static str,
    pub description: &'static str,
    pub languages: &'static [&'static str],
    pub size_mb: u64,
    pub archive_name: &'static str,
    pub archive_inner_dir: &'static str,
    pub download_url: &'static str,
    pub archive_size: u64,
    pub archive_sha256: &'static str,
    pub(crate) extra_files: &'static [TtsDownloadFileSpec],
    pub required_files: &'static [&'static str],
    pub sample_rate: u32,
    pub default_voice_id: &'static str,
    pub clone_only: bool,
    pub voices: Vec<TtsVoice>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TtsDownloadFileSpec {
    pub file_name: &'static str,
    pub download_url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

const ZIPVOICE_EXTRA_FILES: &[TtsDownloadFileSpec] = &[TtsDownloadFileSpec {
    file_name: "vocos_24khz.onnx",
    download_url:
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/vocoder-models/vocos_24khz.onnx",
    size: 54_157_409,
    sha256: "bcb3b970e384161c4d634f0bb9e999ff1c471b34c9bc0b1049a5014065ed3cc0",
}];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsModelInfo {
    pub id: String,
    pub family: TtsModelFamily,
    pub name: String,
    pub description: String,
    pub languages: Vec<String>,
    pub size_mb: u64,
    pub download_url: String,
    pub extra_download_urls: Vec<String>,
    pub sample_rate: u32,
    pub default_voice_id: String,
    pub clone_only: bool,
    pub voices: Vec<TtsVoice>,
    pub status: TtsModelStatus,
    pub path: Option<String>,
    pub location: Option<TtsModelLocation>,
    pub missing_files: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ReadyTtsModel {
    pub spec: TtsModelSpec,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct TtsModelRegistry {
    version: u32,
    models_root: String,
    external_paths: HashMap<String, String>,
}

impl Default for TtsModelRegistry {
    fn default() -> Self {
        Self {
            version: REGISTRY_VERSION,
            models_root: default_models_root().to_string_lossy().to_string(),
            external_paths: HashMap::new(),
        }
    }
}

fn default_models_root() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Tools/Local-LLM/tts-models")
}

pub fn resolved_models_root(
    app_config_dir: &Path,
) -> Result<crate::core::settings::ResolvedStoragePath> {
    let registry = load_registry(app_config_dir)?;
    let settings = crate::core::settings::load_settings(app_config_dir)?;
    Ok(crate::core::settings::resolve_model_storage_path(
        &registry.models_root,
        &default_models_root().to_string_lossy(),
        &settings.storage_root,
        "tts-models",
    ))
}

fn registry_path(app_config_dir: &Path) -> PathBuf {
    app_config_dir.join("tts").join("models.json")
}

fn load_registry(app_config_dir: &Path) -> Result<TtsModelRegistry> {
    let path = registry_path(app_config_dir);
    if !path.exists() {
        return Ok(TtsModelRegistry::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let registry: TtsModelRegistry = serde_json::from_str(&content)?;
    if registry.version != REGISTRY_VERSION {
        return Err(FinalSubError::Validation(format!(
            "不支持的 TTS 模型注册表版本：{}",
            registry.version
        )));
    }
    validate_root(&registry.models_root)?;
    Ok(registry)
}

fn save_registry(app_config_dir: &Path, registry: &TtsModelRegistry) -> Result<()> {
    let path = registry_path(app_config_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(registry)?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, content)?;
    std::fs::rename(&temporary, &path)?;
    Ok(())
}

fn validate_root(value: &str) -> Result<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().any(|ch| ch == '\0') {
        return Err(FinalSubError::Validation(
            "TTS 模型目录不能为空或包含非法字符".into(),
        ));
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        return Err(FinalSubError::Validation(
            "TTS 模型目录必须使用绝对路径".into(),
        ));
    }
    Ok(path)
}

fn kokoro_voices() -> Vec<TtsVoice> {
    let mut voices = vec![
        TtsVoice {
            id: "0".into(),
            sid: 0,
            label: "英文女声 Maple（美）".into(),
            label_en: "English Female Maple (US)".into(),
            language: "en".into(),
            gender: "female".into(),
        },
        TtsVoice {
            id: "1".into(),
            sid: 1,
            label: "英文女声 Sol（美）".into(),
            label_en: "English Female Sol (US)".into(),
            language: "en".into(),
            gender: "female".into(),
        },
        TtsVoice {
            id: "2".into(),
            sid: 2,
            label: "英文女声 Vale（英）".into(),
            label_en: "English Female Vale (UK)".into(),
            language: "en".into(),
            gender: "female".into(),
        },
    ];
    for sid in 3..=57 {
        voices.push(TtsVoice {
            id: sid.to_string(),
            sid,
            label: format!("中文女声 {:02}", sid - 2),
            label_en: format!("Chinese Female {:02}", sid - 2),
            language: "zh".into(),
            gender: "female".into(),
        });
    }
    for sid in 58..=102 {
        voices.push(TtsVoice {
            id: sid.to_string(),
            sid,
            label: format!("中文男声 {:02}", sid - 57),
            label_en: format!("Chinese Male {:02}", sid - 57),
            language: "zh".into(),
            gender: "male".into(),
        });
    }
    voices
}

fn aishell3_voices() -> Vec<TtsVoice> {
    (0..174)
        .map(|sid| TtsVoice {
            id: sid.to_string(),
            sid,
            label: format!("中文说话人 {:03}", sid + 1),
            label_en: format!("Chinese Speaker {:03}", sid + 1),
            language: "zh".into(),
            gender: "unknown".into(),
        })
        .collect()
}

pub(crate) fn catalog() -> Vec<TtsModelSpec> {
    vec![
        TtsModelSpec {
            id: "kokoro-multi-lang-v1_1",
            family: TtsModelFamily::Kokoro,
            name: "Kokoro 多语 v1.1",
            description: "中英双语、103 个内置音色，原生 sherpa-onnx 离线合成",
            languages: &["zh", "en"],
            size_mb: 217,
            archive_name: "kokoro-int8-multi-lang-v1_1.tar.bz2",
            archive_inner_dir: "kokoro-int8-multi-lang-v1_1",
            download_url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-int8-multi-lang-v1_1.tar.bz2",
            archive_size: 147_031_220,
            archive_sha256: "a1e94694776049035c4f2c6529f003aaece993c76aae9a78995831c3c4dcafc6",
            extra_files: &[],
            required_files: &[
                "model.int8.onnx",
                "voices.bin",
                "tokens.txt",
                "lexicon-us-en.txt",
                "lexicon-zh.txt",
                "espeak-ng-data/phontab",
            ],
            sample_rate: 24_000,
            default_voice_id: "10",
            clone_only: false,
            voices: kokoro_voices(),
        },
        TtsModelSpec {
            id: "vits-zh-aishell3",
            family: TtsModelFamily::Vits,
            name: "VITS 中文 AIShell3",
            description: "174 个中文说话人，原生 sherpa-onnx 离线合成",
            languages: &["zh"],
            size_mb: 227,
            archive_name: "vits-icefall-zh-aishell3.tar.bz2",
            archive_inner_dir: "vits-icefall-zh-aishell3",
            download_url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/vits-icefall-zh-aishell3.tar.bz2",
            archive_size: 31_559_701,
            archive_sha256: "ab468db3a3308cdd861495e0db2f25d79418a0c00639f74944c7cdf5dd8c6ec1",
            extra_files: &[],
            required_files: &["model.onnx", "tokens.txt", "lexicon.txt"],
            sample_rate: 8_000,
            default_voice_id: "0",
            clone_only: false,
            voices: aishell3_voices(),
        },
        TtsModelSpec {
            id: "zipvoice-distill-zh-en",
            family: TtsModelFamily::Zipvoice,
            name: "ZipVoice 中英声音克隆",
            description: "本地零样本声音克隆，无内置音色；参考音频不会离开设备",
            languages: &["zh", "en"],
            size_mb: 217,
            archive_name: "sherpa-onnx-zipvoice-distill-int8-zh-en-emilia.tar.bz2",
            archive_inner_dir: "sherpa-onnx-zipvoice-distill-int8-zh-en-emilia",
            download_url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/sherpa-onnx-zipvoice-distill-int8-zh-en-emilia.tar.bz2",
            archive_size: 109_162_785,
            archive_sha256: "77219c8b40f4ee8d73a7f902305ff6c1128ef9b54461c41b4ca6ed890b6c2803",
            extra_files: ZIPVOICE_EXTRA_FILES,
            required_files: &[
                "encoder.int8.onnx",
                "decoder.int8.onnx",
                "tokens.txt",
                "lexicon.txt",
                "espeak-ng-data/phontab",
                "vocos_24khz.onnx",
            ],
            sample_rate: 24_000,
            default_voice_id: "",
            clone_only: true,
            voices: Vec::new(),
        },
    ]
}

pub(crate) fn find_spec(model_id: &str) -> Result<TtsModelSpec> {
    catalog()
        .into_iter()
        .find(|spec| spec.id == model_id)
        .ok_or_else(|| FinalSubError::Validation(format!("未知 TTS 模型：{model_id}")))
}

pub(crate) fn missing_files(spec: &TtsModelSpec, path: &Path) -> Vec<String> {
    spec.required_files
        .iter()
        .filter(|relative| {
            let candidate = path.join(relative);
            !candidate.is_file()
                || std::fs::metadata(candidate)
                    .map(|metadata| metadata.len() == 0)
                    .unwrap_or(true)
        })
        .map(|relative| (*relative).to_string())
        .collect()
}

fn location_for(path: &Path, models_root: &Path) -> TtsModelLocation {
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let canonical_root = models_root
        .canonicalize()
        .unwrap_or_else(|_| models_root.to_path_buf());
    if canonical_path.starts_with(canonical_root) {
        TtsModelLocation::Managed
    } else {
        TtsModelLocation::External
    }
}

fn candidate_names(spec: &TtsModelSpec) -> [&str; 3] {
    [spec.id, spec.archive_inner_dir, spec.archive_name]
}

fn common_search_roots(models_root: &Path) -> Vec<PathBuf> {
    let mut roots = vec![models_root.to_path_buf()];
    if let Some(parent) = models_root.parent() {
        roots.push(parent.to_path_buf());
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        roots.push(home.join("Tools/Local-LLM"));
    }
    let mut seen = HashSet::new();
    roots
        .into_iter()
        .filter(|root| seen.insert(root.clone()))
        .collect()
}

fn should_skip_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git" | "node_modules" | "target" | "build" | "huggingface" | "whisper.cpp")
    )
}

fn discover_model_path(spec: &TtsModelSpec, models_root: &Path) -> Option<PathBuf> {
    let names = candidate_names(spec);
    for root in common_search_roots(models_root) {
        for name in names.iter().take(2) {
            let candidate = root.join(name);
            if missing_files(spec, &candidate).is_empty() {
                return candidate.canonicalize().ok().or(Some(candidate));
            }
        }
    }

    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();
    for root in common_search_roots(models_root) {
        if root.is_dir() {
            queue.push_back((root, 0_usize));
        }
    }
    while let Some((directory, depth)) = queue.pop_front() {
        let canonical = directory
            .canonicalize()
            .unwrap_or_else(|_| directory.clone());
        if !visited.insert(canonical) || should_skip_dir(&directory) {
            continue;
        }
        if missing_files(spec, &directory).is_empty() {
            return directory.canonicalize().ok().or(Some(directory));
        }
        if depth >= MAX_SCAN_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && !should_skip_dir(&path) {
                queue.push_back((path, depth + 1));
            }
        }
    }
    None
}

fn candidate_from_registry(registry: &TtsModelRegistry, spec: &TtsModelSpec) -> Option<PathBuf> {
    registry
        .external_paths
        .get(spec.id)
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
}

fn model_info(spec: TtsModelSpec, registry: &TtsModelRegistry, models_root: &Path) -> TtsModelInfo {
    let registered = candidate_from_registry(registry, &spec);
    let discovered = registered
        .clone()
        .or_else(|| discover_model_path(&spec, models_root));
    let configured_candidate = models_root.join(spec.id);
    let inspected = discovered.clone().or_else(|| {
        configured_candidate
            .exists()
            .then_some(configured_candidate.clone())
    });
    let missing = inspected
        .as_deref()
        .map(|path| missing_files(&spec, path))
        .unwrap_or_else(|| {
            spec.required_files
                .iter()
                .map(|item| (*item).to_string())
                .collect()
        });
    let status = if discovered.is_some() && missing.is_empty() {
        TtsModelStatus::Ready
    } else if inspected.is_some() {
        TtsModelStatus::Incomplete
    } else {
        TtsModelStatus::NotInstalled
    };
    let location = discovered
        .as_deref()
        .map(|path| location_for(path, models_root));

    TtsModelInfo {
        id: spec.id.into(),
        family: spec.family,
        name: spec.name.into(),
        description: spec.description.into(),
        languages: spec.languages.iter().map(|value| (*value).into()).collect(),
        size_mb: spec.size_mb,
        download_url: spec.download_url.into(),
        extra_download_urls: spec
            .extra_files
            .iter()
            .map(|file| file.download_url.into())
            .collect(),
        sample_rate: spec.sample_rate,
        default_voice_id: spec.default_voice_id.into(),
        clone_only: spec.clone_only,
        voices: spec.voices,
        status,
        path: discovered.map(|path| path.to_string_lossy().to_string()),
        location,
        missing_files: missing,
    }
}

pub fn list_models(app_config_dir: &Path) -> Result<Vec<TtsModelInfo>> {
    let registry = load_registry(app_config_dir)?;
    let models_root = PathBuf::from(resolved_models_root(app_config_dir)?.path);
    Ok(catalog()
        .into_iter()
        .map(|spec| model_info(spec, &registry, &models_root))
        .collect())
}

pub fn register_external_model(
    app_config_dir: &Path,
    model_id: &str,
    source_path: &str,
) -> Result<TtsModelInfo> {
    let spec = find_spec(model_id)?;
    let source = validate_root(source_path)?;
    if !source.is_dir() {
        return Err(FinalSubError::Validation(format!(
            "TTS 模型目录不存在：{}",
            source.display()
        )));
    }
    let missing = missing_files(&spec, &source);
    if !missing.is_empty() {
        return Err(FinalSubError::Validation(format!(
            "目录不是完整的 {} 模型，缺少：{}",
            spec.name,
            missing.join("、")
        )));
    }
    let canonical = source.canonicalize()?;
    let mut registry = load_registry(app_config_dir)?;
    registry
        .external_paths
        .insert(spec.id.into(), canonical.to_string_lossy().to_string());
    save_registry(app_config_dir, &registry)?;
    let models_root = PathBuf::from(resolved_models_root(app_config_dir)?.path);
    Ok(model_info(spec, &registry, &models_root))
}

pub fn remove_external_registration(app_config_dir: &Path, model_id: &str) -> Result<()> {
    find_spec(model_id)?;
    let mut registry = load_registry(app_config_dir)?;
    registry.external_paths.remove(model_id);
    save_registry(app_config_dir, &registry)
}

pub fn set_models_root(app_config_dir: &Path, models_root: &str) -> Result<Vec<TtsModelInfo>> {
    let root = validate_root(models_root)?;
    std::fs::create_dir_all(&root)?;
    let canonical = root.canonicalize()?;
    let mut registry = load_registry(app_config_dir)?;
    registry.models_root = canonical.to_string_lossy().to_string();
    save_registry(app_config_dir, &registry)?;
    list_models(app_config_dir)
}

pub(crate) fn managed_models_root(app_config_dir: &Path) -> Result<PathBuf> {
    let resolved = resolved_models_root(app_config_dir)?;
    let root = validate_root(&resolved.path)?;
    std::fs::create_dir_all(&root)?;
    Ok(root.canonicalize()?)
}

pub(crate) fn finish_managed_install(app_config_dir: &Path, model_id: &str) -> Result<()> {
    find_spec(model_id)?;
    let mut registry = load_registry(app_config_dir)?;
    registry.external_paths.remove(model_id);
    save_registry(app_config_dir, &registry)
}

pub fn delete_managed_model(app_config_dir: &Path, model_id: &str) -> Result<()> {
    let spec = find_spec(model_id)?;
    let root = managed_models_root(app_config_dir)?;
    let target = root.join(spec.id);
    if !target.exists() {
        return Err(FinalSubError::Validation(format!(
            "本机没有可删除的受管 TTS 模型：{}",
            spec.name
        )));
    }
    let canonical = target.canonicalize()?;
    if canonical.parent() != Some(root.as_path()) {
        return Err(FinalSubError::Validation(
            "拒绝删除受管目录之外的 TTS 模型".into(),
        ));
    }
    std::fs::remove_dir_all(canonical)?;
    Ok(())
}

pub(crate) fn resolve_ready_model(app_config_dir: &Path, model_id: &str) -> Result<ReadyTtsModel> {
    let registry = load_registry(app_config_dir)?;
    let spec = find_spec(model_id)?;
    let models_root = PathBuf::from(resolved_models_root(app_config_dir)?.path);
    let path = candidate_from_registry(&registry, &spec)
        .filter(|path| missing_files(&spec, path).is_empty())
        .or_else(|| discover_model_path(&spec, &models_root))
        .ok_or_else(|| {
            FinalSubError::Validation(format!(
                "本机未发现可用的 {}。可直接选择已有模型目录，无需重复下载。",
                spec.name
            ))
        })?;
    let missing = missing_files(&spec, &path);
    if !missing.is_empty() {
        return Err(FinalSubError::Validation(format!(
            "{} 模型不完整，缺少：{}",
            spec.name,
            missing.join("、")
        )));
    }
    Ok(ReadyTtsModel { spec, path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn materialize(spec: &TtsModelSpec, root: &Path) {
        for relative in spec.required_files {
            let path = root.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, b"test").unwrap();
        }
    }

    #[test]
    fn catalog_exposes_local_synthesis_and_clone_families() {
        let specs = catalog();
        assert_eq!(specs.len(), 3);
        assert!(specs
            .iter()
            .any(|spec| spec.family == TtsModelFamily::Kokoro));
        assert!(specs.iter().any(|spec| spec.family == TtsModelFamily::Vits));
        assert!(specs
            .iter()
            .any(|spec| spec.family == TtsModelFamily::Zipvoice));
        assert_eq!(specs[0].voices.len(), 103);
        assert_eq!(specs[1].voices.len(), 174);
        assert_eq!(specs[2].extra_files.len(), 1);
        assert!(specs.iter().all(|spec| spec.archive_size > 0));
        assert!(specs.iter().all(|spec| spec.archive_sha256.len() == 64));
    }

    #[test]
    fn external_registration_reuses_model_without_copying_it() {
        let config = TempDir::new().unwrap();
        let external = TempDir::new().unwrap();
        let spec = find_spec("kokoro-multi-lang-v1_1").unwrap();
        materialize(&spec, external.path());

        let info =
            register_external_model(config.path(), spec.id, external.path().to_str().unwrap())
                .unwrap();

        assert_eq!(info.status, TtsModelStatus::Ready);
        assert_eq!(info.location, Some(TtsModelLocation::External));
        assert_eq!(
            info.path.as_deref(),
            external.path().canonicalize().unwrap().to_str()
        );
        assert!(!config.path().join("tts").join(spec.id).exists());
    }

    #[test]
    fn registration_rejects_incomplete_model_directory() {
        let config = TempDir::new().unwrap();
        let external = TempDir::new().unwrap();
        std::fs::write(external.path().join("model.int8.onnx"), b"test").unwrap();
        let error = register_external_model(
            config.path(),
            "kokoro-multi-lang-v1_1",
            external.path().to_str().unwrap(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("缺少"));
    }

    #[test]
    fn empty_required_model_files_are_not_ready() {
        let root = TempDir::new().unwrap();
        let spec = find_spec("vits-zh-aishell3").unwrap();
        for relative in spec.required_files {
            let path = root.path().join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::File::create(path).unwrap();
        }
        assert!(!missing_files(&spec, root.path()).is_empty());
    }

    #[test]
    fn discovery_finds_archive_named_directory_under_local_llm_root() {
        let root = TempDir::new().unwrap();
        let spec = find_spec("vits-zh-aishell3").unwrap();
        let model = root.path().join(spec.archive_inner_dir);
        materialize(&spec, &model);
        assert_eq!(
            discover_model_path(&spec, root.path()).unwrap(),
            model.canonicalize().unwrap()
        );
    }

    #[test]
    fn managed_delete_never_touches_external_registration() {
        let config = TempDir::new().unwrap();
        let managed_root = TempDir::new().unwrap();
        set_models_root(config.path(), managed_root.path().to_str().unwrap()).unwrap();
        let external = TempDir::new().unwrap();
        let spec = find_spec("vits-zh-aishell3").unwrap();
        materialize(&spec, external.path());
        register_external_model(config.path(), spec.id, external.path().to_str().unwrap()).unwrap();

        let error = delete_managed_model(config.path(), spec.id).unwrap_err();
        assert!(error.to_string().contains("没有可删除"));
        assert!(external.path().join("model.onnx").is_file());
    }

    #[test]
    fn managed_root_follows_unified_storage_until_explicitly_overridden() {
        let config = TempDir::new().unwrap();
        let unified = TempDir::new().unwrap();
        let settings = crate::core::settings::Settings {
            storage_root: unified.path().to_string_lossy().into_owned(),
            ..crate::core::settings::Settings::default()
        };
        crate::core::settings::save_settings(config.path(), &settings).unwrap();

        let followed = resolved_models_root(config.path()).unwrap();
        assert_eq!(
            followed.source,
            crate::core::settings::StoragePathSource::UnifiedRoot
        );
        assert_eq!(
            PathBuf::from(followed.path),
            unified.path().join("tts-models")
        );

        let override_root = TempDir::new().unwrap();
        set_models_root(config.path(), override_root.path().to_str().unwrap()).unwrap();
        let pinned = resolved_models_root(config.path()).unwrap();
        assert_eq!(
            pinned.source,
            crate::core::settings::StoragePathSource::Override
        );
        assert_eq!(
            PathBuf::from(pinned.path),
            override_root.path().canonicalize().unwrap()
        );
    }
}
