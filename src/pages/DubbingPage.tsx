import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import {
  AlertTriangle,
  AudioLines,
  CheckCircle2,
  ChevronDown,
  Clock3,
  Cloud,
  Download,
  FileCheck2,
  FileText,
  FolderOpen,
  Gauge,
  HardDrive,
  LoaderCircle,
  Pause,
  Pencil,
  Play,
  RefreshCw,
  Save,
  ShieldCheck,
  Sparkles,
  Video,
  Volume2,
  WandSparkles,
  X,
} from "lucide-react";

import { Badge } from "../components/ui/Badge";
import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { Input, Select, Textarea } from "../components/ui/Input";
import { Progress } from "../components/ui/Progress";
import { useI18n } from "../lib/i18n";
import {
  acceptDubbingOverflow,
  cancelLocalTts,
  createDubbingSession,
  exportDubbingAudio,
  exportDubbingSubtitle,
  fileAssetUrl,
  getDubbingSession,
  listTtsModels,
  listTtsProviders,
  listVoiceProfiles,
  openDialog,
  revealItemInDir,
  saveDialog,
  synthesizeDubbingCue,
  updateDubbingCue,
  writeBackDubbingSubtitle,
  type DubbingCue,
  type DubbingEngineSelection,
  type DubbingSession,
  type TtsModelInfo,
  type TtsProviderProfile,
  type TtsVoice,
  type VoiceProfile,
} from "../lib/tauri";

const LAST_SESSION_KEY = "finalsub:last-dubbing-session";

