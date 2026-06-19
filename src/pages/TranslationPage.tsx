import { useEffect, useState } from "react";
import { Languages, AlertCircle, CheckCircle } from "lucide-react";
import {
  listTranslationProviders,
  testTranslation,
  getSettings,
  saveSettingsCmd,
  getProviderSecret,
  setProviderSecret,
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
  const [successMsg, setSuccessMsg] = useState("");

  const [apiUrl, setApiUrl] = useState("");
  const [modelName, setModelName] = useState("");
  const [secrets, setSecrets] = useState<Record<string, string>>({});

  useEffect(() => {
    listTranslationProviders().then(setProviders).catch(console.error);
    getSettings().then((s) => {
      setSettings(s);
      setSelectedProvider(s.translate_provider || "");
    }).catch(console.error);
  }, []);

  const selectedProviderInfo = providers.find((p) => p.id === selectedProvider);

  useEffect(() => {
    if (!selectedProvider || !settings) return;
    const ep = settings.translate_endpoints?.[selectedProvider] || selectedProviderInfo?.default_endpoint || "";
    const md = settings.translate_models?.[selectedProvider] || "";
    setApiUrl(ep);
    setModelName(md);

    if (selectedProviderInfo?.secret_fields) {
      const loadSecrets = async () => {
        const loaded: Record<string, string> = {};
        for (const field of selectedProviderInfo.secret_fields) {
          try {
            const val = await getProviderSecret(selectedProvider, field);
            loaded[field] = val || "";
          } catch (e) {
            console.error(`读取密钥 ${field} 失败`, e);
          }
        }
        setSecrets(loaded);
      };
      loadSecrets();
    } else {
      setSecrets({});
    }
  }, [selectedProvider, settings, selectedProviderInfo]);

  const handleSecretChange = (field: string, val: string) => {
    setSecrets((prev) => ({ ...prev, [field]: val }));
  };

  const handleSaveProvider = async () => {
    if (!settings || !selectedProvider) return;
    setSuccessMsg("");
    setError("");
    try {
      const updatedEndpoints = { ...(settings.translate_endpoints || {}), [selectedProvider]: apiUrl };
      const updatedModels = { ...(settings.translate_models || {}), [selectedProvider]: modelName };
      
      const updated = {
        ...settings,
        translate_provider: selectedProvider,
        translate_endpoints: updatedEndpoints,
        translate_models: updatedModels,
      };

      await saveSettingsCmd(updated);
      setSettings(updated);

      if (selectedProviderInfo?.secret_fields) {
        for (const field of selectedProviderInfo.secret_fields) {
          await setProviderSecret(selectedProvider, field, secrets[field] || "");
        }
      }
      setSuccessMsg("配置及密钥保存成功");
      setTimeout(() => setSuccessMsg(""), 3000);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
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
        api_url: apiUrl || undefined,
        model_name: modelName || undefined,
        api_key: secrets["apiKey"] || secrets["appSecret"] || secrets["secretKey"] || undefined,
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
    <div className="max-w-4xl pb-10">
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

        {/* 动态配置表单 */}
        {selectedProviderInfo && (selectedProviderInfo.requires_endpoint || selectedProviderInfo.requires_model || selectedProviderInfo.secret_fields?.length > 0) && (
          <div className="my-5 border-t border-gray-150 pt-5 dark:border-gray-700 space-y-4">
            <h4 className="font-semibold text-sm text-gray-900 dark:text-white">
              配置 {selectedProviderInfo.name} 参数
            </h4>
            
            {selectedProviderInfo.requires_endpoint && (
              <div>
                <label className="mb-1 block text-xs font-medium text-gray-500 dark:text-gray-400">端点 URL</label>
                <input
                  type="text"
                  value={apiUrl}
                  onChange={(e) => setApiUrl(e.target.value)}
                  placeholder={selectedProviderInfo.default_endpoint || "请输入接口端点"}
                  className="w-full rounded-md border border-gray-300 bg-white px-2.5 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700 dark:text-white"
                />
              </div>
            )}
            
            {selectedProviderInfo.requires_model && (
              <div>
                <label className="mb-1 block text-xs font-medium text-gray-500 dark:text-gray-400">模型名称</label>
                <input
                  type="text"
                  value={modelName}
                  onChange={(e) => setModelName(e.target.value)}
                  placeholder="请输入模型名，例如 gpt-4o-mini"
                  className="w-full rounded-md border border-gray-300 bg-white px-2.5 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700 dark:text-white"
                />
              </div>
            )}
            
            {selectedProviderInfo.secret_fields?.map((field) => (
              <div key={field}>
                <label className="mb-1 block text-xs font-medium text-gray-500 dark:text-gray-400">
                  {field === "apiKey" ? "API Key" : field === "appSecret" || field === "secretKey" || field === "apiSecret" || field === "accessKeySecret" ? "Secret Key / 凭证密码" : field}
                </label>
                <input
                  type="password"
                  value={secrets[field] || ""}
                  onChange={(e) => handleSecretChange(field, e.target.value)}
                  placeholder="请输入密钥内容（本地 Keychain 加密存储）"
                  className="w-full rounded-md border border-gray-300 bg-white px-2.5 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700 dark:text-white"
                />
              </div>
            ))}
          </div>
        )}

        <div className="flex items-center gap-3">
          <button
            onClick={handleSaveProvider}
            disabled={!selectedProvider}
            className="rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
          >
            保存选择与配置
          </button>
          {successMsg && (
            <span className="text-sm text-green-600 dark:text-green-400 flex items-center gap-1">
              <CheckCircle size={14} /> {successMsg}
            </span>
          )}
        </div>
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
            className="w-full rounded-md border border-gray-300 bg-white px-3 py-2 text-sm dark:border-gray-600 dark:bg-gray-700 dark:text-white"
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
          注意：API 服务商需要在下方配置端点和 API Key 才能使用。Ollama 可直接使用本地服务。
        </p>
      </section>
    </div>
  );
}
