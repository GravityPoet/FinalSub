use reqwest::header::CONTENT_RANGE;
use std::collections::HashSet;
use std::fs::File as StdFile;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;

use crate::core::models::download::{
    open_part_file, request_download, sha256_file, ModelDownloadProgress,
};
use crate::core::tts::models::{
    find_spec, finish_managed_install, managed_models_root, missing_files, resolve_ready_model,
    TtsDownloadFileSpec, TtsModelSpec,
};
use crate::error::{FinalSubError, Result};

const GITHUB_PROXY_PREFIX: &str = "https://gh-proxy.com/";
const MAX_ARCHIVE_ENTRIES: usize = 50_000;

#[derive(Debug, Clone)]
struct DownloadArtifact {
    file_name: String,
    download_url: String,
    size: u64,
    sha256: String,
}

impl DownloadArtifact {
    fn archive(spec: &TtsModelSpec) -> Self {
        Self {
            file_name: spec.archive_name.into(),
            download_url: spec.download_url.into(),
            size: spec.archive_size,
            sha256: spec.archive_sha256.into(),
        }
    }

    fn extra(spec: TtsDownloadFileSpec) -> Self {
        Self {
            file_name: spec.file_name.into(),
            download_url: spec.download_url.into(),
            size: spec.size,
            sha256: spec.sha256.into(),
        }
    }
}

fn progress_key(model_id: &str) -> String {
    // 事件同时被 ASR 与 TTS 列表消费；使用真实模型 ID，前端不需要理解
    // 后端的控制句柄命名空间，也能在刷新后正确关联进度。
    model_id.to_string()
}

fn source_urls(url: &str) -> [String; 2] {
    [format!("{GITHUB_PROXY_PREFIX}{url}"), url.to_string()]
}

fn artifact_part_path(root: &Path, model_id: &str, file_name: &str) -> PathBuf {
    let safe_name = file_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    root.join(format!(".finalsub-tts-{model_id}-{safe_name}.part"))
}

#[allow(clippy::too_many_arguments)]
fn emit_progress(
    app: &AppHandle,
    key: &str,
    bytes_downloaded: u64,
    total_bytes: u64,
    status: &str,
    phase: &str,
    bytes_per_second: Option<u64>,
    eta_seconds: Option<u64>,
    error: Option<String>,
) {
    let progress = if total_bytes == 0 {
        0.0
    } else {
        (bytes_downloaded as f32 / total_bytes as f32).clamp(0.0, 1.0)
    };
    let _ = app.emit(
        "model-download-updated",
        ModelDownloadProgress {
            model_id: key.into(),
            bytes_downloaded,
            total_bytes,
            progress,
            status: status.into(),
            phase: phase.into(),
            bytes_per_second,
            eta_seconds,
            error,
        },
    );
}

fn parse_total(response: &reqwest::Response, existing: u64) -> Option<u64> {
    response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split_once('/'))
        .and_then(|(_, total)| total.parse().ok())
        .or_else(|| response.content_length().map(|length| length + existing))
}

fn checked_download_size(
    downloaded: u64,
    chunk_size: usize,
    artifact: &DownloadArtifact,
) -> Result<u64> {
    let next = downloaded.saturating_add(chunk_size as u64);
    if next > artifact.size {
        return Err(FinalSubError::Validation(format!(
            "TTS 工件 {} 超过固定大小，已停止写入",
            artifact.file_name
        )));
    }
    Ok(next)
}

