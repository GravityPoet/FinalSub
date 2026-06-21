import { useEffect, useState } from "react";
import { Languages, AlertCircle, CheckCircle, Eye, EyeOff } from "lucide-react";
import {
  listTranslationProviders,
  testTranslation,
  getSettings,
  saveSettingsCmd,
  hasProviderSecret,
  getProviderSecret,
  setProviderSecret,
  type TranslationProvider,
  type Settings,
} from "../lib/tauri";

const CUSTOM_OPENAI_PROVIDER_ID = "custom-openai";

const SECRET_FIELD_LABELS: Record<string, string> = {
  apiKey: "API Key",
  appId: "App ID",
  appSecret: "App Secret",
  secretId: "Secret ID",
  secretKey: "Secret Key",
  accessKeyId: "Access Key ID",
  accessKeySecret: "Access Key Secret",
  apiSecret: "API Secret",
  apiVersion: "API Version",
  region: "Region",
};

function secretFieldLabel(field: string): string {
  return SECRET_FIELD_LABELS[field] ?? field;
}

function requiredSecretFields(providerId: string): string[] {
  switch (providerId) {
    case "baidu":
      return ["appId", "secretKey"];
    case "google":
    case "doubao":
    case "deepseek":
    case "deerapi":
    case "gemini":
    case "siliconflow":
    case "qwen":
    case CUSTOM_OPENAI_PROVIDER_ID:
    case "azure":
    case "azureopenai":
    case "niutrans":
      return ["apiKey"];
    case "aliyun":
    case "volc":
      return ["accessKeyId", "accessKeySecret"];
    case "tencent":
      return ["secretId", "secretKey"];
    case "xunfei":
      return ["appId", "apiKey", "apiSecret"];
    default:
      return [];
  }
}

function secretDraftKey(providerId: string, field: string): string {
  return `finalsub:translate-secret-draft:${providerId}:${field}`;
}

function readSecretDraft(providerId: string, field: string): string {
  try {
    return window.sessionStorage.getItem(secretDraftKey(providerId, field)) ?? "";
  } catch {
    return "";
  }
}

