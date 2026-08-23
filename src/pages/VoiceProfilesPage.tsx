import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  AlertTriangle,
  AudioLines,
  CheckCircle2,
  Cloud,
  CloudDownload,
  Download,
  FileText,
  FolderOpen,
  HardDrive,
  Import,
  Languages,
  LoaderCircle,
  Mic,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  ShieldCheck,
  Sparkles,
  Trash2,
  UserRound,
  Volume2,
  X,
} from "lucide-react";

import { VoiceRecorder } from "../components/VoiceRecorder";
import { Badge } from "../components/ui/Badge";
import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { Input, Select, Textarea } from "../components/ui/Input";
import { useI18n } from "../lib/i18n";
import {
  createCloudVoiceProfile,
  createVoiceProfile,
  deleteCloudVoiceRemote,
  discardPreparedVoiceSample,
  discardVoiceRecording,
  exportVoiceProfile,
  fileAssetUrl,
  importVoiceProfile,
  inspectVoiceSource,
  linkCloudVoiceProfile,
  listCloudVoices,
  listTtsProviders,
  listVoiceSubtitleCues,
  listVoiceProfiles,
  openDialog,
  prepareVoiceSample,
  removeVoiceProfile,
  renameVoiceProfile,
  refreshCloudVoiceStatus,
  retrainCloudVoiceProfile,
  saveDialog,
  type PreparedVoiceSample,
  type CloudVoiceSummary,
  type TtsProviderProfile,
  type VoiceCloneEngine,
  type VoiceProfile,
  type VoiceProfileLanguage,
  type VoiceQualityIssue,
  type VoiceQualityVerdict,
  type VoiceSourceInfo,
  type VoiceSubtitleCue,
} from "../lib/tauri";

const MEDIA_EXTENSIONS = ["wav", "mp3", "m4a", "aac", "flac", "ogg", "opus", "mp4", "mov", "mkv", "webm", "avi", "m4v", "ts"];

