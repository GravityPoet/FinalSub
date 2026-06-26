import { useEffect, useState } from "react";
import { useI18n } from "../lib/i18n";
import { Languages, AlertCircle, CheckCircle, Eye, EyeOff } from "lucide-react";
import {
  listTranslationProviders,
  testTranslation,
  getSettings,
  saveSettingsCmd,
  hasProviderSecret,
  setProviderSecret,
  type TranslationProvider,
  type Settings,
} from "../lib/tauri";

import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { Input, Textarea } from "../components/ui/Input";

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
    // Session storage is a convenience cache only
  }
}

export default function TranslationPage() {
  const { t, locale } = useI18n();
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
    .join(locale === "en" ? ", " : "、");

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
            const hasSecret = await hasProviderSecret(selectedProvider, field);
            configured[field] = hasSecret;
            if (hasSecret) {
              loadedSecrets[field] = "••••••••";
              dirty[field] = false;
            } else {
              const draftSecret = readSecretDraft(selectedProvider, field);
              if (draftSecret) {
                loadedSecrets[field] = draftSecret;
                dirty[field] = true;
              } else {
                loadedSecrets[field] = "";
                dirty[field] = false;
              }
            }
          } catch (e) {
            console.error(`Failed to check key ${field}`, e);
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

  const handleSecretFocus = (field: string) => {
    if (!secretDirty[field] && secrets[field] === "••••••••") {
      setSecrets((prev) => ({ ...prev, [field]: "" }));
      setSecretDirty((prev) => ({ ...prev, [field]: true }));
    }
  };

  const handleSaveProvider = async () => {
    if (!settings || !selectedProvider) return;
    if (selectedProviderUnavailable) {
      setError(t("translation.notImplementedSelectError").replace("{name}", selectedProviderInfo?.name ?? selectedProvider));
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
          if (secretDirty[field] && value && value !== "••••••••") {
            await setProviderSecret(selectedProvider, field, value);
          }
        }
      }

      const confirmedConfigured: Record<string, boolean> = {};
      const confirmedSecrets: Record<string, string> = {};
      for (const field of selectedProviderInfo?.secret_fields || []) {
        const hasSecret = await hasProviderSecret(selectedProvider, field);
        confirmedConfigured[field] = hasSecret;
        if (hasSecret) {
          confirmedSecrets[field] = "••••••••";
        } else {
          confirmedSecrets[field] = "";
        }
        writeSecretDraft(selectedProvider, field, "");
      }

      await saveSettingsCmd(updated);
      setSettings(updated);
      setSecretConfigured(confirmedConfigured);
      setSecretDirty({});
      setSecrets(confirmedSecrets);
      setSuccessMsg(t("translation.saveSuccess"));
      setTimeout(() => setSuccessMsg(""), 3000);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleTest = async () => {
    if (!selectedProvider) {
      setError(t("translation.providerPrereq"));
      return;
    }
    if (selectedProviderUnavailable) {
      setError(t("translation.notImplementedSelectError").replace("{name}", selectedProviderInfo?.name ?? selectedProvider));
      return;
    }
    if (!validateSelectedProviderConfig()) return;
    setTesting(true);
    setError("");
    setTestResult("");
    try {
      // 仅发送 dirty (用户本次输入) 的密钥，未修改的由 Rust 自动 fallback 去 Keychain 读取，避免暴露
      const testSecrets: Record<string, string> = {};
      for (const field of selectedProviderInfo?.secret_fields || []) {
        if (secretDirty[field] && secrets[field] && secrets[field] !== "••••••••") {
          testSecrets[field] = secrets[field].trim();
        }
      }

      const resp = await testTranslation({
        text: testText,
        source_language: "en",
        target_language: "zh",
        provider: selectedProvider,
        api_url: apiUrl.trim() || undefined,
        model_name: modelName.trim() || undefined,
        api_key: testSecrets["apiKey"] || undefined,
        secret_fields: Object.keys(testSecrets).length > 0 ? testSecrets : undefined,
      });

      if (resp.success) {
        setTestResult(resp.translated_text);
      } else {
        setError(resp.error || t("translation.testFailed"));
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
      setError(t("translation.endpointMissing").replace("{name}", selectedProviderInfo.name));
      return false;
    }
    if (selectedProviderInfo.requires_model && !modelName.trim()) {
      setError(t("translation.modelMissing").replace("{name}", selectedProviderInfo.name));
      return false;
    }

    const missingSecrets = requiredSecretFields(selectedProviderInfo.id).filter((field) => {
      const typedValue = secrets[field]?.trim();
      return !typedValue && !secretConfigured[field];
    });
    if (missingSecrets.length > 0) {
      setError(
        t("translation.keyMissing")
          .replace("{name}", selectedProviderInfo.name)
          .replace("{secrets}", missingSecrets.map(secretFieldLabel).join(locale === "en" ? ", " : "、"))
      );
      return false;
    }
    return true;
  };

  const renderProviderButton = (provider: TranslationProvider) => {
    const isSelected = selectedProvider === provider.id;
    return (
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
            setError(t("translation.notImplementedSelectError").replace("{name}", provider.name));
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
        className={`rounded-lg border p-3.5 text-left text-sm transition-all duration-150 flex flex-col justify-between h-20 ${
          isSelected
            ? provider.implemented
              ? "border-brand bg-brand-subtle text-brand-text font-semibold shadow-sm"
              : "border-warning/35 bg-warning/10 text-warning"
            : provider.implemented
            ? "border-border-default text-text-secondary hover:border-border-strong hover:bg-surface-overlay hover:text-text-primary"
            : "border-border-subtle bg-surface-overlay/50 text-text-tertiary cursor-not-allowed"
        }`}
        title={provider.implemented ? undefined : t("translation.notImplementedTitle")}
      >
        <span className="flex items-center justify-between w-full gap-2">
          <span className="truncate">{provider.name}</span>
          {!provider.implemented && (
            <span className="shrink-0 rounded bg-surface-overlay border border-border-subtle px-1.5 py-0.5 text-[9px] text-text-tertiary uppercase font-mono">
              {t("translation.notImplemented")}
            </span>
          )}
        </span>
      </button>
    );
  };

  return (
    <div className="max-w-4xl pb-10 space-y-6">
      <h2 className="text-display font-bold tracking-tight text-text-primary">{t("translation.title")}</h2>

      {/* Provider 选择 */}
      <Card className="p-5">
        <h3 className="mb-4 font-semibold text-text-primary text-h2">{t("translation.providers")}</h3>

        <div className="mb-5">
          <label className="mb-2 block text-xs font-semibold text-text-secondary tracking-wider uppercase">
            {t("translation.apiProvider")}
          </label>
          <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
            {apiProviders.map(renderProviderButton)}
          </div>
        </div>

        <div className="mb-5">
          <label className="mb-2 block text-xs font-semibold text-text-secondary tracking-wider uppercase">
            {t("translation.aiProvider")}
          </label>
          <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
            {aiProviders.map(renderProviderButton)}
          </div>
        </div>

        {/* 动态配置表单 */}
        {selectedProviderInfo && (selectedProviderInfo.requires_endpoint || selectedProviderInfo.requires_model || selectedProviderInfo.secret_fields?.length > 0) && (
          <div className="my-6 border-t border-border-subtle pt-6 space-y-4">
            <h4 className="font-semibold text-sm text-text-primary">
              {t("translation.configParams").replace("{name}", selectedProviderInfo.name)}
            </h4>
            
            {selectedProviderInfo.requires_endpoint && (
              <div>
                <label className="mb-1.5 block text-xs font-medium text-text-secondary">{t("translation.endpointUrl")}</label>
                <Input
                  type="text"
                  value={apiUrl}
                  onChange={(e) => setApiUrl(e.target.value)}
                  placeholder={
                    selectedProvider === CUSTOM_OPENAI_PROVIDER_ID
                      ? "https://your-gateway.example.com/v1"
                      : selectedProviderInfo.default_endpoint || t("translation.endpointPlaceholder")
                  }
                />
              </div>
            )}
            
            {selectedProviderInfo.requires_model && (
              <div>
                <label className="mb-1.5 block text-xs font-medium text-text-secondary">{t("translation.modelName")}</label>
                <Input
                  type="text"
                  value={modelName}
                  onChange={(e) => setModelName(e.target.value)}
                  placeholder={
                    selectedProvider === CUSTOM_OPENAI_PROVIDER_ID
                      ? t("translation.modelPlaceholderOp")
                      : t("translation.modelPlaceholder")
                  }
                />
              </div>
            )}
            
            {selectedProviderInfo.secret_fields?.map((field) => (
              <div key={field}>
                <label className="mb-1.5 block text-xs font-medium text-text-secondary">
                  {secretFieldLabel(field)}
                </label>
                <div className="relative">
                  <Input
                     type={visibleSecrets[field] ? "text" : "password"}
                     value={secrets[field] || ""}
                     onChange={(e) => handleSecretChange(field, e.target.value)}
                     onFocus={() => handleSecretFocus(field)}
                     placeholder={t("translation.keyPlaceholder")}
                     className="pr-10"
                  />
                  <button
                    type="button"
                    onClick={() => {
                      setVisibleSecrets((prev) => ({ ...prev, [field]: !prev[field] }));
                    }}
                    className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded p-1 text-text-tertiary hover:bg-surface-overlay hover:text-text-primary transition"
                    title={visibleSecrets[field] ? t("translation.hideKey") : t("translation.showKey")}
                  >
                    {visibleSecrets[field] ? <EyeOff size={14} /> : <Eye size={14} />}
                  </button>
                </div>
                {secretDirty[field] && secrets[field]?.trim() && secrets[field] !== "••••••••" ? (
                  <p className="mt-1.5 text-[11px] text-warning">
                    {t("translation.toSaveKeychain")}
                  </p>
                ) : secretConfigured[field] ? (
                  <p className="mt-1.5 text-[11px] text-success">
                    {t("translation.savedKeychain")}
                  </p>
                ) : null}
              </div>
            ))}
          </div>
        )}

        {selectedProviderUnavailable && (
          <div className="mb-5 flex items-start gap-2 rounded-lg border border-warning/20 bg-warning/10 px-3 py-2.5 text-xs text-warning">
            <AlertCircle className="mt-0.5 shrink-0" size={14} />
            <span className="leading-5">{t("translation.notImplementedSelect").replace("{name}", selectedProviderInfo?.name ?? "").replace("{available}", availableProviderNames)}</span>
          </div>
        )}

        <div className="flex items-center gap-3">
          <Button
            onClick={handleSaveProvider}
            disabled={!selectedProvider || selectedProviderUnavailable}
            variant="primary"
            title={selectedProviderUnavailable ? t("translation.notImplementedBtnTooltip") : undefined}
          >
            {t("translation.saveBtn")}
          </Button>
          {successMsg && (
            <span className="text-xs text-success flex items-center gap-1.5 font-medium">
              <CheckCircle size={13} /> {successMsg}
            </span>
          )}
        </div>
      </Card>

      {/* 测试翻译 */}
      <Card className="p-5">
        <h3 className="mb-4 font-semibold text-text-primary text-h2">{t("translation.testTitle")}</h3>

        <div className="mb-4">
          <label className="mb-1.5 block text-sm font-medium text-text-secondary">
            {t("translation.testLabel")}
          </label>
          <Textarea
            value={testText}
            onChange={(e) => setTestText(e.target.value)}
            rows={3}
          />
        </div>

        {error && (
          <div className="mb-4 flex items-start gap-2 rounded-lg border border-danger/20 bg-danger/10 px-3 py-2.5 text-xs text-danger">
            <AlertCircle className="mt-0.5 shrink-0" size={14} />
            <span>{error}</span>
          </div>
        )}

        {testResult && (
          <div className="mb-4 rounded-lg border border-success/20 bg-success/10 px-3.5 py-3">
            <div className="flex items-center gap-1.5 text-xs text-success font-semibold mb-1.5">
              <CheckCircle size={13} /> {t("translation.testResult")}
            </div>
            <p className="text-sm text-text-primary leading-relaxed">{testResult}</p>
          </div>
        )}

        <div className="flex items-center gap-3">
          <Button
            onClick={handleTest}
            disabled={testing || !selectedProvider || selectedProviderUnavailable}
            variant="primary"
            title={selectedProviderUnavailable ? t("translation.notImplementedBtnTooltip") : undefined}
          >
            <Languages size={14} />
            {testing ? t("translation.testingBtn") : t("translation.testBtn")}
          </Button>
        </div>

        <p className="mt-3.5 text-[11px] text-text-tertiary leading-4">
          {t("translation.testNotice")}
        </p>
      </Card>
    </div>
  );
}
