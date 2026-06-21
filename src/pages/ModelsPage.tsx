import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  scanModels,
  deleteModel,
  getSettings,
  downloadModel,
  cancelModelDownload,
  type AsrModelInfo,
  type ModelDownloadProgress,
} from "../lib/tauri";
import { Download, CheckCircle, AlertCircle, Clock, Trash2, RefreshCw, XCircle } from "lucide-react";

function StatusBadge({
  status,
  downloadInfo,
}: {
  status: AsrModelInfo["status"];
  downloadInfo?: ModelDownloadProgress;
}) {
  const currentStatus = downloadInfo ? downloadInfo.status : status;
  const errorMsg =
    downloadInfo && downloadInfo.status === "error"
      ? downloadInfo.error
      : typeof status === "object" && "error" in status
      ? status.error
      : undefined;

  if (currentStatus === "available")
    return (
      <span className="flex items-center gap-1 text-blue-600">
        <Download size={14} /> 未安装
      </span>
    );
  if (currentStatus === "downloaded" || currentStatus === "done")
    return (
      <span className="flex items-center gap-1 text-green-600">
        <CheckCircle size={14} /> 已下载
      </span>
    );
  if (currentStatus === "downloading") {
    const pct = downloadInfo ? Math.round(downloadInfo.progress * 100) : 0;
    return (
      <span className="flex items-center gap-1 text-yellow-600 font-medium">
        <Clock size={14} className="animate-spin" /> 下载中 ({pct}%)
      </span>
    );
  }
  if (currentStatus === "cancelled")
    return (
      <span className="flex items-center gap-1 text-gray-400">
        <XCircle size={14} /> 已取消
      </span>
    );
  if (currentStatus === "not-ready")
    return (
      <span className="flex items-center gap-1 text-gray-400">
        <Clock size={14} /> 使用时准备
      </span>
    );
  if (currentStatus === "error" || errorMsg)
    return (
      <span className="flex items-center gap-1 text-red-600" title={errorMsg || "未知错误"}>
        <AlertCircle size={14} /> 错误
      </span>
    );
  return null;
}

function engineLabel(engineId: string): string {
  const labels: Record<string, string> = {
    "whisper-cpp": "Whisper.cpp",
    "parakeet-mlx": "Parakeet MLX",
    sensevoice: "SenseVoice",
    "custom-command": "自定义命令",
  };
  return labels[engineId] ?? engineId;
}

