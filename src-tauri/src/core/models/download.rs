use crate::core::asr::parakeet::{PARAKEET_ARCHIVE_DIR, PARAKEET_MODEL_ID};
use crate::core::asr::sensevoice::{SENSEVOICE_ARCHIVE_DIR, SENSEVOICE_MODEL_ID};
use crate::core::asr::sherpa_native::{
    FIRERED_ARCHIVE_DIR, FIRERED_MODEL_ID, PARAFORMER_ARCHIVE_DIR, PARAFORMER_MODEL_ID,
    QWEN3_ARCHIVE_DIR, QWEN3_MODEL_ID,
};
use crate::core::models::{builtin_model_catalog, validate_whisper_model_id, whisper_model_path};
use crate::error::{FinalSubError, Result};
use reqwest::header::{CONTENT_RANGE, RANGE};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::File as StdFile;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;

const PARAKEET_ARCHIVE_SHA256: &str =
    "157c157bc51155e03e37d2466522a3a737dd9c72bb25f36eb18912964161e1ad";
const PARAKEET_ARCHIVE_SIZE: u64 = 482_468_385;
const SENSEVOICE_ARCHIVE_SHA256: &str =
    "7305f7905bfcf77fa0b39388a313f3da35c68d971661a65475b56fb2162c8e63";
const SENSEVOICE_ARCHIVE_SIZE: u64 = 165_783_878;
const PARAFORMER_ARCHIVE_SHA256: &str =
    "a071ee5419e14adb34d7f970ab98105a45e6608018b168f023ca2e4810744abe";
const PARAFORMER_ARCHIVE_SIZE: u64 = 228_262_632;
const QWEN3_ARCHIVE_SHA256: &str =
    "393f8a14e2f5fb96746aaab342997a40641001fbd5bf9592a080a8329178ee96";
const QWEN3_ARCHIVE_SIZE: u64 = 878_702_423;
const FIRERED_ARCHIVE_SHA256: &str =
    "1da8b737ecc5e29f36759a4460c754863e7c919a4ba325aea187331fbfc83274";
const FIRERED_ARCHIVE_SIZE: u64 = 520_516_278;

#[derive(Clone, Copy, Debug)]
struct ArchiveFileSpec {
    source: &'static str,
    target: &'static str,
}

#[derive(Debug)]
struct ManagedArchiveSpec {
    model_id: &'static str,
    label: &'static str,
    archive_dir: &'static str,
    sha256: &'static str,
    size: u64,
    files: &'static [ArchiveFileSpec],
}

impl ManagedArchiveSpec {
    fn is_installed_at(&self, directory: &Path) -> bool {
        self.files.iter().all(|file| {
            let path = directory.join(file.target);
            path.is_file() && std::fs::metadata(path).is_ok_and(|metadata| metadata.len() > 0)
        })
    }
}

const PARAKEET_FILES: &[ArchiveFileSpec] = &[
    ArchiveFileSpec {
        source: "encoder.int8.onnx",
        target: "encoder.int8.onnx",
    },
    ArchiveFileSpec {
        source: "decoder.int8.onnx",
        target: "decoder.int8.onnx",
    },
    ArchiveFileSpec {
        source: "joiner.int8.onnx",
        target: "joiner.int8.onnx",
    },
    ArchiveFileSpec {
        source: "tokens.txt",
        target: "tokens.txt",
    },
];
const SENSEVOICE_FILES: &[ArchiveFileSpec] = &[
    ArchiveFileSpec {
        source: "model.int8.onnx",
        target: "model.onnx",
    },
    ArchiveFileSpec {
        source: "tokens.txt",
        target: "tokens.txt",
    },
];
const PARAFORMER_FILES: &[ArchiveFileSpec] = &[
    ArchiveFileSpec {
        source: "model.int8.onnx",
        target: "model.int8.onnx",
    },
    ArchiveFileSpec {
        source: "tokens.txt",
        target: "tokens.txt",
    },
];
const QWEN3_FILES: &[ArchiveFileSpec] = &[
    ArchiveFileSpec {
        source: "conv_frontend.onnx",
        target: "conv_frontend.onnx",
    },
    ArchiveFileSpec {
        source: "encoder.int8.onnx",
        target: "encoder.int8.onnx",
    },
    ArchiveFileSpec {
        source: "decoder.int8.onnx",
        target: "decoder.int8.onnx",
    },
    ArchiveFileSpec {
        source: "tokenizer/vocab.json",
        target: "tokenizer/vocab.json",
    },
    ArchiveFileSpec {
        source: "tokenizer/merges.txt",
        target: "tokenizer/merges.txt",
    },
    ArchiveFileSpec {
        source: "tokenizer/tokenizer_config.json",
        target: "tokenizer/tokenizer_config.json",
    },
];
const FIRERED_FILES: &[ArchiveFileSpec] = &[
    ArchiveFileSpec {
        source: "model.int8.onnx",
        target: "model.int8.onnx",
    },
    ArchiveFileSpec {
        source: "tokens.txt",
        target: "tokens.txt",
    },
];

