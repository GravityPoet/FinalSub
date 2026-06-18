import { useEffect, useState } from "react";
import { listAsrModels, type AsrModelInfo } from "../lib/tauri";
import { Download, CheckCircle, AlertCircle, Clock } from "lucide-react";

function StatusBadge({ status }: { status: AsrModelInfo["status"] }) {
  if (status === "available") return <span className="flex items-center justify-end gap-1 text-green-600"><CheckCircle size={14} /> 可用</span>;
  if (status === "downloaded") return <span className="flex items-center justify-end gap-1 text-green-600"><CheckCircle size={14} /> 已下载</span>;
  if (status === "downloading") return <span className="flex items-center justify-end gap-1 text-yellow-600"><Clock size={14} /> 下载中</span>;
  if (status === "not-ready") return <span className="flex items-center justify-end gap-1 text-gray-400"><Download size={14} /> 待接入</span>;
  if (typeof status === "object" && "error" in status) return <span className="flex items-center justify-end gap-1 text-red-600"><AlertCircle size={14} /> 错误</span>;
  return null;
}

function engineLabel(engineId: string): string {
  const labels: Record<string, string> = {
    "whisper-cpp": "Whisper.cpp",
    "parakeet-mlx": "Parakeet MLX",
    sensevoice: "SenseVoice",
    "custom-command": "自定义命令",
  };

  return labels[engineId] ?? engineId;
}

export default function ModelsPage() {
  const [models, setModels] = useState<AsrModelInfo[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    listAsrModels()
      .then(setModels)
      .catch(console.error)
      .finally(() => setLoading(false));
  }, []);

  if (loading) return <div className="text-gray-500">正在加载模型...</div>;

  const engineGroups = models.reduce<Record<string, AsrModelInfo[]>>((acc, model) => {
    (acc[model.engine_id] ??= []).push(model);
    return acc;
  }, {});

  return (
    <div>
      <h2 className="text-2xl font-bold text-gray-900 dark:text-white mb-6">ASR 语音识别模型</h2>

      {Object.entries(engineGroups).map(([engineId, engineModels]) => (
        <div key={engineId} className="mb-8">
          <h3 className="text-lg font-semibold text-gray-800 dark:text-gray-200 mb-3">
            {engineLabel(engineId)}
          </h3>
          <div className="grid gap-3">
            {engineModels.map((model) => (
              <div
                key={model.id}
                className="bg-white dark:bg-gray-800 rounded-lg p-4 border border-gray-200 dark:border-gray-700 shadow-sm"
              >
                <div className="grid gap-4 sm:grid-cols-[minmax(0,1fr)_8rem]">
                  <div className="min-w-0">
                    <h4 className="font-medium text-gray-900 dark:text-white">{model.name}</h4>
                    <p className="text-sm text-gray-500 mt-1">{model.description}</p>
                    <div className="flex flex-wrap gap-2 mt-2">
                      {model.languages.map((lang) => (
                        <span key={lang} className="text-xs bg-gray-100 dark:bg-gray-700 px-2 py-0.5 rounded">
                          {lang}
                        </span>
                      ))}
                    </div>
                  </div>
                  <div className="text-right text-sm sm:pt-1">
                    <StatusBadge status={model.status} />
                    {model.size_mb && (
                      <p className="text-xs text-gray-400 mt-1">{model.size_mb} MB</p>
                    )}
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
