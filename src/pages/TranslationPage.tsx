import { useEffect, useState } from "react";
import { useI18n } from "../lib/i18n";
import { Languages, AlertCircle, CheckCircle, Eye, EyeOff, RefreshCw, Braces, Network, Save } from "lucide-react";
import {
  listTranslationProviders,
  listTranslationModels,
  testTranslation,
  testTranslationProxy,
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

function parseJsonObject(
  text: string,
  label: string,
  invalidJsonMessage: string,
  objectRequiredMessage: string,
): Record<string, unknown> {
  const trimmed = text.trim();
  if (!trimmed) return {};
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    throw new Error(`${label}: ${invalidJsonMessage}`);
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error(`${label}: ${objectRequiredMessage}`);
  }
  return parsed as Record<string, unknown>;
}

function parseHeaderObject(
  text: string,
  label: string,
  invalidJsonMessage: string,
  objectRequiredMessage: string,
  stringRequiredMessage: string,
): Record<string, string> {
  const parsed = parseJsonObject(text, label, invalidJsonMessage, objectRequiredMessage);
  for (const [key, value] of Object.entries(parsed)) {
    if (typeof value !== "string") {
      throw new Error(`${label}: ${stringRequiredMessage.replace("{key}", key)}`);
    }
  }
  return parsed as Record<string, string>;
}