const PARAKEET_SPEC: ManagedArchiveSpec = ManagedArchiveSpec {
    model_id: PARAKEET_MODEL_ID,
    label: "Parakeet",
    archive_dir: PARAKEET_ARCHIVE_DIR,
    sha256: PARAKEET_ARCHIVE_SHA256,
    size: PARAKEET_ARCHIVE_SIZE,
    files: PARAKEET_FILES,
};
const SENSEVOICE_SPEC: ManagedArchiveSpec = ManagedArchiveSpec {
    model_id: SENSEVOICE_MODEL_ID,
    label: "SenseVoice",
    archive_dir: SENSEVOICE_ARCHIVE_DIR,
    sha256: SENSEVOICE_ARCHIVE_SHA256,
    size: SENSEVOICE_ARCHIVE_SIZE,
    files: SENSEVOICE_FILES,
};
const PARAFORMER_SPEC: ManagedArchiveSpec = ManagedArchiveSpec {
    model_id: PARAFORMER_MODEL_ID,
    label: "Paraformer",
    archive_dir: PARAFORMER_ARCHIVE_DIR,
    sha256: PARAFORMER_ARCHIVE_SHA256,
    size: PARAFORMER_ARCHIVE_SIZE,
    files: PARAFORMER_FILES,
};
const QWEN3_SPEC: ManagedArchiveSpec = ManagedArchiveSpec {
    model_id: QWEN3_MODEL_ID,
    label: "Qwen3-ASR",
    archive_dir: QWEN3_ARCHIVE_DIR,
    sha256: QWEN3_ARCHIVE_SHA256,
    size: QWEN3_ARCHIVE_SIZE,
    files: QWEN3_FILES,
};
const FIRERED_SPEC: ManagedArchiveSpec = ManagedArchiveSpec {
    model_id: FIRERED_MODEL_ID,
    label: "FireRedASR2 CTC",
    archive_dir: FIRERED_ARCHIVE_DIR,
    sha256: FIRERED_ARCHIVE_SHA256,
    size: FIRERED_ARCHIVE_SIZE,
    files: FIRERED_FILES,
};

fn managed_archive_spec(model_id: &str) -> Option<&'static ManagedArchiveSpec> {
    match model_id {
        PARAKEET_MODEL_ID => Some(&PARAKEET_SPEC),
        SENSEVOICE_MODEL_ID => Some(&SENSEVOICE_SPEC),
        PARAFORMER_MODEL_ID => Some(&PARAFORMER_SPEC),
        QWEN3_MODEL_ID => Some(&QWEN3_SPEC),
        FIRERED_MODEL_ID => Some(&FIRERED_SPEC),
        _ => None,
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct ModelDownloadProgress {
    pub model_id: String,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub progress: f32,
    pub status: String,
    pub phase: String,
    pub bytes_per_second: Option<u64>,
    pub eta_seconds: Option<u64>,
    pub error: Option<String>,
}

enum ModelArtifact {
    WhisperFile {
        final_path: PathBuf,
    },
    ManagedArchive {
        spec: &'static ManagedArchiveSpec,
        final_dir: PathBuf,
    },
}

impl ModelArtifact {
    fn is_installed(&self) -> bool {
        match self {
            Self::WhisperFile { final_path } => final_path.is_file(),
            Self::ManagedArchive { spec, final_dir } => spec.is_installed_at(final_dir),
        }
    }

    fn expected_size(&self) -> Option<u64> {
        match self {
            Self::WhisperFile { .. } => None,
            Self::ManagedArchive { spec, .. } => Some(spec.size),
        }
    }

    fn archive_spec(&self) -> Option<&'static ManagedArchiveSpec> {
        match self {
            Self::WhisperFile { .. } => None,
            Self::ManagedArchive { spec, .. } => Some(*spec),
        }
    }
}

