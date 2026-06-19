import { useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Film, FolderOpen, AlertCircle, CheckCircle } from "lucide-react";
import { burnSubtitle } from "../lib/tauri";

const presets = [
  { name: "经典白字黑边", font_size: 24, font_color: "&H00FFFFFF", outline_color: "&H00000000", margin_v: 30 },
  { name: "电影字幕", font_size: 28, font_color: "&H00FFFFFF", outline_color: "&H00000000", margin_v: 40 },
  { name: "YouTube", font_size: 20, font_color: "&H00FFFFFF", outline_color: "&H00000000", margin_v: 20 },
  { name: "清新简约", font_size: 22, font_color: "&H00FFFFFF", outline_color: "&H00808080", margin_v: 25 },
  { name: "醒目加粗", font_size: 32, font_color: "&H00FFFFFF", outline_color: "&H00000000", margin_v: 35 },
];

export default function SubtitleMergePage() {
  const [videoPath, setVideoPath] = useState("");
  const [subtitlePath, setSubtitlePath] = useState("");
  const [outputPath, setOutputPath] = useState("");
  const [preset, setPreset] = useState(0);
  const [fontSize, setFontSize] = useState(24);
  const [fontColor, setFontColor] = useState("&H00FFFFFF");
  const [outlineColor, setOutlineColor] = useState("&H00000000");
  const [marginV, setMarginV] = useState(30);
  const [processing, setProcessing] = useState(false);
  const [error, setError] = useState("");
  const [result, setResult] = useState("");

  const handleSelectVideo = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "视频文件", extensions: ["mp4", "mkv", "mov", "webm"] }],
    });
    if (typeof selected === "string") setVideoPath(selected);
  };

  const handleSelectSubtitle = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "字幕文件", extensions: ["srt", "ass", "vtt"] }],
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
      setError("请选择视频、字幕和输出路径");
      return;
    }
    setProcessing(true);
    setError("");
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
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setProcessing(false);
    }
  };

  return (
    <div className="max-w-4xl">
      <h2 className="mb-6 text-2xl font-bold text-gray-900 dark:text-white">视频合字幕</h2>

      <div className="space-y-5">
        {/* 文件选择 */}
        <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-700 dark:bg-gray-800">
          <h3 className="mb-4 font-semibold text-gray-900 dark:text-white">选择文件</h3>
          <div className="space-y-3">
            <div className="flex items-center gap-3">
              <button onClick={handleSelectVideo} className="flex items-center gap-2 rounded-md border border-gray-300 px-3 py-1.5 text-sm hover:bg-gray-50 dark:border-gray-600 dark:hover:bg-gray-700">
                <FolderOpen size={14} /> 选择视频
              </button>
              <span className="truncate text-xs text-gray-500">{videoPath || "未选择"}</span>
            </div>
            <div className="flex items-center gap-3">
              <button onClick={handleSelectSubtitle} className="flex items-center gap-2 rounded-md border border-gray-300 px-3 py-1.5 text-sm hover:bg-gray-50 dark:border-gray-600 dark:hover:bg-gray-700">
                <FolderOpen size={14} /> 选择字幕
              </button>
              <span className="truncate text-xs text-gray-500">{subtitlePath || "未选择"}</span>
            </div>
            <div className="flex items-center gap-3">
              <button onClick={handleSelectOutput} className="flex items-center gap-2 rounded-md border border-gray-300 px-3 py-1.5 text-sm hover:bg-gray-50 dark:border-gray-600 dark:hover:bg-gray-700">
                <FolderOpen size={14} /> 输出路径
              </button>
              <span className="truncate text-xs text-gray-500">{outputPath || "未选择"}</span>
            </div>
          </div>
        </section>

        {/* 样式设置 */}
        <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-700 dark:bg-gray-800">
          <h3 className="mb-4 font-semibold text-gray-900 dark:text-white">字幕样式</h3>

          <div className="mb-4">
            <label className="mb-2 block text-sm font-medium text-gray-700 dark:text-gray-300">预设</label>
            <div className="flex flex-wrap gap-2">
              {presets.map((p, i) => (
                <button
                  key={p.name}
                  onClick={() => applyPreset(i)}
                  className={`rounded-md border px-3 py-1 text-sm ${
                    preset === i
                      ? "border-blue-500 bg-blue-50 text-blue-700 dark:border-blue-400 dark:bg-blue-900/30"
                      : "border-gray-200 text-gray-600 dark:border-gray-600 dark:text-gray-400"
                  }`}
                >
                  {p.name}
                </button>
              ))}
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
            <div>
              <label className="mb-1 block text-xs text-gray-500">字号</label>
              <input type="number" min={10} max={72} value={fontSize} onChange={(e) => setFontSize(Number(e.target.value))} className="w-full rounded border border-gray-300 px-2 py-1 text-sm dark:border-gray-600 dark:bg-gray-700" />
            </div>
            <div>
              <label className="mb-1 block text-xs text-gray-500">字色 (ASS)</label>
              <input type="text" value={fontColor} onChange={(e) => setFontColor(e.target.value)} className="w-full rounded border border-gray-300 px-2 py-1 text-sm font-mono dark:border-gray-600 dark:bg-gray-700" />
            </div>
            <div>
              <label className="mb-1 block text-xs text-gray-500">描边色 (ASS)</label>
              <input type="text" value={outlineColor} onChange={(e) => setOutlineColor(e.target.value)} className="w-full rounded border border-gray-300 px-2 py-1 text-sm font-mono dark:border-gray-600 dark:bg-gray-700" />
            </div>
            <div>
              <label className="mb-1 block text-xs text-gray-500">垂直边距</label>
              <input type="number" min={0} max={100} value={marginV} onChange={(e) => setMarginV(Number(e.target.value))} className="w-full rounded border border-gray-300 px-2 py-1 text-sm dark:border-gray-600 dark:bg-gray-700" />
            </div>
          </div>
        </section>

        {/* 执行 */}
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
              <span>生成完成：{result}</span>
            </div>
          )}

          <button
            onClick={handleBurn}
            disabled={processing || !videoPath || !subtitlePath || !outputPath}
            className="inline-flex items-center gap-2 rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
          >
            <Film size={16} />
            {processing ? "处理中..." : "开始烧录"}
          </button>
        </section>
      </div>
    </div>
  );
}
