import { useState, useEffect } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Film, FolderOpen, AlertCircle, CheckCircle, Loader2 } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { useI18n } from "../lib/i18n";
import {
  burnSubtitle,
  cancelBurnSubtitle,
  getVideoMetadata,
  generateSubtitlePreview,
  type VideoMetadata,
} from "../lib/tauri";

import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { Input } from "../components/ui/Input";
import { Progress } from "../components/ui/Progress";

function assColorToCss(assColor: string): string {
  if (!assColor) return "rgb(255, 255, 255)";
  const cleanColor = assColor.trim().toUpperCase();
  const match = cleanColor.match(/^&H([0-9A-F]{2})([0-9A-F]{2})([0-9A-F]{2})([0-9A-F]{2})$/);
  if (match) {
    const aa = match[1];
    const bb = match[2];
    const gg = match[3];
    const rr = match[4];
    const alpha = (1 - parseInt(aa, 16) / 255).toFixed(2);
    const r = parseInt(rr, 16);
    const g = parseInt(gg, 16);
    const b = parseInt(bb, 16);
    return `rgba(${r}, ${g}, ${b}, ${alpha})`;
  }
  const matchNoAlpha = cleanColor.match(/^&H([0-9A-F]{2})([0-9A-F]{2})([0-9A-F]{2})$/);
  if (matchNoAlpha) {
    const bb = matchNoAlpha[1];
    const gg = matchNoAlpha[2];
    const rr = matchNoAlpha[3];
    const r = parseInt(rr, 16);
    const g = parseInt(gg, 16);
    const b = parseInt(bb, 16);
    return `rgb(${r}, ${g}, ${b})`;
  }
  return "rgb(255, 255, 255)";
}

const presets = [
  { key: "merge.style.classic", font_size: 24, font_color: "&H00FFFFFF", outline_color: "&H00000000", margin_v: 30 },
  { key: "merge.style.movie", font_size: 28, font_color: "&H00FFFFFF", outline_color: "&H00000000", margin_v: 40 },
  { key: "YouTube", font_size: 20, font_color: "&H00FFFFFF", outline_color: "&H00000000", margin_v: 20 },
  { key: "merge.style.minimal", font_size: 22, font_color: "&H00FFFFFF", outline_color: "&H00808080", margin_v: 25 },
  { key: "merge.style.bold", font_size: 32, font_color: "&H00FFFFFF", outline_color: "&H00000000", margin_v: 35 },
];

