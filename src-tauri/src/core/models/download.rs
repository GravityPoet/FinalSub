use crate::core::models::{builtin_model_catalog, validate_whisper_model_id, whisper_model_path};
use crate::error::{FinalSubError, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::{watch, RwLock};

#[derive(Serialize, Clone, Debug)]
pub struct ModelDownloadProgress {
    pub model_id: String,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub progress: f32,
    pub status: String, // "downloading" | "done" | "cancelled" | "error"
    pub error: Option<String>,
}

pub async fn download_model_impl(
    app: AppHandle,
    model_controls: Arc<RwLock<HashMap<String, watch::Sender<bool>>>>,
    models_dir: PathBuf,
    model_id: String,
    mut cancel_rx: watch::Receiver<bool>,
) -> Result<()> {
    // 1. 验证 ID 防路径逃逸
    let normalized = validate_whisper_model_id(&model_id)?;

    // 2. 找到对应下载 URL
    let catalog = builtin_model_catalog();
    let model_info = catalog
        .iter()
        .find(|m| m.id == normalized)
        .ok_or_else(|| FinalSubError::Validation(format!("未知模型 ID: {normalized}")))?;

    let url = model_info
        .download_url
        .as_ref()
        .ok_or_else(|| FinalSubError::Validation(format!("模型 {normalized} 暂无可用下载链接")))?;

    // 3. 构建路径
    let final_path = whisper_model_path(&models_dir, &normalized);
    let tmp_path = models_dir.join(format!(".finalsub-download-{normalized}.tmp"));

    // 创建目录
    if let Some(parent) = tmp_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    if final_path.exists() {
        let size = tokio::fs::metadata(&final_path)
            .await
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        emit_download_progress(&app, &normalized, size, size, "done", None);
        return Ok(());
    }

    // 4. 发起 HTTP 请求
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600)) // 10分钟超时
        .build()
        .map_err(|e| FinalSubError::Validation(format!("初始化 HTTP 客户端失败: {e}")))?;

    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| FinalSubError::Validation(format!("发起下载请求失败: {e}")))?;

    if !resp.status().is_success() {
        return Err(FinalSubError::Validation(format!(
            "服务器返回错误: {}",
            resp.status()
        )));
    }

    let total_bytes = resp.content_length().unwrap_or(0);
    let mut file = File::create(&tmp_path).await?;
    let mut bytes_downloaded = 0u64;

    // 5. 循环读取 Chunk
    loop {
        // 检查取消
        if *cancel_rx.borrow() {
            // 清理
            drop(file);
            let _ = tokio::fs::remove_file(&tmp_path).await;
            emit_download_progress(
                &app,
                &normalized,
                bytes_downloaded,
                total_bytes,
                "cancelled",
                None,
            );
            return Ok(());
        }

        let chunk_res = tokio::select! {
            res = resp.chunk() => res,
            _ = cancel_rx.changed() => {
                if *cancel_rx.borrow() {
                    drop(file);
                    let _ = tokio::fs::remove_file(&tmp_path).await;
                    emit_download_progress(&app, &normalized, bytes_downloaded, total_bytes, "cancelled", None);
                    return Ok(());
                }
                resp.chunk().await
            }
        };

        let chunk = match chunk_res {
            Ok(Some(c)) => c,
            Ok(None) => break, // 完成
            Err(e) => {
                drop(file);
                let _ = tokio::fs::remove_file(&tmp_path).await;
                let err_msg = format!("下载数据流中断: {e}");
                emit_download_progress(
                    &app,
                    &normalized,
                    bytes_downloaded,
                    total_bytes,
                    "error",
                    Some(err_msg.clone()),
                );
                return Err(FinalSubError::Validation(err_msg));
            }
        };

        if let Err(e) = file.write_all(&chunk).await {
            drop(file);
            let _ = tokio::fs::remove_file(&tmp_path).await;
            let err_msg = format!("写入模型文件失败: {e}");
            emit_download_progress(
                &app,
                &normalized,
                bytes_downloaded,
                total_bytes,
                "error",
                Some(err_msg.clone()),
            );
            return Err(FinalSubError::Validation(err_msg));
        }
        bytes_downloaded += chunk.len() as u64;

        emit_download_progress(
            &app,
            &normalized,
            bytes_downloaded,
            total_bytes,
            "downloading",
            None,
        );
    }

    // 强制刷盘
    if let Err(e) = file.flush().await {
        drop(file);
        let _ = tokio::fs::remove_file(&tmp_path).await;
        let err_msg = format!("保存模型文件失败: {e}");
        emit_download_progress(
            &app,
            &normalized,
            bytes_downloaded,
            total_bytes,
            "error",
            Some(err_msg.clone()),
        );
        return Err(FinalSubError::Validation(err_msg));
    }
    drop(file);

    // 6. 校验
    let metadata = tokio::fs::metadata(&tmp_path).await?;
    if metadata.len() == 0 {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        let err_msg = "下载文件大小为 0".to_string();
        emit_download_progress(
            &app,
            &normalized,
            bytes_downloaded,
            total_bytes,
            "error",
            Some(err_msg.clone()),
        );
        return Err(FinalSubError::Validation(err_msg));
    }

    if total_bytes > 0 && metadata.len() != total_bytes {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        let err_msg = format!(
            "下载文件大小不匹配: 期望 {total_bytes} 字节, 实际 {} 字节",
            metadata.len()
        );
        emit_download_progress(
            &app,
            &normalized,
            bytes_downloaded,
            total_bytes,
            "error",
            Some(err_msg.clone()),
        );
        return Err(FinalSubError::Validation(err_msg));
    }

    // 7. 原子 rename 覆盖
    if let Err(e) = tokio::fs::rename(&tmp_path, &final_path).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        let err_msg = format!("保存模型文件失败: {e}");
        emit_download_progress(
            &app,
            &normalized,
            bytes_downloaded,
            total_bytes,
            "error",
            Some(err_msg.clone()),
        );
        return Err(FinalSubError::Validation(err_msg));
    }

    // 移除控制器
    {
        let mut controls = model_controls.write().await;
        controls.remove(&normalized);
    }

    emit_download_progress(&app, &normalized, total_bytes, total_bytes, "done", None);
    Ok(())
}

fn emit_download_progress(
    app: &AppHandle,
    model_id: &str,
    bytes_downloaded: u64,
    total_bytes: u64,
    status: &str,
    error: Option<String>,
) {
    let progress = if total_bytes > 0 {
        (bytes_downloaded as f32 / total_bytes as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let payload = ModelDownloadProgress {
        model_id: model_id.to_string(),
        bytes_downloaded,
        total_bytes,
        progress,
        status: status.to_string(),
        error,
    };

    let _ = app.emit("model-download-updated", payload);
}