function formatJsonObject(value: Record<string, unknown> | undefined): string {
  return JSON.stringify(value ?? {}, null, 2);
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
  const [systemPrompt, setSystemPrompt] = useState("");
  const [userPrompt, setUserPrompt] = useState("");
  const [customHeadersJson, setCustomHeadersJson] = useState("{}");
  const [customBodyJson, setCustomBodyJson] = useState("{}");
  const [secrets, setSecrets] = useState<Record<string, string>>({});
  const [secretConfigured, setSecretConfigured] = useState<Record<string, boolean>>({});
  const [secretDirty, setSecretDirty] = useState<Record<string, boolean>>({});
  const [visibleSecrets, setVisibleSecrets] = useState<Record<string, boolean>>({});
  const [availableModels, setAvailableModels] = useState<string[]>([]);
  const [fetchingModels, setFetchingModels] = useState(false);
  const [testingProxy, setTestingProxy] = useState(false);
  const [proxyStatus, setProxyStatus] = useState("");
  const [runtimeSaved, setRuntimeSaved] = useState(false);

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
  const parseCustomHeaders = () => parseHeaderObject(
    customHeadersJson,
    t("translation.customHeaders"),
    t("translation.invalidJson"),
    t("translation.jsonObjectRequired"),
    t("translation.headerStringRequired"),
  );
  const parseCustomBody = () => parseJsonObject(
    customBodyJson,
    t("translation.customBody"),
    t("translation.invalidJson"),
    t("translation.jsonObjectRequired"),
  );

  useEffect(() => {
    if (!selectedProvider || !settings) return;
    const ep = settings.translate_endpoints?.[selectedProvider] || selectedProviderInfo?.default_endpoint || "";
    const md = settings.translate_models?.[selectedProvider] || "";
    setApiUrl(ep);
    setModelName(md);
    setAvailableModels([]);
    setSystemPrompt(settings.translate_system_prompts?.[selectedProvider] || "");
    setUserPrompt(settings.translate_user_prompts?.[selectedProvider] || "");
    setCustomHeadersJson(formatJsonObject(settings.translate_custom_headers?.[selectedProvider]));
    setCustomBodyJson(formatJsonObject(settings.translate_custom_body?.[selectedProvider]));
    setProxyStatus("");

    if (selectedProviderInfo?.secret_fields) {
      const loadSecrets = async () => {
        const configured: Record<string, boolean> = {};
        const dirty: Record<string, boolean> = {};
        const loadedSecrets: Record<string, string> = {};
        for (const field of selectedProviderInfo.secret_fields) {
          try {
            const hasSecret = await hasProviderSecret(selectedProvider, ep, field);
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
      const parsedCustomHeaders = selectedProviderInfo?.is_ai ? parseCustomHeaders() : {};
      const parsedCustomBody = selectedProviderInfo?.is_ai ? parseCustomBody() : {};
      const updatedEndpoints = { ...(settings.translate_endpoints || {}), [selectedProvider]: apiUrl.trim() };
      const updatedModels = { ...(settings.translate_models || {}), [selectedProvider]: modelName.trim() };
      const updatedSystemPrompts = { ...(settings.translate_system_prompts || {}), [selectedProvider]: systemPrompt.trim() };
      const updatedUserPrompts = { ...(settings.translate_user_prompts || {}), [selectedProvider]: userPrompt.trim() };
      const updatedCustomHeaders = { ...(settings.translate_custom_headers || {}), [selectedProvider]: parsedCustomHeaders };
      const updatedCustomBody = { ...(settings.translate_custom_body || {}), [selectedProvider]: parsedCustomBody };

      const updated = {
        ...settings,
        translate_provider: selectedProvider,
        translate_endpoints: updatedEndpoints,
        translate_models: updatedModels,
        translate_system_prompts: updatedSystemPrompts,
        translate_user_prompts: updatedUserPrompts,
        translate_custom_headers: updatedCustomHeaders,
        translate_custom_body: updatedCustomBody,
      };

      if (selectedProviderInfo?.secret_fields) {
        for (const field of selectedProviderInfo.secret_fields) {
          const value = secrets[field]?.trim();
          if (secretDirty[field] && value && value !== "••••••••") {
            await setProviderSecret(selectedProvider, apiUrl.trim(), field, value);
          }
        }
      }

      const confirmedConfigured: Record<string, boolean> = {};
      const confirmedSecrets: Record<string, string> = {};
      for (const field of selectedProviderInfo?.secret_fields || []) {
        const hasSecret = await hasProviderSecret(selectedProvider, apiUrl.trim(), field);
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
      const parsedCustomHeaders = selectedProviderInfo?.is_ai ? parseCustomHeaders() : {};
      const parsedCustomBody = selectedProviderInfo?.is_ai ? parseCustomBody() : {};
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
        system_prompt: systemPrompt.trim() || undefined,
        user_prompt: userPrompt.trim() || undefined,
        proxy_url: settings?.proxy_enabled ? settings.proxy_url.trim() || undefined : undefined,
        custom_headers: Object.keys(parsedCustomHeaders).length > 0 ? parsedCustomHeaders : undefined,
        custom_body: Object.keys(parsedCustomBody).length > 0 ? parsedCustomBody : undefined,
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

  const handleFetchModels = async () => {
    if (!selectedProviderInfo || !selectedProviderInfo.requires_model) return;
    if (!apiUrl.trim()) {
      setError(t("translation.endpointMissing").replace("{name}", selectedProviderInfo.name));
      return;
    }
    setFetchingModels(true);
    setError("");
    try {
      const parsedCustomHeaders = selectedProviderInfo.is_ai ? parseCustomHeaders() : {};
      for (const field of selectedProviderInfo.secret_fields || []) {
        const value = secrets[field]?.trim();
        if (secretDirty[field] && value && value !== "••••••••") {
          await setProviderSecret(selectedProvider, apiUrl.trim(), field, value);
        }
      }
      const models = await listTranslationModels(
        selectedProvider,
        apiUrl.trim(),
        Object.keys(parsedCustomHeaders).length > 0 ? parsedCustomHeaders : undefined,
      );
      setAvailableModels(models);
      if (!modelName.trim() && models[0]) setModelName(models[0]);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setFetchingModels(false);
    }
  };

  const handleSaveRuntime = async () => {
    if (!settings) return;
    setError("");
    setRuntimeSaved(false);
    try {
      const saved = await saveSettingsCmd(settings);
      setSettings(saved);
      setRuntimeSaved(true);
      window.setTimeout(() => setRuntimeSaved(false), 3000);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleProxyTest = async () => {
    if (!settings?.proxy_url.trim()) {
      setError(t("translation.proxyMissing"));
      return;
    }
    setTestingProxy(true);
    setProxyStatus("");
    setError("");
    try {
      const targetUrl = apiUrl.trim() || selectedProviderInfo?.default_endpoint || "https://example.com/";
      const status = await testTranslationProxy(settings.proxy_url.trim(), targetUrl);
      setProxyStatus(t("translation.proxySuccess", { status }));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setTestingProxy(false);
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
        className={`flex min-h-24 flex-col justify-between rounded-xl border p-4 text-left text-sm transition-all duration-150 ${
          isSelected
            ? provider.implemented
              ? "liquid-selected font-semibold text-brand-text"
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
    <div className="page-shell space-y-7 pb-10">
      <h2 className="font-display text-display font-bold tracking-tight text-text-primary">{t("translation.title")}</h2>

      {/* Provider 选择 */}
      <Card className="p-6">
        <h3 className="mb-5 font-display text-h2 font-semibold text-text-primary">{t("translation.providers")}</h3>

        <div className="mb-5">
          <label className="mb-2 block text-sm font-semibold text-text-secondary">
            {t("translation.apiProvider")}
          </label>
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            {apiProviders.map(renderProviderButton)}
          </div>
        </div>

        <div className="mb-5">
          <label className="mb-2 block text-sm font-semibold text-text-secondary">
            {t("translation.aiProvider")}
          </label>
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
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
                <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("translation.endpointUrl")}</label>
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
                <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("translation.modelName")}</label>
                <div className="flex gap-2">
                  <Input
                    type="text"
                    list="translation-model-options"
                    value={modelName}
                    onChange={(e) => setModelName(e.target.value)}
                    placeholder={
                      selectedProvider === CUSTOM_OPENAI_PROVIDER_ID
                        ? t("translation.modelPlaceholderOp")
                        : t("translation.modelPlaceholder")
                    }
                  />
                  <Button type="button" onClick={handleFetchModels} disabled={fetchingModels} variant="secondary" size="sm">
                    <RefreshCw size={13} className={fetchingModels ? "animate-spin" : ""} />
                    {t("translation.fetchModels")}
                  </Button>
                  <datalist id="translation-model-options">
                    {availableModels.map((model) => <option key={model} value={model} />)}
                  </datalist>
                </div>
                {availableModels.length > 0 && <p className="mt-1.5 text-xs text-success">{t("translation.modelsFound", { count: availableModels.length })}</p>}
              </div>
            )}

            {selectedProviderInfo.is_ai && (
              <div className="grid gap-4 lg:grid-cols-2">
                <div>
                  <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("translation.systemPrompt")}</label>
                  <Textarea
                    value={systemPrompt}
                    onChange={(event) => setSystemPrompt(event.target.value)}
                    rows={5}
                    maxLength={20000}
                    placeholder={t("translation.systemPromptPlaceholder")}
                  />
                </div>
                <div>
                  <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("translation.userPrompt")}</label>
                  <Textarea
                    value={userPrompt}
                    onChange={(event) => setUserPrompt(event.target.value)}
                    rows={5}
                    maxLength={20000}
                    placeholder={t("translation.userPromptPlaceholder")}
                  />
                </div>
                <p className="text-xs leading-5 text-text-tertiary lg:col-span-2">{t("translation.promptTokensHint")}</p>
                <div className="liquid-panel lg:col-span-2 rounded-2xl border border-border-subtle p-4">
                  <div className="mb-4 flex items-start gap-3">
                    <span className="liquid-icon grid h-9 w-9 shrink-0 place-items-center rounded-xl text-brand">
                      <Braces size={17} />
                    </span>
                    <div>
                      <h5 className="text-sm font-semibold text-text-primary">{t("translation.customParamsTitle")}</h5>
                      <p className="mt-1 text-xs leading-5 text-text-tertiary">{t("translation.customParamsDesc")}</p>
                    </div>
                  </div>
                  <div className="grid gap-4 lg:grid-cols-2">
                    <div>
                      <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("translation.customHeaders")}</label>
                      <Textarea
                        value={customHeadersJson}
                        onChange={(event) => {
                          setCustomHeadersJson(event.target.value);
                          setError("");
                        }}
                        rows={8}
                        spellCheck={false}
                        className="font-mono text-xs leading-5"
                        placeholder={'{\n  "X-Client": "FinalSub",\n  "Authorization": "Bearer ${API_KEY}"\n}'}
                      />
                    </div>
                    <div>
                      <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("translation.customBody")}</label>
                      <Textarea
                        value={customBodyJson}
                        onChange={(event) => {
                          setCustomBodyJson(event.target.value);
                          setError("");
                        }}
                        rows={8}
                        spellCheck={false}
                        className="font-mono text-xs leading-5"
                        placeholder={'{\n  "temperature": 0.2,\n  "max_tokens": 2048\n}'}
                      />
                    </div>
                  </div>
                  <p className="mt-3 text-xs leading-5 text-text-tertiary">{t("translation.customParamsHint")}</p>
                </div>
              </div>
            )}
            
            {selectedProviderInfo.secret_fields?.map((field) => (
              <div key={field}>
                <label className="mb-1.5 block text-sm font-medium text-text-secondary">
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
          <div className="mb-5 flex items-start gap-2 rounded-xl border border-warning/20 bg-warning/10 px-3.5 py-3 text-sm text-warning">
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
            <span className="flex items-center gap-1.5 text-sm font-medium text-success">
              <CheckCircle size={15} /> {successMsg}
            </span>
          )}
        </div>
      </Card>

      {settings && (
        <Card className="p-6">
          <h3 className="mb-1 font-display text-h2 font-semibold text-text-primary">{t("translation.runtimeTitle")}</h3>
          <p className="mb-5 text-sm leading-6 text-text-tertiary">{t("translation.runtimeDesc")}</p>
          <div className="grid gap-4 sm:grid-cols-3">
            <div>
              <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("translation.batchSize")}</label>
              <Input
                type="number"
                min={1}
                max={50}
                value={settings.translate_batch_size}
                onChange={(event) => setSettings({ ...settings, translate_batch_size: Math.min(50, Math.max(1, Number(event.target.value) || 1)) })}
              />
            </div>
            <div>
              <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("translation.concurrency")}</label>
              <Input
                type="number"
                min={1}
                max={8}
                value={settings.translate_concurrency}
                onChange={(event) => setSettings({ ...settings, translate_concurrency: Math.min(8, Math.max(1, Number(event.target.value) || 1)) })}
              />
            </div>
            <div>
              <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("translation.requestInterval")}</label>
              <Input
                type="number"
                min={0}
                max={60000}
                step={100}
                value={settings.translate_request_interval_ms}
                onChange={(event) => setSettings({ ...settings, translate_request_interval_ms: Math.min(60000, Math.max(0, Number(event.target.value) || 0)) })}
              />
            </div>
          </div>
          <div className="mt-5 border-t border-border-subtle pt-5">
            <label className="flex cursor-pointer items-center gap-3 text-sm text-text-secondary">
              <input
                type="checkbox"
                checked={settings.proxy_enabled}
                onChange={(event) => {
                  setSettings({ ...settings, proxy_enabled: event.target.checked });
                  setError("");
                  setProxyStatus("");
                }}
                className="h-4 w-4 accent-brand"
              />
              <span className="font-semibold text-text-primary">{t("translation.enableProxy")}</span>
            </label>
            <Input
              className="mt-3"
              type="url"
              disabled={!settings.proxy_enabled}
              value={settings.proxy_url}
              onChange={(event) => {
                setSettings({ ...settings, proxy_url: event.target.value });
                setError("");
                setProxyStatus("");
              }}
              placeholder="http://127.0.0.1:7890"
            />
            <p className="mt-1.5 text-xs leading-5 text-text-tertiary">{t("translation.proxyHint")}</p>
            <div className="mt-4 flex flex-wrap items-center gap-3">
              <Button type="button" variant="secondary" size="sm" onClick={handleProxyTest} disabled={!settings.proxy_enabled || testingProxy}>
                <Network size={14} className={testingProxy ? "animate-pulse" : ""} />
                {testingProxy ? t("translation.proxyTesting") : t("translation.proxyTest")}
              </Button>
              <Button type="button" variant="primary" size="sm" onClick={handleSaveRuntime}>
                <Save size={14} />
                {t("translation.runtimeSave")}
              </Button>
              {proxyStatus && <span className="text-sm font-medium text-success">{proxyStatus}</span>}
              {runtimeSaved && (
                <span className="flex items-center gap-1.5 text-sm font-medium text-success">
                  <CheckCircle size={14} /> {t("translation.runtimeSaved")}
                </span>
              )}
            </div>
          </div>
        </Card>
      )}

      {/* 测试翻译 */}
      <Card className="p-6">
        <h3 className="mb-5 font-display text-h2 font-semibold text-text-primary">{t("translation.testTitle")}</h3>

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
          <div className="mb-4 flex items-start gap-2 rounded-xl border border-danger/20 bg-danger/10 px-3.5 py-3 text-sm text-danger">
            <AlertCircle className="mt-0.5 shrink-0" size={14} />
            <span>{error}</span>
          </div>
        )}

        {testResult && (
          <div className="mb-4 rounded-xl border border-success/20 bg-success/10 px-3.5 py-3">
            <div className="mb-1.5 flex items-center gap-1.5 text-sm font-semibold text-success">
              <CheckCircle size={15} /> {t("translation.testResult")}
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

        <p className="mt-3.5 text-sm leading-5 text-text-tertiary">
          {t("translation.testNotice")}
        </p>
      </Card>
    </div>
  );
}
