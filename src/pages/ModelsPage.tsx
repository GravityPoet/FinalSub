import { useEffect, useState } from "react";
import { useI18n } from "../lib/i18n";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  scanModels,
  deleteModel,
  getSettings,
  downloadModel,
  cancelModelDownload,
  importLocalModel,
  importSensevoiceModel,
  type AsrModelInfo,
  type ModelDownloadProgress,
} from "../lib/tauri";
import { Download, CheckCircle, AlertCircle, Clock, Trash2, RefreshCw, XCircle, FileInput } from "lucide-react";

import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { Badge } from "../components/ui/Badge";
import { Progress } from "../components/ui/Progress";

function StatusBadge({
  status,
  downloadInfo,
}: {
  status: AsrModelInfo["status"];
  downloadInfo?: ModelDownloadProgress;
}) {
  const { t } = useI18n();
  const currentStatus = downloadInfo ? downloadInfo.status : status;
  const errorMsg =
    downloadInfo && downloadInfo.status === "error"
      ? downloadInfo.error
      : typeof status === "object" && "error" in status
      ? status.error
      : undefined;

  if (currentStatus === "available")
    return (
      <span className="flex items-center gap-1.5 text-sm font-semibold text-brand">
        <Download size={15} /> {t("models.notInstalled")}
      </span>
    );
  if (currentStatus === "downloaded" || currentStatus === "done")
    return (
      <span className="flex items-center gap-1.5 text-sm font-semibold text-success">
        <CheckCircle size={15} /> {t("models.downloaded")}
      </span>
    );
  if (currentStatus === "downloading") {
    const pct = downloadInfo ? Math.round(downloadInfo.progress * 100) : 0;
    return (
      <span className="flex items-center gap-1.5 text-sm font-semibold text-warning">
        <Clock size={15} className="animate-spin" /> {t("models.downloading")} ({pct}%)
      </span>
    );
  }
  if (currentStatus === "cancelled")
    return (
      <span className="flex items-center gap-1.5 text-sm font-semibold text-text-tertiary">
        <XCircle size={15} /> {t("models.cancelled")}
      </span>
    );
  if (currentStatus === "not-ready")
    return (
      <span className="flex items-center gap-1.5 text-sm font-semibold text-text-tertiary">
        <Clock size={15} /> {t("models.lazyLoad")}
      </span>
    );
  if (currentStatus === "error" || errorMsg)
    return (
      <span className="flex items-center gap-1.5 text-sm font-semibold text-danger" title={errorMsg || t("common.error")}>
        <AlertCircle size={15} /> {t("models.error")}
      </span>
    );
  return null;
}