function formatTime(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  const millis = ms % 1000;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}.${String(millis).padStart(3, "0")}`;
}

function outputPathFor(subtitlePath: string): string {
  return `${subtitlePath.replace(/\.[^/.]+$/, "")}.finalsub-dub.wav`;
}

function editedSubtitlePathFor(subtitlePath: string): string {
  const match = subtitlePath.match(/(\.[^/.]+)$/);
  const extension = match?.[1] ?? ".srt";
  return `${subtitlePath.slice(0, match ? -extension.length : undefined)}.finalsub-edited${extension}`;
}

function subtitleExtension(subtitlePath: string): string {
  const extension = subtitlePath.split(".").pop()?.toLowerCase() ?? "srt";
  return ["srt", "vtt", "ass", "ssa", "lrc"].includes(extension) ? extension : "srt";
}

export default function DubbingPage() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const requestedVoiceId = searchParams.get("voice") ?? "";
  const requestedSessionId = searchParams.get("session") ?? "";
  const [models, setModels] = useState<TtsModelInfo[]>([]);
  const [providers, setProviders] = useState<TtsProviderProfile[]>([]);
  const [voiceProfiles, setVoiceProfiles] = useState<VoiceProfile[]>([]);
  const [selectedVoiceProfileId, setSelectedVoiceProfileId] = useState("");
  const [session, setSession] = useState<DubbingSession | null>(null);
  const [subtitlePath, setSubtitlePath] = useState("");
  const [videoPath, setVideoPath] = useState("");
  const [engineValue, setEngineValue] = useState("");
  const [voice, setVoice] = useState("");
  const [globalSpeed, setGlobalSpeed] = useState(1);
  const [referenceAudio, setReferenceAudio] = useState("");
  const [referenceText, setReferenceText] = useState("");
  const [cloneQuality, setCloneQuality] = useState<"standard" | "high">("standard");
  const [loading, setLoading] = useState(true);
  const [creating, setCreating] = useState(false);
  const [activeGenerationId, setActiveGenerationId] = useState<string | null>(null);
  const [activeCueIndex, setActiveCueIndex] = useState<number | null>(null);
  const [batchRunning, setBatchRunning] = useState(false);
  const [batchProgress, setBatchProgress] = useState(0);
  const [exporting, setExporting] = useState(false);
  const [subtitleSaving, setSubtitleSaving] = useState<"copy" | "source" | null>(null);
  const [writeBackOpen, setWriteBackOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [subtitleArtifact, setSubtitleArtifact] = useState<{ kind: "copy" | "backup"; path: string } | null>(null);
  const [currentTimeMs, setCurrentTimeMs] = useState(0);
  const [videoPlaying, setVideoPlaying] = useState(false);
  const [message, setMessage] = useState<{ type: "ok" | "err" | "warn"; text: string } | null>(null);
  const cancelRequested = useRef(false);
  const videoRef = useRef<HTMLVideoElement>(null);
  const cueElements = useRef(new Map<number, HTMLDivElement>());

  const readyModels = useMemo(() => models.filter((model) => model.status === "ready"), [models]);
  const selectedLocalModel = useMemo(() => {
    if (!engineValue.startsWith("local:")) return null;
    return models.find((model) => model.id === engineValue.slice("local:".length)) ?? null;
  }, [engineValue, models]);
  const selectedProvider = useMemo(() => {
    if (!engineValue.startsWith("cloud:")) return null;
    return providers.find((provider) => provider.id === engineValue.slice("cloud:".length)) ?? null;
  }, [engineValue, providers]);
  const selectedVoiceProfile = useMemo(
    () => voiceProfiles.find((profile) => profile.id === selectedVoiceProfileId) ?? null,
    [selectedVoiceProfileId, voiceProfiles],
  );
  const localVoiceProfiles = useMemo(
    () => voiceProfiles.filter((profile) => profile.engine === "zipvoice"),
    [voiceProfiles],
  );
  const cloudVoiceProfiles = useMemo(
    () => selectedProvider
      ? voiceProfiles.filter((profile) => profile.engine !== "zipvoice"
        && profile.provider_id === selectedProvider.id
        && profile.cloud_status === "ready")
      : [],
    [selectedProvider, voiceProfiles],
  );
  const playbackCue = useMemo(
    () => session?.cues.find((cue) => currentTimeMs >= cue.start_ms && currentTimeMs < cue.end_ms) ?? null,
    [currentTimeMs, session],
  );

  useEffect(() => {
    if (!videoPlaying || !playbackCue) return;
    cueElements.current.get(playbackCue.index)?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [playbackCue?.index, videoPlaying]);

  useEffect(() => {
    if (!loading && !engineValue) setSettingsOpen(true);
  }, [engineValue, loading]);

  const applyDefaultEngine = (loadedModels: TtsModelInfo[], loadedProviders: TtsProviderProfile[]) => {
    const local = loadedModels.find((model) => model.status === "ready" && !model.clone_only)
      ?? loadedModels.find((model) => model.status === "ready");
    if (local) {
      setEngineValue(`local:${local.id}`);
      setVoice(local.default_voice_id);
      return local;
    }
    const cloud = loadedProviders[0];
    if (cloud) {
      setEngineValue(`cloud:${cloud.id}`);
      setVoice(cloud.voice);
    }
    return null;
  };

  useEffect(() => {
    Promise.all([listTtsModels(), listTtsProviders(), listVoiceProfiles()])
      .then(async ([loadedModels, loadedProviders, loadedVoiceProfiles]) => {
        setModels(loadedModels);
        setProviders(loadedProviders);
        setVoiceProfiles(loadedVoiceProfiles);
        const defaultModel = applyDefaultEngine(loadedModels, loadedProviders);
        const requestedProfile = requestedVoiceId
          ? loadedVoiceProfiles.find((profile) => profile.id === requestedVoiceId)
          : undefined;
        let restoredConfig = false;
        const previous = requestedSessionId || localStorage.getItem(LAST_SESSION_KEY);
        if (previous) {
          try {
            const restored = await getDubbingSession(previous);
            setSession(restored);
            setSubtitlePath(restored.subtitle_path);
            setVideoPath(restored.video_path ?? "");
            localStorage.setItem(LAST_SESSION_KEY, restored.id);
            if (restored.last_config) {
              restoredConfig = true;
              const engine = restored.last_config.engine;
              setEngineValue(engine.kind === "local" ? `local:${engine.model_id}` : `cloud:${engine.provider_id}`);
              setVoice(restored.last_config.voice);
              setGlobalSpeed(restored.last_config.global_speed);
              setReferenceAudio(restored.last_config.reference_audio_path ?? "");
              setReferenceText(restored.last_config.reference_text ?? "");
              const restoredProfile = loadedVoiceProfiles.find((profile) => engine.kind === "local"
                ? profile.engine === "zipvoice"
                  && profile.reference_audio_path === restored.last_config?.reference_audio_path
                : profile.engine !== "zipvoice"
                  && profile.provider_id === engine.provider_id
                  && profile.cloud_voice_id === restored.last_config?.voice);
              setSelectedVoiceProfileId(restoredProfile?.id ?? "");
              setCloneQuality((restored.last_config.num_steps ?? 4) >= 8 ? "high" : "standard");
            }
          } catch (error) {
            if (requestedSessionId) {
              setMessage({ type: "err", text: String(error) });
            } else {
              localStorage.removeItem(LAST_SESSION_KEY);
            }
          }
        }

        // An explicit “Use for dubbing” action must win over a restored session.
        if (requestedProfile) {
          if (requestedProfile.engine === "zipvoice") {
            const cloneModel = loadedModels.find((model) => model.status === "ready" && model.clone_only);
            if (cloneModel) {
              setEngineValue(`local:${cloneModel.id}`);
              setVoice(cloneModel.default_voice_id);
            } else {
              setEngineValue("");
              setVoice("");
              setSettingsOpen(true);
              setMessage({ type: "warn", text: t("dubbing.cloneEngineUnavailable") });
            }
            setReferenceAudio(requestedProfile.reference_audio_path);
            setReferenceText(requestedProfile.reference_text);
          } else {
            const provider = loadedProviders.find((item) => item.id === requestedProfile.provider_id);
            if (provider && requestedProfile.cloud_voice_id && requestedProfile.cloud_status === "ready") {
              setEngineValue(`cloud:${provider.id}`);
              setVoice(requestedProfile.cloud_voice_id);
            } else {
              setEngineValue("");
              setVoice("");
              setSettingsOpen(true);
              setMessage({
                type: "warn",
                text: provider && requestedProfile.cloud_voice_id
                  ? t("dubbing.cloudVoiceNotReady")
                  : t("dubbing.cloudVoiceUnavailable"),
              });
            }
            setReferenceAudio("");
            setReferenceText("");
          }
          setSelectedVoiceProfileId(requestedProfile.id);
        } else if (!restoredConfig && defaultModel?.clone_only && loadedVoiceProfiles.some((profile) => profile.engine === "zipvoice")) {
          const profile = loadedVoiceProfiles.find((item) => item.engine === "zipvoice")!;
          setSelectedVoiceProfileId(profile.id);
          setReferenceAudio(profile.reference_audio_path);
          setReferenceText(profile.reference_text);
        }
      })
      .catch((error) => setMessage({ type: "err", text: String(error) }))
      .finally(() => setLoading(false));
  }, [requestedSessionId, requestedVoiceId]);

  const changeEngine = (value: string) => {
    setEngineValue(value);
    if (value.startsWith("local:")) {
      const model = models.find((item) => item.id === value.slice("local:".length));
      setVoice(model?.default_voice_id ?? "");
      if (model?.clone_only && localVoiceProfiles.length > 0) {
        const profile = selectedVoiceProfile?.engine === "zipvoice" ? selectedVoiceProfile : localVoiceProfiles[0];
        setSelectedVoiceProfileId(profile.id);
        setReferenceAudio(profile.reference_audio_path);
        setReferenceText(profile.reference_text);
      } else if (!model?.clone_only) {
        setSelectedVoiceProfileId("");
      }
    } else {
      const provider = providers.find((item) => item.id === value.slice("cloud:".length));
      const savedCloudVoice = voiceProfiles.find((profile) => profile.engine !== "zipvoice"
        && profile.provider_id === provider?.id
        && profile.cloud_voice_id
        && profile.cloud_status === "ready");
      setVoice(savedCloudVoice?.cloud_voice_id ?? provider?.voice ?? "");
      setSelectedVoiceProfileId(savedCloudVoice?.id ?? "");
      setReferenceAudio("");
      setReferenceText("");
    }
    setMessage(null);
  };

  const chooseSubtitle = async () => {
    const selected = await openDialog({
      multiple: false,
      filters: [{ name: t("dubbing.subtitleFiles"), extensions: ["srt", "vtt", "ass", "ssa", "lrc"] }],
    });
    if (typeof selected === "string") setSubtitlePath(selected);
  };

  const chooseVideo = async () => {
    const selected = await openDialog({
      multiple: false,
      filters: [{ name: t("dubbing.videoFiles"), extensions: ["mp4", "mov", "mkv", "webm", "avi"] }],
    });
    if (typeof selected === "string") setVideoPath(selected);
  };

  const chooseReference = async () => {
    const selected = await openDialog({
      multiple: false,
      filters: [{ name: t("dubbing.audioFiles"), extensions: ["wav", "mp3", "m4a", "flac", "ogg", "opus"] }],
    });
    if (typeof selected === "string") {
      setSelectedVoiceProfileId("");
      setReferenceAudio(selected);
      setReferenceText("");
    }
  };

  const selectVoiceProfile = (profileId: string) => {
    setSelectedVoiceProfileId(profileId);
    const profile = voiceProfiles.find((item) => item.id === profileId);
    if (profile) {
      if (profile.engine === "zipvoice") {
        setReferenceAudio(profile.reference_audio_path);
        setReferenceText(profile.reference_text);
      } else {
        setVoice(profile.cloud_voice_id ?? "");
        setReferenceAudio("");
        setReferenceText("");
      }
    } else {
      if (selectedProvider) setVoice(selectedProvider.voice);
      setReferenceAudio("");
      setReferenceText("");
    }
  };

  const createSession = async () => {
    if (!subtitlePath) return;
    setCreating(true);
    setMessage(null);
    try {
      const created = await createDubbingSession(subtitlePath, videoPath || undefined);
      setSession(created);
      setCurrentTimeMs(0);
      setVideoPlaying(false);
      setSubtitleArtifact(null);
      localStorage.setItem(LAST_SESSION_KEY, created.id);
      setMessage({ type: "ok", text: t("dubbing.sessionCreated", { count: created.cues.length }) });
    } catch (error) {
      setMessage({ type: "err", text: String(error) });
    } finally {
      setCreating(false);
    }
  };

  const resetSession = () => {
    localStorage.removeItem(LAST_SESSION_KEY);
    setSession(null);
    setSubtitlePath("");
    setVideoPath("");
    setCurrentTimeMs(0);
    setVideoPlaying(false);
    setSubtitleArtifact(null);
    setMessage(null);
  };

  const engineSelection = (): DubbingEngineSelection | null => {
    if (engineValue.startsWith("local:")) {
      return { kind: "local", model_id: engineValue.slice("local:".length) };
    }
    if (engineValue.startsWith("cloud:")) {
      return { kind: "cloud", provider_id: engineValue.slice("cloud:".length) };
    }
    return null;
  };

  const runCue = async (cueIndex: number): Promise<DubbingSession> => {
    if (!session) throw new Error(t("dubbing.noSession"));
    const engine = engineSelection();
    if (!engine) throw new Error(t("dubbing.noEngine"));
    if (selectedLocalModel?.clone_only && (!referenceAudio || !referenceText.trim())) {
      throw new Error(t("dubbing.cloneReferenceRequired"));
    }
    const generationId = crypto.randomUUID();
    const cue = session.cues.find((item) => item.index === cueIndex);
    const effectiveVoice = cue?.voice_id?.trim() || voice;
    setActiveGenerationId(generationId);
    setActiveCueIndex(cueIndex);
    try {
      const updated = await synthesizeDubbingCue(generationId, {
        session_id: session.id,
        cue_index: cueIndex,
        engine,
        voice: effectiveVoice,
        global_speed: globalSpeed,
        reference_audio_path: referenceAudio || undefined,
        reference_text: referenceText.trim() || undefined,
        num_steps: cloneQuality === "high" ? 8 : 4,
      });
      setSession(updated);
      return updated;
    } finally {
      setActiveGenerationId(null);
      setActiveCueIndex(null);
    }
  };

  const editCue = async (cueIndex: number, text: string, voiceId: string) => {
    if (!session) return;
    try {
      const updated = await updateDubbingCue({
        session_id: session.id,
        cue_index: cueIndex,
        text,
        voice_id: voiceId,
      });
      setSession(updated);
      setMessage({ type: "ok", text: t("dubbing.cueEdited", { index: cueIndex + 1 }) });
    } catch (error) {
      setMessage({ type: "err", text: String(error) });
      throw error;
    }
  };

  const generateSingle = async (cueIndex: number) => {
    setMessage(null);
    try {
      const updated = await runCue(cueIndex);
      const cue = updated.cues[cueIndex];
      setMessage({
        type: cue.status === "overlong" ? "warn" : "ok",
        text: cue.status === "overlong" ? t("dubbing.cueOverlongNotice", { index: cueIndex + 1 }) : t("dubbing.cueReadyNotice", { index: cueIndex + 1 }),
      });
    } catch (error) {
      if (session) {
        getDubbingSession(session.id).then(setSession).catch(() => undefined);
      }
      setMessage({ type: "err", text: String(error) });
    }
  };

  const generateBatch = async () => {
    if (!session) return;
    const targets = session.cues.filter((cue) => ["pending", "failed"].includes(cue.status));
    if (targets.length === 0) {
      setMessage({ type: "warn", text: t("dubbing.noPending") });
      return;
    }
    setBatchRunning(true);
    setBatchProgress(0);
    setMessage(null);
    cancelRequested.current = false;
    let completed = 0;
    let latest = session;
    const failures: number[] = [];
    for (const cue of targets) {
      if (cancelRequested.current) break;
      try {
        latest = await runCue(cue.index);
      } catch {
        failures.push(cue.index + 1);
        try {
          latest = await getDubbingSession(session.id);
          setSession(latest);
        } catch {
          // Keep the most recent valid snapshot; the backend already persisted the failure.
        }
      }
      completed += 1;
      setBatchProgress((completed / targets.length) * 100);
    }
    setBatchRunning(false);
    setActiveGenerationId(null);
    setActiveCueIndex(null);
    if (cancelRequested.current) {
      setMessage({ type: "warn", text: t("dubbing.batchCancelled") });
    } else if (failures.length > 0) {
      setMessage({ type: "err", text: t("dubbing.batchPartial", { rows: failures.join("、") }) });
    } else {
      const overlong = latest.cues.filter((cue) => cue.status === "overlong").length;
      setMessage({
        type: overlong > 0 ? "warn" : "ok",
        text: overlong > 0
          ? t("dubbing.batchNeedsReview", { count: overlong })
          : t("dubbing.batchComplete"),
      });
    }
  };

  const cancelGeneration = async () => {
    cancelRequested.current = true;
    if (activeGenerationId) await cancelLocalTts(activeGenerationId);
  };

  const acceptOverflow = async (cueIndex: number) => {
    if (!session) return;
    const generationId = crypto.randomUUID();
    setActiveGenerationId(generationId);
    setActiveCueIndex(cueIndex);
    setMessage(null);
    try {
      const updated = await acceptDubbingOverflow(generationId, session.id, cueIndex);
      setSession(updated);
      setMessage({ type: "ok", text: t("dubbing.overflowAccepted", { index: cueIndex + 1 }) });
    } catch (error) {
      setMessage({ type: "err", text: String(error) });
    } finally {
      setActiveGenerationId(null);
      setActiveCueIndex(null);
    }
  };

  const exportAudio = async () => {
    if (!session) return;
    const selected = await saveDialog({
      defaultPath: outputPathFor(session.subtitle_path),
      filters: [
        { name: "WAV", extensions: ["wav"] },
        { name: "MP3", extensions: ["mp3"] },
      ],
    });
    if (!selected) return;
    const generationId = crypto.randomUUID();
    setExporting(true);
    setActiveGenerationId(generationId);
    setMessage(null);
    try {
      const updated = await exportDubbingAudio(generationId, session.id, selected);
      setSession(updated);
      setMessage({ type: "ok", text: t("dubbing.exportSuccess") });
    } catch (error) {
      setMessage({ type: "err", text: String(error) });
    } finally {
      setExporting(false);
      setActiveGenerationId(null);
    }
  };

  const exportSubtitleCopy = async () => {
    if (!session) return;
    const extension = subtitleExtension(session.subtitle_path);
    const selected = await saveDialog({
      defaultPath: editedSubtitlePathFor(session.subtitle_path),
      filters: [{ name: t("dubbing.subtitleFiles"), extensions: [extension] }],
    });
    if (!selected) return;
    setSubtitleSaving("copy");
    setMessage(null);
    try {
      const output = await exportDubbingSubtitle(session.id, selected);
      setSubtitleArtifact({ kind: "copy", path: output });
      setMessage({ type: "ok", text: t("dubbing.subtitleCopySuccess", { path: output }) });
    } catch (error) {
      setMessage({ type: "err", text: String(error) });
    } finally {
      setSubtitleSaving(null);
    }
  };

  const writeBackSubtitle = async () => {
    if (!session) return;
    setSubtitleSaving("source");
    setMessage(null);
    try {
      const result = await writeBackDubbingSubtitle(session.id);
      setSession(result.session);
      setSubtitleArtifact({ kind: "backup", path: result.backup_path });
      setWriteBackOpen(false);
      setMessage({ type: "ok", text: t("dubbing.writeBackSuccess", { path: result.backup_path }) });
    } catch (error) {
      setMessage({ type: "err", text: String(error) });
    } finally {
      setSubtitleSaving(null);
    }
  };

  const seekToCue = (cue: DubbingCue) => {
    const seconds = cue.start_ms / 1000;
    if (videoRef.current) videoRef.current.currentTime = seconds;
    setCurrentTimeMs(cue.start_ms);
    cueElements.current.get(cue.index)?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  };

  if (loading) {
    return <div className="page-shell grid min-h-[30rem] place-items-center text-sm text-text-tertiary"><LoaderCircle className="animate-spin" /></div>;
  }

  const doneCount = session?.cues.filter((cue) => ["ready", "accepted"].includes(cue.status)).length ?? 0;
  const overlongCount = session?.cues.filter((cue) => cue.status === "overlong").length ?? 0;
  const failedCount = session?.cues.filter((cue) => cue.status === "failed").length ?? 0;
  const autoAdjustedCount = session?.cues.filter((cue) =>
    cue.resynthesized || ["precontrolled", "postprocessed"].includes(cue.alignment_action ?? "")
  ).length ?? 0;
  const canExport = Boolean(session && session.cues.length > 0 && doneCount === session.cues.length);

  return (
    <div className="page-shell space-y-6">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <p className="text-xs font-semibold uppercase tracking-[0.18em] text-brand">{t("dubbing.eyebrow")}</p>
          <h2 className="mt-1 font-display text-display font-bold tracking-tight text-text-primary">{t("dubbing.title")}</h2>
          <p className="mt-2 max-w-3xl text-sm leading-6 text-text-tertiary">{t("dubbing.subtitle")}</p>
        </div>
        {session && (
          <Button type="button" variant="secondary" size="sm" onClick={resetSession} disabled={batchRunning || exporting || activeGenerationId !== null}>
            <RefreshCw size={14} /> {t("dubbing.newSession")}
          </Button>
        )}
      </div>

      {message && (
        <div className={`rounded-xl border px-4 py-3 text-sm font-semibold leading-6 ${
          message.type === "ok"
            ? "border-success/20 bg-success/10 text-success"
            : message.type === "warn"
              ? "border-warning/20 bg-warning/10 text-warning"
              : "border-danger/20 bg-danger/10 text-danger"
        }`}>
          {message.text}
        </div>
      )}

      {!session ? (
        <Card className="p-6">
          <div className="mx-auto max-w-3xl space-y-5">
            <div className="text-center">
              <span className="liquid-icon mx-auto grid h-14 w-14 place-items-center rounded-2xl text-brand"><AudioLines size={24} /></span>
              <h3 className="mt-4 font-display text-h2 font-semibold text-text-primary">{t("dubbing.startTitle")}</h3>
              <p className="mt-2 text-sm leading-6 text-text-tertiary">{t("dubbing.startDesc")}</p>
            </div>
            <div className="grid gap-3 sm:grid-cols-2">
              <button type="button" onClick={chooseSubtitle} className="rounded-2xl border border-border-default bg-surface-card p-4 text-left transition hover:border-brand/25 hover:bg-surface-overlay">
                <span className="flex items-center gap-2 text-sm font-semibold text-text-primary"><FileText size={17} className="text-brand" /> {t("dubbing.chooseSubtitle")}</span>
                <span className="mt-2 block break-all text-xs leading-5 text-text-tertiary">{subtitlePath || t("dubbing.subtitleRequired")}</span>
              </button>
              <button type="button" onClick={chooseVideo} className="rounded-2xl border border-border-default bg-surface-card p-4 text-left transition hover:border-brand/25 hover:bg-surface-overlay">
                <span className="flex items-center gap-2 text-sm font-semibold text-text-primary"><Video size={17} className="text-brand" /> {t("dubbing.chooseVideo")}</span>
                <span className="mt-2 block break-all text-xs leading-5 text-text-tertiary">{videoPath || t("dubbing.videoOptional")}</span>
              </button>
            </div>
            <div className="flex justify-center">
              <Button type="button" variant="primary" size="lg" onClick={createSession} disabled={!subtitlePath || creating}>
                {creating ? <LoaderCircle size={16} className="animate-spin" /> : <Sparkles size={16} />}
                {creating ? t("dubbing.creating") : t("dubbing.createSession")}
              </Button>
            </div>
          </div>
        </Card>
      ) : (
        <>
          {session.source_changed && (
            <div className="rounded-2xl border border-warning/25 bg-warning/10 px-4 py-3 text-sm leading-6 text-warning">
              <AlertTriangle size={16} className="mr-2 inline" /> {t("dubbing.sourceChanged")}
            </div>
          )}

          <Card className="overflow-hidden p-0">
            <details className="group" open={settingsOpen} onToggle={(event) => setSettingsOpen(event.currentTarget.open)}>
              <summary className="flex cursor-pointer list-none items-center justify-between gap-4 px-5 py-4 marker:content-none">
                <div className="min-w-0">
                  <p className="flex items-center gap-2 text-sm font-semibold text-text-primary"><Gauge size={16} className="text-brand" /> {t("dubbing.settingsTitle")}</p>
                  <p className="mt-1 truncate text-xs text-text-tertiary">
                    {selectedLocalModel?.name ?? selectedProvider?.name ?? t("dubbing.chooseEngine")} · {(selectedVoiceProfile?.name ?? voice) || "—"} · {globalSpeed.toFixed(2)}×
                  </p>
                </div>
                <span className="flex shrink-0 items-center gap-2 text-xs font-semibold text-brand">
                  {t("dubbing.settingsExpand")} <ChevronDown size={16} className="transition-transform group-open:rotate-180" />
                </span>
              </summary>
              <div className="border-t border-border-subtle p-5">
                <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_minmax(20rem,0.72fr)]">
              <div className="space-y-4">
                <div className="flex flex-wrap items-center gap-2">
                  <Badge variant="success"><HardDrive size={12} className="mr-1" /> {t("dubbing.localBadge")}</Badge>
                  <Badge variant="warning"><Cloud size={12} className="mr-1" /> {t("dubbing.cloudBadge")}</Badge>
                  <span className="text-xs text-text-tertiary">{t("dubbing.engineSeparation")}</span>
                </div>
                <label className="block space-y-1.5 text-sm font-medium text-text-secondary">
                  <span>{t("dubbing.engine")}</span>
                  <Select value={engineValue} onChange={(event) => changeEngine(event.target.value)}>
                    <option value="">{t("dubbing.chooseEngine")}</option>
                    {readyModels.length > 0 && (
                      <optgroup label={t("dubbing.localEngines")}>
                        {readyModels.map((model) => <option key={model.id} value={`local:${model.id}`}>{model.name}</option>)}
                      </optgroup>
                    )}
                    {providers.length > 0 && (
                      <optgroup label={t("dubbing.cloudEngines")}>
                        {providers.map((provider) => <option key={provider.id} value={`cloud:${provider.id}`}>{provider.name}</option>)}
                      </optgroup>
                    )}
                  </Select>
                </label>

                {!engineValue && (
                  <button type="button" onClick={() => navigate("/models")} className="text-sm font-semibold text-brand underline underline-offset-4">
                    {t("dubbing.configureEngine")}
                  </button>
                )}

                {selectedLocalModel && !selectedLocalModel.clone_only ? (
                  <label className="block space-y-1.5 text-sm font-medium text-text-secondary">
                    <span>{t("dubbing.voice")}</span>
                    <Select value={voice} onChange={(event) => setVoice(event.target.value)}>
                      {selectedLocalModel.voices.map((item) => <option key={item.id} value={item.id}>{item.label}</option>)}
                    </Select>
                  </label>
                ) : selectedProvider ? (
                  <div className="space-y-3">
                    {cloudVoiceProfiles.length > 0 && (
                      <label className="block space-y-1.5 text-xs font-semibold text-text-secondary">
                        <span>{t("dubbing.myCloudVoice")}</span>
                        <Select value={selectedVoiceProfile?.engine === "zipvoice" ? "" : selectedVoiceProfileId} onChange={(event) => selectVoiceProfile(event.target.value)}>
                          <option value="">{t("dubbing.manualCloudVoice")}</option>
                          {cloudVoiceProfiles.map((profile) => <option key={profile.id} value={profile.id}>{profile.name} · {profile.cloud_voice_id}</option>)}
                        </Select>
                      </label>
                    )}
                    <label className="block space-y-1.5 text-sm font-medium text-text-secondary">
                      <span>{t("dubbing.voice")}</span>
                      <Input value={voice} onChange={(event) => { setVoice(event.target.value); setSelectedVoiceProfileId(""); }} placeholder={selectedProvider.voice} />
                    </label>
                    {selectedVoiceProfile?.engine !== "zipvoice" && selectedVoiceProfile && <div className="flex items-start gap-2 rounded-xl border border-success/20 bg-success/10 px-3 py-2.5 text-xs leading-5 text-success"><CheckCircle2 size={15} className="mt-0.5 shrink-0" /><span><strong className="block">{t("dubbing.savedCloudVoiceActive", { name: selectedVoiceProfile.name })}</strong>{t("dubbing.savedCloudVoiceHint")}</span></div>}
                  </div>
                ) : null}

                {selectedLocalModel?.clone_only && (
                  <div className="space-y-3 rounded-2xl border border-brand/15 bg-brand/5 p-4">
                    <div className="flex items-center gap-2 text-sm font-semibold text-text-primary"><WandSparkles size={16} className="text-brand" /> {t("dubbing.cloneConfig")}</div>
                    <label className="block space-y-1.5 text-xs font-semibold text-text-secondary">
                      <span>{t("dubbing.myVoice")}</span>
                      <Select value={selectedVoiceProfileId} onChange={(event) => selectVoiceProfile(event.target.value)}>
                        <option value="">{t("dubbing.temporaryReference")}</option>
                        {localVoiceProfiles.map((profile) => <option key={profile.id} value={profile.id}>{profile.name} · {profile.language === "zh" ? t("language.zh") : t("language.en")}</option>)}
                      </Select>
                    </label>
                    {selectedVoiceProfile ? (
                      <div className="flex items-start gap-2 rounded-xl border border-success/20 bg-success/10 px-3 py-2.5 text-xs leading-5 text-success">
                        <CheckCircle2 size={15} className="mt-0.5 shrink-0" />
                        <span className="min-w-0"><strong className="block">{t("dubbing.savedVoiceActive", { name: selectedVoiceProfile.name })}</strong>{t("dubbing.savedVoiceHint", { seconds: (selectedVoiceProfile.quality.duration_ms / 1000).toFixed(1) })}</span>
                      </div>
                    ) : (
                      <>
                        <div className="flex gap-2">
                          <Input value={referenceAudio} readOnly placeholder={t("dubbing.referenceAudio")} />
                          <Button type="button" variant="secondary" size="sm" onClick={chooseReference}><FolderOpen size={14} /> {t("common.browse")}</Button>
                        </div>
                        <Textarea value={referenceText} onChange={(event) => setReferenceText(event.target.value)} rows={3} placeholder={t("dubbing.referenceText")} />
                        <button type="button" onClick={() => navigate("/voices")} className="text-left text-xs font-semibold text-brand underline underline-offset-4">{t("dubbing.createSavedVoice")}</button>
                      </>
                    )}
                    <label className="block space-y-1.5 text-xs font-medium text-text-secondary">
                      <span>{t("dubbing.cloneQuality")}</span>
                      <Select value={cloneQuality} onChange={(event) => setCloneQuality(event.target.value as "standard" | "high")}>
                        <option value="standard">{t("dubbing.cloneStandard")}</option>
                        <option value="high">{t("dubbing.cloneHigh")}</option>
                      </Select>
                    </label>
                  </div>
                )}
              </div>

              <div className="space-y-4">
                <label className="block space-y-2 text-sm font-medium text-text-secondary">
                  <span className="flex items-center justify-between"><span>{t("dubbing.globalSpeed")}</span><span className="font-mono text-brand">{globalSpeed.toFixed(2)}×</span></span>
                  <input type="range" min="0.5" max="2" step="0.05" value={globalSpeed} onChange={(event) => setGlobalSpeed(Number(event.target.value))} className="w-full accent-brand" />
                </label>
                <div className="rounded-2xl border border-border-subtle bg-surface-overlay p-4 text-xs leading-5 text-text-tertiary">
                  <p className="flex items-start gap-2"><Gauge size={15} className="mt-0.5 shrink-0 text-brand" /> {t("dubbing.alignmentDesc")}</p>
                  <p className="mt-2 flex items-start gap-2"><AudioLines size={15} className="mt-0.5 shrink-0 text-brand" /> {t("dubbing.overlapDesc")}</p>
                </div>
                <div className="grid grid-cols-2 gap-2 text-center sm:grid-cols-4 xl:grid-cols-2 2xl:grid-cols-4">
                  <div className="rounded-xl bg-surface-overlay p-3"><div className="font-display text-lg font-semibold text-text-primary">{session.cues.length}</div><div className="text-xs text-text-tertiary">{t("dubbing.total")}</div></div>
                  <div className="rounded-xl bg-success/10 p-3"><div className="font-display text-lg font-semibold text-success">{doneCount}</div><div className="text-xs text-text-tertiary">{t("dubbing.readyCount")}</div></div>
                  <div className="rounded-xl bg-brand/8 p-3"><div className="font-display text-lg font-semibold text-brand">{autoAdjustedCount}</div><div className="text-xs text-text-tertiary">{t("dubbing.autoAdjusted")}</div></div>
                  <div className="rounded-xl bg-warning/10 p-3"><div className="font-display text-lg font-semibold text-warning">{overlongCount + failedCount}</div><div className="text-xs text-text-tertiary">{t("dubbing.attention")}</div></div>
                </div>
              </div>
                </div>
              </div>
            </details>
          </Card>

          <div className="sticky top-3 z-20 rounded-[1.3rem] border border-white/40 bg-surface-card/90 p-3 shadow-lg backdrop-blur-xl">
            <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
              <div className="min-w-0">
                <p className="truncate text-sm font-semibold text-text-primary">{session.subtitle_path}</p>
                <p className="mt-0.5 text-xs text-text-tertiary">{t("dubbing.persisted", { id: session.id.slice(0, 8) })}</p>
              </div>
              <div className="flex flex-wrap gap-2">
                {batchRunning ? (
                  <Button type="button" variant="danger" size="sm" onClick={cancelGeneration}><Pause size={14} /> {t("dubbing.cancelBatch")}</Button>
                ) : (
                  <Button type="button" variant="primary" size="sm" onClick={generateBatch} disabled={!engineValue || activeGenerationId !== null}><Play size={14} /> {t("dubbing.generatePending")}</Button>
                )}
                <Button type="button" variant="secondary" size="sm" onClick={exportAudio} disabled={!canExport || exporting || activeGenerationId !== null}>
                  {exporting ? <LoaderCircle size={14} className="animate-spin" /> : <Save size={14} />} {t("dubbing.export")}
                </Button>
                <Button type="button" variant="secondary" size="sm" onClick={exportSubtitleCopy} disabled={subtitleSaving !== null || activeGenerationId !== null}>
                  {subtitleSaving === "copy" ? <LoaderCircle size={14} className="animate-spin" /> : <Download size={14} />}
                  {subtitleSaving === "copy" ? t("dubbing.subtitleSaving") : t("dubbing.exportSubtitle")}
                </Button>
                <Button
                  type="button"
                  variant="secondary"
                  size="sm"
                  onClick={() => setWriteBackOpen(true)}
                  disabled={!session.subtitle_dirty || session.source_changed || subtitleSaving !== null || activeGenerationId !== null}
                  title={!session.subtitle_dirty ? t("dubbing.writeBackModalDesc") : undefined}
                >
                  <FileCheck2 size={14} /> {t("dubbing.writeBackSubtitle")}
                </Button>
              </div>
            </div>
            {batchRunning && <div className="mt-3"><Progress value={batchProgress} /></div>}
          </div>

          {subtitleArtifact && (
            <Card className="flex flex-col gap-3 border-brand/15 bg-brand/5 p-4 sm:flex-row sm:items-center sm:justify-between">
              <div className="min-w-0">
                <p className="flex items-center gap-2 text-sm font-semibold text-text-primary">
                  <ShieldCheck size={16} className="text-brand" />
                  {subtitleArtifact.kind === "backup" ? t("dubbing.subtitleArtifactBackup") : t("dubbing.subtitleArtifactCopy")}
                </p>
                <p className="mt-1 truncate font-mono text-xs text-text-tertiary">{subtitleArtifact.path}</p>
              </div>
              <Button type="button" variant="secondary" size="sm" onClick={() => revealItemInDir(subtitleArtifact.path)}>
                <FolderOpen size={14} /> {t("dubbing.revealSubtitle")}
              </Button>
            </Card>
          )}

          <div className={`grid gap-4 ${session.video_path ? "xl:grid-cols-[minmax(20rem,0.72fr)_minmax(0,1.28fr)] xl:items-start" : ""}`}>
            {session.video_path && (
              <div className="xl:sticky xl:top-[6.5rem]">
                <DubbingVideoPanel
                  videoPath={session.video_path}
                  videoRef={videoRef}
                  currentTimeMs={currentTimeMs}
                  currentCue={playbackCue}
                  onTimeUpdate={setCurrentTimeMs}
                  onPlayingChange={setVideoPlaying}
                  t={t}
                />
              </div>
            )}
            <div className="min-w-0 space-y-3">
              {session.cues.map((cue) => (
                <div
                  key={cue.index}
                  ref={(element) => {
                    if (element) cueElements.current.set(cue.index, element);
                    else cueElements.current.delete(cue.index);
                  }}
                >
                  <CueCard
                    cue={cue}
                    generating={activeCueIndex === cue.index}
                    playbackActive={playbackCue?.index === cue.index}
                    disabled={batchRunning || exporting || subtitleSaving !== null || activeGenerationId !== null}
                    onGenerate={() => generateSingle(cue.index)}
                    onAccept={() => acceptOverflow(cue.index)}
                    onUpdate={editCue}
                    onSeek={session.video_path ? () => seekToCue(cue) : undefined}
                    globalVoice={voice}
                    voiceOptions={selectedLocalModel && !selectedLocalModel.clone_only ? selectedLocalModel.voices : []}
                    t={t}
                  />
                </div>
              ))}
            </div>
          </div>

          {session.output_path && (
            <Card className="flex flex-col gap-3 border-success/20 bg-success/5 p-4 sm:flex-row sm:items-center sm:justify-between">
              <div className="min-w-0"><p className="flex items-center gap-2 text-sm font-semibold text-success"><CheckCircle2 size={16} /> {t("dubbing.outputReady")}</p><p className="mt-1 truncate font-mono text-xs text-text-tertiary">{session.output_path}</p></div>
              <Button type="button" variant="secondary" size="sm" onClick={() => revealItemInDir(session.output_path!)}><FolderOpen size={14} /> {t("dubbing.revealOutput")}</Button>
            </Card>
          )}

          {writeBackOpen && (
            <div
              className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4 backdrop-blur-sm"
              role="dialog"
              aria-modal="true"
              aria-labelledby="dubbing-writeback-title"
              onMouseDown={(event) => {
                if (event.target === event.currentTarget && subtitleSaving !== "source") setWriteBackOpen(false);
              }}
            >
              <Card className="w-full max-w-lg border border-border-default bg-surface-overlay p-6 shadow-2xl">
                <div className="flex items-start gap-3">
                  <span className="rounded-full bg-brand/10 p-2 text-brand"><ShieldCheck size={20} /></span>
                  <div className="min-w-0">
                    <h3 id="dubbing-writeback-title" className="font-display text-h2 font-bold text-text-primary">{t("dubbing.writeBackModalTitle")}</h3>
                    <p className="mt-2 text-sm leading-6 text-text-secondary">{t("dubbing.writeBackModalDesc")}</p>
                  </div>
                </div>
                <div className="mt-5 space-y-2 rounded-2xl border border-border-subtle bg-surface-card p-4 text-xs leading-5 text-text-secondary">
                  <p className="flex gap-2"><CheckCircle2 size={15} className="mt-0.5 shrink-0 text-success" /> {t("dubbing.writeBackGuardChanged")}</p>
                  <p className="flex gap-2"><CheckCircle2 size={15} className="mt-0.5 shrink-0 text-success" /> {t("dubbing.writeBackGuardBackup")}</p>
                  <p className="flex gap-2 text-warning"><AlertTriangle size={15} className="mt-0.5 shrink-0" /> {t("dubbing.writeBackFormatNotice")}</p>
                </div>
                <p className="mt-4 break-all font-mono text-xs leading-5 text-text-tertiary">{session.subtitle_path}</p>
                <div className="mt-6 flex justify-end gap-2.5">
                  <Button type="button" variant="secondary" size="sm" onClick={() => setWriteBackOpen(false)} disabled={subtitleSaving === "source"}>
                    {t("common.cancel")}
                  </Button>
                  <Button type="button" variant="primary" size="sm" onClick={writeBackSubtitle} disabled={subtitleSaving === "source"}>
                    {subtitleSaving === "source" ? <LoaderCircle size={14} className="animate-spin" /> : <ShieldCheck size={14} />}
                    {subtitleSaving === "source" ? t("dubbing.subtitleSaving") : t("dubbing.writeBackConfirm")}
                  </Button>
                </div>
              </Card>
            </div>
          )}
        </>
      )}
    </div>
  );
}

function DubbingVideoPanel({
  videoPath,
  videoRef,
  currentTimeMs,
  currentCue,
  onTimeUpdate,
  onPlayingChange,
  t,
}: {
  videoPath: string;
  videoRef: RefObject<HTMLVideoElement | null>;
  currentTimeMs: number;
  currentCue: DubbingCue | null;
  onTimeUpdate: (timeMs: number) => void;
  onPlayingChange: (playing: boolean) => void;
  t: ReturnType<typeof useI18n>["t"];
}) {
  const [videoError, setVideoError] = useState(false);
  const videoUrl = useMemo(() => fileAssetUrl(videoPath), [videoPath]);

  useEffect(() => {
    setVideoError(false);
    onTimeUpdate(0);
    onPlayingChange(false);
  }, [onPlayingChange, onTimeUpdate, videoPath]);

  return (
    <Card className="overflow-hidden p-0">
      <div className="flex items-center justify-between gap-3 border-b border-border-subtle px-4 py-3">
        <div className="min-w-0">
          <p className="flex items-center gap-2 text-sm font-semibold text-text-primary"><Video size={16} className="text-brand" /> {t("dubbing.playerTitle")}</p>
          <p className="mt-0.5 truncate text-xs text-text-tertiary" title={videoPath}>{videoPath}</p>
        </div>
        <Badge variant="info">{t("dubbing.playerLinked")}</Badge>
      </div>
      <div className="relative flex aspect-video max-h-[42vh] items-center justify-center overflow-hidden bg-[#080b12]">
        {videoUrl && !videoError ? (
          <video
            ref={videoRef}
            src={videoUrl}
            controls
            playsInline
            preload="metadata"
            className="h-full max-h-[42vh] w-full object-contain"
            onTimeUpdate={(event) => onTimeUpdate(Math.round(event.currentTarget.currentTime * 1000))}
            onSeeked={(event) => onTimeUpdate(Math.round(event.currentTarget.currentTime * 1000))}
            onPlay={() => onPlayingChange(true)}
            onPause={() => onPlayingChange(false)}
            onEnded={() => onPlayingChange(false)}
            onError={() => {
              setVideoError(true);
              onPlayingChange(false);
            }}
          />
        ) : (
          <div className="flex max-w-sm flex-col items-center gap-2 px-6 text-center text-sm leading-6 text-white/60">
            <Video size={26} className="text-white/45" />
            {videoError ? t("dubbing.playerLoadError") : t("dubbing.playerNativeOnly")}
          </div>
        )}
      </div>
      <div className="p-4">
        <div className="flex items-center justify-between gap-3 text-xs text-text-tertiary">
          <span className="font-mono">{formatTime(currentTimeMs)}</span>
          <span>{currentCue ? t("dubbing.currentCue", { index: currentCue.index + 1 }) : t("dubbing.noActiveCue")}</span>
        </div>
        {currentCue && <p className="mt-2 line-clamp-3 text-sm font-medium leading-6 text-text-primary">{currentCue.text}</p>}
      </div>
    </Card>
  );
}

function CueCard({
  cue,
  generating,
  playbackActive,
  disabled,
  onGenerate,
  onAccept,
  onUpdate,
  onSeek,
  globalVoice,
  voiceOptions,
  t,
}: {
  cue: DubbingCue;
  generating: boolean;
  playbackActive: boolean;
  disabled: boolean;
  onGenerate: () => void;
  onAccept: () => void;
  onUpdate: (cueIndex: number, text: string, voiceId: string) => Promise<void>;
  onSeek?: () => void;
  globalVoice: string;
  voiceOptions: TtsVoice[];
  t: ReturnType<typeof useI18n>["t"];
}) {
  const [editing, setEditing] = useState(false);
  const [draftText, setDraftText] = useState(cue.text);
  const [draftVoice, setDraftVoice] = useState(cue.voice_id ?? "");
  const [saving, setSaving] = useState(false);
  useEffect(() => {
    if (!editing) {
      setDraftText(cue.text);
      setDraftVoice(cue.voice_id ?? "");
    }
  }, [cue.text, cue.voice_id, editing]);

  const saveEdit = async () => {
    setSaving(true);
    try {
      await onUpdate(cue.index, draftText, draftVoice);
      setEditing(false);
    } catch {
      // The parent has already surfaced the actionable error message.
    } finally {
      setSaving(false);
    }
  };

  const status = {
    pending: { label: t("dubbing.statusPending"), className: "text-text-tertiary", icon: Clock3 },
    synthesizing: { label: t("dubbing.statusSynthesizing"), className: "text-brand", icon: LoaderCircle },
    ready: { label: t("dubbing.statusReady"), className: "text-success", icon: CheckCircle2 },
    overlong: { label: t("dubbing.statusOverlong"), className: "text-warning", icon: AlertTriangle },
    accepted: { label: t("dubbing.statusAccepted"), className: "text-success", icon: CheckCircle2 },
    failed: { label: t("dubbing.statusFailed"), className: "text-danger", icon: AlertTriangle },
  }[cue.status];
  const Icon = status.icon;
  const alignment = cue.alignment_action ? {
    natural: { label: t("dubbing.actionNatural"), className: "text-text-tertiary" },
    precontrolled: { label: t("dubbing.actionPrecontrolled"), className: "text-brand" },
    resynthesized: { label: t("dubbing.actionResynthesized"), className: "text-success" },
    postprocessed: { label: t("dubbing.actionPostprocessed"), className: "text-brand" },
    "manual-accepted": { label: t("dubbing.actionManualAccepted"), className: "text-success" },
    "manual-review": { label: t("dubbing.actionManualReview"), className: "text-warning" },
  }[cue.alignment_action] : null;
  const audioUrl = cue.wav_path ? fileAssetUrl(cue.wav_path) : "";
  const cueHeading = (
    <>
      <span className="flex items-center gap-1.5 font-display text-lg font-semibold text-text-primary">
        #{cue.index + 1}
        {onSeek && <Play size={11} className="text-brand opacity-70 transition group-hover:opacity-100" />}
      </span>
      <span className="mt-1 block font-mono text-[11px] leading-5 text-text-tertiary">{formatTime(cue.start_ms)}<br />{formatTime(cue.end_ms)}</span>
    </>
  );

  return (
    <Card
      className={`p-4 ${playbackActive ? "border-brand/45 bg-brand/5 shadow-brand-glow" : generating ? "border-brand/30" : ""}`}
      aria-current={playbackActive ? "true" : undefined}
    >
      <div className="grid gap-4 sm:grid-cols-[5.5rem_minmax(0,1fr)] sm:items-center 2xl:grid-cols-[5.5rem_minmax(0,1fr)_15rem]">
        {onSeek ? (
          <button type="button" onClick={onSeek} className="group rounded-xl p-1 text-left transition hover:bg-brand/8" aria-label={t("dubbing.seekCue", { index: cue.index + 1 })}>
            {cueHeading}
          </button>
        ) : (
          <div>{cueHeading}</div>
        )}
        <div className="min-w-0">
          <p className="text-sm font-medium leading-6 text-text-primary">{cue.text}</p>
          <div className="mt-2 flex flex-wrap items-center gap-2 text-xs text-text-tertiary">
            <span className={`inline-flex items-center gap-1 font-semibold ${status.className}`}><Icon size={13} className={cue.status === "synthesizing" ? "animate-spin" : ""} /> {status.label}</span>
            <span>{t("dubbing.slot", { duration: (cue.slot_ms / 1000).toFixed(2) })}</span>
            {cue.estimated_ms !== null && <span>{t("dubbing.estimatedDuration", { duration: (cue.estimated_ms / 1000).toFixed(2) })}</span>}
            {cue.synthesized_ms !== null && <span>{t("dubbing.audioDuration", { duration: (cue.synthesized_ms / 1000).toFixed(2) })}</span>}
            {cue.ratio !== null && <span>{t("dubbing.ratio", { ratio: cue.ratio.toFixed(2) })}</span>}
            {cue.planned_speed !== null && <span>{t("dubbing.plannedSpeed", { speed: cue.planned_speed.toFixed(2) })}</span>}
            {cue.applied_speed !== null && <span>{t("dubbing.appliedSpeed", { speed: cue.applied_speed.toFixed(2) })}</span>}
            {alignment && (
              <span
                data-testid={`dubbing-alignment-action-${cue.index}`}
                className={`inline-flex items-center gap-1 text-[11px] font-semibold uppercase tracking-[0.08em] ${alignment.className}`}
              >
                <WandSparkles size={12} /> {alignment.label}
              </span>
            )}
            {cue.resynthesized && cue.alignment_action !== "resynthesized" && <span className="text-success">{t("dubbing.autoResynthesized")}</span>}
            {cue.overlap && <Badge variant="warning">{t("dubbing.overlap")}</Badge>}
            {cue.voice_id && <Badge variant="info">{t("dubbing.voiceOverride")}: {cue.voice_id}</Badge>}
          </div>
          {cue.error && <p className="mt-2 text-xs leading-5 text-danger">{cue.error}</p>}
        </div>
        <div className="flex flex-col items-stretch gap-2 sm:col-start-2 2xl:col-start-auto 2xl:items-end">
          {audioUrl && <audio controls preload="none" src={audioUrl} className="h-9 w-full max-w-[17rem]" />}
          <div className="flex flex-wrap justify-end gap-2">
            <Button type="button" variant="secondary" size="sm" onClick={() => setEditing((current) => !current)} disabled={disabled || saving}>
              {editing ? <X size={14} /> : <Pencil size={14} />}
              {editing ? t("common.cancel") : t("dubbing.editCue")}
            </Button>
            {cue.status === "overlong" && (
              <Button type="button" variant="secondary" size="sm" onClick={onAccept} disabled={disabled}>
                <Gauge size={14} /> {t("dubbing.acceptSpeed")}
              </Button>
            )}
            <Button type="button" variant={cue.status === "pending" || cue.status === "failed" ? "primary" : "secondary"} size="sm" onClick={onGenerate} disabled={disabled}>
              {generating ? <LoaderCircle size={14} className="animate-spin" /> : <Volume2 size={14} />}
              {cue.wav_path ? t("dubbing.regenerate") : t("dubbing.generate")}
            </Button>
          </div>
        </div>
      </div>
      {editing && (
        <div className="mt-4 grid gap-3 rounded-2xl border border-brand/20 bg-brand/5 p-3 sm:grid-cols-[minmax(0,1fr)_minmax(12rem,0.45fr)]">
          <label className="space-y-1.5 text-xs font-semibold text-text-secondary">
            <span>{t("dubbing.editCueText")}</span>
            <Textarea value={draftText} onChange={(event) => setDraftText(event.target.value)} rows={2} />
          </label>
          <label className="space-y-1.5 text-xs font-semibold text-text-secondary">
            <span>{t("dubbing.editCueVoice")}</span>
            {voiceOptions.length > 0 ? (
              <Select value={draftVoice} onChange={(event) => setDraftVoice(event.target.value)}>
                <option value="">{t("dubbing.useGlobalVoice", { voice: globalVoice || "—" })}</option>
                {voiceOptions.map((item) => <option key={item.id} value={item.id}>{item.label}</option>)}
              </Select>
            ) : (
              <Input value={draftVoice} onChange={(event) => setDraftVoice(event.target.value)} placeholder={globalVoice || t("dubbing.voice")} />
            )}
          </label>
          <div className="flex flex-wrap justify-end gap-2 sm:col-span-2">
            <Button type="button" variant="primary" size="sm" onClick={saveEdit} disabled={disabled || saving || !draftText.trim()}>
              {saving ? <LoaderCircle size={14} className="animate-spin" /> : <Save size={14} />}
              {t("dubbing.saveCueEdit")}
            </Button>
          </div>
        </div>
      )}
    </Card>
  );
}
