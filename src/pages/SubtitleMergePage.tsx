import { useState, useEffect } from "react";
import { Film, FolderOpen, AlertCircle, CheckCircle, Loader2, AudioLines, X, Cpu, Zap } from "lucide-react";
import { useI18n } from "../lib/i18n";
import {
  composeRequiresMkv,
  defaultComposeOutputPath,
  replaceMediaExtension,
  type ComposeAudioMode,
} from "../lib/compose";
import {
  burnSubtitle,
  cancelBurnSubtitle,
  getVideoEncoderInfo,
  getVideoMetadata,
  generateSubtitlePreview,
  listen,
  openDialog,
  saveDialog,
  type VideoEncoderInfo,
  type VideoEncoderMode,
  type VideoMetadata,
  type SubtitleStyle,
} from "../lib/tauri";
import { assColorToCss, DEFAULT_SUBTITLE_STYLE } from "../lib/subtitleStyles";

import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { Input, Select } from "../components/ui/Input";
import { Progress } from "../components/ui/Progress";
import { SubtitleStylePresetManager } from "../components/SubtitleStylePresetManager";

export default function SubtitleMergePage() {
  const { t } = useI18n();
  const [videoPath, setVideoPath] = useState("");
  const [subtitlePath, setSubtitlePath] = useState("");
  const [outputPath, setOutputPath] = useState("");
  const [activePresetId, setActivePresetId] = useState<string | null>("builtin:classic");
  const [fontName, setFontName] = useState(DEFAULT_SUBTITLE_STYLE.font_name);
  const [fontSize, setFontSize] = useState(DEFAULT_SUBTITLE_STYLE.font_size);
  const [fontColor, setFontColor] = useState(DEFAULT_SUBTITLE_STYLE.font_color);
  const [outlineColor, setOutlineColor] = useState(DEFAULT_SUBTITLE_STYLE.outline_color);
  const [outlineWidth, setOutlineWidth] = useState(DEFAULT_SUBTITLE_STYLE.outline_width);
  const [shadow, setShadow] = useState(DEFAULT_SUBTITLE_STYLE.shadow);
  const [backgroundColor, setBackgroundColor] = useState(DEFAULT_SUBTITLE_STYLE.background_color);
  const [opaqueBackground, setOpaqueBackground] = useState(DEFAULT_SUBTITLE_STYLE.opaque_background);
  const [alignment, setAlignment] = useState(DEFAULT_SUBTITLE_STYLE.alignment);
  const [marginV, setMarginV] = useState(DEFAULT_SUBTITLE_STYLE.margin_v);
  const [crf, setCrf] = useState(20);
  const [encodingPreset, setEncodingPreset] = useState("medium");
  const [encoderMode, setEncoderMode] = useState<VideoEncoderMode>("auto");
  const [encoderInfo, setEncoderInfo] = useState<VideoEncoderInfo | null>(null);
  const [loadingEncoderInfo, setLoadingEncoderInfo] = useState(true);
  const [processing, setProcessing] = useState(false);
  const [previewing, setPreviewing] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [result, setResult] = useState("");
  const [softSubtitle, setSoftSubtitle] = useState(false);
  const [audioPath, setAudioPath] = useState("");
  const [audioMode, setAudioMode] = useState<ComposeAudioMode>("replace");
  const [subtitleLanguage, setSubtitleLanguage] = useState("und");
  const [subtitleTitle, setSubtitleTitle] = useState(() => t("merge.subtitleTrackDefaultTitle"));
  const [audioLanguage, setAudioLanguage] = useState("und");
  const [audioTitle, setAudioTitle] = useState(() => t("merge.audioTrackDefaultTitle"));

  // Progress state
  const [progress, setProgress] = useState<number | null>(null);

  // Metadata state
  const [metadata, setMetadata] = useState<VideoMetadata | null>(null);
  const [loadingMetadata, setLoadingMetadata] = useState(false);
  const requiresMkv = composeRequiresMkv(softSubtitle, audioPath, audioMode);
  const encoderStatus = loadingEncoderInfo
    ? t("merge.hardwareChecking")
    : encoderInfo?.available
      ? `${encoderInfo.encoder_label ?? "Hardware"} · ${encoderInfo.rate_mode === "bitrate" ? "bitrate" : "CQ"}`
      : encoderInfo?.platform_supported
        ? t("merge.hardwareUnavailable")
        : t("merge.hardwareUnsupported");

  const missingInputs = [
    !videoPath ? t("merge.missingVideo") : "",
    !subtitlePath ? t("merge.missingSubtitle") : "",
    !outputPath ? t("merge.missingOutput") : "",
  ].filter(Boolean);
  const prerequisiteHint = missingInputs.length > 0
    ? t("merge.pleaseSelect", { items: missingInputs.join(t("merge.listSeparator")) })
    : "";

  // Fetch video metadata when videoPath changes
  useEffect(() => {
    if (videoPath) {
      setLoadingMetadata(true);
      setMetadata(null);
      getVideoMetadata(videoPath)
        .then((meta) => {
          setMetadata(meta);
        })
        .catch((err) => {
          console.error("Failed to get video metadata:", err);
        })
        .finally(() => {
          setLoadingMetadata(false);
        });
    } else {
      setMetadata(null);
    }
  }, [videoPath]);

  useEffect(() => {
    let active = true;
    setLoadingEncoderInfo(true);
    getVideoEncoderInfo()
      .then((info) => {
        if (active) setEncoderInfo(info);
      })
      .catch(() => {
        if (active) setEncoderInfo({ available: false, platform_supported: true });
      })
      .finally(() => {
        if (active) setLoadingEncoderInfo(false);
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (metadata?.audio_tracks === 0 && audioMode === "mix") {
      setAudioMode("replace");
    }
  }, [audioMode, metadata]);

  useEffect(() => {
    if (requiresMkv && outputPath && !outputPath.toLowerCase().endsWith(".mkv")) {
      setOutputPath(replaceMediaExtension(outputPath, "mkv"));
    }
  }, [outputPath, requiresMkv]);

  // Listen for burn progress updates
  useEffect(() => {
    let unlistenProgress: (() => void) | undefined;
    let unlistenFallback: (() => void) | undefined;
    let disposed = false;

    if (processing && outputPath) {
      listen<{ burn_id: string; video_path: string; progress: number }>("subtitle-burn-updated", (event) => {
        if (event.payload.burn_id === outputPath) {
          setProgress(event.payload.progress);
        }
      }).then((unsub) => {
        if (disposed) unsub();
        else unlistenProgress = unsub;
      });
      listen<{ burn_id: string; encoder: string }>("subtitle-burn-fallback", (event) => {
        if (event.payload.burn_id === outputPath) {
          setNotice(t("merge.hardwareFallback", { encoder: event.payload.encoder }));
        }
      }).then((unsub) => {
        if (disposed) unsub();
        else unlistenFallback = unsub;
      });
    } else {
      setProgress(null);
    }

    return () => {
      disposed = true;
      unlistenProgress?.();
      unlistenFallback?.();
    };
  }, [processing, outputPath]);

  const handleSelectVideo = async () => {
    const selected = await openDialog({
      multiple: false,
      filters: [{ name: t("merge.videoFiles"), extensions: ["mp4", "mkv", "mov", "webm"] }],
    });
    if (typeof selected === "string") setVideoPath(selected);
  };

  const handleSelectSubtitle = async () => {
    const selected = await openDialog({
      multiple: false,
      filters: [{ name: t("merge.subtitleFiles"), extensions: ["srt", "ass", "vtt"] }],
    });
    if (typeof selected === "string") setSubtitlePath(selected);
  };

  const handleSelectAudio = async () => {
    const selected = await openDialog({
      multiple: false,
      filters: [{ name: t("merge.audioFiles"), extensions: ["wav", "mp3", "m4a", "aac", "flac", "ogg", "opus"] }],
    });
    if (typeof selected === "string") setAudioPath(selected);
  };

  const handleSelectOutput = async () => {
    const selected = await saveDialog({
      defaultPath: defaultComposeOutputPath(videoPath, requiresMkv),
      filters: requiresMkv
        ? [{ name: "Matroska Video", extensions: ["mkv"] }]
        : [
            { name: "MP4 Video", extensions: ["mp4"] },
            { name: "Matroska Video", extensions: ["mkv"] },
          ],
    });
    if (selected) setOutputPath(selected);
  };

  const currentStyle: SubtitleStyle = {
    font_name: fontName,
    font_size: fontSize,
    font_color: fontColor,
    outline_color: outlineColor,
    outline_width: outlineWidth,
    shadow,
    background_color: backgroundColor,
    opaque_background: opaqueBackground,
    alignment,
    margin_v: marginV,
  };

  const applyStyle = (style: SubtitleStyle, presetId: string) => {
    setActivePresetId(presetId);
    setFontName(style.font_name);
    setFontSize(style.font_size);
    setFontColor(style.font_color);
    setOutlineColor(style.outline_color);
    setOutlineWidth(style.outline_width);
    setShadow(style.shadow);
    setBackgroundColor(style.background_color);
    setOpaqueBackground(style.opaque_background);
    setAlignment(style.alignment);
    setMarginV(style.margin_v);
  };

  const handleBurn = async () => {
    if (!videoPath || !subtitlePath || !outputPath) {
      setError(prerequisiteHint || t("merge.selectPrereqError"));
      return;
    }
    setProcessing(true);
    setProgress(0);
    setError("");
    setNotice("");
    setResult("");
    try {
      const out = await burnSubtitle({
        video_path: videoPath,
        subtitle_path: subtitlePath,
        output_path: outputPath,
        font_name: fontName,
        font_size: fontSize,
        font_color: fontColor,
        outline_color: outlineColor,
        outline_width: outlineWidth,
        shadow,
        background_color: backgroundColor,
        opaque_background: opaqueBackground,
        alignment,
        margin_v: marginV,
        crf,
        preset: encodingPreset,
        encoder_mode: softSubtitle ? "cpu" : encoderMode,
        soft_subtitle: softSubtitle,
        audio_path: audioPath || undefined,
        audio_mode: audioPath ? audioMode : "keep",
        subtitle_language: subtitleLanguage,
        subtitle_title: subtitleTitle,
        audio_language: audioLanguage,
        audio_title: audioTitle,
      });
      setResult(out);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (message.includes("finalsub:burn-cancelled")) {
        setNotice(t("merge.cancelled"));
      } else {
        setError(message);
      }
    } finally {
      setProcessing(false);
    }
  };

  const handleCancelBurn = async () => {
    if (!outputPath) return;
    try {
      await cancelBurnSubtitle(outputPath);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const handlePreview = async () => {
    if (!videoPath || !subtitlePath) {
      setError(t("merge.previewPrereqError"));
      return;
    }
    setPreviewing(true);
    setError("");
    setNotice("");
    try {
      await generateSubtitlePreview({
        video_path: videoPath,
        subtitle_path: subtitlePath,
        output_path: "",
        font_name: fontName,
        font_size: fontSize,
        font_color: fontColor,
        outline_color: outlineColor,
        outline_width: outlineWidth,
        shadow,
        background_color: backgroundColor,
        opaque_background: opaqueBackground,
        alignment,
        margin_v: marginV,
        crf,
        preset: "ultrafast",
        encoder_mode: "cpu",
      });
      setNotice(t("merge.previewSuccess"));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setPreviewing(false);
    }
  };

  return (
    <div className="page-shell space-y-7">
      <h2 className="font-display text-display font-bold tracking-tight text-text-primary">{t("merge.title")}</h2>

      <div className="space-y-6">
        {/* 选择文件 */}
        <Card className="p-6">
          <h3 className="mb-5 font-display text-h2 font-semibold text-text-primary">{t("merge.selectFiles")}</h3>
          <div className="space-y-3.5">
            <div className="flex items-center gap-3">
              <Button onClick={handleSelectVideo} disabled={processing} variant="secondary" size="sm">
                <FolderOpen size={14} />
                <span>{t("merge.selectVideo")}</span>
              </Button>
              <span className="truncate font-mono text-sm text-text-secondary">{videoPath || t("merge.notSelected")}</span>
            </div>
            <div className="flex items-center gap-3">
              <Button onClick={handleSelectSubtitle} disabled={processing} variant="secondary" size="sm">
                <FolderOpen size={14} />
                <span>{t("merge.selectSubtitle")}</span>
              </Button>
              <span className="truncate font-mono text-sm text-text-secondary">{subtitlePath || t("merge.notSelected")}</span>
            </div>
            <div className="flex items-center gap-3">
              <Button onClick={handleSelectAudio} disabled={processing} variant="secondary" size="sm">
                <AudioLines size={14} />
                <span>{t("merge.selectAudio")}</span>
              </Button>
              <span className="min-w-0 flex-1 truncate font-mono text-sm text-text-secondary">
                {audioPath || t("merge.audioOptional")}
              </span>
              {audioPath && (
                <button
                  type="button"
                  onClick={() => setAudioPath("")}
                  disabled={processing}
                  aria-label={t("merge.removeAudio")}
                  className="rounded-lg p-1.5 text-text-tertiary transition hover:bg-surface-overlay hover:text-text-primary disabled:opacity-50"
                >
                  <X size={14} />
                </button>
              )}
            </div>
            <div className="flex items-center gap-3">
              <Button onClick={handleSelectOutput} disabled={processing} variant="secondary" size="sm">
                <FolderOpen size={14} />
                <span>{t("merge.selectOutput")}</span>
              </Button>
              <span className="truncate font-mono text-sm text-text-secondary">{outputPath || t("merge.notSelected")}</span>
            </div>
          </div>
        </Card>

        {loadingMetadata && (
          <div className="flex items-center gap-2 p-2 text-sm text-text-tertiary">
            <Loader2 className="animate-spin h-3.5 w-3.5" />
            <span>{t("merge.analyzingMetadata")}</span>
          </div>
        )}

        {metadata && (
          <Card className="p-6">
            <h3 className="mb-4 font-display text-h3 font-semibold text-text-primary">{t("merge.metadataOutline")}</h3>
            <div className="grid grid-cols-2 gap-4 font-mono text-sm sm:grid-cols-4">
              <div className="p-3 bg-surface-overlay border border-border-subtle rounded-lg">
                <span className="text-text-tertiary block mb-1 font-sans">{t("merge.resolution")}</span>
                <span className="font-semibold text-text-primary">{metadata.width} x {metadata.height}</span>
              </div>
              <div className="p-3 bg-surface-overlay border border-border-subtle rounded-lg">
                <span className="text-text-tertiary block mb-1 font-sans">{t("merge.duration")}</span>
                <span className="font-semibold text-text-primary">{metadata.duration_string} ({metadata.duration_seconds.toFixed(1)}s)</span>
              </div>
              <div className="p-3 bg-surface-overlay border border-border-subtle rounded-lg">
                <span className="text-text-tertiary block mb-1 font-sans">{t("merge.fps")}</span>
                <span className="font-semibold text-text-primary">{metadata.fps} fps</span>
              </div>
              <div className="p-3 bg-surface-overlay border border-border-subtle rounded-lg">
                <span className="text-text-tertiary block mb-1 font-sans">{t("merge.codec")}</span>
                <span className="font-semibold text-text-primary">{metadata.codec}</span>
              </div>
              {metadata.audio_codec && (
                <div className="p-3 bg-surface-overlay border border-border-subtle rounded-lg">
                  <span className="text-text-tertiary block mb-1 font-sans">{t("merge.audioCodec")}</span>
                  <span className="font-semibold text-text-primary">{metadata.audio_codec}</span>
                </div>
              )}
              {metadata.audio_sample_rate && (
                <div className="p-3 bg-surface-overlay border border-border-subtle rounded-lg">
                  <span className="text-text-tertiary block mb-1 font-sans">{t("merge.audioSampleRate")}</span>
                  <span className="font-semibold text-text-primary">{metadata.audio_sample_rate} Hz</span>
                </div>
              )}
              {metadata.audio_channels && (
                <div className="p-3 bg-surface-overlay border border-border-subtle rounded-lg">
                  <span className="text-text-tertiary block mb-1 font-sans">{t("merge.audioChannels")}</span>
                  <span className="font-semibold text-text-primary">{metadata.audio_channels} ch</span>
                </div>
              )}
              <div className="p-3 bg-surface-overlay border border-border-subtle rounded-lg">
                <span className="text-text-tertiary block mb-1 font-sans">{t("merge.audioTracks")}</span>
                <span className="font-semibold text-text-primary">{metadata.audio_tracks} tracks</span>
              </div>
            </div>
          </Card>
        )}

        <Card className="p-6">
          <div className="mb-5">
            <h3 className="font-display text-h2 font-semibold text-text-primary">{t("merge.composeMode")}</h3>
            <p className="mt-1.5 text-sm leading-6 text-text-secondary">{t("merge.composeModeDesc")}</p>
          </div>

          <div className="grid gap-3 sm:grid-cols-2">
            <button
              type="button"
              data-testid="compose-mode-hard"
              aria-pressed={!softSubtitle}
              disabled={processing}
              onClick={() => setSoftSubtitle(false)}
              className={`rounded-2xl border p-4 text-left transition ${!softSubtitle ? "liquid-selected border-brand/30" : "border-border-subtle bg-surface-overlay hover:border-border-strong"}`}
            >
              <span className="block text-sm font-semibold text-text-primary">{t("merge.hardSubtitleLabel")}</span>
              <span className="mt-1.5 block text-sm leading-6 text-text-secondary">{t("merge.hardSubtitleDesc")}</span>
            </button>
            <button
              type="button"
              data-testid="compose-mode-soft"
              aria-pressed={softSubtitle}
              disabled={processing}
              onClick={() => setSoftSubtitle(true)}
              className={`rounded-2xl border p-4 text-left transition ${softSubtitle ? "liquid-selected border-brand/30" : "border-border-subtle bg-surface-overlay hover:border-border-strong"}`}
            >
              <span className="block text-sm font-semibold text-text-primary">{t("merge.softSubtitleLabel")}</span>
              <span className="mt-1.5 block text-sm leading-6 text-text-secondary">{t("merge.softSubtitleDesc")}</span>
            </button>
          </div>

          {softSubtitle && (
            <div className="mt-4 grid gap-4 rounded-2xl border border-border-subtle bg-surface-overlay p-4 sm:grid-cols-[minmax(0,0.38fr)_minmax(0,1fr)]">
              <div>
                <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("merge.trackLanguage")}</label>
                <Input
                  value={subtitleLanguage}
                  maxLength={16}
                  disabled={processing}
                  onChange={(event) => setSubtitleLanguage(event.target.value)}
                  placeholder="zho / eng / und"
                  className="h-9 font-mono"
                />
              </div>
              <div>
                <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("merge.trackTitle")}</label>
                <Input
                  value={subtitleTitle}
                  maxLength={128}
                  disabled={processing}
                  onChange={(event) => setSubtitleTitle(event.target.value)}
                  className="h-9"
                />
              </div>
            </div>
          )}

          <div className="mt-6 border-t border-border-subtle pt-5">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <h4 className="text-sm font-semibold text-text-primary">{t("merge.audioCompose")}</h4>
                <p className="mt-1 text-sm leading-6 text-text-secondary">
                  {audioPath ? t("merge.audioComposeReady") : t("merge.audioComposeEmpty")}
                </p>
              </div>
              {!audioPath && (
                <Button onClick={handleSelectAudio} disabled={processing} variant="secondary" size="sm">
                  <AudioLines size={14} />
                  <span>{t("merge.selectAudio")}</span>
                </Button>
              )}
            </div>

            {audioPath && (
              <div className="mt-4 space-y-4">
                <div className="rounded-xl border border-border-subtle bg-surface-overlay px-3.5 py-2.5 font-mono text-xs text-text-secondary">
                  {audioPath}
                </div>
                <div className="grid gap-2 sm:grid-cols-3">
                  {(["replace", "mix", "add-track"] as const).map((mode) => {
                    const mixUnavailable = mode === "mix" && metadata?.audio_tracks === 0;
                    return (
                      <button
                        key={mode}
                        type="button"
                        data-testid={`compose-audio-mode-${mode}`}
                        aria-pressed={audioMode === mode}
                        disabled={processing || mixUnavailable}
                        onClick={() => setAudioMode(mode)}
                        className={`rounded-xl border px-3.5 py-3 text-left transition ${audioMode === mode ? "liquid-selected border-brand/30" : "border-border-subtle bg-surface-overlay hover:border-border-strong"} disabled:cursor-not-allowed disabled:opacity-45`}
                      >
                        <span className="block text-sm font-semibold text-text-primary">{t(`merge.audioMode.${mode}` as any)}</span>
                        <span className="mt-1 block text-xs leading-5 text-text-secondary">{t(`merge.audioMode.${mode}Desc` as any)}</span>
                      </button>
                    );
                  })}
                </div>
                {metadata?.audio_tracks === 0 && (
                  <p className="text-xs leading-5 text-warning">{t("merge.mixNeedsSourceAudio")}</p>
                )}
                <div className="grid gap-4 sm:grid-cols-[minmax(0,0.38fr)_minmax(0,1fr)]">
                  <div>
                    <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("merge.trackLanguage")}</label>
                    <Input
                      value={audioLanguage}
                      maxLength={16}
                      disabled={processing}
                      onChange={(event) => setAudioLanguage(event.target.value)}
                      placeholder="zho / eng / und"
                      className="h-9 font-mono"
                    />
                  </div>
                  <div>
                    <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("merge.trackTitle")}</label>
                    <Input
                      value={audioTitle}
                      maxLength={128}
                      disabled={processing}
                      onChange={(event) => setAudioTitle(event.target.value)}
                      className="h-9"
                    />
                  </div>
                </div>
              </div>
            )}
          </div>

          <div className="mt-5 flex flex-wrap gap-2 border-t border-border-subtle pt-4 text-xs font-semibold">
            <span className="rounded-full bg-surface-overlay px-3 py-1.5 text-text-secondary">
              {softSubtitle ? t("merge.summary.videoCopy") : t("merge.summary.videoEncode")}
            </span>
            <span className="rounded-full bg-surface-overlay px-3 py-1.5 text-text-secondary">
              {softSubtitle ? t("merge.summary.subtitleSwitchable") : t("merge.summary.subtitlePermanent")}
            </span>
            {!softSubtitle && (
              <span className="rounded-full bg-surface-overlay px-3 py-1.5 text-text-secondary">
                {t(`merge.summary.encoder.${encoderMode}` as any)}
              </span>
            )}
            <span className="rounded-full bg-surface-overlay px-3 py-1.5 text-text-secondary">
              {audioPath ? t(`merge.summary.audio.${audioMode}` as any) : t("merge.summary.audio.keep")}
            </span>
            <span className="rounded-full bg-brand-subtle px-3 py-1.5 text-brand-text">
              {requiresMkv ? "MKV" : "MP4 / MKV"}
            </span>
          </div>
        </Card>

        {/* 字幕样式 */}
        {!softSubtitle && (
          <Card className="p-6">
          <h3 className="mb-5 font-display text-h2 font-semibold text-text-primary">{t("merge.subtitleStyle")}</h3>

          <div className="mb-5">
            <SubtitleStylePresetManager
              currentStyle={currentStyle}
              activePresetId={activePresetId}
              disabled={processing || softSubtitle}
              onApply={applyStyle}
              onActivePresetRemoved={() => setActivePresetId(null)}
            />
          </div>

          <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
            <div className="col-span-2">
              <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("merge.fontName")}</label>
              <Input type="text" maxLength={128} value={fontName} disabled={processing || softSubtitle} onChange={(e) => setFontName(e.target.value)} className="h-9" />
            </div>
            <div>
              <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("merge.fontSize")}</label>
              <Input type="number" min={10} max={72} value={fontSize} disabled={processing || softSubtitle} onChange={(e) => setFontSize(Number(e.target.value))} className="h-9" />
            </div>
            <div>
              <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("merge.fontColor")}</label>
              <Input type="text" value={fontColor} disabled={processing || softSubtitle} onChange={(e) => setFontColor(e.target.value)} className="h-9 font-mono" />
            </div>
            <div>
              <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("merge.outlineColor")}</label>
              <Input type="text" value={outlineColor} disabled={processing || softSubtitle} onChange={(e) => setOutlineColor(e.target.value)} className="h-9 font-mono" />
            </div>
            <div>
              <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("merge.outlineWidth")}</label>
              <Input type="number" min={0} max={10} step={0.5} value={outlineWidth} disabled={processing || softSubtitle} onChange={(e) => setOutlineWidth(Number(e.target.value))} className="h-9" />
            </div>
            <div>
              <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("merge.shadow")}</label>
              <Input type="number" min={0} max={20} step={0.5} value={shadow} disabled={processing || softSubtitle} onChange={(e) => setShadow(Number(e.target.value))} className="h-9" />
            </div>
            <div>
              <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("merge.marginV")}</label>
              <Input type="number" min={0} max={100} value={marginV} disabled={processing || softSubtitle} onChange={(e) => setMarginV(Number(e.target.value))} className="h-9" />
            </div>
            <div>
              <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("merge.quality")}</label>
              <Input type="number" min={0} max={51} value={crf} disabled={processing || softSubtitle} onChange={(e) => setCrf(Number(e.target.value))} className="h-9" />
            </div>
            <div>
              <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("merge.encodingPreset")}</label>
              <Select value={encodingPreset} disabled={processing || softSubtitle || encoderMode === "hardware"} onChange={(e) => setEncodingPreset(e.target.value)} className="h-9">
                {['ultrafast', 'veryfast', 'fast', 'medium', 'slow', 'veryslow'].map((value) => <option key={value} value={value}>{value}</option>)}
              </Select>
            </div>
            <div
              data-testid="video-encoder-panel"
              className="col-span-2 rounded-xl border border-border-subtle bg-surface-overlay p-3.5 sm:col-span-4"
            >
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="flex items-center gap-2 text-sm font-semibold text-text-primary">
                      <Cpu size={15} className="text-brand" />
                      {t("merge.encoderMode")}
                    </span>
                    <span
                      data-testid="video-encoder-status"
                      className={`rounded-full border px-2.5 py-1 font-mono text-[11px] font-semibold ${
                        encoderInfo?.available
                          ? "border-success/20 bg-success/10 text-success"
                          : "border-border-subtle bg-surface-elevated text-text-tertiary"
                      }`}
                    >
                      {encoderStatus}
                    </span>
                  </div>
                  <p className="mt-1 text-xs leading-5 text-text-tertiary">{t("merge.encoderModeDesc")}</p>
                </div>
                {loadingEncoderInfo && <Loader2 size={14} className="mt-1 animate-spin text-text-tertiary" />}
              </div>

              <div className="mt-3 grid grid-cols-3 gap-1 rounded-lg border border-border-subtle bg-surface-elevated p-1">
                {(["auto", "cpu", "hardware"] as VideoEncoderMode[]).map((mode) => {
                  const disabled = processing || (mode === "hardware" && !encoderInfo?.available);
                  return (
                    <button
                      key={mode}
                      type="button"
                      data-testid={`video-encoder-${mode}`}
                      aria-pressed={encoderMode === mode}
                      disabled={disabled}
                      onClick={() => setEncoderMode(mode)}
                      className={`flex min-h-9 items-center justify-center gap-1.5 rounded-md px-2 text-xs font-semibold transition ${
                        encoderMode === mode
                          ? "liquid-selected text-brand-text"
                          : "text-text-secondary hover:bg-surface-overlay hover:text-text-primary"
                      } disabled:cursor-not-allowed disabled:opacity-40`}
                    >
                      {mode === "hardware" && <Zap size={12} />}
                      {t(`merge.encoderMode.${mode}` as any)}
                    </button>
                  );
                })}
              </div>
              <p className="mt-2 text-xs leading-5 text-text-secondary">
                {t(`merge.encoderMode.${encoderMode}Desc` as any)}
              </p>
            </div>
            <label className="col-span-2 flex cursor-pointer items-center gap-3 rounded-xl border border-border-subtle bg-surface-overlay px-3.5 py-3 text-sm text-text-secondary">
              <input type="checkbox" checked={opaqueBackground} disabled={processing || softSubtitle} onChange={(e) => setOpaqueBackground(e.target.checked)} className="h-4 w-4 accent-brand" />
              <span className="font-semibold text-text-primary">{t("merge.opaqueBackground")}</span>
            </label>
            <div className="col-span-2">
              <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("merge.backgroundColor")}</label>
              <Input type="text" value={backgroundColor} disabled={processing || softSubtitle || !opaqueBackground} onChange={(e) => setBackgroundColor(e.target.value)} className="h-9 font-mono" />
            </div>
          </div>

          <div className="mt-5">
            <label className="mb-2 block text-sm font-medium text-text-secondary">{t("merge.alignment")}</label>
            <div className="grid w-44 grid-cols-3 gap-1.5 rounded-xl border border-border-subtle bg-surface-overlay p-2">
              {[7, 8, 9, 4, 5, 6, 1, 2, 3].map((value) => (
                <button
                  key={value}
                  type="button"
                  disabled={processing || softSubtitle}
                  onClick={() => setAlignment(value)}
                  aria-pressed={alignment === value}
                  className={`h-9 rounded-lg text-xs font-bold transition ${alignment === value ? "liquid-selected text-brand" : "text-text-tertiary hover:bg-surface-overlay hover:text-text-primary"}`}
                >
                  {value}
                </button>
              ))}
            </div>
          </div>

          <div className="mt-5">
            <label className="mb-2 block text-sm font-medium text-text-secondary">{t("merge.previewStyle")}</label>
            <div
              className="relative flex h-52 w-full overflow-hidden rounded-xl border border-border-subtle bg-black p-5 shadow-inner"
              style={{
                alignItems: alignment >= 7 ? "flex-start" : alignment >= 4 ? "center" : "flex-end",
                justifyContent: [1, 4, 7].includes(alignment) ? "flex-start" : [3, 6, 9].includes(alignment) ? "flex-end" : "center",
              }}
            >
              <div className="absolute inset-0 bg-[linear-gradient(45deg,#1f2937_25%,transparent_25%),linear-gradient(-45deg,#1f2937_25%,transparent_25%),linear-gradient(45deg,transparent_75%,#1f2937_75%),linear-gradient(-45deg,transparent_75%,#1f2937_75%)] bg-[size:16px_16px] bg-[position:0_0,0_8px,8px_-8px,-8px_0] opacity-30"></div>
              <div
                className="relative z-10 max-w-[90%] px-4 py-1 text-center font-bold select-none"
                style={{
                  fontFamily: fontName,
                  fontSize: `${fontSize}px`,
                  color: assColorToCss(fontColor),
                  WebkitTextStroke: `${outlineWidth}px ${assColorToCss(outlineColor)}`,
                  paintOrder: "stroke fill",
                  textShadow: shadow > 0 ? `${shadow}px ${shadow}px ${Math.max(1, shadow / 2)}px rgba(0,0,0,.85)` : "none",
                  background: opaqueBackground ? assColorToCss(backgroundColor) : "transparent",
                  borderRadius: opaqueBackground ? "0.4rem" : undefined,
                  transform: alignment <= 3 ? `translateY(-${marginV / 6}px)` : alignment >= 7 ? `translateY(${marginV / 6}px)` : undefined,
                }}
              >
                {t("merge.previewPlaceholder")}
              </div>
            </div>
          </div>
          </Card>
        )}

        {/* 合成执行与状态 */}
        <Card className="p-6">
          {error && (
            <div className="mb-4 flex items-start gap-2 rounded-xl border border-danger/20 bg-danger/10 px-3.5 py-3 text-sm leading-6 text-danger">
              <AlertCircle className="mt-0.5 shrink-0" size={14} />
              <span>{error}</span>
            </div>
          )}

          {result && (
            <div className="mb-4 flex items-start gap-2 rounded-xl border border-success/20 bg-success/10 px-3.5 py-3 text-sm leading-6 text-success">
              <CheckCircle className="mt-0.5 shrink-0" size={14} />
              <span>{t("merge.burnCompleted").replace("{result}", result)}</span>
            </div>
          )}

          {notice && (
            <div className="mb-4 flex items-start gap-2 rounded-xl border border-warning/20 bg-warning/10 px-3.5 py-3 text-sm leading-6 text-warning">
              <AlertCircle className="mt-0.5 shrink-0" size={14} />
              <span>{notice}</span>
            </div>
          )}

          {prerequisiteHint && !error && !processing && (
            <div className="mb-4 flex items-start gap-2 rounded-xl border border-warning/20 bg-warning/10 px-3.5 py-3 text-sm leading-6 text-warning">
              <AlertCircle className="mt-0.5 shrink-0" size={14} />
              <span>{prerequisiteHint}</span>
            </div>
          )}

          {processing && progress !== null && (
            <div className="mb-5 space-y-2">
              <div className="flex items-center justify-between text-sm text-text-secondary">
                <span className="flex items-center gap-1.5 font-semibold">
                  <Loader2 className="animate-spin h-3.5 w-3.5 text-brand" />
                  {t("merge.burning")}
                </span>
                <span className="font-semibold text-brand-text">{progress.toFixed(1)}%</span>
              </div>
              <Progress value={progress} />
            </div>
          )}

          <div className="flex flex-wrap gap-3">
            <Button
              onClick={handleBurn}
              disabled={processing || previewing || !!prerequisiteHint}
              variant="primary"
              title={prerequisiteHint || undefined}
            >
              <Film size={14} />
              <span>{processing ? t("merge.burningBtn") : t("merge.startBurn")}</span>
            </Button>

            {processing && (
              <Button
                onClick={handleCancelBurn}
                variant="danger"
              >
                <span>{t("merge.cancelBurn")}</span>
              </Button>
            )}

            {!processing && !softSubtitle && (
              <Button
                onClick={handlePreview}
                disabled={previewing || !videoPath || !subtitlePath}
                variant="secondary"
              >
                {previewing ? (
                  <>
                    <Loader2 className="animate-spin h-3.5 w-3.5 text-text-tertiary" />
                    <span>{t("merge.generatingPreview")}</span>
                  </>
                ) : (
                  <span>{t("merge.generatePreview")}</span>
                )}
              </Button>
            )}
          </div>
        </Card>
      </div>
    </div>
  );
}