export default function ModelsPage() {
  const [models, setModels] = useState<AsrModelInfo[]>([]);
  const [downloads, setDownloads] = useState<Record<string, ModelDownloadProgress>>({});
  const [loading, setLoading] = useState(true);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<AsrModelInfo | null>(null);
  const [modelsPath, setModelsPath] = useState("~/Tools/Local-LLM/whisper-models");
  const [message, setMessage] = useState<{ type: "ok" | "err"; text: string } | null>(null);

  const refresh = () => {
    setLoading(true);
    Promise.all([scanModels(), getSettings()])
      .then(([nextModels, settings]) => {
        setModels(nextModels);
        setModelsPath(settings.models_path);
      })
      .catch((err) => setMessage({ type: "err", text: `扫描模型失败：${err}` }))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    refresh();
  }, []);

  useEffect(() => {
    let unlistenFn: (() => void) | undefined;

    listen<ModelDownloadProgress>("model-download-updated", (event) => {
      const p = event.payload;
      setDownloads((prev) => ({ ...prev, [p.model_id]: p }));
      if (p.status === "done" || p.status === "cancelled" || p.status === "error") {
        refresh();
      }
    })
      .then((unsub) => {
        unlistenFn = unsub;
      })
      .catch(console.error);

    return () => {
      if (unlistenFn) unlistenFn();
    };
  }, []);

  const handleDelete = async (modelId: string) => {
    setMessage(null);
    setDeleting(modelId);
    try {
      await deleteModel(modelId);
      setMessage({ type: "ok", text: "模型已删除" });
      setDownloads((prev) => {
        const next = { ...prev };
        delete next[modelId];
        return next;
      });
      refresh();
    } catch (err) {
      setMessage({ type: "err", text: `删除失败：${err}` });
    } finally {
      setDeleting(null);
      setPendingDelete(null);
    }
  };

  const handleDownload = async (modelId: string) => {
    setMessage(null);
    try {
      setDownloads((prev) => ({
        ...prev,
        [modelId]: {
          model_id: modelId,
          bytes_downloaded: 0,
          total_bytes: 0,
          progress: 0,
          status: "downloading",
          error: null,
        },
      }));
      await downloadModel(modelId);
    } catch (err) {
      setMessage({ type: "err", text: `启动下载失败：${err}` });
      setDownloads((prev) => {
        const next = { ...prev };
        delete next[modelId];
        return next;
      });
    }
  };

  const handleCancelDownload = async (modelId: string) => {
    try {
      await cancelModelDownload(modelId);
    } catch (err) {
      setMessage({ type: "err", text: `取消下载失败：${err}` });
    }
  };

  if (loading && models.length === 0) return <div className="text-gray-500">正在扫描模型...</div>;

  const visibleModels = models.filter(
    (model) => model.engine_id !== "sensevoice" && model.engine_id !== "custom-command"
  );
  const engineGroups = visibleModels.reduce<Record<string, AsrModelInfo[]>>((acc, model) => {
    (acc[model.engine_id] ??= []).push(model);
    return acc;
  }, {});

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-2xl font-bold text-gray-900 dark:text-white">ASR 语音识别模型</h2>
        <button
          onClick={refresh}
          className="flex items-center gap-2 text-sm text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-white"
        >
          <RefreshCw size={16} /> 刷新
        </button>
      </div>

      {message && (
        <div
          className={`mb-4 rounded-md border px-3 py-2 text-sm ${
            message.type === "ok"
              ? "border-green-200 bg-green-50 text-green-700 dark:border-green-900/60 dark:bg-green-950/30 dark:text-green-300"
              : "border-red-200 bg-red-50 text-red-700 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-300"
          }`}
        >
          {message.text}
        </div>
      )}

      {Object.entries(engineGroups).map(([engineId, engineModels]) => (
        <div key={engineId} className="mb-8">
          <h3 className="text-lg font-semibold text-gray-800 dark:text-gray-200 mb-3">
            {engineLabel(engineId)}
          </h3>
          <div className="grid gap-3">
            {engineModels.map((model) => {
              const downloadInfo = downloads[model.id];
              const isDownloading = downloadInfo?.status === "downloading";
              const showProgress = isDownloading && downloadInfo;

              return (
                <div
                  key={model.id}
                  className="bg-white dark:bg-gray-800 rounded-lg p-4 border border-gray-200 dark:border-gray-700 shadow-sm animate-fade-in"
                >
                  <div className="grid gap-4 sm:grid-cols-[minmax(0,1fr)_10rem]">
                    <div className="min-w-0">
                      <h4 className="font-medium text-gray-900 dark:text-white">{model.name}</h4>
                      <p className="text-sm text-gray-500 mt-1">{model.description}</p>
                      <div className="flex flex-wrap gap-2 mt-2">
                        {model.languages.map((lang) => (
                          <span
                            key={lang}
                            className="text-xs bg-gray-100 dark:bg-gray-700 px-2 py-0.5 rounded"
                          >
                            {lang}
                          </span>
                        ))}
                      </div>
                    </div>
                    <div className="flex flex-col items-end justify-between gap-3 sm:pt-1">
                      <StatusBadge status={model.status} downloadInfo={downloadInfo} />
                      <div className="flex items-center gap-2">
                        {model.size_mb && (
                          <span className="text-xs text-gray-400">{model.size_mb} MB</span>
                        )}
                        {model.status === "available" && model.download_url && !isDownloading && (
                          <button
                            type="button"
                            onClick={() => handleDownload(model.id)}
                            className="inline-flex items-center gap-1 rounded bg-blue-50 px-2 py-1 text-xs font-medium text-blue-700 hover:bg-blue-100 dark:bg-blue-900/30 dark:text-blue-300"
                          >
                            <Download size={12} />
                            应用内下载
                          </button>
                        )}
                        {isDownloading && (
                          <button
                            type="button"
                            onClick={() => handleCancelDownload(model.id)}
                            className="inline-flex items-center gap-1 rounded bg-red-50 px-2 py-1 text-xs font-medium text-red-700 hover:bg-red-100 dark:bg-red-900/30 dark:text-red-300"
                          >
                            取消
                          </button>
                        )}
                        {downloadInfo?.status === "error" && (
                          <button
                            type="button"
                            onClick={() => handleDownload(model.id)}
                            className="inline-flex items-center gap-1 rounded bg-amber-50 px-2 py-1 text-xs font-medium text-amber-700 hover:bg-amber-100 dark:bg-amber-900/30 dark:text-amber-300"
                          >
                            重试下载
                          </button>
                        )}
                        {model.status === "downloaded" && model.engine_id === "whisper-cpp" && (
                          <button
                            type="button"
                            onClick={() => setPendingDelete(model)}
                            disabled={deleting === model.id}
                            className="text-red-500 hover:text-red-700 disabled:opacity-50"
                            title="删除模型"
                          >
                            <Trash2 size={14} />
                          </button>
                        )}
                      </div>
                    </div>
                  </div>

                  {showProgress && (
                    <div className="mt-4 w-full">
                      <div className="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-1.5 overflow-hidden">
                        <div
                          className="bg-blue-600 h-full rounded-full transition-all duration-300"
                          style={{ width: `${(downloadInfo.progress * 100).toFixed(1)}%` }}
                        />
                      </div>
                      <div className="flex justify-between items-center mt-1.5 text-[10px] text-gray-400">
                        <span>{(downloadInfo.progress * 100).toFixed(0)}%</span>
                        {downloadInfo.total_bytes > 0 && (
                          <span>
                            {(downloadInfo.bytes_downloaded / 1024 / 1024).toFixed(1)} MB /{" "}
                            {(downloadInfo.total_bytes / 1024 / 1024).toFixed(1)} MB
                          </span>
                        )}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      ))}

      {pendingDelete && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/35 p-4">
          <div className="w-full max-w-md rounded-lg border border-gray-200 bg-white p-5 shadow-xl dark:border-gray-700 dark:bg-gray-800">
            <div className="mb-4 flex items-start gap-3">
              <div className="rounded-full bg-red-50 p-2 text-red-600 dark:bg-red-950/40 dark:text-red-300">
                <AlertCircle size={20} />
              </div>
              <div className="min-w-0">
                <h3 className="font-semibold text-gray-900 dark:text-white">删除模型</h3>
                <p className="mt-1 text-sm text-gray-600 dark:text-gray-300">
                  将从本机移除 {pendingDelete.name}，需要时可以重新下载。
                </p>
              </div>
            </div>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setPendingDelete(null)}
                disabled={deleting === pendingDelete.id}
                className="rounded-md border border-gray-300 px-3 py-1.5 text-sm text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
              >
                取消
              </button>
              <button
                type="button"
                onClick={() => handleDelete(pendingDelete.id)}
                disabled={deleting === pendingDelete.id}
                className="rounded-md bg-red-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50"
              >
                {deleting === pendingDelete.id ? "删除中..." : "删除"}
              </button>
            </div>
          </div>
        </div>
      )}

      <div className="mt-6 text-xs text-gray-400 space-y-1">
        <p>Whisper 模型路径：{modelsPath}</p>
        <p>模型会自动下载并安全放置到上述目录。下载过程可随时取消，失败后可一键重试。</p>
        <p>Parakeet 模型：首次使用时自动缓存，无需手动下载；需要本机已安装 uv。</p>
      </div>
    </div>
  );
}