function writeSecretDraft(providerId: string, field: string, value: string) {
  try {
    const key = secretDraftKey(providerId, field);
    if (value) {
      window.sessionStorage.setItem(key, value);
    } else {
      window.sessionStorage.removeItem(key);
    }
  } catch {
    // Session storage is a convenience cache only; Keychain remains the source of truth.
  }
}

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
  const [secretConfigured, setSecretConfigured] = useState<Record<string, boolean>>({});
  const [secretDirty, setSecretDirty] = useState<Record<string, boolean>>({});
  const [visibleSecrets, setVisibleSecrets] = useState<Record<string, boolean>>({});

  useEffect(() => {
    listTranslationProviders().then(setProviders).catch(console.error);
    getSettings().then((s) => {
      setSettings(s);
      setSelectedProvider(s.translate_provider || "");
    }).catch(console.error);
  }, []);

  const selectedProviderInfo = providers.find((p) => p.id === selectedProvider);
  const selectedProviderUnavailable = Boolean(selectedProviderInfo && !selectedProviderInfo.implemented);
  const availableProviderNames = providers
    .filter((provider) => provider.implemented)
    .map((provider) => provider.name)
    .join("、");

  useEffect(() => {
    if (!selectedProvider || !settings) return;
    const ep = settings.translate_endpoints?.[selectedProvider] || selectedProviderInfo?.default_endpoint || "";
    const md = settings.translate_models?.[selectedProvider] || "";
    setApiUrl(ep);
    setModelName(md);

    if (selectedProviderInfo?.secret_fields) {
      const loadSecrets = async () => {
        const configured: Record<string, boolean> = {};
        const dirty: Record<string, boolean> = {};
        const loadedSecrets: Record<string, string> = {};
        for (const field of selectedProviderInfo.secret_fields) {
          try {
            const savedSecret = await getProviderSecret(selectedProvider, field);
            if (savedSecret && savedSecret.trim()) {
              configured[field] = true;
              loadedSecrets[field] = savedSecret;
            } else {
              const draftSecret = readSecretDraft(selectedProvider, field);
              if (draftSecret) {
                loadedSecrets[field] = draftSecret;
                dirty[field] = true;
              }
              configured[field] = await hasProviderSecret(selectedProvider, field);
            }
          } catch (e) {
            console.error(`检查密钥 ${field} 失败`, e);
            configured[field] = false;
          }
        }
        setSecretConfigured(configured);
        setSecretDirty(dirty);
        setSecrets(loadedSecrets);
      };
      loadSecrets();
    } else {
      setSecretConfigured({});
      setSecretDirty({});
      setSecrets({});
      setVisibleSecrets({});
    }
  }, [
    selectedProvider,
    selectedProviderInfo,
    settings?.translate_endpoints?.[selectedProvider],
    settings?.translate_models?.[selectedProvider],
  ]);

  const handleSecretChange = (field: string, val: string) => {
    setSuccessMsg("");
    setError("");
    setSecrets((prev) => ({ ...prev, [field]: val }));
    setSecretDirty((prev) => ({ ...prev, [field]: true }));
    if (selectedProvider) {
      writeSecretDraft(selectedProvider, field, val);
    }
  };

  const handleSaveProvider = async () => {
    if (!settings || !selectedProvider) return;
    if (selectedProviderUnavailable) {
      setError(`${selectedProviderInfo?.name ?? selectedProvider} 暂未接入，请选择已可用的翻译服务。`);
      return;
    }
    if (!validateSelectedProviderConfig()) return;
    setSuccessMsg("");
    setError("");
    try {
      const updatedEndpoints = { ...(settings.translate_endpoints || {}), [selectedProvider]: apiUrl.trim() };
      const updatedModels = { ...(settings.translate_models || {}), [selectedProvider]: modelName.trim() };

      const updated = {
        ...settings,
        translate_provider: selectedProvider,
        translate_endpoints: updatedEndpoints,
        translate_models: updatedModels,
      };

      if (selectedProviderInfo?.secret_fields) {
        for (const field of selectedProviderInfo.secret_fields) {
          const value = secrets[field]?.trim();
          if (value) {
            await setProviderSecret(selectedProvider, field, value);
          }
        }
      }

      const confirmedSecrets: Record<string, string> = {};
      const confirmedConfigured: Record<string, boolean> = {};
      for (const field of selectedProviderInfo?.secret_fields || []) {
        const savedSecret = await getProviderSecret(selectedProvider, field);
        if (savedSecret && savedSecret.trim()) {
          confirmedSecrets[field] = savedSecret;
          confirmedConfigured[field] = true;
        } else {
          confirmedConfigured[field] = false;
        }
      }

      const missingSecrets = requiredSecretFields(selectedProvider).filter(
        (field) => !confirmedSecrets[field]?.trim()
      );
      if (missingSecrets.length > 0) {
        setSecretConfigured(confirmedConfigured);
        setSecrets(confirmedSecrets);
        setError(
          `${selectedProviderInfo?.name ?? selectedProvider} 没有保存成功：${missingSecrets
            .map(secretFieldLabel)
            .join("、")} 为空。`
        );
        return;
      }

      await saveSettingsCmd(updated);
      setSettings(updated);
      setSecretConfigured(confirmedConfigured);
      setSecretDirty({});
      setSecrets(confirmedSecrets);
      for (const field of selectedProviderInfo?.secret_fields || []) {
        writeSecretDraft(selectedProvider, field, "");
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
    if (selectedProviderUnavailable) {
      setError(`${selectedProviderInfo?.name ?? selectedProvider} 暂未接入，请选择已可用的翻译服务。`);
      return;
    }
    if (!validateSelectedProviderConfig()) return;
    setTesting(true);
    setError("");
    setTestResult("");
    try {
      const resp = await testTranslation({
        text: testText,
        source_language: "en",
        target_language: "zh",
        provider: selectedProvider,
        api_url: apiUrl.trim() || undefined,
        model_name: modelName.trim() || undefined,
        api_key:
          secrets["apiKey"]?.trim() ||
          secrets["appSecret"]?.trim() ||
          secrets["secretKey"]?.trim() ||
          undefined,
        secret_fields: Object.keys(secrets).length > 0
          ? Object.fromEntries(
              Object.entries(secrets)
                .map(([field, value]) => [field, value.trim()])
                .filter(([, value]) => value)
            )
          : undefined,
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

  const validateSelectedProviderConfig = () => {
    if (!selectedProviderInfo) return true;
    if (selectedProviderInfo.requires_endpoint && !apiUrl.trim()) {
      setError(`${selectedProviderInfo.name} 需要填写端点 URL。`);
      return false;
    }
    if (selectedProviderInfo.requires_model && !modelName.trim()) {
      setError(`${selectedProviderInfo.name} 需要填写模型名称。`);
      return false;
    }

    const missingSecrets = requiredSecretFields(selectedProviderInfo.id).filter((field) => {
      const typedValue = secrets[field]?.trim();
      return !typedValue && !secretConfigured[field];
    });
    if (missingSecrets.length > 0) {
      setError(
        `${selectedProviderInfo.name} 缺少必要凭据：${missingSecrets
          .map(secretFieldLabel)
          .join("、")}。`
      );
      return false;
    }
    return true;
  };

  const renderProviderButton = (provider: TranslationProvider) => (
    <button
      key={provider.id}
      type="button"
      onClick={() => {
        if (!provider.implemented) {
          setSelectedProvider(provider.id);
          setSecretConfigured({});
          setSecretDirty({});
          setSecrets({});
          setVisibleSecrets({});
          setError(`${provider.name} 暂未接入，请选择已可用的翻译服务。`);
          setTestResult("");
          return;
        }
        setSelectedProvider(provider.id);
        setSecretConfigured({});
        setSecretDirty({});
        setSecrets({});
        setVisibleSecrets({});
        setError("");
      }}
      className={`rounded-lg border p-2 text-left text-sm transition ${
        selectedProvider === provider.id
          ? provider.implemented
            ? "border-blue-500 bg-blue-50 text-blue-700 dark:border-blue-400 dark:bg-blue-900/30"
            : "border-amber-300 bg-amber-50 text-amber-800 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-200"
          : provider.implemented
            ? "border-gray-200 text-gray-600 hover:border-gray-300 dark:border-gray-600 dark:text-gray-400"
            : "border-gray-200 bg-gray-50 text-gray-400 dark:border-gray-700 dark:bg-gray-900/40 dark:text-gray-500"
      }`}
      title={provider.implemented ? undefined : "暂未接入，后续版本开放"}
    >
      <span className="flex items-center justify-between gap-2">
        <span>{provider.name}</span>
        {!provider.implemented && (
          <span className="shrink-0 rounded bg-gray-200 px-1.5 py-0.5 text-[10px] text-gray-500 dark:bg-gray-800 dark:text-gray-400">
            暂未接入
          </span>
        )}
      </span>
    </button>
  );

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
            {apiProviders.map(renderProviderButton)}
          </div>
        </div>

        <div className="mb-4">
          <label className="mb-2 block text-sm font-medium text-gray-700 dark:text-gray-300">
            AI 服务商
          </label>
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
            {aiProviders.map(renderProviderButton)}
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
                  placeholder={
                    selectedProvider === CUSTOM_OPENAI_PROVIDER_ID
                      ? "https://your-gateway.example.com/v1"
                      : selectedProviderInfo.default_endpoint || "请输入接口端点"
                  }
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
                  placeholder={
                    selectedProvider === CUSTOM_OPENAI_PROVIDER_ID
                      ? "请输入你的网关模型名，例如 gpt-4o-mini"
                      : "请输入模型名，例如 gpt-4o-mini"
                  }
                  className="w-full rounded-md border border-gray-300 bg-white px-2.5 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700 dark:text-white"
                />
              </div>
            )}
            
            {selectedProviderInfo.secret_fields?.map((field) => (
              <div key={field}>
                <label className="mb-1 block text-xs font-medium text-gray-500 dark:text-gray-400">
                  {secretFieldLabel(field)}
                </label>
                <div className="relative">
                  <input
                    type={visibleSecrets[field] ? "text" : "password"}
                    value={secrets[field] || ""}
                    onChange={(e) => handleSecretChange(field, e.target.value)}
                    placeholder="请输入密钥内容"
                    className="w-full rounded-md border border-gray-300 bg-white px-2.5 py-1.5 pr-10 text-sm dark:border-gray-600 dark:bg-gray-700 dark:text-white"
                  />
                  <button
                    type="button"
                    onClick={() => {
                      setVisibleSecrets((prev) => ({ ...prev, [field]: !prev[field] }));
                    }}
                    className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-700 dark:hover:bg-gray-600 dark:hover:text-gray-100"
                    title={visibleSecrets[field] ? "隐藏密钥" : "显示密钥"}
                  >
                    {visibleSecrets[field] ? <EyeOff size={16} /> : <Eye size={16} />}
                  </button>
                </div>
                {secretDirty[field] && secrets[field]?.trim() ? (
                  <p className="mt-1 text-[11px] text-amber-600 dark:text-amber-400">
                    待点击“保存选择与配置”写入本机 Keychain
                  </p>
                ) : secretConfigured[field] ? (
                  <p className="mt-1 text-[11px] text-green-600 dark:text-green-400">
                    已保存到本机 Keychain
                  </p>
                ) : null}
              </div>
            ))}
          </div>
        )}

        {selectedProviderUnavailable && (
          <div className="mb-4 flex items-start gap-2 rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-800 dark:border-amber-900/60 dark:bg-amber-950/30 dark:text-amber-200">
            <AlertCircle className="mt-0.5 shrink-0" size={16} />
            <span>{selectedProviderInfo?.name} 暂未接入，请选择 {availableProviderNames}。</span>
          </div>
        )}

        <div className="flex items-center gap-3">
          <button
            onClick={handleSaveProvider}
            disabled={!selectedProvider || selectedProviderUnavailable}
            className="rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
            title={selectedProviderUnavailable ? "该服务暂未接入" : undefined}
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
          disabled={testing || !selectedProvider || selectedProviderUnavailable}
          className="inline-flex items-center gap-2 rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
          title={selectedProviderUnavailable ? "该服务暂未接入" : undefined}
        >
          <Languages size={14} />
          {testing ? "翻译中..." : "测试翻译"}
        </button>

        <p className="mt-3 text-xs text-gray-400">
          注意：API 服务商需要配置端点和 API Key 才能使用。Ollama 可直接使用本地服务；自定义 OpenAI 兼容可接入私有网关或第三方兼容接口。
        </p>
      </section>
    </div>
  );
}
