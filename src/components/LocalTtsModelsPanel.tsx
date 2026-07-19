import { useCallback, useEffect, useState } from "react";
import {
  CheckCircle2,
  ExternalLink,
  FileInput,
  HardDrive,
  RefreshCw,
  Unlink,
  Volume2,
  WandSparkles,
} from "lucide-react";

import { useI18n } from "../lib/i18n";
import {
  forgetTtsModelPath,
  listTtsModels,
  openDialog,
  openUrl,
  registerTtsModelPath,
  type TtsModelInfo,
} from "../lib/tauri";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { Card } from "./ui/Card";

interface LocalTtsModelsPanelProps {
  refreshSignal: number;
}

export function LocalTtsModelsPanel({ refreshSignal }: LocalTtsModelsPanelProps) {
  const { t } = useI18n();
  const [models, setModels] = useState<TtsModelInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyModel, setBusyModel] = useState<string | null>(null);
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
        {models.map((model) => (
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
                {model.status === "ready" ? (
                  <span className="inline-flex items-center gap-1.5 text-sm font-semibold text-success">
                    <CheckCircle2 size={15} /> {t("models.ttsReady")}
                  </span>
                ) : model.status === "incomplete" ? (
                  <span className="text-sm font-semibold text-warning">{t("models.ttsIncomplete")}</span>
                ) : (
                  <span className="text-sm font-semibold text-text-tertiary">{t("models.ttsNotFound")}</span>
                )}

                <div className="flex w-full flex-wrap gap-2 lg:justify-end">
                  {model.status !== "ready" && (
                    <Button
                      type="button"
                      variant="primary"
                      size="sm"
                      onClick={() => chooseExisting(model)}
                      disabled={busyModel === model.id}
                    >
                      <FileInput size={14} />
                      {t("models.ttsUseExisting")}
                    </Button>
                  )}
                  {model.status !== "ready" && (
                    <Button type="button" variant="secondary" size="sm" onClick={() => openUrl(model.download_url)}>
                      <ExternalLink size={14} />
                      {t("models.ttsGetModel")}
                    </Button>
                  )}
                  {model.status !== "ready" && model.extra_download_urls.map((url) => (
                    <Button key={url} type="button" variant="secondary" size="sm" onClick={() => openUrl(url)}>
                      <ExternalLink size={14} />
                      {t("models.ttsGetVocoder")}
                    </Button>
                  ))}
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
                  <span className="self-center font-mono text-xs text-text-tertiary">{model.size_mb} MB</span>
                </div>
              </div>
            </div>
          </Card>
        ))}
      </div>

      <div className="rounded-2xl border border-brand/15 bg-brand/5 px-4 py-3 text-sm leading-6 text-text-secondary">
        <span className="font-semibold text-text-primary">{t("models.ttsReuseNoticeTitle")}</span>{" "}
        {t("models.ttsReuseNotice")}
      </div>
    </section>
  );
}
