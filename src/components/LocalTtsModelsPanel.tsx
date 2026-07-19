import { useCallback, useEffect, useState } from "react";
import {
  CheckCircle2,
  Download,
  ExternalLink,
  FileInput,
  HardDrive,
  AlertCircle,
  Clock,
  RefreshCw,
  Trash2,
  Unlink,
  Volume2,
  WandSparkles,
} from "lucide-react";

import { useI18n } from "../lib/i18n";
import {
  cancelTtsModelDownload,
  deleteTtsModel,
  downloadTtsModel,
  forgetTtsModelPath,
  listTtsModels,
  openDialog,
  openUrl,
  registerTtsModelPath,
  type ModelDownloadProgress,
  type TtsModelInfo,
} from "../lib/tauri";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { Card } from "./ui/Card";
import { Progress } from "./ui/Progress";

interface LocalTtsModelsPanelProps {
  refreshSignal: number;
  downloadProgress: Record<string, ModelDownloadProgress>;
  onDownloadStateChange: (modelId: string, progress: ModelDownloadProgress | null) => void;
}

export function LocalTtsModelsPanel({
  refreshSignal,
  downloadProgress,
  onDownloadStateChange,
}: LocalTtsModelsPanelProps) {
  const { t } = useI18n();
  const [models, setModels] = useState<TtsModelInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyModel, setBusyModel] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<TtsModelInfo | null>(null);
  const [message, setMessage] = useState<{ type: "ok" | "err"; text: string } | null>(null);

  const refresh = useCallback(() => {
    setLoading(true);
    listTtsModels()
      .then(setModels)
      .catch((error) => setMessage({ type: "err", text: String(error) }))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh, refreshSignal]);

  const chooseExisting = async (model: TtsModelInfo) => {
    setMessage(null);
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: t("models.ttsChooseTitle", { name: model.name }),
    });
    if (!selected) return;
    const path = Array.isArray(selected) ? selected[0] : selected;
    setBusyModel(model.id);
    try {
      await registerTtsModelPath(model.id, path);
      setMessage({ type: "ok", text: t("models.ttsReuseSuccess", { name: model.name }) });
      refresh();
    } catch (error) {
      setMessage({ type: "err", text: String(error) });
    } finally {
      setBusyModel(null);
    }
  };

  const forgetExternal = async (model: TtsModelInfo) => {
    setBusyModel(model.id);
    setMessage(null);
    try {
      await forgetTtsModelPath(model.id);
      setMessage({ type: "ok", text: t("models.ttsForgetSuccess", { name: model.name }) });
      refresh();
    } catch (error) {
      setMessage({ type: "err", text: String(error) });
    } finally {
      setBusyModel(null);
    }
  };

  const startDownload = async (model: TtsModelInfo) => {
    setBusyModel(model.id);
    setMessage(null);
    onDownloadStateChange(model.id, {
      model_id: model.id,
      bytes_downloaded: 0,
      total_bytes: model.size_mb * 1024 * 1024,
      progress: 0,
      status: "downloading",
      phase: "downloading",
      error: null,
    });
    try {
      await downloadTtsModel(model.id);
    } catch (error) {
      const previous = downloadProgress[model.id];
      onDownloadStateChange(model.id, {
        ...(previous ?? {
          model_id: model.id,
          bytes_downloaded: 0,
          total_bytes: model.size_mb * 1024 * 1024,
          progress: 0,
          status: "error",
          phase: "error",
          error: null,
        }),
        status: "error",
        phase: "error",
        error: String(error),
      });
      setMessage({ type: "err", text: String(error) });
    } finally {
      setBusyModel(null);
    }
  };

  const cancelDownload = async (model: TtsModelInfo) => {
    try {
      await cancelTtsModelDownload(model.id);
    } catch (error) {
      setMessage({ type: "err", text: String(error) });
    }
  };

  const deleteManaged = async (model: TtsModelInfo) => {
    setBusyModel(model.id);
    setMessage(null);
    try {
      await deleteTtsModel(model.id);
      onDownloadStateChange(model.id, null);
      setMessage({ type: "ok", text: t("models.ttsDeleteSuccess", { name: model.name }) });
      refresh();
    } catch (error) {
      setMessage({ type: "err", text: String(error) });
    } finally {
      setBusyModel(null);
      setPendingDelete(null);
    }
  };

  const readyCount = models.filter((model) => model.status === "ready").length;

  return (
    <section className="space-y-4" aria-labelledby="local-tts-heading">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <div className="flex flex-wrap items-center gap-2">
            <h3 id="local-tts-heading" className="font-display text-h3 font-semibold text-text-secondary">
              {t("models.ttsLocalTitle")}
            </h3>
            <Badge variant="success">{t("models.ttsReadyCount", { count: readyCount })}</Badge>
          </div>
          <p className="mt-1 max-w-3xl text-sm leading-6 text-text-tertiary">
            {t("models.ttsLocalDesc")}
          </p>
        </div>
        <Button type="button" variant="ghost" size="sm" onClick={refresh} disabled={loading}>
          <RefreshCw size={14} className={loading ? "animate-spin" : ""} />
          {t("models.ttsRescan")}
        </Button>
      </div>

      {message && (
        <div className={`rounded-xl border px-4 py-3 text-sm font-semibold ${
          message.type === "ok"
            ? "border-success/20 bg-success/10 text-success"
            : "border-danger/20 bg-danger/10 text-danger"
        }`}>
          {message.text}
        </div>
      )}

      <div className="grid gap-3.5">
        {models.map((model) => {
          const download = downloadProgress[model.id];
          return (
            <Card key={model.id} className="p-4">
              <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_21rem]">
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="liquid-icon grid h-9 w-9 place-items-center rounded-xl text-brand">
                      {model.clone_only ? <WandSparkles size={17} /> : <Volume2 size={17} />}
                    </span>
                    <h4 className="font-display text-base font-semibold text-text-primary">{model.name}</h4>
                    <Badge variant="default">{model.family.toUpperCase()}</Badge>
                    {model.clone_only && <Badge variant="warning">{t("models.ttsCloneOnly")}</Badge>}
                  </div>
                  <p className="mt-2 text-sm leading-6 text-text-secondary">{model.description}</p>
                  <div className="mt-3 flex flex-wrap gap-1.5">
                    {model.languages.map((language) => (
                      <Badge key={language} variant="default" className="border-none bg-surface-overlay font-normal text-text-secondary">
                        {language}
                      </Badge>
                    ))}
                    {!model.clone_only && (
                      <Badge variant="default" className="border-none bg-surface-overlay font-normal text-text-secondary">
                        {t("models.ttsVoiceCount", { count: model.voices.length })}
                      </Badge>
                    )}
                    <Badge variant="default" className="border-none bg-surface-overlay font-normal text-text-secondary">
                      {(model.sample_rate / 1000).toFixed(0)} kHz
                    </Badge>
                  </div>

                  {model.path && (
                    <div className="mt-3 min-w-0 rounded-xl border border-border-subtle bg-surface-overlay px-3 py-2.5 text-xs leading-5">
                      <div className="flex flex-wrap items-center gap-2">
                        <HardDrive size={14} className="shrink-0 text-success" />
                        <span className="font-semibold text-text-primary">
                          {model.location === "external" ? t("models.ttsExternalPath") : t("models.ttsManagedPath")}
                        </span>
                      </div>
                      <p className="mt-1 break-all font-mono text-text-tertiary">{model.path}</p>
                    </div>
                  )}

                  {model.status === "incomplete" && model.missing_files.length > 0 && (
                    <p className="mt-3 text-xs leading-5 text-warning">
                      {t("models.ttsMissing", { files: model.missing_files.slice(0, 4).join(" · ") })}
                    </p>
                  )}
                  {model.status !== "ready" && model.extra_download_urls.length > 0 && (
                    <p className="mt-2 text-xs leading-5 text-text-tertiary">
                      {t("models.ttsExtraDownloadNotice")}
                    </p>
                  )}
                </div>

                <div className="flex flex-col items-start justify-between gap-3 lg:items-end">
                  {download?.status === "downloading" ? (
                    <span className="inline-flex items-center gap-1.5 text-sm font-semibold text-warning">
                      <Clock size={15} className="animate-spin" />
                      {t("models.ttsDownloading", { progress: Math.round((download?.progress ?? 0) * 100) })}
                    </span>
                  ) : download?.status === "error" ? (
                    <span className="inline-flex items-center gap-1.5 text-sm font-semibold text-danger" title={download.error ?? undefined}>
                      <AlertCircle size={15} /> {t("models.ttsDownloadFailed")}
                    </span>
                  ) : model.status === "ready" ? (
                    <span className="inline-flex items-center gap-1.5 text-sm font-semibold text-success">
                      <CheckCircle2 size={15} /> {t("models.ttsReady")}
                    </span>
                  ) : model.status === "incomplete" ? (
                    <span className="text-sm font-semibold text-warning">{t("models.ttsIncomplete")}</span>
                  ) : (
                    <span className="text-sm font-semibold text-text-tertiary">{t("models.ttsNotFound")}</span>
                  )}

                  <div className="flex w-full flex-wrap gap-2 lg:justify-end">
                    {model.status !== "ready" && download?.status !== "downloading" && (
                      <Button
                        type="button"
                        variant="primary"
                        size="sm"
                        onClick={() => startDownload(model)}
                        disabled={busyModel === model.id}
                      >
                        <Download size={14} />
                        {download?.status === "error" ? t("models.ttsRetryDownload") : t("models.ttsDownload")}
                      </Button>
                    )}
                    {model.status !== "ready" && download?.status !== "downloading" && (
                      <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        onClick={() => chooseExisting(model)}
                        disabled={busyModel === model.id}
                      >
                        <FileInput size={14} />
                        {t("models.ttsUseExisting")}
                      </Button>
                    )}
                    {model.status !== "ready" && download?.status !== "downloading" && (
                      <Button type="button" variant="secondary" size="sm" onClick={() => openUrl(model.download_url)}>
                        <ExternalLink size={14} />
                        {t("models.ttsOfficialSource")}
                      </Button>
                    )}
                    {download?.status === "downloading" && (
                      <Button type="button" variant="danger" size="sm" onClick={() => cancelDownload(model)}>
                        {t("models.ttsCancelDownload")}
                      </Button>
                    )}
                    {model.status === "ready" && model.location === "external" && (
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={() => forgetExternal(model)}
                        disabled={busyModel === model.id}
                        title={t("models.ttsForgetHint")}
                      >
                        <Unlink size={14} />
                        {t("models.ttsForget")}
                      </Button>
                    )}
                    {model.status === "ready" && model.location === "managed" && (
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={() => setPendingDelete(model)}
                        disabled={busyModel === model.id}
                        title={t("models.ttsDeleteHint")}
                      >
                        <Trash2 size={14} />
                        {t("models.ttsDelete")}
                      </Button>
                    )}
                    <span className="self-center font-mono text-xs text-text-tertiary">{model.size_mb} MB</span>
                  </div>
                </div>
              </div>

              {download && download.status !== "done" && (
                <div className="mt-4 w-full space-y-1.5">
                  <Progress value={Number(((download.progress ?? 0) * 100).toFixed(1))} />
                  <div className="flex flex-wrap items-center justify-between gap-2 font-mono text-xs text-text-tertiary">
                    <span>
                      {download.phase === "verifying"
                        ? t("models.phaseVerifying")
                        : download.phase === "installing"
                          ? t("models.phaseInstalling")
                          : download.status === "cancelled"
                            ? t("models.phasePaused")
                            : t("models.phaseDownloading")} · {Math.round((download.progress ?? 0) * 100)}%
                    </span>
                    {Boolean(download.total_bytes) && (
                      <span>
                        {(download.bytes_downloaded / 1024 / 1024).toFixed(1)} MB / {(download.total_bytes / 1024 / 1024).toFixed(1)} MB
                      </span>
                    )}
                  </div>
                  {download.status === "error" && download.error && (
                    <p className="break-words text-xs leading-5 text-danger">{download.error}</p>
                  )}
                </div>
              )}
            </Card>
          );
        })}
      </div>

      <div className="rounded-2xl border border-brand/15 bg-brand/5 px-4 py-3 text-sm leading-6 text-text-secondary">
        <span className="font-semibold text-text-primary">{t("models.ttsReuseNoticeTitle")}</span>{" "}
        {t("models.ttsReuseNotice")}
      </div>

      {pendingDelete && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4 backdrop-blur-sm">
          <Card className="w-full max-w-md border border-border-default bg-surface-overlay p-6 shadow-lg">
            <div className="mb-5 flex items-start gap-3">
              <div className="rounded-full bg-danger/10 p-2 text-danger"><AlertCircle size={20} /></div>
              <div className="min-w-0">
                <h3 className="font-semibold text-text-primary text-h2 mb-1.5">{t("models.ttsDeleteModalTitle")}</h3>
                <p className="text-sm leading-6 text-text-secondary">{t("models.ttsDeleteModalDesc", { name: pendingDelete.name })}</p>
              </div>
            </div>
            <div className="flex justify-end gap-2.5">
              <Button type="button" variant="secondary" size="sm" onClick={() => setPendingDelete(null)} disabled={busyModel === pendingDelete.id}>
                {t("common.cancel")}
              </Button>
              <Button type="button" variant="danger" size="sm" onClick={() => deleteManaged(pendingDelete)} disabled={busyModel === pendingDelete.id}>
                {busyModel === pendingDelete.id ? t("models.ttsDeleting") : t("models.ttsDeleteModalConfirm")}
              </Button>
            </div>
          </Card>
        </div>
      )}
    </section>
  );
}