#[allow(clippy::too_many_arguments)]
async fn download_artifact(
    app: &AppHandle,
    client: &reqwest::Client,
    key: &str,
    root: &Path,
    model_id: &str,
    artifact: &DownloadArtifact,
    completed_before: u64,
    total_bytes: u64,
    cancel_rx: &mut watch::Receiver<bool>,
) -> Result<Option<PathBuf>> {
    let part_path = artifact_part_path(root, model_id, &artifact.file_name);
    let mut existing = tokio::fs::metadata(&part_path)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if existing > artifact.size {
        tokio::fs::remove_file(&part_path).await?;
        existing = 0;
    }

    if existing == artifact.size {
        emit_progress(
            app,
            key,
            completed_before + existing,
            total_bytes,
            "downloading",
            "verifying",
            None,
            None,
            None,
        );
        if sha256_file(part_path.clone()).await? == artifact.sha256 {
            return Ok(Some(part_path));
        }
        tokio::fs::remove_file(&part_path).await?;
        existing = 0;
    }

    let mut last_error = None;
    for url in source_urls(&artifact.download_url) {
        if *cancel_rx.borrow() {
            emit_progress(
                app,
                key,
                completed_before + existing,
                total_bytes,
                "cancelled",
                "paused",
                None,
                None,
                None,
            );
            return Ok(None);
        }
        let attempt = async {
            let (response, append) = request_download(client, &url, existing).await?;
            if !append {
                existing = 0;
            }
            if let Some(server_total) = parse_total(&response, existing) {
                if server_total != artifact.size {
                    return Err(FinalSubError::Validation(format!(
                        "TTS 工件大小与固定清单不符：期望 {} 字节，服务器报告 {server_total} 字节",
                        artifact.size
                    )));
                }
            }
            let resume_offset = existing;
            let mut downloaded = existing;
            let started_at = Instant::now();
            let mut response = response;
            let mut file = open_part_file(&part_path, append).await?;
            loop {
                if *cancel_rx.borrow() {
                    file.flush().await?;
                    emit_progress(
                        app,
                        key,
                        completed_before + downloaded,
                        total_bytes,
                        "cancelled",
                        "paused",
                        None,
                        None,
                        None,
                    );
                    return Ok::<bool, FinalSubError>(false);
                }
                let chunk = tokio::select! {
                    chunk = response.chunk() => chunk.map_err(|error| {
                        FinalSubError::Validation(format!("TTS 下载流中断，可重试续传：{error}"))
                    })?,
                    changed = cancel_rx.changed() => {
                        if changed.is_err() || *cancel_rx.borrow() {
                            file.flush().await?;
                            emit_progress(
                                app,
                                key,
                                completed_before + downloaded,
                                total_bytes,
                                "cancelled",
                                "paused",
                                None,
                                None,
                                None,
                            );
                            return Ok(false);
                        }
                        continue;
                    }
                };
                let Some(chunk) = chunk else { break };
                let next_downloaded = checked_download_size(downloaded, chunk.len(), artifact)?;
                file.write_all(&chunk).await?;
                downloaded = next_downloaded;
                let elapsed = started_at.elapsed().as_secs_f64().max(0.001);
                let speed = ((downloaded - resume_offset) as f64 / elapsed) as u64;
                let remaining = total_bytes.saturating_sub(completed_before + downloaded);
                emit_progress(
                    app,
                    key,
                    completed_before + downloaded,
                    total_bytes,
                    "downloading",
                    if resume_offset > 0 {
                        "resuming"
                    } else {
                        "downloading"
                    },
                    Some(speed),
                    (speed > 0).then_some(remaining.div_ceil(speed)),
                    None,
                );
            }
            file.flush().await?;
            file.sync_all().await?;
            drop(file);
            let actual = tokio::fs::metadata(&part_path).await?.len();
            if actual != artifact.size {
                return Err(FinalSubError::Validation(format!(
                    "TTS 下载不完整，可重试续传：期望 {} 字节，当前 {actual} 字节",
                    artifact.size
                )));
            }
            emit_progress(
                app,
                key,
                completed_before + actual,
                total_bytes,
                "downloading",
                "verifying",
                None,
                None,
                None,
            );
            let digest = sha256_file(part_path.clone()).await?;
            if digest != artifact.sha256 {
                tokio::fs::remove_file(&part_path).await.ok();
                existing = 0;
                return Err(FinalSubError::Validation(format!(
                    "TTS 工件 {} SHA-256 校验失败，损坏文件已清理",
                    artifact.file_name
                )));
            }
            Ok(true)
        }
        .await;

        match attempt {
            Ok(true) => return Ok(Some(part_path)),
            Ok(false) => return Ok(None),
            Err(error) => {
                existing = tokio::fs::metadata(&part_path)
                    .await
                    .map(|metadata| metadata.len())
                    .unwrap_or(0);
                if existing > artifact.size {
                    tokio::fs::remove_file(&part_path).await.ok();
                    existing = 0;
                }
                last_error = Some(error);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| FinalSubError::Validation("TTS 下载失败".into())))
}

pub async fn download_model_impl(
    app: AppHandle,
    app_config_dir: PathBuf,
    model_id: String,
    mut cancel_rx: watch::Receiver<bool>,
) -> Result<()> {
    let spec = find_spec(model_id.trim())?;
    let key = progress_key(spec.id);
    // 外部登记或自动扫描到的完整模型与受管模型同等可用。即使前端被重复触发，
    // 也先复用本机现有目录，不为同一模型再下载一份受管副本。
    if resolve_ready_model(&app_config_dir, spec.id).is_ok() {
        emit_progress(&app, &key, 1, 1, "done", "ready", None, Some(0), None);
        return Ok(());
    }
    let root = managed_models_root(&app_config_dir)?;
    let final_dir = root.join(spec.id);

    let artifacts = std::iter::once(DownloadArtifact::archive(&spec))
        .chain(
            spec.extra_files
                .iter()
                .copied()
                .map(DownloadArtifact::extra),
        )
        .collect::<Vec<_>>();
    let total_bytes = artifacts.iter().map(|artifact| artifact.size).sum();
    let client = reqwest::Client::builder()
        .user_agent("FinalSub-TTS-ModelManager/1.0")
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(90))
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .map_err(|error| {
            FinalSubError::Validation(format!("初始化 TTS 下载客户端失败：{error}"))
        })?;

    let mut completed = 0;
    let mut verified_paths = Vec::new();
    for artifact in &artifacts {
        let Some(path) = download_artifact(
            &app,
            &client,
            &key,
            &root,
            spec.id,
            artifact,
            completed,
            total_bytes,
            &mut cancel_rx,
        )
        .await?
        else {
            return Ok(());
        };
        completed += artifact.size;
        verified_paths.push(path);
    }

    if *cancel_rx.borrow() {
        emit_progress(
            &app,
            &key,
            completed,
            total_bytes,
            "cancelled",
            "paused",
            None,
            None,
            None,
        );
        return Ok(());
    }
    emit_progress(
        &app,
        &key,
        total_bytes,
        total_bytes,
        "downloading",
        "installing",
        None,
        None,
        None,
    );
    let archive_path = verified_paths
        .first()
        .cloned()
        .ok_or_else(|| FinalSubError::Validation("TTS 主模型工件缺失".into()))?;
    let extra_paths = verified_paths.into_iter().skip(1).collect::<Vec<_>>();
    let install_spec = spec.clone();
    let install_dir = final_dir.clone();
    tokio::task::spawn_blocking(move || {
        install_archive_sync(&archive_path, &extra_paths, &install_dir, &install_spec)
    })
    .await
    .map_err(|error| FinalSubError::Validation(format!("TTS 安装线程异常：{error}")))??;
    finish_managed_install(&app_config_dir, spec.id)?;

    for artifact in &artifacts {
        let part = artifact_part_path(&root, spec.id, &artifact.file_name);
        tokio::fs::remove_file(part).await.ok();
    }
    emit_progress(
        &app,
        &key,
        total_bytes,
        total_bytes,
        "done",
        "ready",
        None,
        Some(0),
        None,
    );
    Ok(())
}