pub async fn download_model_impl(
    app: AppHandle,
    models_dir: PathBuf,
    model_id: String,
    mut cancel_rx: watch::Receiver<bool>,
) -> Result<()> {
    let normalized = validate_whisper_model_id(&model_id)?;
    let model_info = builtin_model_catalog()
        .into_iter()
        .find(|model| model.id == normalized)
        .ok_or_else(|| FinalSubError::Validation(format!("未知模型 ID: {normalized}")))?;
    let url = model_info
        .download_url
        .ok_or_else(|| FinalSubError::Validation(format!("模型 {normalized} 暂无可用下载链接")))?;

    let artifact = if let Some(spec) = managed_archive_spec(&normalized) {
        ModelArtifact::ManagedArchive {
            spec,
            final_dir: models_dir.join(spec.model_id),
        }
    } else if model_info.engine_id == "whisper-cpp" {
        ModelArtifact::WhisperFile {
            final_path: whisper_model_path(&models_dir, &normalized),
        }
    } else {
        return Err(FinalSubError::Validation(format!(
            "模型 {} 尚未配置受管安装器",
            normalized
        )));
    };

    tokio::fs::create_dir_all(&models_dir).await?;
    if artifact.is_installed() {
        emit_download_progress(&app, &normalized, 1, 1, "done", "ready", None, None, None);
        return Ok(());
    }

    let part_path = models_dir.join(format!(".finalsub-download-{normalized}.part"));
    let expected_size = artifact.expected_size();
    let mut existing_bytes = tokio::fs::metadata(&part_path)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if expected_size.is_some_and(|expected| existing_bytes > expected) {
        tokio::fs::remove_file(&part_path).await?;
        existing_bytes = 0;
    }

    let client = reqwest::Client::builder()
        .user_agent("FinalSub-ModelManager/1.0")
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(90))
        .build()
        .map_err(|error| FinalSubError::Validation(format!("初始化模型下载客户端失败: {error}")))?;

    if expected_size.is_none_or(|expected| existing_bytes != expected) {
        let (response, append) = request_download(&client, &url, existing_bytes).await?;
        if !append {
            existing_bytes = 0;
        }
        let response_total = response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_content_range_total)
            .or_else(|| {
                response
                    .content_length()
                    .map(|length| length + existing_bytes)
            });
        let total_bytes = expected_size.or(response_total).unwrap_or(0);
        if let (Some(expected), Some(actual)) = (expected_size, response_total) {
            if expected != actual {
                return Err(FinalSubError::Validation(format!(
                    "模型下载大小与固定清单不符：期望 {expected} 字节，服务器报告 {actual} 字节"
                )));
            }
        }

        let mut response = response;
        let mut file = open_part_file(&part_path, append).await?;
        let started_at = Instant::now();
        let resume_offset = existing_bytes;
        let mut bytes_downloaded = existing_bytes;

        loop {
            if *cancel_rx.borrow() {
                file.flush().await?;
                emit_download_progress(
                    &app,
                    &normalized,
                    bytes_downloaded,
                    total_bytes,
                    "cancelled",
                    "paused",
                    None,
                    None,
                    None,
                );
                return Ok(());
            }

            let chunk = tokio::select! {
                chunk = response.chunk() => chunk.map_err(|error| {
                    FinalSubError::Validation(format!("模型下载流中断，可重试续传：{error}"))
                })?,
                changed = cancel_rx.changed() => {
                    if changed.is_err() || *cancel_rx.borrow() {
                        file.flush().await?;
                        emit_download_progress(
                            &app,
                            &normalized,
                            bytes_downloaded,
                            total_bytes,
                            "cancelled",
                            "paused",
                            None,
                            None,
                            None,
                        );
                        return Ok(());
                    }
                    continue;
                }
            };
            let Some(chunk) = chunk else { break };
            file.write_all(&chunk).await?;
            bytes_downloaded += chunk.len() as u64;

            let elapsed = started_at.elapsed().as_secs_f64().max(0.001);
            let speed = ((bytes_downloaded - resume_offset) as f64 / elapsed) as u64;
            let eta = (total_bytes > bytes_downloaded && speed > 0)
                .then_some((total_bytes - bytes_downloaded).div_ceil(speed));
            emit_download_progress(
                &app,
                &normalized,
                bytes_downloaded,
                total_bytes,
                "downloading",
                if resume_offset > 0 {
                    "resuming"
                } else {
                    "downloading"
                },
                Some(speed),
                eta,
                None,
            );
        }

        file.flush().await?;
        file.sync_all().await?;
        drop(file);
        let actual_size = tokio::fs::metadata(&part_path).await?.len();
        if total_bytes > 0 && actual_size != total_bytes {
            return Err(FinalSubError::Validation(format!(
                "模型下载不完整，可重试续传：期望 {total_bytes} 字节，当前 {actual_size} 字节"
            )));
        }
        if actual_size == 0 {
            return Err(FinalSubError::Validation("模型下载文件为空".into()));
        }
    }

    if *cancel_rx.borrow() {
        return Ok(());
    }
    let archive_size = tokio::fs::metadata(&part_path).await?.len();
    emit_download_progress(
        &app,
        &normalized,
        archive_size,
        expected_size.unwrap_or(archive_size),
        "downloading",
        "verifying",
        None,
        None,
        None,
    );

    if let Some(spec) = artifact.archive_spec() {
        let digest = sha256_file(part_path.clone()).await?;
        if digest != spec.sha256 {
            tokio::fs::remove_file(&part_path).await.ok();
            return Err(FinalSubError::Validation(format!(
                "{} 模型 SHA-256 校验失败：期望 {}，实际 {digest}；损坏文件已清理",
                spec.label, spec.sha256
            )));
        }
    }

    emit_download_progress(
        &app,
        &normalized,
        archive_size,
        expected_size.unwrap_or(archive_size),
        "downloading",
        "installing",
        None,
        None,
        None,
    );
    match artifact {
        ModelArtifact::WhisperFile { final_path } => {
            tokio::fs::rename(&part_path, &final_path)
                .await
                .map_err(|error| {
                    FinalSubError::Validation(format!("原子安装 Whisper 模型失败: {error}"))
                })?;
        }
        ModelArtifact::ManagedArchive { spec, final_dir } => {
            install_managed_archive(part_path.clone(), final_dir, spec).await?;
            tokio::fs::remove_file(&part_path).await.ok();
        }
    }

    emit_download_progress(
        &app,
        &normalized,
        archive_size,
        expected_size.unwrap_or(archive_size),
        "done",
        "ready",
        None,
        Some(0),
        None,
    );
    Ok(())
}

