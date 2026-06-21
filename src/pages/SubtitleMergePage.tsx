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
  VideoMetadata,
} from "../lib/tauri";

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
    <div className="max-w-4xl">
      <h2 className="mb-6 text-2xl font-bold text-gray-900 dark:text-white">{t("merge.title")}</h2>

      <div className="space-y-5">
        <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-700 dark:bg-gray-800">
          <h3 className="mb-4 font-semibold text-gray-900 dark:text-white">{t("merge.selectFiles")}</h3>
          <div className="space-y-3">
            <div className="flex items-center gap-3">
              <button onClick={handleSelectVideo} disabled={processing} className="flex items-center gap-2 rounded-md border border-gray-300 px-3 py-1.5 text-sm hover:bg-gray-50 dark:border-gray-600 dark:hover:bg-gray-700 disabled:opacity-50">
                <FolderOpen size={14} /> {t("merge.selectVideo")}
              </button>
              <span className="truncate text-xs text-gray-500">{videoPath || t("merge.notSelected")}</span>
            </div>
            <div className="flex items-center gap-3">
              <button onClick={handleSelectSubtitle} disabled={processing} className="flex items-center gap-2 rounded-md border border-gray-300 px-3 py-1.5 text-sm hover:bg-gray-50 dark:border-gray-600 dark:hover:bg-gray-700 disabled:opacity-50">
                <FolderOpen size={14} /> {t("merge.selectSubtitle")}
              </button>
              <span className="truncate text-xs text-gray-500">{subtitlePath || t("merge.notSelected")}</span>
            </div>
            <div className="flex items-center gap-3">
              <button onClick={handleSelectOutput} disabled={processing} className="flex items-center gap-2 rounded-md border border-gray-300 px-3 py-1.5 text-sm hover:bg-gray-50 dark:border-gray-600 dark:hover:bg-gray-700 disabled:opacity-50">
                <FolderOpen size={14} /> {t("merge.selectOutput")}
              </button>
              <span className="truncate text-xs text-gray-500">{outputPath || t("merge.notSelected")}</span>
            </div>
          </div>
        </section>

        {loadingMetadata && (
          <div className="flex items-center gap-2 text-xs text-gray-500 p-2">
            <Loader2 className="animate-spin h-3.5 w-3.5" />
            <span>{t("merge.analyzingMetadata")}</span>
          </div>
        )}

        {metadata && (
          <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-700 dark:bg-gray-800">
            <h3 className="mb-3 font-semibold text-gray-900 dark:text-white text-sm">{t("merge.metadataOutline")}</h3>
            <div className="grid grid-cols-2 gap-4 sm:grid-cols-4 text-xs">
              <div className="p-3 bg-gray-50 dark:bg-gray-900/40 rounded-lg">
                <span className="text-gray-500 block mb-1">{t("merge.resolution")}</span>
                <span className="font-semibold text-gray-800 dark:text-gray-200">{metadata.width} x {metadata.height}</span>
              </div>
              <div className="p-3 bg-gray-50 dark:bg-gray-900/40 rounded-lg">
                <span className="text-gray-500 block mb-1">{t("merge.duration")}</span>
                <span className="font-semibold text-gray-800 dark:text-gray-200">{metadata.duration_string} ({metadata.duration_seconds.toFixed(1)}s)</span>
              </div>
              <div className="p-3 bg-gray-50 dark:bg-gray-900/40 rounded-lg">
                <span className="text-gray-500 block mb-1">{t("merge.fps")}</span>
                <span className="font-semibold text-gray-800 dark:text-gray-200">{metadata.fps} fps</span>
              </div>
              <div className="p-3 bg-gray-50 dark:bg-gray-900/40 rounded-lg">
                <span className="text-gray-500 block mb-1">{t("merge.codec")}</span>
                <span className="font-semibold text-gray-800 dark:text-gray-200 font-mono">{metadata.codec}</span>
              </div>
            </div>
          </section>
        )}

        <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-700 dark:bg-gray-800">
          <h3 className="mb-4 font-semibold text-gray-900 dark:text-white">{t("merge.subtitleStyle")}</h3>

          <div className="mb-4">
            <label className="mb-2 block text-sm font-medium text-gray-700 dark:text-gray-300">{t("merge.preset")}</label>
            <div className="flex flex-wrap gap-2">
              {presets.map((p, i) => (
                <button
                  key={p.key}
                  onClick={() => applyPreset(i)}
                  disabled={processing}
                  className={`rounded-md border px-3 py-1 text-sm ${
                    preset === i
                      ? "border-blue-500 bg-blue-50 text-blue-700 dark:border-blue-400 dark:bg-blue-900/30"
                      : "border-gray-200 text-gray-600 dark:border-gray-600 dark:text-gray-400"
                  } disabled:opacity-50`}
                >
                  {p.key.startsWith("merge.") ? t(p.key as any) : p.key}
                </button>
              ))}
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
            <div>
              <label className="mb-1 block text-xs text-gray-500">{t("merge.fontSize")}</label>
              <input type="number" min={10} max={72} value={fontSize} disabled={processing} onChange={(e) => setFontSize(Number(e.target.value))} className="w-full rounded border border-gray-300 px-2 py-1 text-sm dark:border-gray-600 dark:bg-gray-700 disabled:opacity-50" />
            </div>
            <div>
              <label className="mb-1 block text-xs text-gray-500">{t("merge.fontColor")}</label>
              <input type="text" value={fontColor} disabled={processing} onChange={(e) => setFontColor(e.target.value)} className="w-full rounded border border-gray-300 px-2 py-1 text-sm font-mono dark:border-gray-600 dark:bg-gray-700 disabled:opacity-50" />
            </div>
            <div>
              <label className="mb-1 block text-xs text-gray-500">{t("merge.outlineColor")}</label>
              <input type="text" value={outlineColor} disabled={processing} onChange={(e) => setOutlineColor(e.target.value)} className="w-full rounded border border-gray-300 px-2 py-1 text-sm font-mono dark:border-gray-600 dark:bg-gray-700 disabled:opacity-50" />
            </div>
            <div>
              <label className="mb-1 block text-xs text-gray-500">{t("merge.marginV")}</label>
              <input type="number" min={0} max={100} value={marginV} disabled={processing} onChange={(e) => setMarginV(Number(e.target.value))} className="w-full rounded border border-gray-300 px-2 py-1 text-sm dark:border-gray-600 dark:bg-gray-700 disabled:opacity-50" />
            </div>
          </div>

          <div className="mt-4">
            <label className="mb-2 block text-xs text-gray-500">{t("merge.previewStyle")}</label>
            <div className="relative flex h-28 w-full items-center justify-center rounded-lg border border-gray-300 bg-gray-900 overflow-hidden dark:border-gray-600">
              <div className="absolute inset-0 bg-[linear-gradient(45deg,#1f2937_25%,transparent_25%),linear-gradient(-45deg,#1f2937_25%,transparent_25%),linear-gradient(45deg,transparent_75%,#1f2937_75%),linear-gradient(-45deg,transparent_75%,#1f2937_75%)] bg-[size:16px_16px] bg-[position:0_0,0_8px,8px_-8px,-8px_0] opacity-40"></div>
              <div
                className="relative z-10 px-4 py-1 text-center select-none font-bold"
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
                  fontFamily: "sans-serif"
                }}
              >
                {t("merge.previewPlaceholder")}
              </div>
            </div>
          </div>
        </section>

        <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-700 dark:bg-gray-800">
          {error && (
            <div className="mb-4 flex items-start gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-300">
              <AlertCircle className="mt-0.5 shrink-0" size={16} />
              <span>{error}</span>
            </div>
          )}

          {result && (
            <div className="mb-4 flex items-start gap-2 rounded-md border border-green-200 bg-green-50 px-3 py-2 text-sm text-green-700 dark:border-green-900/60 dark:bg-green-950/30 dark:text-green-300">
              <CheckCircle className="mt-0.5 shrink-0" size={16} />
              <span>{t("merge.burnCompleted").replace("{result}", result)}</span>
            </div>
          )}

          {notice && (
            <div className="mb-4 flex items-start gap-2 rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-800 dark:border-amber-900/60 dark:bg-amber-950/30 dark:text-amber-200">
              <AlertCircle className="mt-0.5 shrink-0" size={16} />
              <span>{notice}</span>
            </div>
          )}

          {prerequisiteHint && !error && !processing && (
            <div className="mb-4 flex items-start gap-2 rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-800 dark:border-amber-900/60 dark:bg-amber-950/30 dark:text-amber-200">
              <AlertCircle className="mt-0.5 shrink-0" size={16} />
              <span>{prerequisiteHint}</span>
            </div>
          )}

          {processing && progress !== null && (
            <div className="mb-5 space-y-2">
              <div className="flex items-center justify-between text-xs text-gray-500">
                <span className="flex items-center gap-1.5">
                  <Loader2 className="animate-spin h-3.5 w-3.5 text-blue-500" />
                  {t("merge.burning")}
                </span>
                <span className="font-semibold text-blue-600 dark:text-blue-400">{progress.toFixed(1)}%</span>
              </div>
              <div className="w-full bg-gray-200 dark:bg-gray-700 h-2.5 rounded-full overflow-hidden">
                <div
                  className="bg-blue-600 h-full rounded-full transition-all duration-300"
                  style={{ width: `${progress}%` }}
                />
              </div>
            </div>
          )}

          <div className="flex flex-wrap gap-3">
            <button
              onClick={handleBurn}
              disabled={processing || previewing || !!prerequisiteHint}
              className="inline-flex items-center gap-2 rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
              title={prerequisiteHint || undefined}
            >
              <Film size={16} />
              {processing ? t("merge.burningBtn") : t("merge.startBurn")}
            </button>

            {processing && (
              <button
                onClick={handleCancelBurn}
                className="inline-flex items-center gap-2 rounded-md bg-red-600 hover:bg-red-700 px-4 py-2 text-sm font-medium text-white transition-colors"
              >
                {t("merge.cancelBurn")}
              </button>
            )}

            {!processing && (
              <button
                onClick={handlePreview}
                disabled={previewing || !videoPath || !subtitlePath}
                className="inline-flex items-center gap-2 rounded-md border border-gray-300 hover:border-gray-400 dark:border-gray-600 dark:hover:border-gray-500 px-4 py-2 text-sm font-medium text-gray-700 dark:text-gray-350 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {previewing ? (
                  <>
                    <Loader2 className="animate-spin h-3.5 w-3.5 text-slate-500" />
                    {t("merge.generatingPreview")}
                  </>
                ) : (
                  <>{t("merge.generatePreview")}</>
                )}
              </button>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