export default function ModelsPage() {
  const { t } = useI18n();
  const [models, setModels] = useState<AsrModelInfo[]>([]);
  const [downloads, setDownloads] = useState<Record<string, ModelDownloadProgress>>({});
  const [loading, setLoading] = useState(true);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<AsrModelInfo | null>(null);
  const [modelsPath, setModelsPath] = useState("~/Tools/Local-LLM/whisper-models");
  const [message, setMessage] = useState<{ type: "ok" | "err"; text: string } | null>(null);

  const engineLabel = (engineId: string): string => {
    const labels: Record<string, string> = {
      "whisper-cpp": "Whisper.cpp",
      "parakeet-mlx": "Parakeet MLX",
      sensevoice: "SenseVoice",
      "custom-command": t("models.customCommand"),
    };
    return labels[engineId] ?? engineId;
  };

  const refresh = () => {
    setLoading(true);
    Promise.all([scanModels(), getSettings()])
      .then(([nextModels, settings]) => {
        setModels(nextModels);
        setModelsPath(settings.models_path);
      })
      .catch((err) => setMessage({ type: "err", text: `${t("models.scanFailed")}${err}` }))
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
      setMessage({ type: "ok", text: t("models.deleted") });
      setDownloads((prev) => {
        const next = { ...prev };
        delete next[modelId];
        return next;
      });
      refresh();
    } catch (err) {
      setMessage({ type: "err", text: `${t("models.deleteFailed")}${err}` });
    } finally {
      setDeleting(null);
      setPendingDelete(null);
    }
  };

  const handleImportModel = async (modelId: string) => {
    setMessage(null);
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "Whisper Model", extensions: ["bin"] }],
      });
      if (!selected) return;

      const path = Array.isArray(selected) ? selected[0] : selected;
      setMessage({ type: "ok", text: t("models.importing") });
      await importLocalModel(modelId, path);
      setMessage({ type: "ok", text: t("models.importSuccess") });
      refresh();
    } catch (err) {
      setMessage({ type: "err", text: t("models.importFailed", { error: String(err) }) });
    }
  };

  const handleImportSensevoice = async () => {
    setMessage(null);
    try {
      const onnx = await open({
        multiple: false,
        filters: [{ name: "ONNX Model", extensions: ["onnx"] }],
      });
      if (!onnx) return;
      const tokens = await open({
        multiple: false,
        filters: [{ name: "Tokens", extensions: ["txt"] }],
      });
      if (!tokens) return;
      const onnxPath = Array.isArray(onnx) ? onnx[0] : onnx;
      const tokensPath = Array.isArray(tokens) ? tokens[0] : tokens;
      setMessage({ type: "ok", text: t("models.importingSensevoice") });
      await importSensevoiceModel(onnxPath, tokensPath);
      setMessage({ type: "ok", text: t("models.importSensevoiceSuccess") });
      refresh();
    } catch (err) {
      setMessage({ type: "err", text: t("models.importSensevoiceFailed", { error: String(err) }) });
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
      setMessage({ type: "err", text: `${t("models.downloadStartFailed")}${err}` });
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
      setMessage({ type: "err", text: `${t("models.downloadCancelFailed")}${err}` });
    }
  };

  if (loading && models.length === 0) {
    return <div className="text-text-tertiary py-16 text-center text-sm">{t("models.scanning")}</div>;
  }

  const visibleModels = models.filter(
    (model) => model.engine_id !== "custom-command"
  );
  const engineGroups = visibleModels.reduce<Record<string, AsrModelInfo[]>>((acc, model) => {
    (acc[model.engine_id] ??= []).push(model);
    return acc;
  }, {});

  return (
    <div className="page-shell space-y-7">
      <div className="flex items-center justify-between">
        <h2 className="font-display text-display font-bold tracking-tight text-text-primary">{t("models.title")}</h2>
        <Button
          onClick={refresh}
          variant="secondary"
          size="sm"
        >
          <RefreshCw size={15} />
          <span>{t("models.refresh")}</span>
        </Button>
      </div>

      {message && (
        <div
          className={`rounded-xl border px-4 py-3 text-sm font-semibold leading-6 ${
            message.type === "ok"
              ? "border-success/20 bg-success/10 text-success"
              : "border-danger/20 bg-danger/10 text-danger"
          }`}
        >
          {message.text}
        </div>
      )}

      {Object.entries(engineGroups).map(([engineId, engineModels]) => (
        <div key={engineId} className="space-y-4">
          <h3 className="font-display text-h3 font-semibold text-text-secondary">
            {engineLabel(engineId)}
          </h3>
          <div className="grid gap-3.5">
            {engineModels.map((model) => {
              const downloadInfo = downloads[model.id];
              const isDownloading = downloadInfo?.status === "downloading";
              const showProgress = isDownloading && downloadInfo;

              return (
                <Card
                  key={model.id}
                  className="p-4"
                >
                  <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_20rem]">
                    <div className="min-w-0">
                      <h4 className="font-display text-base font-semibold tracking-tight text-text-primary">{model.name}</h4>
                      <p className="mt-1.5 text-sm leading-6 text-text-secondary">{model.description}</p>
                      <div className="flex flex-wrap gap-1.5 mt-3">
                        {model.languages.map((lang) => (
                          <Badge
                            key={lang}
                            variant="default"
                            className="font-normal border-none bg-surface-overlay text-text-secondary"
                          >
                            {lang}
                          </Badge>
                        ))}
                      </div>
                    </div>
                    <div className="flex flex-col items-start justify-between gap-3 lg:items-end lg:pt-1">
                      <StatusBadge status={model.status} downloadInfo={downloadInfo} />
                      <div className="flex w-full flex-wrap items-center justify-start gap-2 lg:justify-end">
                        {model.size_mb && (
                          <span className="mr-1.5 shrink-0 font-mono text-sm text-text-tertiary">{model.size_mb} MB</span>
                        )}
                        {model.status === "available" && !isDownloading && (
                          <div className="flex flex-wrap justify-start gap-2 lg:justify-end">
                            {model.engine_id !== "sensevoice" && model.download_url && (
                              <Button
                                type="button"
                                onClick={() => handleDownload(model.id)}
                                variant="primary"
                                size="sm"
                                className="min-w-[7rem]"
                              >
                                <Download size={14} />
                                <span>{t("models.downloadAction")}</span>
                              </Button>
                            )}
                            {model.engine_id === "whisper-cpp" && (
                              <Button
                                type="button"
                                onClick={() => handleImportModel(model.id)}
                                variant="secondary"
                                size="sm"
                                className="min-w-[7.5rem]"
                              >
                                <FileInput size={14} />
                                <span>{t("models.importLocalAction")}</span>
                              </Button>
                            )}
                            {model.engine_id === "sensevoice" && (
                              <Button
                                type="button"
                                onClick={handleImportSensevoice}
                                variant="secondary"
                                size="sm"
                                className="min-w-[7.5rem]"
                              >
                                <FileInput size={14} />
                                <span>{t("models.importAction")}</span>
                              </Button>
                            )}
                          </div>
                        )}
                        {isDownloading && (
                          <Button
                            type="button"
                            onClick={() => handleCancelDownload(model.id)}
                            variant="danger"
                            size="sm"
                          >
                            <span>{t("models.cancelAction")}</span>
                          </Button>
                        )}
                        {downloadInfo?.status === "error" && (
                          <Button
                            type="button"
                            onClick={() => handleDownload(model.id)}
                            size="sm"
                            className="border-warning/20 bg-warning/10 text-warning hover:bg-warning/20"
                          >
                            <span>{t("models.retryAction")}</span>
                          </Button>
                        )}
                        {model.status === "downloaded" && model.engine_id === "whisper-cpp" && (
                          <button
                            type="button"
                            onClick={() => setPendingDelete(model)}
                            disabled={deleting === model.id}
                            className="rounded-lg p-2 text-text-tertiary transition hover:bg-surface-overlay hover:text-danger disabled:opacity-50"
                            title={t("models.deleteAction")}
                          >
                            <Trash2 size={16} />
                          </button>
                        )}
                      </div>
                    </div>
                  </div>

                  {showProgress && (
                    <div className="mt-4 w-full space-y-1.5">
                      <Progress value={Number((downloadInfo.progress * 100).toFixed(1))} />
                      <div className="flex items-center justify-between font-mono text-xs text-text-tertiary">
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
                </Card>
              );
            })}
          </div>
        </div>
      ))}

      {pendingDelete && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm p-4">
          <Card className="w-full max-w-md bg-surface-overlay p-6 shadow-lg border border-border-default">
            <div className="mb-5 flex items-start gap-3">
              <div className="rounded-full bg-danger/10 p-2 text-danger">
                <AlertCircle size={20} />
              </div>
              <div className="min-w-0">
                <h3 className="font-semibold text-text-primary text-h2 mb-1.5">{t("models.deleteModalTitle")}</h3>
                <p className="text-sm leading-6 text-text-secondary">
                  {t("models.deleteModalDesc").replace("{name}", pendingDelete.name)}
                </p>
              </div>
            </div>
            <div className="flex justify-end gap-2.5">
              <Button
                type="button"
                onClick={() => setPendingDelete(null)}
                disabled={deleting === pendingDelete.id}
                variant="secondary"
                size="sm"
              >
                {t("models.deleteModalCancel")}
              </Button>
              <Button
                type="button"
                onClick={() => handleDelete(pendingDelete.id)}
                disabled={deleting === pendingDelete.id}
                variant="danger"
                size="sm"
              >
                {deleting === pendingDelete.id ? t("models.deleting") : t("models.deleteModalConfirm")}
              </Button>
            </div>
          </Card>
        </div>
      )}

      <div className="mt-8 space-y-1.5 text-sm leading-6 text-text-tertiary">
        <p>{t("models.pathInfo")}{modelsPath}</p>
        <p>{t("models.pathDesc")}</p>
        <p>{t("models.parakeetDesc")}</p>
      </div>
    </div>
  );
}