pub(crate) async fn request_download(
    client: &reqwest::Client,
    url: &str,
    existing_bytes: u64,
) -> Result<(reqwest::Response, bool)> {
    let mut request = client.get(url);
    if existing_bytes > 0 {
        request = request.header(RANGE, format!("bytes={existing_bytes}-"));
    }
    let response = request.send().await.map_err(|error| {
        FinalSubError::Validation(format!("发起模型下载失败，可稍后重试：{error}"))
    })?;
    let status = response.status();
    if status == reqwest::StatusCode::PARTIAL_CONTENT {
        let (range_start, _, _) = response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_content_range)
            .ok_or_else(|| {
                FinalSubError::Validation("模型服务器的断点续传响应缺少有效 Content-Range".into())
            })?;
        if range_start != existing_bytes {
            return Err(FinalSubError::Validation(format!(
                "模型服务器返回的续传起点不匹配：请求从 {existing_bytes} 字节继续，实际从 {range_start} 字节返回"
            )));
        }
        return Ok((response, existing_bytes > 0));
    }
    if status.is_success() {
        return Ok((response, false));
    }
    Err(FinalSubError::Validation(format!(
        "模型服务器返回错误状态：{status}"
    )))
}

pub(crate) async fn open_part_file(path: &Path, append: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).write(true);
    if append {
        options.append(true);
    } else {
        options.truncate(true);
    }
    Ok(options.open(path).await?)
}

fn parse_content_range_total(value: &str) -> Option<u64> {
    parse_content_range(value).map(|(_, _, total)| total)
}

fn parse_content_range(value: &str) -> Option<(u64, u64, u64)> {
    let value = value.strip_prefix("bytes ")?;
    let (range, total) = value.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let start = start.parse().ok()?;
    let end = end.parse().ok()?;
    let total = total.parse().ok()?;
    (start <= end && end < total).then_some((start, end, total))
}

pub(crate) async fn sha256_file(path: PathBuf) -> Result<String> {
    tokio::task::spawn_blocking(move || {
        let mut file = StdFile::open(&path)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0_u8; 1024 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok::<_, std::io::Error>(hex::encode(hasher.finalize()))
    })
    .await
    .map_err(|error| FinalSubError::Validation(format!("模型校验线程异常: {error}")))?
    .map_err(FinalSubError::Io)
}

async fn install_managed_archive(
    archive_path: PathBuf,
    final_dir: PathBuf,
    spec: &'static ManagedArchiveSpec,
) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        install_managed_archive_sync(&archive_path, &final_dir, spec)
    })
    .await
    .map_err(|error| FinalSubError::Validation(format!("模型安装线程异常: {error}")))?
}