fn install_archive_sync(
    archive_path: &Path,
    extra_paths: &[PathBuf],
    final_dir: &Path,
    spec: &TtsModelSpec,
) -> Result<()> {
    let parent = final_dir
        .parent()
        .ok_or_else(|| FinalSubError::Validation("TTS 安装目录缺少父目录".into()))?;
    let staging = parent.join(format!(
        ".finalsub-tts-install-{}-{}",
        spec.id,
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&staging)?;
    let result = (|| -> Result<()> {
        let file = StdFile::open(archive_path)?;
        let decoder = bzip2::read::BzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        let mut seen = HashSet::new();
        let mut extracted_bytes = 0_u64;
        let max_install_bytes = spec.size_mb.saturating_mul(1024 * 1024).saturating_mul(2);
        let mut entries = 0_usize;

        for entry in archive.entries()? {
            entries += 1;
            if entries > MAX_ARCHIVE_ENTRIES {
                return Err(FinalSubError::Validation("TTS 压缩包文件数量异常".into()));
            }
            let mut entry = entry?;
            let entry_type = entry.header().entry_type();
            // tar 可能携带 GNU long-name / PAX 元数据条目；tar crate 会把
            // 它们应用到后续文件，但不会把它们当作模型文件。跳过这些元数据，
            // 其它非普通文件（链接、设备、FIFO）一律拒绝。
            if entry_type.is_gnu_longname()
                || entry_type.is_gnu_longlink()
                || entry_type.is_pax_global_extensions()
                || entry_type.is_pax_local_extensions()
            {
                continue;
            }
            let path = entry.path()?.into_owned();
            if path
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
            {
                return Err(FinalSubError::Validation(format!(
                    "TTS 压缩包包含不安全路径：{}",
                    path.display()
                )));
            }
            let relative = path
                .strip_prefix(Path::new(spec.archive_inner_dir))
                .map_err(|_| {
                    FinalSubError::Validation(format!(
                        "TTS 压缩包出现预期目录之外的条目：{}",
                        path.display()
                    ))
                })?;
            if relative.as_os_str().is_empty() {
                continue;
            }
            if !seen.insert(relative.to_path_buf()) {
                return Err(FinalSubError::Validation(format!(
                    "TTS 压缩包包含重复条目：{}",
                    relative.display()
                )));
            }
            let target = staging.join(relative);
            if entry_type.is_dir() {
                std::fs::create_dir_all(&target)?;
                continue;
            }
            if !entry_type.is_file() {
                return Err(FinalSubError::Validation(format!(
                    "TTS 压缩包包含不允许的链接或特殊条目：{}",
                    relative.display()
                )));
            }
            let declared = entry.header().size()?;
            extracted_bytes = extracted_bytes.saturating_add(declared);
            if extracted_bytes > max_install_bytes {
                return Err(FinalSubError::Validation("TTS 压缩包解包体积异常".into()));
            }
            if let Some(directory) = target.parent() {
                std::fs::create_dir_all(directory)?;
            }
            let mut output = StdFile::create(&target)?;
            std::io::copy(&mut entry, &mut output)?;
            output.flush()?;
        }

        if extra_paths.len() != spec.extra_files.len() {
            return Err(FinalSubError::Validation("TTS 附加工件数量不完整".into()));
        }
        for (extra, source) in spec.extra_files.iter().zip(extra_paths) {
            let target = staging.join(extra.file_name);
            std::fs::copy(source, target)?;
        }
        let missing = missing_files(spec, &staging);
        if !missing.is_empty() {
            return Err(FinalSubError::Validation(format!(
                "TTS 安装包缺少必要文件：{}",
                missing.join("、")
            )));
        }

        let backup = parent.join(format!(
            ".finalsub-tts-backup-{}-{}",
            spec.id,
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
        let _ = std::fs::remove_dir_all(staging);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use bzip2::write::BzEncoder;
    use bzip2::Compression;
    use tempfile::TempDir;

    fn write_archive(path: &Path, root: &str, files: &[&str]) {
        let output = StdFile::create(path).unwrap();
        let encoder = BzEncoder::new(output, Compression::best());
        let mut archive = tar::Builder::new(encoder);
        for relative in files {
            let data = format!("fixture-{relative}");
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            archive
                .append_data(&mut header, format!("{root}/{relative}"), data.as_bytes())
                .unwrap();
        }
        archive.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn source_order_uses_proxy_then_official() {
        let urls = source_urls("https://github.com/example/model.bin");
        assert!(urls[0].starts_with(GITHUB_PROXY_PREFIX));
        assert_eq!(urls[1], "https://github.com/example/model.bin");
    }

    #[test]
    fn oversized_stream_chunk_is_rejected_before_write() {
        let artifact = DownloadArtifact {
            file_name: "model.tar.bz2".into(),
            download_url: "https://github.com/example/model.tar.bz2".into(),
            size: 10,
            sha256: "00".into(),
        };

        assert_eq!(checked_download_size(6, 4, &artifact).unwrap(), 10);
        assert!(checked_download_size(6, 5, &artifact)
            .unwrap_err()
            .to_string()
            .contains("超过固定大小"));
    }

    #[test]
    fn archive_install_is_atomic_and_includes_all_model_files() {
        let temp = TempDir::new().unwrap();
        let spec = find_spec("vits-zh-aishell3").unwrap();
        let archive = temp.path().join("model.tar.bz2");
        write_archive(
            &archive,
            spec.archive_inner_dir,
            &["model.onnx", "tokens.txt", "lexicon.txt", "phone.fst"],
        );
        let target = temp.path().join(spec.id);
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("old-marker"), b"old").unwrap();

        install_archive_sync(&archive, &[], &target, &spec).unwrap();

        assert!(!target.join("old-marker").exists());
        assert!(target.join("phone.fst").is_file());
        assert!(missing_files(&spec, &target).is_empty());
    }

    #[test]
    fn incomplete_archive_keeps_previous_model() {
        let temp = TempDir::new().unwrap();
        let spec = find_spec("vits-zh-aishell3").unwrap();
        let archive = temp.path().join("model.tar.bz2");
        write_archive(&archive, spec.archive_inner_dir, &["model.onnx"]);
        let target = temp.path().join(spec.id);
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("old-marker"), b"old").unwrap();

        let error = install_archive_sync(&archive, &[], &target, &spec).unwrap_err();

        assert!(error.to_string().contains("缺少必要文件"));
        assert!(target.join("old-marker").is_file());
    }

    #[test]
    fn zipvoice_install_requires_verified_vocoder() {
        let temp = TempDir::new().unwrap();
        let spec = find_spec("zipvoice-distill-zh-en").unwrap();
        let archive = temp.path().join("model.tar.bz2");
        let archive_files = spec
            .required_files
            .iter()
            .copied()
            .filter(|path| *path != "vocos_24khz.onnx")
            .collect::<Vec<_>>();
        write_archive(&archive, spec.archive_inner_dir, &archive_files);
        let target = temp.path().join(spec.id);

        assert!(install_archive_sync(&archive, &[], &target, &spec).is_err());
        let vocoder = temp.path().join("vocos.part");
        std::fs::write(&vocoder, b"vocoder").unwrap();
        install_archive_sync(&archive, &[vocoder], &target, &spec).unwrap();
        assert!(target.join("vocos_24khz.onnx").is_file());
    }

    #[test]
    #[ignore = "requires FINALSUB_TTS_VITS_ARCHIVE pointing to the official release asset"]
    fn official_vits_archive_installs_with_real_release_layout() {
        let archive = PathBuf::from(
            std::env::var("FINALSUB_TTS_VITS_ARCHIVE")
                .expect("set FINALSUB_TTS_VITS_ARCHIVE to the official archive"),
        );
        let temp = TempDir::new().unwrap();
        let spec = find_spec("vits-zh-aishell3").unwrap();
        let target = temp.path().join(spec.id);

        install_archive_sync(&archive, &[], &target, &spec).unwrap();

        assert!(missing_files(&spec, &target).is_empty());
        assert!(target.join("phone.fst").is_file());
    }

    #[test]
    #[ignore = "requires FINALSUB_TTS_ZIPVOICE_ARCHIVE and FINALSUB_TTS_VOCODER pointing to official release assets"]
    fn official_zipvoice_archive_installs_with_vocoder() {
        let archive = PathBuf::from(
            std::env::var("FINALSUB_TTS_ZIPVOICE_ARCHIVE")
                .expect("set FINALSUB_TTS_ZIPVOICE_ARCHIVE to the official archive"),
        );
        let vocoder = PathBuf::from(
            std::env::var("FINALSUB_TTS_VOCODER")
                .expect("set FINALSUB_TTS_VOCODER to the official vocoder"),
        );
        let temp = TempDir::new().unwrap();
        let spec = find_spec("zipvoice-distill-zh-en").unwrap();
        let target = temp.path().join(spec.id);

        install_archive_sync(&archive, &[vocoder], &target, &spec).unwrap();

        assert!(missing_files(&spec, &target).is_empty());
        assert!(target.join("vocos_24khz.onnx").is_file());
    }
}
