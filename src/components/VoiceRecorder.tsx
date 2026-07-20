import { useCallback, useEffect, useRef, useState } from "react";
import { Check, LoaderCircle, Mic, Play, RotateCcw, Square, X } from "lucide-react";

import { saveVoiceRecording } from "../lib/tauri";
import { useI18n } from "../lib/i18n";
import { Button } from "./ui/Button";

const MAX_RECORDING_MS = 120_000;

type RecorderPhase = "idle" | "recording" | "recorded";

function formatElapsed(milliseconds: number): string {
  const seconds = Math.floor(milliseconds / 1000);
  return `${String(Math.floor(seconds / 60)).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`;
}

function blobBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error("Failed to read recording"));
    reader.onload = () => {
      const value = String(reader.result ?? "");
      const comma = value.indexOf(",");
      if (comma < 0) reject(new Error("Invalid recording payload"));
      else resolve(value.slice(comma + 1));
    };
    reader.readAsDataURL(blob);
  });
}

export function VoiceRecorder({
  onConfirm,
  onCancel,
}: {
  onConfirm: (path: string) => Promise<void> | void;
  onCancel: () => void;
}) {
  const { t } = useI18n();
  const [phase, setPhase] = useState<RecorderPhase>("idle");
  const [elapsedMs, setElapsedMs] = useState(0);
  const [level, setLevel] = useState(0);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const [playing, setPlaying] = useState(false);
  const streamRef = useRef<MediaStream | null>(null);
  const recorderRef = useRef<MediaRecorder | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const blobRef = useRef<Blob | null>(null);
  const audioContextRef = useRef<AudioContext | null>(null);
  const animationRef = useRef(0);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const startedAtRef = useRef(0);
  const previewUrlRef = useRef("");
  const previewAudioRef = useRef<HTMLAudioElement | null>(null);

  const cleanupCapture = useCallback(() => {
    cancelAnimationFrame(animationRef.current);
    if (timerRef.current) clearInterval(timerRef.current);
    timerRef.current = null;
    audioContextRef.current?.close().catch(() => undefined);
    audioContextRef.current = null;
    streamRef.current?.getTracks().forEach((track) => track.stop());
    streamRef.current = null;
    recorderRef.current = null;
    setLevel(0);
  }, []);

  const cleanupPreview = useCallback(() => {
    previewAudioRef.current?.pause();
    previewAudioRef.current = null;
    setPlaying(false);
    if (previewUrlRef.current) URL.revokeObjectURL(previewUrlRef.current);
    previewUrlRef.current = "";
  }, []);

  useEffect(() => () => {
    cleanupCapture();
    cleanupPreview();
  }, [cleanupCapture, cleanupPreview]);

  const stop = useCallback(() => {
    const recorder = recorderRef.current;
    if (recorder && recorder.state !== "inactive") recorder.stop();
  }, []);

  const start = useCallback(async () => {
    setError("");
    cleanupPreview();
    blobRef.current = null;
    if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === "undefined") {
      setError(t("voices.recordUnavailable"));
      return;
    }
    let stream: MediaStream;
    try {
      stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          channelCount: 1,
          echoCancellation: false,
          noiseSuppression: false,
          autoGainControl: false,
        },
      });
    } catch (reason) {
      const name = (reason as DOMException)?.name;
      setError(name === "NotAllowedError" || name === "SecurityError"
        ? t("voices.recordDenied")
        : t("voices.recordFailed", { error: String(reason) }));
      return;
    }
    streamRef.current = stream;
    try {
      const context = new AudioContext();
      audioContextRef.current = context;
      const analyser = context.createAnalyser();
      analyser.fftSize = 1024;
      context.createMediaStreamSource(stream).connect(analyser);
      const samples = new Uint8Array(analyser.fftSize);
      const updateLevel = () => {
        analyser.getByteTimeDomainData(samples);
        let energy = 0;
        for (const sample of samples) {
          const normalized = (sample - 128) / 128;
          energy += normalized * normalized;
        }
        setLevel(Math.min(1, Math.sqrt(energy / samples.length) * 3));
        animationRef.current = requestAnimationFrame(updateLevel);
      };
      animationRef.current = requestAnimationFrame(updateLevel);

      const mimeType = ["audio/webm;codecs=opus", "audio/mp4", "audio/ogg;codecs=opus"]
        .find((candidate) => MediaRecorder.isTypeSupported(candidate));
      const recorder = mimeType
        ? new MediaRecorder(stream, { mimeType, audioBitsPerSecond: 128_000 })
        : new MediaRecorder(stream, { audioBitsPerSecond: 128_000 });
      recorderRef.current = recorder;
      chunksRef.current = [];
      recorder.ondataavailable = (event) => {
        if (event.data.size > 0) chunksRef.current.push(event.data);
      };
      recorder.onstop = () => {
        blobRef.current = new Blob(chunksRef.current, { type: recorder.mimeType || mimeType || "audio/mp4" });
        cleanupCapture();
        setPhase("recorded");
      };
      recorder.start(500);
      startedAtRef.current = Date.now();
      setElapsedMs(0);
      setPhase("recording");
      timerRef.current = setInterval(() => {
        const elapsed = Date.now() - startedAtRef.current;
        setElapsedMs(elapsed);
        if (elapsed >= MAX_RECORDING_MS) stop();
      }, 200);
    } catch (reason) {
      cleanupCapture();
      setError(t("voices.recordFailed", { error: String(reason) }));
    }
  }, [cleanupCapture, cleanupPreview, stop, t]);

  const preview = () => {
    if (playing) {
      cleanupPreview();
      return;
    }
    if (!blobRef.current) return;
    const url = URL.createObjectURL(blobRef.current);
    previewUrlRef.current = url;
    const audio = new Audio(url);
    previewAudioRef.current = audio;
    setPlaying(true);
    audio.onended = cleanupPreview;
    audio.onerror = cleanupPreview;
    audio.play().catch(cleanupPreview);
  };

  const confirm = async () => {
    const blob = blobRef.current;
    if (!blob || saving) return;
    setSaving(true);
    setError("");
    try {
      const path = await saveVoiceRecording(await blobBase64(blob), blob.type || "audio/mp4");
      cleanupPreview();
      await onConfirm(path);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="space-y-4 rounded-2xl border border-brand/15 bg-brand/5 p-4">
      <div className="flex items-start justify-between gap-3">
        <div>
          <p className="text-sm font-semibold text-text-primary">{t("voices.recordTitle")}</p>
          <p className="mt-1 text-xs leading-5 text-text-tertiary">{t("voices.recordHint")}</p>
        </div>
        <Button type="button" variant="ghost" size="sm" onClick={onCancel} disabled={saving || phase === "recording"}>
          <X size={14} /> {t("common.close")}
        </Button>
      </div>
      <div className="rounded-xl bg-surface-card p-3">
        <p className="text-[11px] font-semibold uppercase tracking-wider text-text-tertiary">{t("voices.recordScriptTitle")}</p>
        <p className="mt-1.5 text-sm leading-6 text-text-primary">{t("voices.recordScript")}</p>
      </div>
      <div className="flex items-center gap-3">
        <div className="h-2 flex-1 overflow-hidden rounded-full bg-surface-overlay">
          <div className={`h-full rounded-full transition-[width] duration-100 ${phase === "recording" ? "bg-danger" : "bg-brand/40"}`} style={{ width: `${Math.round(level * 100)}%` }} />
        </div>
        <span className={`w-12 font-mono text-sm tabular-nums ${phase === "recording" ? "text-danger" : "text-text-secondary"}`}>
          {formatElapsed(elapsedMs)}
        </span>
      </div>
      {error && <p className="rounded-xl border border-danger/20 bg-danger/10 px-3 py-2 text-xs leading-5 text-danger">{error}</p>}
      <div className="flex flex-wrap justify-end gap-2">
        {phase === "idle" && <Button type="button" variant="primary" size="sm" onClick={start}><Mic size={14} /> {t("voices.recordStart")}</Button>}
        {phase === "recording" && <Button type="button" variant="danger" size="sm" onClick={stop}><Square size={13} /> {t("voices.recordStop")}</Button>}
        {phase === "recorded" && (
          <>
            <Button type="button" variant="secondary" size="sm" onClick={preview} disabled={saving}>
              {playing ? <Square size={13} /> : <Play size={14} />} {t("voices.recordPreview")}
            </Button>
            <Button type="button" variant="secondary" size="sm" onClick={() => { cleanupPreview(); setElapsedMs(0); setPhase("idle"); blobRef.current = null; }} disabled={saving}>
              <RotateCcw size={14} /> {t("voices.recordAgain")}
            </Button>
            <Button type="button" variant="primary" size="sm" onClick={confirm} disabled={saving}>
              {saving ? <LoaderCircle size={14} className="animate-spin" /> : <Check size={14} />} {t("voices.recordUse")}
            </Button>
          </>
        )}
      </div>
    </div>
  );
}