fn install_managed_archive_sync(
    archive_path: &Path,
    final_dir: &Path,
    spec: &ManagedArchiveSpec,
) -> Result<()> {
    let parent = final_dir
        .parent()
        .ok_or_else(|| FinalSubError::Validation(format!("{} 安装目录缺少父目录", spec.label)))?;
    let staging = parent.join(format!(
        ".finalsub-install-{}-{}",
        spec.model_id,
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&staging)?;

    let result = (|| -> Result<()> {
        let file = StdFile::open(archive_path)?;
        let decoder = bzip2::read::BzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        let mut extracted = std::collections::HashSet::new();

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.into_owned();
            if path
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
            {
                return Err(FinalSubError::Validation(format!(
                    "{} 模型压缩包包含不安全路径：{}",
                    spec.label,
                    path.display()
                )));
            }
            if !entry.header().entry_type().is_file() {
                continue;
            }

            let Some(relative) = path.strip_prefix(Path::new(spec.archive_dir)).ok() else {
                continue;
            };
            let Some(file_spec) = spec
                .files
                .iter()
                .find(|file| relative == Path::new(file.source))
            else {
                continue;
            };
            if !extracted.insert(file_spec.target) {
                return Err(FinalSubError::Validation(format!(
                    "{} 模型压缩包包含重复文件：{}",
                    spec.label, file_spec.source
                )));
            }
            let target = staging.join(file_spec.target);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            entry.unpack(target)?;
        }

        if extracted.len() != spec.files.len() || !spec.is_installed_at(&staging) {
            let missing = spec
                .files
                .iter()
                .filter(|file| {
                    let path = staging.join(file.target);
                    !path.is_file()
                        || match std::fs::metadata(path) {
                            Ok(metadata) => metadata.len() == 0,
                            Err(_) => true,
                        }
                })
                .map(|file| file.source)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(FinalSubError::Validation(format!(
                "{} 模型压缩包缺少必要文件：{}",
                spec.label, missing
            )));
        }
        let backup = parent.join(format!(
            ".finalsub-backup-{}-{}",
            spec.model_id,
            uuid::Uuid::new_v4()
        ));
        let had_previous = final_dir.exists();
        if had_previous {
            std::fs::rename(final_dir, &backup)?;
        }
        if let Err(error) = std::fs::rename(&staging, final_dir) {
            if had_previous {
                let _ = std::fs::rename(&backup, final_dir);
            }
            return Err(error.into());
        }
        if had_previous {
            let _ = std::fs::remove_dir_all(backup);
        }
        Ok(())
    })();

    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

#[cfg(test)]
fn install_parakeet_archive_sync(archive_path: &Path, final_dir: &Path) -> Result<()> {
    install_managed_archive_sync(archive_path, final_dir, &PARAKEET_SPEC)
}

#[cfg(test)]
fn install_sensevoice_archive_sync(archive_path: &Path, final_dir: &Path) -> Result<()> {
    install_managed_archive_sync(archive_path, final_dir, &SENSEVOICE_SPEC)
}

#[allow(clippy::too_many_arguments)]
fn emit_download_progress(
    app: &AppHandle,
    model_id: &str,
    bytes_downloaded: u64,
    total_bytes: u64,
    status: &str,
    phase: &str,
    bytes_per_second: Option<u64>,
    eta_seconds: Option<u64>,
    error: Option<String>,
) {
    let progress = if total_bytes > 0 {
        (bytes_downloaded as f32 / total_bytes as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let _ = app.emit(
        "model-download-updated",
        ModelDownloadProgress {
            model_id: model_id.to_string(),
            bytes_downloaded,
            total_bytes,
            progress,
            status: status.to_string(),
            phase: phase.to_string(),
            bytes_per_second,
            eta_seconds,
            error,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::asr::sensevoice::SenseVoiceEngine;
    use crate::core::asr::sherpa_native::{SherpaNativeEngine, SherpaNativeKind};
    use crate::core::asr::{AsrEngine, AsrModelRef, TranscribeJob};
    use bzip2::write::BzEncoder;
    use bzip2::Compression;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    const REQUIRED_MODEL_FILES: [&str; 4] = [
        "encoder.int8.onnx",
        "decoder.int8.onnx",
        "joiner.int8.onnx",
        "tokens.txt",
    ];

    fn spawn_range_server(
        expected_range: &'static str,
        content_range: &'static str,
        body: &'static [u8],
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let listener_endpoint = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = vec![0_u8; 4096];
            let read = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains(&format!("range: {}", expected_range).to_ascii_lowercase()),
                "request did not contain expected Range header: {request}"
            );
            write!(
                stream,
                "HTTP/1.1 206 Partial Content\r\nContent-Range: {content_range}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
            stream.flush().unwrap();
        });
        (format!("http://{listener_endpoint}/model.bin"), handle)
    }

    fn write_parakeet_archive(path: &Path, entries: &[(&str, &[u8])]) {
        let file = StdFile::create(path).unwrap();
        let encoder = BzEncoder::new(file, Compression::best());
        let mut archive = tar::Builder::new(encoder);
        for (entry_path, data) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            archive
                .append_data(&mut header, *entry_path, *data)
                .unwrap();
        }
        let encoder = archive.into_inner().unwrap();
        encoder.finish().unwrap();
    }

    fn write_path_traversal_archive(path: &Path, unsafe_path: &[u8]) {
        let file = StdFile::create(path).unwrap();
        let encoder = BzEncoder::new(file, Compression::best());
        let mut archive = tar::Builder::new(encoder);
        let data = b"escape";
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.as_mut_bytes()[..unsafe_path.len()].copy_from_slice(unsafe_path);
        header.set_cksum();
        archive.append(&header, &data[..]).unwrap();
        let encoder = archive.into_inner().unwrap();
        encoder.finish().unwrap();
    }

    #[test]
    fn parses_http_content_range_total() {
        assert_eq!(
            parse_content_range_total("bytes 1024-2047/4096"),
            Some(4096)
        );
        assert_eq!(parse_content_range("bytes 3-5/6"), Some((3, 5, 6)));
        assert_eq!(parse_content_range("bytes 5-3/6"), None);
        assert_eq!(parse_content_range("bytes 0-6/6"), None);
        assert_eq!(parse_content_range_total("invalid"), None);
    }

    #[tokio::test]
    async fn range_request_resumes_and_appends_without_corruption() {
        let temp = tempfile::tempdir().unwrap();
        let part_path = temp.path().join("model.part");
        std::fs::write(&part_path, b"abc").unwrap();
        let (url, server) = spawn_range_server("bytes=3-", "bytes 3-5/6", b"def");
        let client = reqwest::Client::builder().no_proxy().build().unwrap();

        let (response, append) = request_download(&client, &url, 3).await.unwrap();
        assert!(append);
        assert_eq!(
            response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                .and_then(parse_content_range_total),
            Some(6)
        );
        let bytes = response.bytes().await.unwrap();
        let mut file = open_part_file(&part_path, append).await.unwrap();
        file.write_all(&bytes).await.unwrap();
        file.flush().await.unwrap();
        drop(file);

        server.join().unwrap();
        assert_eq!(std::fs::read(part_path).unwrap(), b"abcdef");
    }

    #[tokio::test]
    async fn range_request_rejects_mismatched_server_offset() {
        let (url, server) = spawn_range_server("bytes=3-", "bytes 2-5/6", b"cdef");
        let client = reqwest::Client::builder().no_proxy().build().unwrap();

        let error = request_download(&client, &url, 3).await.unwrap_err();

        server.join().unwrap();
        assert!(error.to_string().contains("续传起点不匹配"));
    }

    #[test]
    fn installs_parakeet_archive_atomically_and_replaces_previous_model() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("parakeet.tar.bz2");
        let final_dir = temp.path().join(PARAKEET_MODEL_ID);
        std::fs::create_dir_all(&final_dir).unwrap();
        std::fs::write(final_dir.join("old-marker"), b"old").unwrap();
        let entries = REQUIRED_MODEL_FILES
            .iter()
            .map(|name| {
                (
                    format!("{PARAKEET_ARCHIVE_DIR}/{name}"),
                    format!("new-{name}").into_bytes(),
                )
            })
            .collect::<Vec<_>>();
        let entry_refs = entries
            .iter()
            .map(|(name, data)| (name.as_str(), data.as_slice()))
            .collect::<Vec<_>>();
        write_parakeet_archive(&archive_path, &entry_refs);

        install_parakeet_archive_sync(&archive_path, &final_dir).unwrap();

        assert!(!final_dir.join("old-marker").exists());
        for name in REQUIRED_MODEL_FILES {
            assert!(final_dir.join(name).is_file());
        }
        assert!(std::fs::read_dir(temp.path())
            .unwrap()
            .flatten()
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .starts_with(".finalsub-")));
    }

    #[test]
    fn invalid_archive_keeps_previous_model_and_cleans_staging() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("parakeet-missing.tar.bz2");
        let final_dir = temp.path().join(PARAKEET_MODEL_ID);
        std::fs::create_dir_all(&final_dir).unwrap();
        std::fs::write(final_dir.join("old-marker"), b"old").unwrap();
        write_parakeet_archive(
            &archive_path,
            &[(
                &format!("{PARAKEET_ARCHIVE_DIR}/encoder.int8.onnx"),
                b"encoder",
            )],
        );

        let error = install_parakeet_archive_sync(&archive_path, &final_dir).unwrap_err();

        assert!(error.to_string().contains("缺少必要"));
        assert_eq!(std::fs::read(final_dir.join("old-marker")).unwrap(), b"old");
        assert!(std::fs::read_dir(temp.path())
            .unwrap()
            .flatten()
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .starts_with(".finalsub-install-")));
    }

    #[test]
    fn installs_sensevoice_archive_atomically_and_renames_int8_model() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("sensevoice.tar.bz2");
        let final_dir = temp.path().join(SENSEVOICE_MODEL_ID);
        std::fs::create_dir_all(&final_dir).unwrap();
        std::fs::write(final_dir.join("old-marker"), b"old").unwrap();
        write_parakeet_archive(
            &archive_path,
            &[
                (
                    &format!("{SENSEVOICE_ARCHIVE_DIR}/model.int8.onnx"),
                    b"onnx",
                ),
                (&format!("{SENSEVOICE_ARCHIVE_DIR}/tokens.txt"), b"tokens"),
            ],
        );

        install_sensevoice_archive_sync(&archive_path, &final_dir).unwrap();

        assert!(!final_dir.join("old-marker").exists());
        assert_eq!(
            std::fs::read(final_dir.join("model.onnx")).unwrap(),
            b"onnx"
        );
        assert_eq!(
            std::fs::read(final_dir.join("tokens.txt")).unwrap(),
            b"tokens"
        );
        assert!(!final_dir.join("model.int8.onnx").exists());
        assert!(std::fs::read_dir(temp.path())
            .unwrap()
            .flatten()
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .starts_with(".finalsub-")));
    }

    #[test]
    fn installs_qwen_archive_with_nested_tokenizer_files() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("qwen.tar.bz2");
        let final_dir = temp.path().join(QWEN3_MODEL_ID);
        let entries = QWEN3_FILES
            .iter()
            .map(|file| {
                (
                    format!("{QWEN3_ARCHIVE_DIR}/{}", file.source),
                    format!("qwen-{}", file.source).into_bytes(),
                )
            })
            .collect::<Vec<_>>();
        let entry_refs = entries
            .iter()
            .map(|(name, data)| (name.as_str(), data.as_slice()))
            .collect::<Vec<_>>();
        write_parakeet_archive(&archive_path, &entry_refs);

        install_managed_archive_sync(&archive_path, &final_dir, &QWEN3_SPEC).unwrap();

        assert!(QWEN3_SPEC.is_installed_at(&final_dir));
        assert_eq!(
            std::fs::read(final_dir.join("tokenizer/vocab.json")).unwrap(),
            b"qwen-tokenizer/vocab.json"
        );
    }

    #[test]
    fn archive_path_traversal_is_rejected_without_writing_outside_staging() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("parakeet-traversal.tar.bz2");
        let final_dir = temp.path().join(PARAKEET_MODEL_ID);
        write_path_traversal_archive(&archive_path, b"../encoder.int8.onnx");

        let result = install_parakeet_archive_sync(&archive_path, &final_dir);

        assert!(result.is_err());
        assert!(!temp.path().join("encoder.int8.onnx").exists());
        assert!(!final_dir.exists());
    }

    #[test]
    fn sensevoice_archive_path_traversal_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("sensevoice-traversal.tar.bz2");
        let final_dir = temp.path().join(SENSEVOICE_MODEL_ID);
        write_path_traversal_archive(&archive_path, b"../model.int8.onnx");

        let result = install_sensevoice_archive_sync(&archive_path, &final_dir);

        assert!(result.is_err());
        assert!(!temp.path().join("model.int8.onnx").exists());
        assert!(!final_dir.exists());
    }

    #[test]
    fn archive_sha_is_pinned_to_official_release_asset_digest() {
        let specs = [
            &PARAKEET_SPEC,
            &SENSEVOICE_SPEC,
            &PARAFORMER_SPEC,
            &QWEN3_SPEC,
            &FIRERED_SPEC,
        ];
        assert!(specs.iter().all(|spec| spec.sha256.len() == 64));
        assert_eq!(PARAKEET_SPEC.size, 482_468_385);
        assert_eq!(SENSEVOICE_SPEC.size, 165_783_878);
        assert_eq!(PARAFORMER_SPEC.size, 228_262_632);
        assert_eq!(QWEN3_SPEC.size, 878_702_423);
        assert_eq!(FIRERED_SPEC.size, 520_516_278);
    }

    #[tokio::test]
    #[ignore = "requires FINALSUB_SENSEVOICE_ARCHIVE pointing to the official 2025 int8 archive"]
    async fn official_sensevoice_archive_installs_and_transcribes_fixture() {
        let archive_path = PathBuf::from(
            std::env::var("FINALSUB_SENSEVOICE_ARCHIVE")
                .expect("set FINALSUB_SENSEVOICE_ARCHIVE to the official archive"),
        );
        let temp = tempfile::tempdir().unwrap();
        let final_dir = temp.path().join(SENSEVOICE_MODEL_ID);
        install_sensevoice_archive_sync(&archive_path, &final_dir).unwrap();

        let wav_path = temp.path().join("zh.wav");
        let file = StdFile::open(&archive_path).unwrap();
        let decoder = bzip2::read::BzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        let expected = format!("{SENSEVOICE_ARCHIVE_DIR}/test_wavs/zh.wav");
        let mut found = false;
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            if entry.path().unwrap() == Path::new(&expected) {
                entry.unpack(&wav_path).unwrap();
                found = true;
                break;
            }
        }
        assert!(found, "official archive did not contain {expected}");

        let vad_model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("vad")
            .join("silero_vad.onnx");
        let engine = SenseVoiceEngine::new(temp.path().to_path_buf(), vad_model_path);
        let model = AsrModelRef {
            engine_id: "sensevoice".into(),
            model_id: SENSEVOICE_MODEL_ID.into(),
            model_path: None,
        };
        engine.prepare(&model).await.unwrap();
        let (progress_tx, _progress_rx) = tokio::sync::mpsc::channel(8);
        let track = engine
            .transcribe(
                TranscribeJob {
                    audio_path: wav_path.to_string_lossy().to_string(),
                    output_path: temp.path().join("out.srt").to_string_lossy().to_string(),
                    language: Some("zh".into()),
                    model,
                    max_subtitle_chars: 0,
                },
                progress_tx,
                None,
            )
            .await
            .unwrap();

        assert!(!track.cues.is_empty());
        assert!(track.cues.iter().any(|cue| !cue.text.trim().is_empty()));
        eprintln!("SenseVoice fixture transcript: {}", track.cues[0].text);
    }

    #[tokio::test]
    #[ignore = "requires FINALSUB_PARAFORMER_ARCHIVE pointing to the official 2025 int8 archive"]
    async fn official_paraformer_archive_installs_and_transcribes_fixture() {
        let archive_path = PathBuf::from(
            std::env::var("FINALSUB_PARAFORMER_ARCHIVE")
                .expect("set FINALSUB_PARAFORMER_ARCHIVE to the official archive"),
        );
        let temp = tempfile::tempdir().unwrap();
        let final_dir = temp.path().join(PARAFORMER_MODEL_ID);
        install_managed_archive_sync(&archive_path, &final_dir, &PARAFORMER_SPEC).unwrap();

        let wav_path = temp.path().join("paraformer-1.wav");
        let file = StdFile::open(&archive_path).unwrap();
        let decoder = bzip2::read::BzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        let expected = format!("{PARAFORMER_ARCHIVE_DIR}/test_wavs/1.wav");
        let mut found = false;
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            if entry.path().unwrap() == Path::new(&expected) {
                entry.unpack(&wav_path).unwrap();
                found = true;
                break;
            }
        }
        assert!(found, "official archive did not contain {expected}");

        let vad_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("vad")
            .join("silero_vad.onnx");
        let engine = SherpaNativeEngine::new(
            SherpaNativeKind::Paraformer,
            temp.path().to_path_buf(),
            vad_path,
        );
        let model = AsrModelRef {
            engine_id: "paraformer".into(),
            model_id: PARAFORMER_MODEL_ID.into(),
            model_path: None,
        };
        engine.prepare(&model).await.unwrap();
        let (progress_tx, _progress_rx) = tokio::sync::mpsc::channel(32);
        let track = engine
            .transcribe(
                TranscribeJob {
                    audio_path: wav_path.to_string_lossy().to_string(),
                    output_path: temp.path().join("out.srt").to_string_lossy().to_string(),
                    language: Some("zh".into()),
                    model,
                    max_subtitle_chars: 0,
                },
                progress_tx,
                None,
            )
            .await
            .unwrap();

        assert!(!track.cues.is_empty());
        assert!(track.cues.iter().any(|cue| !cue.text.trim().is_empty()));
        eprintln!(
            "Paraformer fixture transcript: {}",
            track
                .cues
                .iter()
                .map(|cue| cue.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        );
    }

    #[tokio::test]
    #[ignore = "requires FINALSUB_QWEN3_ARCHIVE pointing to the official 2026 int8 archive"]
    async fn official_qwen3_archive_installs_and_transcribes_fixture() {
        let archive_path = PathBuf::from(
            std::env::var("FINALSUB_QWEN3_ARCHIVE")
                .expect("set FINALSUB_QWEN3_ARCHIVE to the official archive"),
        );
        let temp = tempfile::tempdir().unwrap();
        let final_dir = temp.path().join(QWEN3_MODEL_ID);
        install_managed_archive_sync(&archive_path, &final_dir, &QWEN3_SPEC).unwrap();

        let wav_path = temp.path().join("qwen3-qiqiu.wav");
        let file = StdFile::open(&archive_path).unwrap();
        let decoder = bzip2::read::BzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        let expected = format!("{QWEN3_ARCHIVE_DIR}/test_wavs/qiqiu1.wav");
        let mut found = false;
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            if entry.path().unwrap() == Path::new(&expected) {
                entry.unpack(&wav_path).unwrap();
                found = true;
                break;
            }
        }
        assert!(found, "official archive did not contain {expected}");

        let vad_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("vad")
            .join("silero_vad.onnx");
        let engine =
            SherpaNativeEngine::new(SherpaNativeKind::Qwen3, temp.path().to_path_buf(), vad_path);
        let model = AsrModelRef {
            engine_id: "qwen3-asr".into(),
            model_id: QWEN3_MODEL_ID.into(),
            model_path: None,
        };
        engine.prepare(&model).await.unwrap();
        let (progress_tx, _progress_rx) = tokio::sync::mpsc::channel(32);
        let track = engine
            .transcribe(
                TranscribeJob {
                    audio_path: wav_path.to_string_lossy().to_string(),
                    output_path: temp.path().join("out.srt").to_string_lossy().to_string(),
                    language: Some("zh".into()),
                    model,
                    max_subtitle_chars: 0,
                },
                progress_tx,
                None,
            )
            .await
            .unwrap();

        assert!(!track.cues.is_empty());
        assert!(track.cues.iter().any(|cue| !cue.text.trim().is_empty()));
        eprintln!(
            "Qwen3-ASR fixture transcript: {}",
            track
                .cues
                .iter()
                .map(|cue| cue.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        );
    }

    #[tokio::test]
    #[ignore = "requires FINALSUB_FIRERED_ARCHIVE pointing to the official 2026 int8 archive"]
    async fn official_firered_archive_installs_and_transcribes_fixture() {
        let archive_path = PathBuf::from(
            std::env::var("FINALSUB_FIRERED_ARCHIVE")
                .expect("set FINALSUB_FIRERED_ARCHIVE to the official archive"),
        );
        let temp = tempfile::tempdir().unwrap();
        let final_dir = temp.path().join(FIRERED_MODEL_ID);
        install_managed_archive_sync(&archive_path, &final_dir, &FIRERED_SPEC).unwrap();

        let wav_path = temp.path().join("firered-0.wav");
        let file = StdFile::open(&archive_path).unwrap();
        let decoder = bzip2::read::BzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        let expected = format!("{FIRERED_ARCHIVE_DIR}/test_wavs/0.wav");
        let mut found = false;
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            if entry.path().unwrap() == Path::new(&expected) {
                entry.unpack(&wav_path).unwrap();
                found = true;
                break;
            }
        }
        assert!(found, "official archive did not contain {expected}");

        let vad_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("vad")
            .join("silero_vad.onnx");
        let engine = SherpaNativeEngine::new(
            SherpaNativeKind::FireRedCtc,
            temp.path().to_path_buf(),
            vad_path,
        );
        let model = AsrModelRef {
            engine_id: "firered-asr".into(),
            model_id: FIRERED_MODEL_ID.into(),
            model_path: None,
        };
        engine.prepare(&model).await.unwrap();
        let (progress_tx, _progress_rx) = tokio::sync::mpsc::channel(32);
        let track = engine
            .transcribe(
                TranscribeJob {
                    audio_path: wav_path.to_string_lossy().to_string(),
                    output_path: temp.path().join("out.srt").to_string_lossy().to_string(),
                    language: Some("zh".into()),
                    model,
                    max_subtitle_chars: 0,
                },
                progress_tx,
                None,
            )
            .await
            .unwrap();

        assert!(!track.cues.is_empty());
        assert!(track.cues.iter().any(|cue| !cue.text.trim().is_empty()));
        eprintln!(
            "FireRedASR2 fixture transcript: {}",
            track
                .cues
                .iter()
                .map(|cue| cue.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
}
