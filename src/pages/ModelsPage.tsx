import { useEffect, useState } from "react";
import { scanModels, deleteModel, type AsrModelInfo } from "../lib/tauri";
import { Download, CheckCircle, AlertCircle, Clock, Trash2, RefreshCw } from "lucide-react";

function StatusBadge({ status }: { status: AsrModelInfo["status"] }) {
  if (status === "available")
    return (
      <span className="flex items-center gap-1 text-blue-600">
        <Download size={14} /> 可下载
      </span>
    );
  if (status === "downloaded")
    return (
      <span className="flex items-center gap-1 text-green-600">
        <CheckCircle size={14} /> 已下载
      </span>
    );
  if (status === "downloading")
    return (
      <span className="flex items-center gap-1 text-yellow-600">
        <Clock size={14} /> 下载中
      </span>
    );
  if (status === "not-ready")
    return (
      <span className="flex items-center gap-1 text-gray-400">
        <Clock size={14} /> 敬请期待
      </span>
    );
  if (typeof status === "object" && "error" in status)
    return (
      <span className="flex items-center gap-1 text-red-600">
        <AlertCircle size={14} /> 错误
      </span>
    );
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
  const [deleting, setDeleting] = useState<string | null>(null);

  const refresh = () => {
    setLoading(true);
    scanModels()
      .then(setModels)
      .catch(console.error)
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    refresh();
  }, []);

  const handleDelete = async (modelId: string) => {
    if (!confirm(`确定删除模型 ${modelId}？此操作不可恢复。`)) return;
    setDeleting(modelId);
    try {
      await deleteModel(modelId);
      refresh();
    } catch (err) {
      alert(`删除失败：${err}`);
    } finally {
      setDeleting(null);
    }
  };

  if (loading) return <div className="text-gray-500">正在扫描模型...</div>;

  const engineGroups = models.reduce<Record<string, AsrModelInfo[]>>((acc, model) => {
    (acc[model.engine_id] ??= []).push(model);
    return acc;
  }, {});

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-2xl font-bold text-gray-900 dark:text-white">ASR 语音识别模型</h2>
        <button
          onClick={refresh}
          className="flex items-center gap-2 text-sm text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-white"
        >
          <RefreshCw size={16} /> 刷新
        </button>
      </div>

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
                <div className="grid gap-4 sm:grid-cols-[minmax(0,1fr)_10rem]">
                  <div className="min-w-0">
                    <h4 className="font-medium text-gray-900 dark:text-white">{model.name}</h4>
                    <p className="text-sm text-gray-500 mt-1">{model.description}</p>
                    <div className="flex flex-wrap gap-2 mt-2">
                      {model.languages.map((lang) => (
                        <span
                          key={lang}
                          className="text-xs bg-gray-100 dark:bg-gray-700 px-2 py-0.5 rounded"
                        >
                          {lang}
                        </span>
                      ))}
                    </div>
                  </div>
                  <div className="flex flex-col items-end justify-between sm:pt-1">
                    <StatusBadge status={model.status} />
                    <div className="flex items-center gap-2 mt-2">
                      {model.size_mb && (
                        <span className="text-xs text-gray-400">{model.size_mb} MB</span>
                      )}
                      {model.status === "downloaded" && model.engine_id === "whisper-cpp" && (
                        <button
                          onClick={() => handleDelete(model.id)}
                          disabled={deleting === model.id}
                          className="text-red-500 hover:text-red-700 disabled:opacity-50"
                          title="删除模型"
                        >
                          <Trash2 size={14} />
                        </button>
                      )}
                    </div>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      ))}

      <div className="mt-6 text-xs text-gray-400">
        <p>Whisper 模型路径：~/Tools/Local-LLM/whisper-models</p>
        <p>Parakeet 模型：首次使用时自动缓存，无需手动下载</p>
        <p>SenseVoice：敬请期待</p>
      </div>
    </div>
  );
}