function safeFileName(value: string): string {
  return value.replace(/[\\/:*?"<>|]/g, "-").trim() || "FinalSub Voice";
}

function absorbSubtitleCue(
  cues: VoiceSubtitleCue[],
  index: number,
  limits: { min: number; ideal: number; max: number },
): { startMs: number; endMs: number; text: string } | null {
  const first = cues[index];
  if (!first) return null;
  const startMs = Math.max(0, first.start_ms - 150);
  let endMs = first.end_ms;
  let speechMs = Math.max(0, first.end_ms - first.start_ms);
  const selectedText = [first.text];
  for (let nextIndex = index + 1; nextIndex < cues.length; nextIndex += 1) {
    const next = cues[nextIndex];
    if (next.start_ms - endMs > 2_000) break;
    if (next.end_ms + 150 - startMs > limits.max) break;
    endMs = next.end_ms;
    speechMs += Math.max(0, next.end_ms - next.start_ms);
    selectedText.push(next.text);
    if (speechMs >= limits.ideal) break;
  }
  return {
    startMs,
    endMs: Math.min(startMs + limits.max, endMs + 150),
    text: selectedText.join(" ").trim(),
  };
}

function qualityBadge(verdict: VoiceQualityVerdict, t: ReturnType<typeof useI18n>["t"]) {
  if (verdict === "good") return <Badge variant="success">{t("voices.qualityGood")}</Badge>;
  if (verdict === "fair") return <Badge variant="warning">{t("voices.qualityFair")}</Badge>;
  return <Badge variant="danger">{t("voices.qualityPoor")}</Badge>;
}

function issueText(
  issue: VoiceQualityIssue,
  t: ReturnType<typeof useI18n>["t"],
  engine: Extract<VoiceCloneEngine, "zipvoice" | "elevenlabs" | "volcengine">,
): string {
  const value = issue.value ?? 0;
  const keys = {
    "no-speech": "voices.issueNoSpeech",
    "too-short": "voices.issueTooShort",
    "short-for-engine": engine === "elevenlabs"
      ? "voices.cloudIssueShort"
      : engine === "volcengine"
        ? "voices.volcIssueShort"
        : "voices.issueShort",
    "low-snr": "voices.issueLowSnr",
    clipping: "voices.issueClipping",
    "low-volume": "voices.issueLowVolume",
    "low-speech-ratio": "voices.issueSpeechRatio",
    "long-silence": "voices.issueLongSilence",
  } as const;
  return t(keys[issue.code], {
    seconds: (value / 1000).toFixed(1),
    value: value < 1 ? Math.round(value * 100) : Math.round(value),
  });
}

function CreateVoiceDialog({
  open,
  providers,
  onClose,
  onCreated,
}: {
  open: boolean;
  providers: TtsProviderProfile[];
  onClose: () => void;
  onCreated: (profile: VoiceProfile) => void;
}) {
  const { t } = useI18n();
  const [step, setStep] = useState<1 | 2 | 3>(1);
  const [showRecorder, setShowRecorder] = useState(false);
  const [engine, setEngine] = useState<Extract<VoiceCloneEngine, "zipvoice" | "elevenlabs" | "volcengine">>("zipvoice");
  const [providerId, setProviderId] = useState("");
  const [source, setSource] = useState<VoiceSourceInfo | null>(null);
  const [subtitlePath, setSubtitlePath] = useState("");
  const [subtitleCues, setSubtitleCues] = useState<VoiceSubtitleCue[]>([]);
  const [subtitleBusy, setSubtitleBusy] = useState(false);
  const [recordingPath, setRecordingPath] = useState("");
  const [startMs, setStartMs] = useState(0);
  const [durationMs, setDurationMs] = useState(8000);
  const [prepared, setPrepared] = useState<PreparedVoiceSample | null>(null);
  const [name, setName] = useState("");
  const [language, setLanguage] = useState<VoiceProfileLanguage>("zh");
  const [referenceText, setReferenceText] = useState("");
  const [consent, setConsent] = useState(false);
  const [uploadConsent, setUploadConsent] = useState(false);
  const [removeBackgroundNoise, setRemoveBackgroundNoise] = useState(false);
  const [enableMss, setEnableMss] = useState(false);
  const [localDenoise, setLocalDenoise] = useState(false);
  const [cloudVoiceId, setCloudVoiceId] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const elevenlabsProviders = useMemo(
    () => providers.filter((provider) => provider.protocol === "elevenlabs"),
    [providers],
  );
  const volcengineProviders = useMemo(
    () => providers.filter((provider) => provider.protocol === "volcengine"),
    [providers],
  );
  const limits = engine === "elevenlabs"
    ? { min: 5000, ideal: 60000, max: 180000 }
    : engine === "volcengine"
      ? { min: 5000, ideal: 14000, max: 30000 }
      : { min: 3000, ideal: 8000, max: 10000 };

  const reset = async () => {
    if (prepared?.token) await discardPreparedVoiceSample(prepared.token).catch(() => undefined);
    if (recordingPath) await discardVoiceRecording(recordingPath).catch(() => undefined);
    setStep(1);
    setShowRecorder(false);
    setEngine("zipvoice");
    setProviderId("");
    setSource(null);
    setSubtitlePath("");
    setSubtitleCues([]);
    setSubtitleBusy(false);
    setRecordingPath("");
    setStartMs(0);
    setDurationMs(8000);
    setPrepared(null);
    setName("");
    setLanguage("zh");
    setReferenceText("");
    setConsent(false);
    setUploadConsent(false);
    setRemoveBackgroundNoise(false);
    setEnableMss(false);
    setLocalDenoise(false);
    setCloudVoiceId("");
    setBusy(false);
    setError("");
  };

  useEffect(() => {
    if (!open) void reset();
    // Reset is intentionally tied to the dialog lifecycle, not every field change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const close = async () => {
    if (busy) return;
    await reset();
    onClose();
  };

  const selectEngine = (next: Extract<VoiceCloneEngine, "zipvoice" | "elevenlabs" | "volcengine">) => {
    setEngine(next);
    const targetProviders = next === "elevenlabs" ? elevenlabsProviders : next === "volcengine" ? volcengineProviders : [];
    setProviderId((current) => targetProviders.some((provider) => provider.id === current)
      ? current
      : targetProviders[0]?.id ?? "");
    setUploadConsent(false);
    setCloudVoiceId("");
    setError("");
  };

  const useSource = async (path: string, fromRecording = false) => {
    setBusy(true);
    setError("");
    try {
      const info = await inspectVoiceSource(path);
      if (info.duration_ms < limits.min) {
        throw new Error(
          engine === "elevenlabs"
            ? t("voices.cloudSourceTooShort")
            : engine === "volcengine"
              ? t("voices.volcSourceTooShort")
              : t("voices.sourceTooShort"),
        );
      }
      setSource(info);
      setRecordingPath(fromRecording ? info.path : "");
      setStartMs(0);
      setDurationMs(Math.min(limits.ideal, Math.max(limits.min, info.duration_ms)));
      setName(info.file_name.replace(/\.[^.]+$/, ""));
      setShowRecorder(false);
      setStep(2);
    } catch (reason) {
      if (fromRecording) await discardVoiceRecording(path).catch(() => undefined);
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  const chooseFile = async () => {
    const selected = await openDialog({
      multiple: false,
      filters: [{ name: t("voices.mediaFiles"), extensions: MEDIA_EXTENSIONS }],
    });
    if (typeof selected === "string") await useSource(selected);
  };

  const chooseSubtitle = async () => {
    const selected = await openDialog({
      multiple: false,
      filters: [{ name: t("voices.subtitleFiles"), extensions: ["srt", "vtt", "ass", "ssa", "lrc"] }],
    });
    if (typeof selected !== "string") return;
    setSubtitleBusy(true);
    setError("");
    try {
      const cues = await listVoiceSubtitleCues(selected);
      setSubtitlePath(selected);
      setSubtitleCues(cues);
    } catch (reason) {
      setSubtitlePath("");
      setSubtitleCues([]);
      setError(String(reason));
    } finally {
      setSubtitleBusy(false);
    }
  };

  const chooseSubtitleCue = async (index: number) => {
    if (!source) return;
    const range = absorbSubtitleCue(subtitleCues, index, limits);
    if (!range) return;
    const start = Math.min(Math.max(0, range.startMs), Math.max(0, source.duration_ms - limits.min));
    const available = Math.max(limits.min, source.duration_ms - start);
    setStartMs(start);
    setDurationMs(Math.min(available, Math.max(limits.min, range.endMs - range.startMs)));
    if (engine === "zipvoice" && range.text) setReferenceText(range.text);
    await clearPrepared();
  };

  const clearPrepared = async () => {
    if (prepared?.token) await discardPreparedVoiceSample(prepared.token).catch(() => undefined);
    setPrepared(null);
  };

  const analyze = async () => {
    if (!source) return;
    setBusy(true);
    setError("");
    try {
      await clearPrepared();
      const result = await prepareVoiceSample({
        source_path: source.path,
        start_ms: startMs,
        duration_ms: durationMs,
        engine,
        local_denoise: engine === "zipvoice" && localDenoise,
      });
      setPrepared(result);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  const create = async () => {
    if (!prepared) return;
    setBusy(true);
    setError("");
    try {
      const profile = engine === "zipvoice"
        ? await createVoiceProfile({
          token: prepared.token,
          name,
          language,
          reference_text: referenceText,
          consent,
        })
        : await createCloudVoiceProfile({
          token: prepared.token,
          name,
          language,
          provider_id: providerId,
          consent,
          upload_consent: uploadConsent,
          voice_id: cloudVoiceId,
          remove_background_noise: removeBackgroundNoise,
          enable_mss: enableMss,
        });
      setPrepared(null);
      onCreated(profile);
      await reset();
      onClose();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  const maxStartMs = Math.max(0, (source?.duration_ms ?? limits.min) - limits.min);
  const maxDurationMs = Math.max(
    limits.min,
    Math.min(limits.max, (source?.duration_ms ?? limits.max) - startMs),
  );
  const previewUrl = prepared ? fileAssetUrl(prepared.audio_path) : "";

  if (!open) return null;
  return (
    <div className="fixed inset-0 z-[70] flex items-center justify-center bg-black/50 p-3 backdrop-blur-xl" role="dialog" aria-modal="true" aria-labelledby="create-voice-title" onMouseDown={(event) => { if (event.target === event.currentTarget) void close(); }}>
      <Card className="flex max-h-[92vh] w-full max-w-3xl flex-col overflow-hidden border border-border-default bg-surface-raised p-0 shadow-2xl">
        <div className="z-10 flex shrink-0 items-start justify-between gap-4 border-b border-border-subtle bg-surface-raised px-5 py-4">
          <div>
            <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-brand">{t("voices.wizardEyebrow", { current: step })}</p>
            <h2 id="create-voice-title" className="mt-1 font-display text-h2 font-bold text-text-primary">{engine !== "zipvoice" ? t("voices.createCloudTitle") : t("voices.createTitle")}</h2>
          </div>
          <Button type="button" variant="ghost" size="sm" onClick={() => void close()} disabled={busy}><X size={16} /> {t("common.close")}</Button>
        </div>
        <div className="min-h-0 overflow-y-auto p-5 sm:p-6">
          <div className="mb-6 grid grid-cols-3 gap-2" aria-label={t("voices.wizardProgress")}>
            {[1, 2, 3].map((number) => <div key={number} className={`h-1.5 rounded-full ${number <= step ? "bg-brand" : "bg-border-default"}`} />)}
          </div>
          {error && <div className="mb-4 rounded-xl border border-danger/20 bg-danger/10 px-4 py-3 text-sm leading-6 text-danger">{error}</div>}

          {step === 1 && (
            <div className="space-y-5">
              <div>
                <h3 className="font-display text-lg font-semibold text-text-primary">{t("voices.engineTitle")}</h3>
                <p className="mt-1 text-sm leading-6 text-text-tertiary">{t("voices.engineDesc")}</p>
              </div>
              <div className="grid gap-3 sm:grid-cols-3" role="radiogroup" aria-label={t("voices.engineTitle")}>
                <button
                  type="button"
                  role="radio"
                  aria-checked={engine === "zipvoice"}
                  onClick={() => selectEngine("zipvoice")}
                  className={`min-h-28 rounded-2xl border p-4 text-left transition ${engine === "zipvoice" ? "liquid-selected border-brand/30" : "border-border-default bg-surface-card hover:bg-surface-raised"}`}
                >
                  <span className="flex items-center gap-2 text-sm font-semibold text-text-primary"><HardDrive size={17} className="text-brand" /> {t("voices.localEngine")}</span>
                  <span className="mt-2 block text-xs leading-5 text-text-tertiary">{t("voices.localEngineDesc")}</span>
                </button>
                <button
                  type="button"
                  role="radio"
                  aria-checked={engine === "elevenlabs"}
                  onClick={() => selectEngine("elevenlabs")}
                  disabled={elevenlabsProviders.length === 0}
                  className={`min-h-28 rounded-2xl border p-4 text-left transition disabled:cursor-not-allowed disabled:opacity-50 ${engine === "elevenlabs" ? "liquid-selected border-brand/30" : "border-border-default bg-surface-card hover:bg-surface-raised"}`}
                >
                  <span className="flex items-center gap-2 text-sm font-semibold text-text-primary"><Cloud size={17} className="text-brand" /> {t("voices.cloudEngine")}</span>
                  <span className="mt-2 block text-xs leading-5 text-text-tertiary">{elevenlabsProviders.length > 0 ? t("voices.cloudEngineDesc") : t("voices.cloudProviderMissing")}</span>
                </button>
                <button
                  type="button"
                  role="radio"
                  aria-checked={engine === "volcengine"}
                  onClick={() => selectEngine("volcengine")}
                  disabled={volcengineProviders.length === 0}
                  className={`min-h-28 rounded-2xl border p-4 text-left transition disabled:cursor-not-allowed disabled:opacity-50 ${engine === "volcengine" ? "liquid-selected border-brand/30" : "border-border-default bg-surface-card hover:bg-surface-raised"}`}
                >
                  <span className="flex items-center gap-2 text-sm font-semibold text-text-primary"><Cloud size={17} className="text-brand" /> {t("voices.volcEngine")}</span>
                  <span className="mt-2 block text-xs leading-5 text-text-tertiary">{volcengineProviders.length > 0 ? t("voices.volcEngineDesc") : t("voices.volcProviderMissing")}</span>
                </button>
              </div>
              {engine !== "zipvoice" && (
                <label className="block space-y-1.5 text-xs font-semibold text-text-secondary">
                  <span>{t("voices.cloudProvider")}</span>
                  <Select value={providerId} onChange={(event) => setProviderId(event.target.value)}>
                    {(engine === "elevenlabs" ? elevenlabsProviders : volcengineProviders).map((provider) => <option key={provider.id} value={provider.id}>{provider.name}</option>)}
                  </Select>
                </label>
              )}
              <div>
                <h3 className="font-display text-lg font-semibold text-text-primary">{t("voices.sourceTitle")}</h3>
                <p className="mt-1 text-sm leading-6 text-text-tertiary">{engine === "elevenlabs" ? t("voices.cloudSourceDesc") : engine === "volcengine" ? t("voices.volcSourceDesc") : t("voices.sourceDesc")}</p>
              </div>
              {!showRecorder ? (
                <div className="grid gap-3 sm:grid-cols-2">
                  <button type="button" onClick={chooseFile} disabled={busy} className="rounded-2xl border border-border-default bg-surface-card p-5 text-left transition hover:border-brand/30 hover:bg-surface-raised disabled:opacity-50">
                    <span className="liquid-icon grid h-11 w-11 place-items-center rounded-xl text-brand"><FolderOpen size={20} /></span>
                    <span className="mt-4 block text-sm font-semibold text-text-primary">{t("voices.chooseMedia")}</span>
                    <span className="mt-1 block text-xs leading-5 text-text-tertiary">{t("voices.chooseMediaDesc")}</span>
                  </button>
                  <button type="button" onClick={() => setShowRecorder(true)} disabled={busy} className="rounded-2xl border border-border-default bg-surface-card p-5 text-left transition hover:border-brand/30 hover:bg-surface-raised disabled:opacity-50">
                    <span className="liquid-icon grid h-11 w-11 place-items-center rounded-xl text-brand"><Mic size={20} /></span>
                    <span className="mt-4 block text-sm font-semibold text-text-primary">{t("voices.recordEntry")}</span>
                    <span className="mt-1 block text-xs leading-5 text-text-tertiary">{t("voices.recordEntryDesc")}</span>
                  </button>
                </div>
              ) : <VoiceRecorder onCancel={() => setShowRecorder(false)} onConfirm={(path) => useSource(path, true)} />}
              <div className="rounded-2xl border border-border-subtle bg-surface-card p-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="flex min-w-0 items-start gap-2"><FileText size={17} className="mt-0.5 shrink-0 text-brand" /><div className="min-w-0"><p className="text-sm font-semibold text-text-primary">{t("voices.subtitleOptional")}</p><p className="mt-1 truncate text-xs leading-5 text-text-tertiary">{subtitlePath || t("voices.subtitleOptionalDesc")}</p></div></div>
                  <Button type="button" variant="secondary" size="sm" onClick={() => void chooseSubtitle()} disabled={subtitleBusy}>{subtitleBusy ? <LoaderCircle size={14} className="animate-spin" /> : <FileText size={14} />} {t("voices.chooseSubtitle")}</Button>
                </div>
              </div>
              <div className="flex items-start gap-2 rounded-xl border border-success/15 bg-success/8 px-3.5 py-3 text-xs leading-5 text-text-secondary">
                {engine !== "zipvoice" ? <Cloud size={15} className="mt-0.5 shrink-0 text-brand" /> : <ShieldCheck size={15} className="mt-0.5 shrink-0 text-success" />}
                {engine === "elevenlabs" ? t("voices.cloudPrivacy") : engine === "volcengine" ? t("voices.volcPrivacy") : t("voices.localPrivacy")}
              </div>
            </div>
          )}

          {step === 2 && source && (
            <div className="space-y-5">
              <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
                <div className="min-w-0">
                  <h3 className="font-display text-lg font-semibold text-text-primary">{t("voices.segmentTitle")}</h3>
                  <p className="mt-1 truncate text-sm text-text-tertiary">{source.file_name} · {(source.duration_ms / 1000).toFixed(1)}s</p>
                </div>
                <div className="flex flex-wrap gap-2">
                  <Button type="button" variant="secondary" size="sm" onClick={() => void chooseSubtitle()} disabled={busy || subtitleBusy}>{subtitleBusy ? <LoaderCircle size={14} className="animate-spin" /> : <FileText size={14} />} {subtitleCues.length > 0 ? t("voices.changeSubtitle") : t("voices.chooseSubtitle")}</Button>
                  <Button type="button" variant="secondary" size="sm" onClick={async () => { await clearPrepared(); if (recordingPath) await discardVoiceRecording(recordingPath).catch(() => undefined); setRecordingPath(""); setStep(1); setSource(null); setSubtitlePath(""); setSubtitleCues([]); }} disabled={busy}>{t("voices.changeSource")}</Button>
                </div>
              </div>
              {subtitleCues.length > 0 && <div className="space-y-3 rounded-2xl border border-brand/20 bg-brand/5 p-4">
                <div className="flex items-start gap-2"><FileText size={17} className="mt-0.5 shrink-0 text-brand" /><div><p className="text-sm font-semibold text-text-primary">{t("voices.subtitleCues")}</p><p className="mt-1 text-xs leading-5 text-text-tertiary">{t("voices.subtitleCueHint")}</p></div></div>
                <div className="grid max-h-52 gap-2 overflow-y-auto pr-1">
                  {subtitleCues.map((cue, index) => <button key={`${cue.start_ms}-${index}`} type="button" onClick={() => void chooseSubtitleCue(index)} className="flex items-start gap-3 rounded-xl border border-border-subtle bg-surface-card px-3 py-2.5 text-left transition hover:border-brand/40 hover:bg-surface-raised"><span className="w-12 shrink-0 font-mono text-[11px] text-brand">{(cue.start_ms / 1000).toFixed(1)}s</span><span className="line-clamp-2 text-xs leading-5 text-text-secondary">{cue.text}</span></button>)}
                </div>
              </div>}
              <div className="grid gap-4 rounded-2xl border border-border-subtle bg-surface-card p-4 sm:grid-cols-2">
                <label className="space-y-2 text-xs font-semibold text-text-secondary">
                  <span className="flex justify-between"><span>{t("voices.segmentStart")}</span><span className="font-mono text-brand">{(startMs / 1000).toFixed(1)}s</span></span>
                  <input type="range" min={0} max={maxStartMs} step={100} value={startMs} onChange={(event) => { const next = Number(event.target.value); setStartMs(next); setDurationMs((current) => Math.min(current, Math.max(limits.min, Math.min(limits.max, source.duration_ms - next)))); void clearPrepared(); }} className="w-full accent-brand" />
                </label>
                <label className="space-y-2 text-xs font-semibold text-text-secondary">
                  <span className="flex justify-between"><span>{t("voices.segmentDuration")}</span><span className="font-mono text-brand">{(durationMs / 1000).toFixed(1)}s</span></span>
                  <input type="range" min={limits.min} max={maxDurationMs} step={100} value={Math.min(durationMs, maxDurationMs)} onChange={(event) => { setDurationMs(Number(event.target.value)); void clearPrepared(); }} className="w-full accent-brand" />
                </label>
                <p className="sm:col-span-2 text-xs leading-5 text-text-tertiary">{engine === "elevenlabs" ? t("voices.cloudSegmentHint") : engine === "volcengine" ? t("voices.volcSegmentHint") : t("voices.segmentHint")}</p>
              </div>
              {engine === "zipvoice" && <label className="flex cursor-pointer items-start gap-3 rounded-2xl border border-border-subtle bg-surface-card p-4 text-xs leading-5 text-text-secondary"><input type="checkbox" checked={localDenoise} onChange={(event) => { setLocalDenoise(event.target.checked); void clearPrepared(); }} className="mt-1 h-4 w-4 accent-brand" /><span><strong className="block text-sm text-text-primary">{t("voices.localDenoise")}</strong>{t("voices.localDenoiseDesc")}</span></label>}
              <Button type="button" variant="primary" size="lg" onClick={analyze} disabled={busy} className="w-full sm:w-auto">
                {busy ? <LoaderCircle size={16} className="animate-spin" /> : <Sparkles size={16} />} {busy ? t("voices.analyzing") : prepared ? t("voices.reanalyze") : t("voices.analyze")}
              </Button>
              {prepared && (
                <div className="space-y-4 rounded-2xl border border-border-default bg-surface-card p-4">
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <div className="flex items-center gap-2"><Volume2 size={17} className="text-brand" /><span className="text-sm font-semibold text-text-primary">{t("voices.qualityTitle")}</span>{qualityBadge(prepared.quality.verdict, t)}</div>
                    {previewUrl ? <audio controls preload="metadata" src={previewUrl} className="h-9 max-w-full" /> : <span className="text-xs text-text-tertiary">{t("voices.previewInApp")}</span>}
                  </div>
                  <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                    <div className="rounded-xl bg-surface-overlay p-3"><p className="font-mono text-base font-semibold text-text-primary">{(prepared.quality.speech_ms / 1000).toFixed(1)}s</p><p className="text-[11px] text-text-tertiary">{t("voices.speechTime")}</p></div>
                    <div className="rounded-xl bg-surface-overlay p-3"><p className="font-mono text-base font-semibold text-text-primary">{Math.round(prepared.quality.speech_ratio * 100)}%</p><p className="text-[11px] text-text-tertiary">{t("voices.speechRatio")}</p></div>
                    <div className="rounded-xl bg-surface-overlay p-3"><p className="font-mono text-base font-semibold text-text-primary">{prepared.quality.snr_db.toFixed(0)} dB</p><p className="text-[11px] text-text-tertiary">SNR</p></div>
                    <div className="rounded-xl bg-surface-overlay p-3"><p className="font-mono text-base font-semibold text-text-primary">{prepared.quality.rms_db.toFixed(0)} dB</p><p className="text-[11px] text-text-tertiary">RMS</p></div>
                  </div>
                  {prepared.quality.issues.length === 0 ? (
                    <p className="flex items-start gap-2 text-xs leading-5 text-success"><CheckCircle2 size={15} className="mt-0.5 shrink-0" /> {t("voices.qualityPassed")}</p>
                  ) : (
                    <div className="space-y-2">{prepared.quality.issues.map((issue, index) => <p key={`${issue.code}-${index}`} className={`flex items-start gap-2 text-xs leading-5 ${issue.severity === "error" ? "text-danger" : issue.severity === "warning" ? "text-warning" : "text-brand"}`}><AlertTriangle size={14} className="mt-0.5 shrink-0" /> {issueText(issue, t, engine)}</p>)}</div>
                  )}
                </div>
              )}
              <div className="flex justify-end"><Button type="button" variant="primary" onClick={() => setStep(3)} disabled={!prepared?.can_create || busy}>{t("voices.nextDetails")}</Button></div>
            </div>
          )}

          {step === 3 && prepared && (
            <div className="space-y-5">
              <div>
                <h3 className="font-display text-lg font-semibold text-text-primary">{t("voices.detailsTitle")}</h3>
                <p className="mt-1 text-sm leading-6 text-text-tertiary">{engine !== "zipvoice" ? (engine === "volcengine" ? t("voices.volcDetailsDesc") : t("voices.cloudDetailsDesc")) : t("voices.detailsDesc")}</p>
              </div>
              <div className="grid gap-4 sm:grid-cols-2">
                <label className="space-y-1.5 text-xs font-semibold text-text-secondary"><span>{t("voices.name")}</span><Input value={name} onChange={(event) => setName(event.target.value)} maxLength={60} /></label>
                <label className="space-y-1.5 text-xs font-semibold text-text-secondary"><span>{t("voices.language")}</span><Select value={language} onChange={(event) => setLanguage(event.target.value as VoiceProfileLanguage)}><option value="zh">{t("language.zh")}</option><option value="en">{t("language.en")}</option></Select></label>
              </div>
              {engine === "zipvoice" && (
                <label className="block space-y-1.5 text-xs font-semibold text-text-secondary">
                  <span>{t("voices.referenceText")}</span>
                  <Textarea rows={4} value={referenceText} onChange={(event) => setReferenceText(event.target.value)} placeholder={t("voices.referenceTextPlaceholder")} maxLength={4000} />
                  <span className="block font-normal leading-5 text-text-tertiary">{t("voices.referenceTextHint")}</span>
                </label>
              )}
              {engine === "volcengine" && (
                <>
                  <label className="block space-y-1.5 text-xs font-semibold text-text-secondary"><span>{t("voices.cloudVoiceId")}</span><Input value={cloudVoiceId} onChange={(event) => setCloudVoiceId(event.target.value)} placeholder="S_..." /></label>
                  <div className="grid gap-3 sm:grid-cols-2">
                    <label className="flex cursor-pointer items-start gap-3 rounded-xl border border-border-subtle bg-surface-card p-3 text-xs leading-5 text-text-secondary"><input type="checkbox" checked={removeBackgroundNoise} onChange={(event) => setRemoveBackgroundNoise(event.target.checked)} className="mt-1 h-4 w-4 accent-brand" /><span><strong className="block text-text-primary">{t("voices.removeNoiseTitle")}</strong>{t("voices.volcRemoveNoiseDesc")}</span></label>
                    <label className="flex cursor-pointer items-start gap-3 rounded-xl border border-border-subtle bg-surface-card p-3 text-xs leading-5 text-text-secondary"><input type="checkbox" checked={enableMss} onChange={(event) => setEnableMss(event.target.checked)} className="mt-1 h-4 w-4 accent-brand" /><span><strong className="block text-text-primary">{t("voices.volcMssTitle")}</strong>{t("voices.volcMssDesc")}</span></label>
                  </div>
                </>
              )}
              <label className="flex cursor-pointer items-start gap-3 rounded-2xl border border-border-subtle bg-surface-card p-4 text-sm leading-6 text-text-secondary">
                <input type="checkbox" checked={consent} onChange={(event) => setConsent(event.target.checked)} className="mt-1 h-4 w-4 accent-brand" />
                <span><strong className="block text-text-primary">{t("voices.consentTitle")}</strong>{t("voices.consentDesc")}</span>
              </label>
              {engine !== "zipvoice" && (
                <>
                  <label className="flex cursor-pointer items-start gap-3 rounded-2xl border border-brand/20 bg-brand/5 p-4 text-sm leading-6 text-text-secondary">
                    <input type="checkbox" checked={uploadConsent} onChange={(event) => setUploadConsent(event.target.checked)} className="mt-1 h-4 w-4 accent-brand" />
                    <span><strong className="block text-text-primary">{t("voices.uploadConsentTitle")}</strong>{t("voices.uploadConsentDesc")}</span>
                  </label>
                  {engine === "elevenlabs" && <label className="flex cursor-pointer items-start gap-3 rounded-2xl border border-border-subtle bg-surface-card p-4 text-sm leading-6 text-text-secondary">
                    <input type="checkbox" checked={removeBackgroundNoise} onChange={(event) => setRemoveBackgroundNoise(event.target.checked)} className="mt-1 h-4 w-4 accent-brand" />
                    <span><strong className="block text-text-primary">{t("voices.removeNoiseTitle")}</strong>{t("voices.removeNoiseDesc")}</span>
                  </label>}
                </>
              )}
              <div className="flex flex-wrap justify-between gap-2">
                <Button type="button" variant="secondary" onClick={() => setStep(2)} disabled={busy}>{t("voices.backQuality")}</Button>
                <Button type="button" variant="primary" size="lg" onClick={create} disabled={busy || !name.trim() || !consent || (engine === "zipvoice" ? !referenceText.trim() : !providerId || !uploadConsent || (engine === "volcengine" && !cloudVoiceId.trim()))}>
                  {busy ? <LoaderCircle size={16} className="animate-spin" /> : <CheckCircle2 size={16} />} {busy ? t("voices.creating") : t("voices.createAction")}
                </Button>
              </div>
            </div>
          )}
        </div>
      </Card>
    </div>
  );
}

function CloudVoiceRecoveryDialog({
  open,
  providers,
  linkedProfiles,
  onClose,
  onLinked,
}: {
  open: boolean;
  providers: TtsProviderProfile[];
  linkedProfiles: VoiceProfile[];
  onClose: () => void;
  onLinked: (profile: VoiceProfile) => void;
}) {
  const { t } = useI18n();
  const cloudProviders = useMemo(
    () => providers.filter((provider) => provider.protocol === "elevenlabs" || provider.protocol === "volcengine"),
    [providers],
  );
  const [providerId, setProviderId] = useState("");
  const [remoteVoices, setRemoteVoices] = useState<CloudVoiceSummary[]>([]);
  const [voiceId, setVoiceId] = useState("");
  const [name, setName] = useState("");
  const [language, setLanguage] = useState<VoiceProfileLanguage>("zh");
  const [consent, setConsent] = useState(false);
  const [loadingVoices, setLoadingVoices] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const selectedProvider = cloudProviders.find((provider) => provider.id === providerId) ?? null;
  const linkedRemoteIds = useMemo(
    () => new Set(linkedProfiles.filter((profile) => profile.provider_id === providerId).map((profile) => profile.cloud_voice_id)),
    [linkedProfiles, providerId],
  );

  useEffect(() => {
    if (!open) return;
    const first = cloudProviders[0];
    setProviderId(first?.id ?? "");
    setRemoteVoices([]);
    setVoiceId("");
    setName("");
    setLanguage("zh");
    setConsent(false);
    setLoadingVoices(false);
    setBusy(false);
    setError("");
  }, [open, cloudProviders]);

  useEffect(() => {
    if (!open || selectedProvider?.protocol !== "elevenlabs" || !providerId) return;
    setLoadingVoices(true);
    setError("");
    listCloudVoices(providerId)
      .then(setRemoteVoices)
      .catch((reason) => setError(String(reason)))
      .finally(() => setLoadingVoices(false));
  }, [open, providerId, selectedProvider?.protocol]);

  const chooseRemoteVoice = (voice: CloudVoiceSummary) => {
    setVoiceId(voice.voice_id);
    setName(voice.name);
  };

  const link = async () => {
    if (!providerId || !voiceId.trim() || !name.trim() || !consent) return;
    setBusy(true);
    setError("");
    try {
      const profile = await linkCloudVoiceProfile({
        name,
        language,
        provider_id: providerId,
        voice_id: voiceId,
        consent,
      });
      onLinked(profile);
      onClose();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  if (!open) return null;
  return (
    <div className="fixed inset-0 z-[72] flex items-center justify-center bg-black/50 p-3 backdrop-blur-xl" role="dialog" aria-modal="true" aria-labelledby="recover-cloud-voice-title" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onClose(); }}>
      <Card className="flex max-h-[92vh] w-full max-w-2xl flex-col overflow-hidden border border-border-default bg-surface-raised p-0 shadow-2xl">
        <div className="flex shrink-0 items-start justify-between gap-4 border-b border-border-subtle px-5 py-4">
          <div>
            <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-brand">{t("voices.recoveryEyebrow")}</p>
            <h2 id="recover-cloud-voice-title" className="mt-1 font-display text-h2 font-bold text-text-primary">{t("voices.recoveryTitle")}</h2>
            <p className="mt-1 text-sm leading-6 text-text-tertiary">{t("voices.recoveryDesc")}</p>
          </div>
          <Button type="button" variant="ghost" size="sm" onClick={onClose} disabled={busy}><X size={16} /> {t("common.close")}</Button>
        </div>
        <div className="min-h-0 space-y-5 overflow-y-auto p-5 sm:p-6">
          {cloudProviders.length === 0 ? (
            <div className="rounded-2xl border border-dashed border-border-default px-4 py-8 text-center text-sm leading-6 text-text-tertiary">{t("voices.cloudProviderMissing")}</div>
          ) : (
            <>
              <label className="block space-y-1.5 text-xs font-semibold text-text-secondary">
                <span>{t("voices.cloudProvider")}</span>
                <Select value={providerId} onChange={(event) => { setProviderId(event.target.value); setVoiceId(""); setName(""); setError(""); }}>
                  {cloudProviders.map((provider) => <option key={provider.id} value={provider.id}>{provider.name} · {provider.protocol === "elevenlabs" ? t("voices.elevenLabsShort") : t("voices.volcEngineShort")}</option>)}
                </Select>
              </label>
              {selectedProvider?.protocol === "elevenlabs" ? (
                <div className="space-y-3 rounded-2xl border border-border-subtle bg-surface-card p-4">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <p className="text-sm font-semibold text-text-primary">{t("voices.remoteListTitle")}</p>
                      <p className="mt-1 text-xs leading-5 text-text-tertiary">{t("voices.remoteListDesc")}</p>
                    </div>
                    <Button type="button" variant="secondary" size="sm" onClick={() => { setLoadingVoices(true); listCloudVoices(providerId).then(setRemoteVoices).catch((reason) => setError(String(reason))).finally(() => setLoadingVoices(false)); }} disabled={loadingVoices}>
                      {loadingVoices ? <LoaderCircle size={14} className="animate-spin" /> : <CloudDownload size={14} />} {t("voices.refreshRemote")}
                    </Button>
                  </div>
                  {remoteVoices.length === 0 ? (
                    <p className="rounded-xl bg-surface-overlay px-3 py-3 text-xs leading-5 text-text-tertiary">{loadingVoices ? t("voices.loadingRemote") : t("voices.remoteEmpty")}</p>
                  ) : (
                    <div className="grid gap-2 sm:grid-cols-2">
                      {remoteVoices.map((voice) => {
                        const linked = linkedRemoteIds.has(voice.voice_id);
                        return <button key={voice.voice_id} type="button" disabled={linked} onClick={() => chooseRemoteVoice(voice)} className={`rounded-xl border px-3 py-3 text-left transition ${voice.voice_id === voiceId ? "liquid-selected border-brand/30" : "border-border-subtle bg-surface-raised hover:bg-surface-overlay"} ${linked ? "cursor-not-allowed opacity-45" : ""}`}><span className="block truncate text-sm font-semibold text-text-primary">{voice.name}</span><span className="mt-1 block truncate font-mono text-[11px] text-text-tertiary">{linked ? t("voices.alreadyLinked") : voice.voice_id}</span></button>;
                      })}
                    </div>
                  )}
                </div>
              ) : (
                <div className="rounded-2xl border border-brand/20 bg-brand/5 p-4 text-xs leading-5 text-text-secondary">
                  <p className="font-semibold text-text-primary">{t("voices.volcRecoveryTitle")}</p>
                  <p className="mt-1">{t("voices.volcRecoveryDesc")}</p>
                </div>
              )}
              <div className="grid gap-4 sm:grid-cols-2">
                <label className="space-y-1.5 text-xs font-semibold text-text-secondary"><span>{t("voices.cloudVoiceId")}</span><Input value={voiceId} onChange={(event) => setVoiceId(event.target.value)} placeholder={selectedProvider?.protocol === "volcengine" ? "S_..." : "voice_id"} /></label>
                <label className="space-y-1.5 text-xs font-semibold text-text-secondary"><span>{t("voices.name")}</span><Input value={name} onChange={(event) => setName(event.target.value)} maxLength={60} /></label>
                <label className="space-y-1.5 text-xs font-semibold text-text-secondary"><span>{t("voices.language")}</span><Select value={language} onChange={(event) => setLanguage(event.target.value as VoiceProfileLanguage)}><option value="zh">{t("language.zh")}</option><option value="en">{t("language.en")}</option></Select></label>
              </div>
              <label className="flex cursor-pointer items-start gap-3 rounded-2xl border border-border-subtle bg-surface-card p-4 text-sm leading-6 text-text-secondary"><input type="checkbox" checked={consent} onChange={(event) => setConsent(event.target.checked)} className="mt-1 h-4 w-4 accent-brand" /><span><strong className="block text-text-primary">{t("voices.linkConsentTitle")}</strong>{t("voices.linkConsentDesc")}</span></label>
              {error && <div className="rounded-xl border border-danger/20 bg-danger/10 px-4 py-3 text-sm leading-6 text-danger">{error}</div>}
              <div className="flex justify-end gap-2"><Button type="button" variant="secondary" onClick={onClose} disabled={busy}>{t("common.cancel")}</Button><Button type="button" variant="primary" onClick={() => void link()} disabled={busy || !providerId || !voiceId.trim() || !name.trim() || !consent}>{busy ? <LoaderCircle size={15} className="animate-spin" /> : <CloudDownload size={15} />} {busy ? t("voices.linking") : t("voices.linkCloud")}</Button></div>
            </>
          )}
        </div>
      </Card>
    </div>
  );
}

export default function VoiceProfilesPage() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const [profiles, setProfiles] = useState<VoiceProfile[]>([]);
  const [providers, setProviders] = useState<TtsProviderProfile[]>([]);
  const [loading, setLoading] = useState(true);
  const [wizardOpen, setWizardOpen] = useState(false);
  const [recoveryOpen, setRecoveryOpen] = useState(false);
  const [filter, setFilter] = useState<"all" | "local" | "cloud">("all");
  const [message, setMessage] = useState<{ type: "ok" | "err"; text: string } | null>(null);
  const [renaming, setRenaming] = useState<VoiceProfile | null>(null);
  const [renameDraft, setRenameDraft] = useState("");
  const [deleting, setDeleting] = useState<VoiceProfile | null>(null);
  const [deleteRemote, setDeleteRemote] = useState(false);
  const [busyId, setBusyId] = useState("");

  const refresh = () => {
    setLoading(true);
    Promise.all([listVoiceProfiles(), listTtsProviders()])
      .then(([loadedProfiles, loadedProviders]) => { setProfiles(loadedProfiles); setProviders(loadedProviders); })
      .catch((reason) => setMessage({ type: "err", text: String(reason) }))
      .finally(() => setLoading(false));
  };
  useEffect(refresh, []);

  const localProfiles = useMemo(() => profiles.filter((profile) => profile.engine === "zipvoice"), [profiles]);
  const cloudProfiles = useMemo(() => profiles.filter((profile) => profile.engine !== "zipvoice"), [profiles]);
  const visibleProfiles = useMemo(
    () => filter === "local" ? localProfiles : filter === "cloud" ? cloudProfiles : profiles,
    [cloudProfiles, filter, localProfiles, profiles],
  );

  const handleImport = async () => {
    const selected = await openDialog({ multiple: false, filters: [{ name: "SmartSub / FinalSub Voice", extensions: ["svoice"] }] });
    if (typeof selected !== "string") return;
    setMessage(null);
    setBusyId("import");
    try {
      let profile = await importVoiceProfile(selected);
      if (profile.engine === "volcengine") {
        // The package carries the slot ID; the provider supplies live status
        // and the remaining training count when the account is reachable.
        try {
          profile = await refreshCloudVoiceStatus(profile.id);
        } catch {
          // Keep the imported link usable while the provider is temporarily offline.
        }
      }
      setProfiles((current) => [profile, ...current]);
      setMessage({ type: "ok", text: t("voices.imported", { name: profile.name }) });
    } catch (reason) {
      setMessage({ type: "err", text: String(reason) });
    } finally {
      setBusyId("");
    }
  };

  const handleExport = async (profile: VoiceProfile) => {
    const output = await saveDialog({ defaultPath: `${safeFileName(profile.name)}.svoice`, filters: [{ name: "SmartSub / FinalSub Voice", extensions: ["svoice"] }] });
    if (!output) return;
    setBusyId(profile.id);
    try {
      await exportVoiceProfile(profile.id, output);
      setMessage({ type: "ok", text: t("voices.exported", { name: profile.name }) });
    } catch (reason) {
      setMessage({ type: "err", text: String(reason) });
    } finally {
      setBusyId("");
    }
  };

  const handleRename = async () => {
    if (!renaming) return;
    setBusyId(renaming.id);
    try {
      const updated = await renameVoiceProfile(renaming.id, renameDraft);
      setProfiles((current) => current.map((profile) => profile.id === updated.id ? updated : profile));
      setRenaming(null);
      setMessage({ type: "ok", text: t("voices.renamed") });
    } catch (reason) {
      setMessage({ type: "err", text: String(reason) });
    } finally {
      setBusyId("");
    }
  };

  const handleDelete = async () => {
    if (!deleting) return;
    setBusyId(deleting.id);
    try {
      if (deleteRemote) await deleteCloudVoiceRemote(deleting.id);
      await removeVoiceProfile(deleting.id);
      setProfiles((current) => current.filter((profile) => profile.id !== deleting.id));
      setMessage({ type: "ok", text: deleteRemote ? t("voices.remoteDeleted", { name: deleting.name }) : t("voices.deleted", { name: deleting.name }) });
      setDeleting(null);
      setDeleteRemote(false);
    } catch (reason) {
      setMessage({ type: "err", text: String(reason) });
    } finally {
      setBusyId("");
    }
  };

  const handleRefreshStatus = async (profile: VoiceProfile) => {
    if (profile.engine !== "volcengine") return;
    setBusyId(profile.id);
    try {
      const updated = await refreshCloudVoiceStatus(profile.id);
      setProfiles((current) => current.map((item) => item.id === updated.id ? updated : item));
      setMessage({ type: "ok", text: t("voices.statusRefreshed") });
    } catch (reason) {
      setMessage({ type: "err", text: String(reason) });
    } finally {
      setBusyId("");
    }
  };

  const handleRetrain = async (profile: VoiceProfile) => {
    if (profile.engine !== "volcengine") return;
    setBusyId(profile.id);
    try {
      const updated = await retrainCloudVoiceProfile({
        id: profile.id,
        remove_background_noise: true,
        enable_mss: false,
      });
      setProfiles((current) => current.map((item) => item.id === updated.id ? updated : item));
      setMessage({ type: "ok", text: t("voices.retrainStarted") });
    } catch (reason) {
      setMessage({ type: "err", text: String(reason) });
    } finally {
      setBusyId("");
    }
  };

  return (
    <div className="page-shell space-y-6">
      <div className="flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
        <div>
          <p className="text-[11px] font-semibold uppercase tracking-[0.18em] text-brand">{t("voices.eyebrow")}</p>
          <h2 className="mt-1 font-display text-display font-bold tracking-tight text-text-primary">{t("voices.title")}</h2>
          <p className="mt-2 max-w-3xl text-sm leading-6 text-text-tertiary">{t("voices.subtitle")}</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button type="button" variant="secondary" onClick={() => setRecoveryOpen(true)}><CloudDownload size={15} /> {t("voices.recoveryAction")}</Button>
          <Button type="button" variant="secondary" onClick={handleImport} disabled={busyId === "import"}>{busyId === "import" ? <LoaderCircle size={15} className="animate-spin" /> : <Import size={15} />} {t("voices.importAction")}</Button>
          <Button type="button" variant="primary" onClick={() => setWizardOpen(true)}><Plus size={16} /> {t("voices.createAction")}</Button>
        </div>
      </div>

      <div className="grid gap-3 sm:grid-cols-3">
        <Card className="p-4"><div className="flex items-center gap-3"><span className="liquid-icon grid h-10 w-10 place-items-center rounded-xl text-brand"><UserRound size={18} /></span><div><p className="font-display text-xl font-semibold text-text-primary">{profiles.length}</p><p className="text-xs text-text-tertiary">{t("voices.savedCount")}</p></div></div></Card>
        <Card className="p-4"><div className="flex items-center gap-3"><span className="grid h-10 w-10 place-items-center rounded-xl bg-success/10 text-success"><HardDrive size={18} /></span><div><p className="font-display text-xl font-semibold text-text-primary">{localProfiles.length}</p><p className="text-xs text-text-tertiary">{t("voices.localCount")}</p></div></div></Card>
        <Card className="p-4"><div className="flex items-center gap-3"><span className="grid h-10 w-10 place-items-center rounded-xl bg-brand/10 text-brand"><Cloud size={18} /></span><div><p className="font-display text-xl font-semibold text-text-primary">{cloudProfiles.length}</p><p className="text-xs text-text-tertiary">{t("voices.cloudCount")}</p></div></div></Card>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-border-subtle bg-surface-card p-2">
        <div className="flex flex-wrap gap-1" role="tablist" aria-label={t("voices.filterTitle")}>
          {(["all", "local", "cloud"] as const).map((value) => <button key={value} type="button" role="tab" aria-selected={filter === value} onClick={() => setFilter(value)} className={`rounded-xl px-3 py-2 text-xs font-semibold transition ${filter === value ? "liquid-selected text-text-primary" : "text-text-tertiary hover:bg-surface-overlay hover:text-text-primary"}`}>{value === "all" ? t("voices.allTab") : value === "local" ? t("voices.localTab") : t("voices.cloudTab")}</button>)}
        </div>
        <p className="px-2 text-xs text-text-tertiary">{t("voices.filterHint")}</p>
      </div>

      {message && <div className={`rounded-xl border px-4 py-3 text-sm font-semibold leading-6 ${message.type === "ok" ? "border-success/20 bg-success/10 text-success" : "border-danger/20 bg-danger/10 text-danger"}`}>{message.text}</div>}

      {loading ? <div className="py-16 text-center text-sm text-text-tertiary">{t("voices.loading")}</div> : visibleProfiles.length === 0 ? (
        <Card className="py-12 text-center">
          <span className="liquid-icon mx-auto grid h-14 w-14 place-items-center rounded-2xl text-brand"><AudioLines size={24} /></span>
          <h3 className="mt-4 font-display text-h2 font-semibold text-text-primary">{profiles.length === 0 ? t("voices.emptyTitle") : t("voices.filterEmptyTitle")}</h3>
          <p className="mx-auto mt-2 max-w-lg text-sm leading-6 text-text-tertiary">{profiles.length === 0 ? t("voices.emptyDesc") : t("voices.filterEmptyDesc")}</p>
          {profiles.length === 0 && <Button type="button" variant="primary" className="mt-5" onClick={() => setWizardOpen(true)}><Sparkles size={15} /> {t("voices.createFirst")}</Button>}
        </Card>
      ) : (
        <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
          {visibleProfiles.map((profile) => {
            const isLocal = profile.engine === "zipvoice";
            const canUseForDubbing = isLocal || profile.cloud_status === "ready";
            const audioUrl = profile.reference_audio_path ? fileAssetUrl(profile.reference_audio_path) : "";
            const provider = providers.find((item) => item.id === profile.provider_id);
            return (
              <Card key={profile.id} className="flex min-h-[18rem] flex-col p-5">
                <div className="flex items-start justify-between gap-3">
                  <span className="liquid-icon grid h-11 w-11 place-items-center rounded-xl text-brand">{isLocal ? <HardDrive size={20} /> : <Cloud size={20} />}</span>
                  <div className="flex flex-wrap justify-end gap-1.5">{isLocal ? qualityBadge(profile.quality.verdict, t) : <Badge variant={profile.cloud_status === "failed" ? "danger" : profile.cloud_status === "training" ? "warning" : "success"}>{profile.cloud_status === "training" ? t("voices.cloudTraining") : profile.cloud_status === "failed" ? t("voices.cloudFailed") : t("voices.cloudReady")}</Badge>}<Badge variant="info">{isLocal ? "ZipVoice · " + t("voices.localTab") : (profile.engine === "elevenlabs" ? t("voices.elevenLabsShort") : t("voices.volcEngineShort")) + " · " + t("voices.cloudTab")}</Badge></div>
                </div>
                <h3 className="mt-4 truncate font-display text-lg font-semibold text-text-primary" title={profile.name}>{profile.name}</h3>
                <p className="mt-1 flex items-center gap-1.5 text-xs text-text-tertiary"><Languages size={13} /> {profile.language === "zh" ? t("language.zh") : t("language.en")} · {isLocal ? `${(profile.quality.duration_ms / 1000).toFixed(1)}s · SNR ${profile.quality.snr_db.toFixed(0)} dB` : provider?.name ?? t("voices.cloudProviderUnknown")}</p>
                {!isLocal && profile.engine === "volcengine" && profile.volc_training_times_left != null && <p className="mt-1 text-xs font-semibold text-warning">{t("voices.trainingTimesLeft", { count: profile.volc_training_times_left })}</p>}
                <p className="mt-3 line-clamp-2 text-xs leading-5 text-text-secondary">{isLocal ? profile.reference_text : t("voices.cloudStoredDesc", { id: profile.cloud_voice_id ?? "—" })}</p>
                <div className="mt-auto pt-4">
                  {audioUrl ? <audio controls preload="none" src={audioUrl} className="h-9 w-full" /> : <div className="flex h-9 items-center gap-2 rounded-full bg-surface-overlay px-3 text-xs text-text-tertiary"><Play size={13} /> {t("voices.previewInApp")}</div>}
                  <div className="mt-3 flex flex-wrap gap-2">
                    <Button type="button" variant="primary" size="sm" onClick={() => navigate(`/dubbing?voice=${encodeURIComponent(profile.id)}`)} disabled={!canUseForDubbing} title={!canUseForDubbing ? t("voices.cloudNotReadyHint") : undefined}><Volume2 size={14} /> {t("voices.useForDubbing")}</Button>
                    <Button type="button" variant="secondary" size="sm" onClick={() => void handleExport(profile)} disabled={busyId === profile.id} title={t("voices.exportAction")}>{busyId === profile.id ? <LoaderCircle size={14} className="animate-spin" /> : <Download size={14} />} <span className="sr-only">{t("voices.exportAction")}</span></Button>
                    {!isLocal && profile.engine === "volcengine" && <Button type="button" variant="secondary" size="sm" onClick={() => void handleRefreshStatus(profile)} disabled={busyId === profile.id} title={t("voices.refreshStatus")}>
                      {busyId === profile.id ? <LoaderCircle size={14} className="animate-spin" /> : <RefreshCw size={14} />} <span className="sr-only">{t("voices.refreshStatus")}</span>
                    </Button>}
                    {!isLocal && profile.engine === "volcengine" && <Button type="button" variant="secondary" size="sm" onClick={() => void handleRetrain(profile)} disabled={busyId === profile.id || !profile.reference_audio_path} title={!profile.reference_audio_path ? t("voices.retrainMissingReference") : t("voices.retrainAction")}>
                      {busyId === profile.id ? <LoaderCircle size={14} className="animate-spin" /> : <Sparkles size={14} />} <span className="sr-only">{t("voices.retrainAction")}</span>
                    </Button>}
                    <Button type="button" variant="secondary" size="sm" aria-label={`${t("voices.renameTitle")}: ${profile.name}`} onClick={() => { setRenaming(profile); setRenameDraft(profile.name); }}><Pencil size={14} /></Button>
                    <Button type="button" variant="ghost" size="sm" className="ml-auto text-danger" aria-label={`${t("common.delete")}: ${profile.name}`} onClick={() => { setDeleting(profile); setDeleteRemote(false); }}><Trash2 size={14} /></Button>
                  </div>
                </div>
              </Card>
            );
          })}
        </div>
      )}

      <CreateVoiceDialog providers={providers} open={wizardOpen} onClose={() => setWizardOpen(false)} onCreated={(profile) => { setProfiles((current) => [profile, ...current]); setMessage({ type: "ok", text: t("voices.created", { name: profile.name }) }); }} />
      <CloudVoiceRecoveryDialog providers={providers} linkedProfiles={profiles} open={recoveryOpen} onClose={() => setRecoveryOpen(false)} onLinked={(profile) => { setProfiles((current) => [profile, ...current]); setMessage({ type: "ok", text: t("voices.linked", { name: profile.name }) }); }} />

      {renaming && <div className="fixed inset-0 z-[75] flex items-center justify-center bg-black/50 p-4 backdrop-blur-sm" role="dialog" aria-modal="true" aria-labelledby="rename-voice-title"><Card className="w-full max-w-md border border-border-default bg-surface-raised p-5 shadow-2xl"><h3 id="rename-voice-title" className="font-display text-h2 font-semibold text-text-primary">{t("voices.renameTitle")}</h3><label htmlFor="rename-voice-input" className="sr-only">{t("voices.renameTitle")}</label><Input id="rename-voice-input" className="mt-4" value={renameDraft} onChange={(event) => setRenameDraft(event.target.value)} maxLength={60} autoFocus /><div className="mt-5 flex justify-end gap-2"><Button type="button" variant="secondary" onClick={() => setRenaming(null)}>{t("common.cancel")}</Button><Button type="button" variant="primary" onClick={handleRename} disabled={!renameDraft.trim() || busyId === renaming.id}>{busyId === renaming.id && <LoaderCircle size={14} className="animate-spin" />} {t("common.save")}</Button></div></Card></div>}

      {deleting && <div className="fixed inset-0 z-[75] flex items-center justify-center bg-black/50 p-4 backdrop-blur-sm" role="dialog" aria-modal="true" aria-labelledby="delete-voice-title"><Card className="w-full max-w-md border border-border-default bg-surface-raised p-5 shadow-2xl"><span className="grid h-10 w-10 place-items-center rounded-full bg-danger/10 text-danger"><Trash2 size={18} /></span><h3 id="delete-voice-title" className="mt-4 font-display text-h2 font-semibold text-text-primary">{deleting.engine === "zipvoice" ? t("voices.deleteTitle") : t("voices.unlinkTitle")}</h3><p className="mt-2 text-sm leading-6 text-text-secondary">{deleting.engine === "zipvoice" ? t("voices.deleteDesc", { name: deleting.name }) : t("voices.unlinkDesc", { name: deleting.name })}</p>{deleting.engine === "elevenlabs" && <label className="mt-4 flex cursor-pointer items-start gap-3 rounded-xl border border-danger/20 bg-danger/5 p-3 text-xs leading-5 text-text-secondary"><input type="checkbox" checked={deleteRemote} onChange={(event) => setDeleteRemote(event.target.checked)} className="mt-1 h-4 w-4 accent-danger" /><span><strong className="block text-text-primary">{t("voices.remoteDeleteTitle")}</strong>{t("voices.remoteDeleteDesc")}</span></label>}<div className="mt-5 flex justify-end gap-2"><Button type="button" variant="secondary" onClick={() => { setDeleting(null); setDeleteRemote(false); }}>{t("common.cancel")}</Button><Button type="button" variant="danger" onClick={handleDelete} disabled={busyId === deleting.id}>{busyId === deleting.id && <LoaderCircle size={14} className="animate-spin" />} {deleting.engine === "zipvoice" ? t("common.delete") : t("voices.unlinkAction")}</Button></div></Card></div>}
    </div>
  );
}
