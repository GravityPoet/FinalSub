import { useEffect, useState } from "react";
import { Languages, AlertCircle, CheckCircle } from "lucide-react";
import {
  listTranslationProviders,
  testTranslation,
  getSettings,
  saveSettingsCmd,
  type TranslationProvider,
  type Settings,
} from "../lib/tauri";

export default function TranslationPage() {
  const [providers, setProviders] = useState<TranslationProvider[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [selectedProvider, setSelectedProvider] = useState("");
  const [testText, setTestText] = useState("Hello, how are you?");
  const [testResult, setTestResult] = useState<string>("");
  const [testing, setTesting] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    listTranslationProviders().then(setProviders).catch(console.error);
    getSettings().then((s) => {
      setSettings(s);
      setSelectedProvider(s.translate_provider || "");
    }).catch(console.error);
  }, []);

  const handleSaveProvider = async () => {
    if (!settings) return;
    const updated = { ...settings, translate_provider: selectedProvider };
    await saveSettingsCmd(updated);
    setSettings(updated);
  };

  const handleTest = async () => {
    if (!selectedProvider) {
      setError("请先选择翻译 provider");
      return;
    }
    setTesting(true);
    setError("");
    setTestResult("");
    try {
      const resp = await testTranslation({
        text: testText,
        source_language: "en",
        target_language: "zh",
        provider: selectedProvider,
      });
      if (resp.success) {
        setTestResult(resp.translated_text);
      } else {
        setError(resp.error || "翻译失败");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setTesting(false);
    }
  };

  const apiProviders = providers.filter((p) => !p.is_ai);
  const aiProviders = providers.filter((p) => p.is_ai);

  return (
    <div className="max-w-4xl">
      <h2 className="mb-6 text-2xl font-bold text-gray-900 dark:text-white">翻译管理</h2>

      {/* Provider 选择 */}
      <section className="mb-6 rounded-lg border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-700 dark:bg-gray-800">
        <h3 className="mb-4 font-semibold text-gray-900 dark:text-white">翻译服务商</h3>

        <div className="mb-4">
          <label className="mb-2 block text-sm font-medium text-gray-700 dark:text-gray-300">
            API 服务商
          </label>
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
            {apiProviders.map((p) => (
              <button
                key={p.id}
                onClick={() => setSelectedProvider(p.id)}
                className={`rounded-lg border p-2 text-sm text-left transition ${
                  selectedProvider === p.id
                    ? "border-blue-500 bg-blue-50 text-blue-700 dark:border-blue-400 dark:bg-blue-900/30"
                    : "border-gray-200 text-gray-600 hover:border-gray-300 dark:border-gray-600 dark:text-gray-400"
                }`}
              >
                {p.name}
              </button>
            ))}
          </div>
        </div>

        <div className="mb-4">
          <label className="mb-2 block text-sm font-medium text-gray-700 dark:text-gray-300">
            AI 服务商
          </label>
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
            {aiProviders.map((p) => (
              <button
                key={p.id}
                onClick={() => setSelectedProvider(p.id)}
                className={`rounded-lg border p-2 text-sm text-left transition ${
                  selectedProvider === p.id
                    ? "border-blue-500 bg-blue-50 text-blue-700 dark:border-blue-400 dark:bg-blue-900/30"
                    : "border-gray-200 text-gray-600 hover:border-gray-300 dark:border-gray-600 dark:text-gray-400"
                }`}
              >
                {p.name}
              </button>
            ))}
          </div>
        </div>

        <button
          onClick={handleSaveProvider}
          className="rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700"
        >
          保存选择
        </button>
      </section>

      {/* 测试翻译 */}
      <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-700 dark:bg-gray-800">
        <h3 className="mb-4 font-semibold text-gray-900 dark:text-white">测试翻译</h3>

        <div className="mb-4">
          <label className="mb-1 block text-sm font-medium text-gray-700 dark:text-gray-300">
            测试文本
          </label>
          <textarea
            value={testText}
            onChange={(e) => setTestText(e.target.value)}
            rows={3}
            className="w-full rounded-md border border-gray-300 bg-white px-3 py-2 text-sm dark:border-gray-600 dark:bg-gray-700"
          />
        </div>

        {error && (
          <div className="mb-4 flex items-start gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-300">
            <AlertCircle className="mt-0.5 shrink-0" size={16} />
            <span>{error}</span>
          </div>
        )}

        {testResult && (
          <div className="mb-4 rounded-md border border-green-200 bg-green-50 px-3 py-2 dark:border-green-900/60 dark:bg-green-950/30">
            <div className="flex items-center gap-1 text-sm text-green-700 dark:text-green-300 mb-1">
              <CheckCircle size={14} /> 翻译结果
            </div>
            <p className="text-sm text-gray-800 dark:text-gray-200">{testResult}</p>
          </div>
        )}

        <button
          onClick={handleTest}
          disabled={testing || !selectedProvider}
          className="inline-flex items-center gap-2 rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
        >
          <Languages size={14} />
          {testing ? "翻译中..." : "测试翻译"}
        </button>

        <p className="mt-3 text-xs text-gray-400">
          注意：API 服务商需要在设置中配置 API Key 才能使用。Ollama 可直接使用本地服务。
        </p>
      </section>
    </div>
  );
}