export default function SubtitleMergePage() {
  const { t } = useI18n();
  const [videoPath, setVideoPath] = useState("");
  const [subtitlePath, setSubtitlePath] = useState("");
  const [outputPath, setOutputPath] = useState("");
  const [preset, setPreset] = useState(0);
  const [fontSize, setFontSize] = useState(24);
  const [fontColor, setFontColor] = useState("&H00FFFFFF");
  const [outlineColor, setOutlineColor] = useState("&H00000000");
  const [marginV, setMarginV] = useState(30);
  const [processing, setProcessing] = useState(false);
  const [previewing, setPreviewing] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [result, setResult] = useState("");
  const [softSubtitle, setSoftSubtitle] = useState(false);

  // Progress state
  const [progress, setProgress] = useState<number | null>(null);

  // Metadata state
  const [metadata, setMetadata] = useState<VideoMetadata | null>(null);
  const [loadingMetadata, setLoadingMetadata] = useState(false);

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

  // Listen for burn progress updates
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    if (processing && outputPath) {
      listen<{ burn_id: string; video_path: string; progress: number }>("subtitle-burn-updated", (event) => {
        if (event.payload.burn_id === outputPath) {
          setProgress(event.payload.progress);
        }
      }).then((unsub) => {
        unlisten = unsub;
      });
    } else {
      setProgress(null);
    }

    return () => {
      if (unlisten) unlisten();
    };
  }, [processing, outputPath]);

  const handleSelectVideo = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: t("merge.videoFiles"), extensions: ["mp4", "mkv", "mov", "webm"] }],
    });
    if (typeof selected === "string") setVideoPath(selected);
  };

  const handleSelectSubtitle = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: t("merge.subtitleFiles"), extensions: ["srt", "ass", "vtt"] }],
    });
    if (typeof selected === "string") setSubtitlePath(selected);
  };

  const handleSelectOutput = async () => {
    const selected = await save({
      defaultPath: videoPath ? videoPath.replace(/\.[^.]+$/, "-subtitled.mp4") : "output.mp4",
      filters: [{ name: "MP4", extensions: ["mp4"] }],
    });
    if (selected) setOutputPath(selected);
  };

  const applyPreset = (idx: number) => {
    setPreset(idx);
    const p = presets[idx];
    setFontSize(p.font_size);
    setFontColor(p.font_color);
    setOutlineColor(p.outline_color);
    setMarginV(p.margin_v);
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
        font_size: fontSize,
        font_color: fontColor,
        outline_color: outlineColor,
        margin_v: marginV,
        soft_subtitle: softSubtitle,
      });
      setResult(out);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (message.includes("已取消") || message.includes("cancelled")) {
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
        font_size: fontSize,
        font_color: fontColor,
        outline_color: outlineColor,
        margin_v: marginV,
      });
      setNotice(t("merge.previewSuccess"));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setPreviewing(false);
    }
  };

  return (
    <div className="max-w-4xl space-y-6">
      <h2 className="text-display font-bold tracking-tight text-text-primary">{t("merge.title")}</h2>

      <div className="space-y-6">
        {/* 选择文件 */}
        <Card className="p-5">
          <h3 className="mb-4 font-semibold text-text-primary text-h2">{t("merge.selectFiles")}</h3>
          <div className="space-y-3.5">
            <div className="flex items-center gap-3">
              <Button onClick={handleSelectVideo} disabled={processing} variant="secondary" size="sm" className="h-8">
                <FolderOpen size={12} />
                <span>{t("merge.selectVideo")}</span>
              </Button>
              <span className="truncate text-xs text-text-secondary font-mono">{videoPath || t("merge.notSelected")}</span>
            </div>
            <div className="flex items-center gap-3">
              <Button onClick={handleSelectSubtitle} disabled={processing} variant="secondary" size="sm" className="h-8">
                <FolderOpen size={12} />
                <span>{t("merge.selectSubtitle")}</span>
              </Button>
              <span className="truncate text-xs text-text-secondary font-mono">{subtitlePath || t("merge.notSelected")}</span>
            </div>
            <div className="flex items-center gap-3">
              <Button onClick={handleSelectOutput} disabled={processing} variant="secondary" size="sm" className="h-8">
                <FolderOpen size={12} />
                <span>{t("merge.selectOutput")}</span>
              </Button>
              <span className="truncate text-xs text-text-secondary font-mono">{outputPath || t("merge.notSelected")}</span>
            </div>
          </div>
        </Card>

        {loadingMetadata && (
          <div className="flex items-center gap-2 text-xs text-text-tertiary p-2">
            <Loader2 className="animate-spin h-3.5 w-3.5" />
            <span>{t("merge.analyzingMetadata")}</span>
          </div>
        )}

        {metadata && (
          <Card className="p-5">
            <h3 className="mb-3.5 font-semibold text-text-primary text-h3">{t("merge.metadataOutline")}</h3>
            <div className="grid grid-cols-2 gap-4 sm:grid-cols-4 text-xs font-mono">
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

        {/* 字幕样式 */}
        <Card className="p-5">
          <h3 className="mb-4 font-semibold text-text-primary text-h2">{t("merge.subtitleStyle")}</h3>

          <div className="mb-6 flex items-center gap-3.5 rounded-xl bg-brand-subtle border border-brand/10 p-4">
            <input
              type="checkbox"
              id="softSubtitle"
              checked={softSubtitle}
              disabled={processing}
              onChange={(e) => setSoftSubtitle(e.target.checked)}
              className="h-3.5 w-3.5 rounded border-border-default text-brand focus:ring-0 cursor-pointer disabled:opacity-50"
            />
            <div className="flex flex-col">
              <label htmlFor="softSubtitle" className="text-sm font-semibold text-text-primary cursor-pointer select-none">
                {t("merge.softSubtitleLabel")}
              </label>
              <span className="text-xs text-text-secondary mt-1 leading-5">
                {t("merge.softSubtitleDesc")}
              </span>
            </div>
          </div>

          <div className="mb-5">
            <label className="mb-2 block text-sm font-medium text-text-secondary">{t("merge.preset")}</label>
            <div className="flex flex-wrap gap-2.5">
              {presets.map((p, i) => (
                <button
                  key={p.key}
                  onClick={() => applyPreset(i)}
                  disabled={processing || softSubtitle}
                  className={`rounded-lg border px-3 py-1.5 text-xs transition duration-150 ${
                    preset === i
                      ? "border-brand bg-brand-subtle text-brand-text font-semibold"
                      : "border-border-default text-text-secondary hover:border-border-strong hover:bg-surface-overlay"
                  } disabled:opacity-50 disabled:cursor-not-allowed`}
                >
                  {p.key.startsWith("merge.") ? t(p.key as any) : p.key}
                </button>
              ))}
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
            <div>
              <label className="mb-1.5 block text-xs font-medium text-text-secondary">{t("merge.fontSize")}</label>
              <Input type="number" min={10} max={72} value={fontSize} disabled={processing || softSubtitle} onChange={(e) => setFontSize(Number(e.target.value))} className="h-9" />
            </div>
            <div>
              <label className="mb-1.5 block text-xs font-medium text-text-secondary">{t("merge.fontColor")}</label>
              <Input type="text" value={fontColor} disabled={processing || softSubtitle} onChange={(e) => setFontColor(e.target.value)} className="h-9 font-mono" />
            </div>
            <div>
              <label className="mb-1.5 block text-xs font-medium text-text-secondary">{t("merge.outlineColor")}</label>
              <Input type="text" value={outlineColor} disabled={processing || softSubtitle} onChange={(e) => setOutlineColor(e.target.value)} className="h-9 font-mono" />
            </div>
            <div>
              <label className="mb-1.5 block text-xs font-medium text-text-secondary">{t("merge.marginV")}</label>
              <Input type="number" min={0} max={100} value={marginV} disabled={processing || softSubtitle} onChange={(e) => setMarginV(Number(e.target.value))} className="h-9" />
            </div>
          </div>

          <div className="mt-5">
            <label className="mb-2 block text-xs font-medium text-text-secondary">{t("merge.previewStyle")}</label>
            <div className="relative flex h-32 w-full items-center justify-center rounded-xl border border-border-subtle bg-black overflow-hidden shadow-inner">
              <div className="absolute inset-0 bg-[linear-gradient(45deg,#1f2937_25%,transparent_25%),linear-gradient(-45deg,#1f2937_25%,transparent_25%),linear-gradient(45deg,transparent_75%,#1f2937_75%),linear-gradient(-45deg,transparent_75%,#1f2937_75%)] bg-[size:16px_16px] bg-[position:0_0,0_8px,8px_-8px,-8px_0] opacity-30"></div>
              <div
                className="relative z-10 px-4 py-1 text-center select-none font-bold font-sans"
                style={{
                  fontSize: `${fontSize}px`,
                  color: assColorToCss(fontColor),
                  textShadow: `
                    -1px -1px 0 ${assColorToCss(outlineColor)},  
                     1px -1px 0 ${assColorToCss(outlineColor)},
                    -1px  1px 0 ${assColorToCss(outlineColor)},
                     1px  1px 0 ${assColorToCss(outlineColor)},
                    -2px -2px 0 ${assColorToCss(outlineColor)},  
                     2px -2px 0 ${assColorToCss(outlineColor)},
                    -2px  2px 0 ${assColorToCss(outlineColor)},
                     2px  2px 0 ${assColorToCss(outlineColor)}
                  `,
                  transform: `translateY(${marginV / 4}px)`,
                }}
              >
                {t("merge.previewPlaceholder")}
              </div>
              {softSubtitle && (
                <div className="absolute inset-0 z-20 flex items-center justify-center bg-black/85 backdrop-blur-[1px] text-xs font-medium text-text-tertiary">
                  {t("merge.softSubtitlePlayerHint")}
                </div>
              )}
            </div>
          </div>
        </Card>

        {/* 烧录执行与状态 */}
        <Card className="p-5">
          {error && (
            <div className="mb-4 flex items-start gap-2 rounded-lg border border-danger/20 bg-danger/10 px-3 py-2.5 text-xs text-danger leading-5">
              <AlertCircle className="mt-0.5 shrink-0" size={14} />
              <span>{error}</span>
            </div>
          )}

          {result && (
            <div className="mb-4 flex items-start gap-2 rounded-lg border border-success/20 bg-success/10 px-3 py-2.5 text-xs text-success leading-5">
              <CheckCircle className="mt-0.5 shrink-0" size={14} />
              <span>{t("merge.burnCompleted").replace("{result}", result)}</span>
            </div>
          )}

          {notice && (
            <div className="mb-4 flex items-start gap-2 rounded-lg border border-warning/20 bg-warning/10 px-3 py-2.5 text-xs text-warning leading-5">
              <AlertCircle className="mt-0.5 shrink-0" size={14} />
              <span>{notice}</span>
            </div>
          )}

          {prerequisiteHint && !error && !processing && (
            <div className="mb-4 flex items-start gap-2 rounded-lg border border-warning/20 bg-warning/10 px-3 py-2.5 text-xs text-warning leading-5">
              <AlertCircle className="mt-0.5 shrink-0" size={14} />
              <span>{prerequisiteHint}</span>
            </div>
          )}

          {processing && progress !== null && (
            <div className="mb-5 space-y-2">
              <div className="flex items-center justify-between text-xs text-text-secondary">
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

            {!processing && (
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
