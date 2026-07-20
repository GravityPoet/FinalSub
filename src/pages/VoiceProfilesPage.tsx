import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  AlertTriangle,
  AudioLines,
  CheckCircle2,
  Download,
  FolderOpen,
  Import,
  Languages,
  LoaderCircle,
  Mic,
  Pencil,
  Play,
  Plus,
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
  createVoiceProfile,
  discardPreparedVoiceSample,
  discardVoiceRecording,
  exportVoiceProfile,
  fileAssetUrl,
  importVoiceProfile,
  inspectVoiceSource,
  listVoiceProfiles,
  openDialog,
  prepareVoiceSample,
  removeVoiceProfile,
  renameVoiceProfile,
  saveDialog,
  type PreparedVoiceSample,
  type VoiceProfile,
  type VoiceProfileLanguage,
  type VoiceQualityIssue,
  type VoiceQualityVerdict,
  type VoiceSourceInfo,
} from "../lib/tauri";

const MEDIA_EXTENSIONS = ["wav", "mp3", "m4a", "aac", "flac", "ogg", "opus", "mp4", "mov", "mkv", "webm", "avi", "m4v", "ts"];

function safeFileName(value: string): string {
  return value.replace(/[\\/:*?"<>|]/g, "-").trim() || "FinalSub Voice";
}

function qualityBadge(verdict: VoiceQualityVerdict, t: ReturnType<typeof useI18n>["t"]) {
  if (verdict === "good") return <Badge variant="success">{t("voices.qualityGood")}</Badge>;
  if (verdict === "fair") return <Badge variant="warning">{t("voices.qualityFair")}</Badge>;
  return <Badge variant="danger">{t("voices.qualityPoor")}</Badge>;
}

function issueText(issue: VoiceQualityIssue, t: ReturnType<typeof useI18n>["t"]): string {
  const value = issue.value ?? 0;
  const keys = {
    "no-speech": "voices.issueNoSpeech",
    "too-short": "voices.issueTooShort",
    "short-for-engine": "voices.issueShort",
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
  onClose,
  onCreated,
}: {
  open: boolean;
  onClose: () => void;
  onCreated: (profile: VoiceProfile) => void;
}) {
  const { t } = useI18n();
  const [step, setStep] = useState<1 | 2 | 3>(1);
  const [showRecorder, setShowRecorder] = useState(false);
  const [source, setSource] = useState<VoiceSourceInfo | null>(null);
  const [recordingPath, setRecordingPath] = useState("");
  const [startMs, setStartMs] = useState(0);
  const [durationMs, setDurationMs] = useState(8000);
  const [prepared, setPrepared] = useState<PreparedVoiceSample | null>(null);
  const [name, setName] = useState("");
  const [language, setLanguage] = useState<VoiceProfileLanguage>("zh");
  const [referenceText, setReferenceText] = useState("");
  const [consent, setConsent] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const reset = async () => {
    if (prepared?.token) await discardPreparedVoiceSample(prepared.token).catch(() => undefined);
    if (recordingPath) await discardVoiceRecording(recordingPath).catch(() => undefined);
    setStep(1);
    setShowRecorder(false);
    setSource(null);
    setRecordingPath("");
    setStartMs(0);
    setDurationMs(8000);
    setPrepared(null);
    setName("");
    setLanguage("zh");
    setReferenceText("");
    setConsent(false);
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

  const useSource = async (path: string, fromRecording = false) => {
    setBusy(true);
    setError("");
    try {
      const info = await inspectVoiceSource(path);
      if (info.duration_ms < 3000) throw new Error(t("voices.sourceTooShort"));
      setSource(info);
      setRecordingPath(fromRecording ? info.path : "");
      setStartMs(0);
      setDurationMs(Math.min(8000, Math.max(3000, info.duration_ms)));
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
      const profile = await createVoiceProfile({
        token: prepared.token,
        name,
        language,
        reference_text: referenceText,
        consent,
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

  const maxStartMs = Math.max(0, (source?.duration_ms ?? 3000) - 3000);
  const maxDurationMs = Math.max(3000, Math.min(10000, (source?.duration_ms ?? 10000) - startMs));
  const previewUrl = prepared ? fileAssetUrl(prepared.audio_path) : "";

  if (!open) return null;
  return (
    <div className="fixed inset-0 z-[70] flex items-center justify-center bg-black/50 p-3 backdrop-blur-xl" role="dialog" aria-modal="true" aria-labelledby="create-voice-title" onMouseDown={(event) => { if (event.target === event.currentTarget) void close(); }}>
      <Card className="flex max-h-[92vh] w-full max-w-3xl flex-col overflow-hidden border border-border-default bg-surface-raised p-0 shadow-2xl">
        <div className="z-10 flex shrink-0 items-start justify-between gap-4 border-b border-border-subtle bg-surface-raised px-5 py-4">
          <div>
            <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-brand">{t("voices.wizardEyebrow", { current: step })}</p>
            <h2 id="create-voice-title" className="mt-1 font-display text-h2 font-bold text-text-primary">{t("voices.createTitle")}</h2>
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
                <h3 className="font-display text-lg font-semibold text-text-primary">{t("voices.sourceTitle")}</h3>
                <p className="mt-1 text-sm leading-6 text-text-tertiary">{t("voices.sourceDesc")}</p>
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
              <div className="flex items-start gap-2 rounded-xl border border-success/15 bg-success/8 px-3.5 py-3 text-xs leading-5 text-text-secondary">
                <ShieldCheck size={15} className="mt-0.5 shrink-0 text-success" /> {t("voices.localPrivacy")}
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
                <Button type="button" variant="secondary" size="sm" onClick={async () => { await clearPrepared(); if (recordingPath) await discardVoiceRecording(recordingPath).catch(() => undefined); setRecordingPath(""); setStep(1); setSource(null); }} disabled={busy}>{t("voices.changeSource")}</Button>
              </div>
              <div className="grid gap-4 rounded-2xl border border-border-subtle bg-surface-card p-4 sm:grid-cols-2">
                <label className="space-y-2 text-xs font-semibold text-text-secondary">
                  <span className="flex justify-between"><span>{t("voices.segmentStart")}</span><span className="font-mono text-brand">{(startMs / 1000).toFixed(1)}s</span></span>
                  <input type="range" min={0} max={maxStartMs} step={100} value={startMs} onChange={(event) => { const next = Number(event.target.value); setStartMs(next); setDurationMs((current) => Math.min(current, Math.max(3000, Math.min(10000, source.duration_ms - next)))); void clearPrepared(); }} className="w-full accent-brand" />
                </label>
                <label className="space-y-2 text-xs font-semibold text-text-secondary">
                  <span className="flex justify-between"><span>{t("voices.segmentDuration")}</span><span className="font-mono text-brand">{(durationMs / 1000).toFixed(1)}s</span></span>
                  <input type="range" min={3000} max={maxDurationMs} step={100} value={Math.min(durationMs, maxDurationMs)} onChange={(event) => { setDurationMs(Number(event.target.value)); void clearPrepared(); }} className="w-full accent-brand" />
                </label>
                <p className="sm:col-span-2 text-xs leading-5 text-text-tertiary">{t("voices.segmentHint")}</p>
              </div>
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
                    <div className="space-y-2">{prepared.quality.issues.map((issue, index) => <p key={`${issue.code}-${index}`} className={`flex items-start gap-2 text-xs leading-5 ${issue.severity === "error" ? "text-danger" : issue.severity === "warning" ? "text-warning" : "text-brand"}`}><AlertTriangle size={14} className="mt-0.5 shrink-0" /> {issueText(issue, t)}</p>)}</div>
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
                <p className="mt-1 text-sm leading-6 text-text-tertiary">{t("voices.detailsDesc")}</p>
              </div>
              <div className="grid gap-4 sm:grid-cols-2">
                <label className="space-y-1.5 text-xs font-semibold text-text-secondary"><span>{t("voices.name")}</span><Input value={name} onChange={(event) => setName(event.target.value)} maxLength={60} /></label>
                <label className="space-y-1.5 text-xs font-semibold text-text-secondary"><span>{t("voices.language")}</span><Select value={language} onChange={(event) => setLanguage(event.target.value as VoiceProfileLanguage)}><option value="zh">{t("language.zh")}</option><option value="en">{t("language.en")}</option></Select></label>
              </div>
              <label className="block space-y-1.5 text-xs font-semibold text-text-secondary">
                <span>{t("voices.referenceText")}</span>
                <Textarea rows={4} value={referenceText} onChange={(event) => setReferenceText(event.target.value)} placeholder={t("voices.referenceTextPlaceholder")} maxLength={4000} />
                <span className="block font-normal leading-5 text-text-tertiary">{t("voices.referenceTextHint")}</span>
              </label>
              <label className="flex cursor-pointer items-start gap-3 rounded-2xl border border-border-subtle bg-surface-card p-4 text-sm leading-6 text-text-secondary">
                <input type="checkbox" checked={consent} onChange={(event) => setConsent(event.target.checked)} className="mt-1 h-4 w-4 accent-brand" />
                <span><strong className="block text-text-primary">{t("voices.consentTitle")}</strong>{t("voices.consentDesc")}</span>
              </label>
              <div className="flex flex-wrap justify-between gap-2">
                <Button type="button" variant="secondary" onClick={() => setStep(2)} disabled={busy}>{t("voices.backQuality")}</Button>
                <Button type="button" variant="primary" size="lg" onClick={create} disabled={busy || !name.trim() || !referenceText.trim() || !consent}>
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

export default function VoiceProfilesPage() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const [profiles, setProfiles] = useState<VoiceProfile[]>([]);
  const [loading, setLoading] = useState(true);
  const [wizardOpen, setWizardOpen] = useState(false);
  const [message, setMessage] = useState<{ type: "ok" | "err"; text: string } | null>(null);
  const [renaming, setRenaming] = useState<VoiceProfile | null>(null);
  const [renameDraft, setRenameDraft] = useState("");
  const [deleting, setDeleting] = useState<VoiceProfile | null>(null);
  const [busyId, setBusyId] = useState("");

  const refresh = () => {
    setLoading(true);
    listVoiceProfiles().then(setProfiles).catch((reason) => setMessage({ type: "err", text: String(reason) })).finally(() => setLoading(false));
  };
  useEffect(refresh, []);

  const goodCount = useMemo(() => profiles.filter((profile) => profile.quality.verdict === "good").length, [profiles]);

  const handleImport = async () => {
    const selected = await openDialog({ multiple: false, filters: [{ name: "SmartSub / FinalSub Voice", extensions: ["svoice"] }] });
    if (typeof selected !== "string") return;
    setMessage(null);
    setBusyId("import");
    try {
      const profile = await importVoiceProfile(selected);
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
      await removeVoiceProfile(deleting.id);
      setProfiles((current) => current.filter((profile) => profile.id !== deleting.id));
      setMessage({ type: "ok", text: t("voices.deleted", { name: deleting.name }) });
      setDeleting(null);
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
          <Button type="button" variant="secondary" onClick={handleImport} disabled={busyId === "import"}>{busyId === "import" ? <LoaderCircle size={15} className="animate-spin" /> : <Import size={15} />} {t("voices.importAction")}</Button>
          <Button type="button" variant="primary" onClick={() => setWizardOpen(true)}><Plus size={16} /> {t("voices.createAction")}</Button>
        </div>
      </div>

      <div className="grid gap-3 sm:grid-cols-3">
        <Card className="p-4"><div className="flex items-center gap-3"><span className="liquid-icon grid h-10 w-10 place-items-center rounded-xl text-brand"><UserRound size={18} /></span><div><p className="font-display text-xl font-semibold text-text-primary">{profiles.length}</p><p className="text-xs text-text-tertiary">{t("voices.savedCount")}</p></div></div></Card>
        <Card className="p-4"><div className="flex items-center gap-3"><span className="grid h-10 w-10 place-items-center rounded-xl bg-success/10 text-success"><CheckCircle2 size={18} /></span><div><p className="font-display text-xl font-semibold text-text-primary">{goodCount}</p><p className="text-xs text-text-tertiary">{t("voices.goodCount")}</p></div></div></Card>
        <Card className="p-4"><div className="flex items-center gap-3"><span className="grid h-10 w-10 place-items-center rounded-xl bg-brand/10 text-brand"><ShieldCheck size={18} /></span><div><p className="font-display text-sm font-semibold text-text-primary">{t("voices.localOnly")}</p><p className="text-xs text-text-tertiary">{t("voices.localOnlyDesc")}</p></div></div></Card>
      </div>

      {message && <div className={`rounded-xl border px-4 py-3 text-sm font-semibold leading-6 ${message.type === "ok" ? "border-success/20 bg-success/10 text-success" : "border-danger/20 bg-danger/10 text-danger"}`}>{message.text}</div>}

      {loading ? <div className="py-16 text-center text-sm text-text-tertiary">{t("voices.loading")}</div> : profiles.length === 0 ? (
        <Card className="py-12 text-center">
          <span className="liquid-icon mx-auto grid h-14 w-14 place-items-center rounded-2xl text-brand"><AudioLines size={24} /></span>
          <h3 className="mt-4 font-display text-h2 font-semibold text-text-primary">{t("voices.emptyTitle")}</h3>
          <p className="mx-auto mt-2 max-w-lg text-sm leading-6 text-text-tertiary">{t("voices.emptyDesc")}</p>
          <Button type="button" variant="primary" className="mt-5" onClick={() => setWizardOpen(true)}><Sparkles size={15} /> {t("voices.createFirst")}</Button>
        </Card>
      ) : (
        <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
          {profiles.map((profile) => {
            const audioUrl = fileAssetUrl(profile.reference_audio_path);
            return (
              <Card key={profile.id} className="flex min-h-[18rem] flex-col p-5">
                <div className="flex items-start justify-between gap-3">
                  <span className="liquid-icon grid h-11 w-11 place-items-center rounded-xl text-brand"><UserRound size={20} /></span>
                  <div className="flex flex-wrap justify-end gap-1.5">{qualityBadge(profile.quality.verdict, t)}<Badge variant="info">ZipVoice</Badge></div>
                </div>
                <h3 className="mt-4 truncate font-display text-lg font-semibold text-text-primary" title={profile.name}>{profile.name}</h3>
                <p className="mt-1 flex items-center gap-1.5 text-xs text-text-tertiary"><Languages size={13} /> {profile.language === "zh" ? t("language.zh") : t("language.en")} · {(profile.quality.duration_ms / 1000).toFixed(1)}s · SNR {profile.quality.snr_db.toFixed(0)} dB</p>
                <p className="mt-3 line-clamp-2 text-xs leading-5 text-text-secondary">{profile.reference_text}</p>
                <div className="mt-auto pt-4">
                  {audioUrl ? <audio controls preload="none" src={audioUrl} className="h-9 w-full" /> : <div className="flex h-9 items-center gap-2 rounded-full bg-surface-overlay px-3 text-xs text-text-tertiary"><Play size={13} /> {t("voices.previewInApp")}</div>}
                  <div className="mt-3 flex flex-wrap gap-2">
                    <Button type="button" variant="primary" size="sm" onClick={() => navigate(`/dubbing?voice=${encodeURIComponent(profile.id)}`)}><Volume2 size={14} /> {t("voices.useForDubbing")}</Button>
                    <Button type="button" variant="secondary" size="sm" onClick={() => void handleExport(profile)} disabled={busyId === profile.id}>{busyId === profile.id ? <LoaderCircle size={14} className="animate-spin" /> : <Download size={14} />}</Button>
                    <Button type="button" variant="secondary" size="sm" onClick={() => { setRenaming(profile); setRenameDraft(profile.name); }}><Pencil size={14} /></Button>
                    <Button type="button" variant="ghost" size="sm" className="ml-auto text-danger" onClick={() => setDeleting(profile)}><Trash2 size={14} /></Button>
                  </div>
                </div>
              </Card>
            );
          })}
        </div>
      )}

      <CreateVoiceDialog open={wizardOpen} onClose={() => setWizardOpen(false)} onCreated={(profile) => { setProfiles((current) => [profile, ...current]); setMessage({ type: "ok", text: t("voices.created", { name: profile.name }) }); }} />

      {renaming && <div className="fixed inset-0 z-[75] flex items-center justify-center bg-black/50 p-4 backdrop-blur-sm" role="dialog" aria-modal="true"><Card className="w-full max-w-md border border-border-default bg-surface-raised p-5 shadow-2xl"><h3 className="font-display text-h2 font-semibold text-text-primary">{t("voices.renameTitle")}</h3><Input className="mt-4" value={renameDraft} onChange={(event) => setRenameDraft(event.target.value)} maxLength={60} autoFocus /><div className="mt-5 flex justify-end gap-2"><Button type="button" variant="secondary" onClick={() => setRenaming(null)}>{t("common.cancel")}</Button><Button type="button" variant="primary" onClick={handleRename} disabled={!renameDraft.trim() || busyId === renaming.id}>{busyId === renaming.id && <LoaderCircle size={14} className="animate-spin" />} {t("common.save")}</Button></div></Card></div>}

      {deleting && <div className="fixed inset-0 z-[75] flex items-center justify-center bg-black/50 p-4 backdrop-blur-sm" role="dialog" aria-modal="true"><Card className="w-full max-w-md border border-border-default bg-surface-raised p-5 shadow-2xl"><span className="grid h-10 w-10 place-items-center rounded-full bg-danger/10 text-danger"><Trash2 size={18} /></span><h3 className="mt-4 font-display text-h2 font-semibold text-text-primary">{t("voices.deleteTitle")}</h3><p className="mt-2 text-sm leading-6 text-text-secondary">{t("voices.deleteDesc", { name: deleting.name })}</p><div className="mt-5 flex justify-end gap-2"><Button type="button" variant="secondary" onClick={() => setDeleting(null)}>{t("common.cancel")}</Button><Button type="button" variant="danger" onClick={handleDelete} disabled={busyId === deleting.id}>{busyId === deleting.id && <LoaderCircle size={14} className="animate-spin" />} {t("common.delete")}</Button></div></Card></div>}
    </div>
  );
}
