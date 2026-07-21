import { useEffect, useMemo, useState } from "react";
import { useI18n } from "../lib/i18n";
import {
  AlertCircle,
  ArrowDown,
  ArrowUp,
  BookOpen,
  Braces,
  CheckCircle,
  Copy,
  Download,
  Eye,
  EyeOff,
  Languages,
  Network,
  Plus,
  RefreshCw,
  Save,
  ShieldCheck,
  Sparkles,
  Trash2,
  Upload,
} from "lucide-react";
import {
  openDialog,
  readTextFilePath,
  saveDialog,
  writeTextFilePath,
  listTranslationProviders,
  listTranslationModels,
  testTranslation,
  testTranslationProxy,
  getSettings,
  saveSettingsCmd,
  hasProviderSecret,
  setProviderSecret,
  type TranslationProvider,
  type TranslationGlossary,
  type TranslationStructuredOutputMode,
  type Settings,
} from "../lib/tauri";
import {
  createGlossaryId,
  findGlossaryConflicts,
  mergeImportedEntries,
  moveGlossary,
  normalizeGlossaryOrder,
  parseGlossaryEntries,
  serializeGlossaryEntries,
} from "../lib/glossary";

import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { Input, Select, Textarea } from "../components/ui/Input";

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

function isThinkingOnlyModelName(modelName: string): boolean {
  const model = modelName.trim().toLowerCase();
  return Boolean(model) && (
    model.includes("deepseek-reasoner")
    || model.includes("thinking-")
    || model.endsWith("-thinking")
    || model.includes("-reasoning")
    || model.endsWith("-reasoner")
  );
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

function safeFileStem(value: string): string {
  const stem = value.trim().replace(/[\\/:*?"<>|]+/g, "-").replace(/\s+/g, " ");
  return stem || "FinalSub-glossary";
}

export default function TranslationPage() {
  const { t, locale } = useI18n();
  const [providers, setProviders] = useState<TranslationProvider[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [selectedProvider, setSelectedProvider] = useState("");
  const [testText, setTestText] = useState("Hello, how are you?");
  const [testResult, setTestResult] = useState<string>("");
  const [testThinkingStatus, setTestThinkingStatus] = useState<"disabled" | "active" | null>(null);
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
  const [modelFetchError, setModelFetchError] = useState("");
  const [testingProxy, setTestingProxy] = useState(false);
  const [proxyStatus, setProxyStatus] = useState("");
  const [runtimeSaved, setRuntimeSaved] = useState(false);
  const [structuredOutput, setStructuredOutput] = useState<TranslationStructuredOutputMode>("json_schema");
  const [echoAnchoring, setEchoAnchoring] = useState(true);
  const [enableThinking, setEnableThinking] = useState(false);
  const [glossaryDrafts, setGlossaryDrafts] = useState<TranslationGlossary[]>([]);
  const [activeGlossaryId, setActiveGlossaryId] = useState("");
  const [glossarySaved, setGlossarySaved] = useState(false);
  const [glossaryStatus, setGlossaryStatus] = useState("");
  const [glossaryError, setGlossaryError] = useState("");

  useEffect(() => {
    listTranslationProviders().then(setProviders).catch(console.error);
    getSettings().then((s) => {
      setSettings(s);
      setSelectedProvider(s.translate_provider || "");
      const loadedGlossaries = normalizeGlossaryOrder(s.translation_glossaries ?? []);
      setGlossaryDrafts(loadedGlossaries);
      setActiveGlossaryId(loadedGlossaries[0]?.id ?? "");
    }).catch(console.error);
  }, []);

  const selectedProviderInfo = providers.find((p) => p.id === selectedProvider);
  const selectedProviderUnavailable = Boolean(selectedProviderInfo && !selectedProviderInfo.implemented);
  const availableProviderNames = providers
    .filter((provider) => provider.implemented)
    .map((provider) => provider.name)
    .join(locale === "en" ? ", " : "、");
  const glossaries = useMemo(
    () => normalizeGlossaryOrder(glossaryDrafts),
    [glossaryDrafts],
  );
  const glossaryConflicts = useMemo(() => findGlossaryConflicts(glossaries), [glossaries]);
  const activeGlossary = glossaries.find((glossary) => glossary.id === activeGlossaryId) ?? null;
  const enabledGlossaryCount = glossaries.filter((glossary) => glossary.enabled).length;
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
    if (glossaries.length === 0) {
      if (activeGlossaryId) setActiveGlossaryId("");
      return;
    }
    if (!glossaries.some((glossary) => glossary.id === activeGlossaryId)) {
      setActiveGlossaryId(glossaries[0].id);
    }
  }, [activeGlossaryId, glossaries]);

  useEffect(() => {
    if (!selectedProvider || !settings) return;
    const ep = settings.translate_endpoints?.[selectedProvider] || selectedProviderInfo?.default_endpoint || "";
    const md = settings.translate_models?.[selectedProvider] || "";
    setApiUrl(ep);
    setModelName(md);
    setAvailableModels([]);
    setModelFetchError("");
    setSystemPrompt(settings.translate_system_prompts?.[selectedProvider] || "");
    setUserPrompt(settings.translate_user_prompts?.[selectedProvider] || "");
    setCustomHeadersJson(formatJsonObject(settings.translate_custom_headers?.[selectedProvider]));
    setCustomBodyJson(formatJsonObject(settings.translate_custom_body?.[selectedProvider]));
    setStructuredOutput(settings.translate_structured_output?.[selectedProvider] ?? "json_schema");
    setEchoAnchoring(settings.translate_echo_anchoring?.[selectedProvider] ?? true);
    setEnableThinking(settings.translate_enable_thinking?.[selectedProvider] ?? false);
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
    settings?.translate_structured_output?.[selectedProvider],
    settings?.translate_echo_anchoring?.[selectedProvider],
    settings?.translate_enable_thinking?.[selectedProvider],
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
      const updatedStructuredOutput = { ...(settings.translate_structured_output || {}), [selectedProvider]: structuredOutput };
      const updatedEchoAnchoring = { ...(settings.translate_echo_anchoring || {}), [selectedProvider]: echoAnchoring };
      const updatedEnableThinking = { ...(settings.translate_enable_thinking || {}), [selectedProvider]: enableThinking };

      const updated = {
        ...settings,
        translate_provider: selectedProvider,
        translate_endpoints: updatedEndpoints,
        translate_models: updatedModels,
        translate_system_prompts: updatedSystemPrompts,
        translate_user_prompts: updatedUserPrompts,
        translate_custom_headers: updatedCustomHeaders,
        translate_custom_body: updatedCustomBody,
        translate_structured_output: updatedStructuredOutput,
        translate_echo_anchoring: updatedEchoAnchoring,
        translate_enable_thinking: updatedEnableThinking,
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
    setTestThinkingStatus(null);
    try {
      const parsedCustomHeaders = selectedProviderInfo?.is_ai ? parseCustomHeaders() : {};
      const parsedCustomBody = selectedProviderInfo?.is_ai ? parseCustomBody() : {};
      // 仅发送 dirty（用户本次输入）的密钥，未修改的由 Rust 从本地凭据存储读取，避免暴露。
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
        enable_thinking: enableThinking,
      });

      if (resp.success) {
        setTestResult(resp.translated_text);
        setTestThinkingStatus(
          !enableThinking && typeof resp.thinking_enabled === "boolean"
            ? (resp.thinking_enabled ? "active" : "disabled")
            : null,
        );
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
      setModelFetchError(t("translation.endpointMissing").replace("{name}", selectedProviderInfo.name));
      return;
    }
    setFetchingModels(true);
    setError("");
    setModelFetchError("");
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
      if (models.length === 0) {
        setAvailableModels([]);
        setModelFetchError(t("translation.modelsEmpty"));
        return;
      }
      setAvailableModels(models);
      if (!modelName.trim() && models[0]) setModelName(models[0]);
    } catch (err) {
      setModelFetchError(err instanceof Error ? err.message : String(err));
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

  const updateGlossaries = (next: TranslationGlossary[], nextActiveId?: string) => {
    const normalized = normalizeGlossaryOrder(next);
    setGlossaryDrafts(normalized);
    if (nextActiveId !== undefined) setActiveGlossaryId(nextActiveId);
    setGlossarySaved(false);
    setGlossaryStatus("");
    setGlossaryError("");
  };

  const handleAddGlossary = () => {
    if (!settings || glossaries.length >= 64) return;
    const id = createGlossaryId("glossary");
    updateGlossaries([
      ...glossaries,
      {
        id,
        name: t("translation.glossaryNewName", { count: glossaries.length + 1 }),
        description: "",
        enabled: true,
        order: glossaries.length,
        entries: [],
      },
    ], id);
  };

  const handleCopyGlossary = () => {
    if (!activeGlossary || glossaries.length >= 64) return;
    const id = createGlossaryId("glossary");
    updateGlossaries([
      ...glossaries,
      {
        ...activeGlossary,
        id,
        name: `${activeGlossary.name}${t("translation.glossaryCopySuffix")}`,
        order: glossaries.length,
        entries: activeGlossary.entries.map((entry) => ({ ...entry, id: createGlossaryId("entry") })),
      },
    ], id);
  };

  const handleDeleteGlossary = () => {
    if (!activeGlossary) return;
    if (!window.confirm(t("translation.glossaryDeleteConfirm", { name: activeGlossary.name }))) return;
    const remaining = glossaries.filter((glossary) => glossary.id !== activeGlossary.id);
    updateGlossaries(remaining, remaining[0]?.id ?? "");
  };

  const updateActiveGlossary = (patch: Partial<TranslationGlossary>) => {
    if (!activeGlossary) return;
    updateGlossaries(glossaries.map((glossary) => (
      glossary.id === activeGlossary.id ? { ...glossary, ...patch } : glossary
    )));
  };

  const handleAddGlossaryEntry = () => {
    if (!activeGlossary || activeGlossary.entries.length >= 10_000) return;
    updateActiveGlossary({
      entries: [
        ...activeGlossary.entries,
        { id: createGlossaryId("entry"), source: "", target: "", note: "" },
      ],
    });
  };

  const handleUpdateGlossaryEntry = (
    entryId: string,
    patch: Partial<TranslationGlossary["entries"][number]>,
  ) => {
    if (!activeGlossary) return;
    updateActiveGlossary({
      entries: activeGlossary.entries.map((entry) => (
        entry.id === entryId ? { ...entry, ...patch } : entry
      )),
    });
  };

  const handleDeleteGlossaryEntry = (entryId: string) => {
    if (!activeGlossary) return;
    updateActiveGlossary({ entries: activeGlossary.entries.filter((entry) => entry.id !== entryId) });
  };

  const handleImportGlossary = async () => {
    if (!activeGlossary) return;
    setGlossaryError("");
    setGlossaryStatus("");
    try {
      const path = await openDialog({
        multiple: false,
        directory: false,
        filters: [{ name: "CSV / TXT", extensions: ["csv", "txt"] }],
      });
      if (typeof path !== "string") return;
      const format = path.toLocaleLowerCase().endsWith(".csv") ? "csv" : "txt";
      const parsed = parseGlossaryEntries(await readTextFilePath(path), format);
      if (parsed.length === 0) {
        setGlossaryError(t("translation.glossaryImportEmpty"));
        return;
      }
      const merged = mergeImportedEntries(activeGlossary.entries, parsed);
      updateActiveGlossary({ entries: merged.entries.slice(0, 10_000) });
      setGlossaryStatus(t("translation.glossaryImportSuccess", {
        added: merged.added,
        updated: merged.updated,
      }));
    } catch (err) {
      setGlossaryError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleExportGlossary = async () => {
    if (!activeGlossary) return;
    setGlossaryError("");
    setGlossaryStatus("");
    try {
      const path = await saveDialog({
        defaultPath: `${safeFileStem(activeGlossary.name)}.csv`,
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (!path) return;
      await writeTextFilePath(path, serializeGlossaryEntries(activeGlossary.entries, "csv"));
      setGlossaryStatus(t("translation.glossaryExported"));
    } catch (err) {
      setGlossaryError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleSaveGlossaries = async () => {
    if (!settings) return;
    setGlossaryError("");
    setGlossaryStatus("");
    const cleaned = glossaries.map((glossary) => ({
      ...glossary,
      name: glossary.name.trim(),
      description: glossary.description.trim(),
      entries: glossary.entries
        .map((entry) => ({
          ...entry,
          source: entry.source.trim(),
          target: entry.target.trim(),
          note: entry.note.trim(),
        }))
        .filter((entry) => entry.source || entry.target || entry.note),
    }));
    if (cleaned.some((glossary) => !glossary.name)) {
      setGlossaryError(t("translation.glossaryNameRequired"));
      return;
    }
    if (cleaned.some((glossary) => glossary.entries.some((entry) => !entry.source || !entry.target))) {
      setGlossaryError(t("translation.glossaryEntryIncomplete"));
      return;
    }
    try {
      const saved = await saveSettingsCmd({ ...settings, translation_glossaries: cleaned });
      setSettings(saved);
      setGlossaryDrafts(normalizeGlossaryOrder(saved.translation_glossaries));
      setGlossarySaved(true);
      window.setTimeout(() => setGlossarySaved(false), 3000);
    } catch (err) {
      setGlossaryError(err instanceof Error ? err.message : String(err));
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
                  onChange={(e) => {
                    setApiUrl(e.target.value);
                    setAvailableModels([]);
                    setModelFetchError("");
                  }}
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
                    value={modelName}
                    onChange={(e) => setModelName(e.target.value)}
                    aria-describedby="translation-model-fetch-status"
                    placeholder={
                      selectedProvider === CUSTOM_OPENAI_PROVIDER_ID
                        ? t("translation.modelPlaceholderOp")
                        : t("translation.modelPlaceholder")
                    }
                  />
                  <Button type="button" onClick={handleFetchModels} disabled={fetchingModels} variant="secondary" size="sm">
                    <RefreshCw size={13} className={fetchingModels ? "animate-spin" : ""} />
                    {fetchingModels ? t("translation.fetchingModels") : t("translation.fetchModels")}
                  </Button>
                </div>
                <div id="translation-model-fetch-status" aria-live="polite">
                  {availableModels.length > 0 && (
                    <div className="mt-2 rounded-xl border border-success/20 bg-success/10 p-2.5">
                      <p className="mb-2 flex items-center gap-1.5 text-xs font-semibold text-success">
                        <CheckCircle size={14} />
                        {t("translation.modelsFound", { count: availableModels.length })}
                      </p>
                      <Select
                        value={availableModels.includes(modelName) ? modelName : ""}
                        onChange={(event) => {
                          if (event.target.value) setModelName(event.target.value);
                        }}
                        aria-label={t("translation.selectFetchedModel", { count: availableModels.length })}
                      >
                        <option value="">{t("translation.selectFetchedModel", { count: availableModels.length })}</option>
                        {availableModels.map((model) => <option key={model} value={model}>{model}</option>)}
                      </Select>
                    </div>
                  )}
                  {!fetchingModels && !modelFetchError && availableModels.length === 0 && (
                    <p className="mt-1.5 text-xs text-text-tertiary">{t("translation.fetchModelsHint")}</p>
                  )}
                  {modelFetchError && (
                    <p role="alert" className="mt-2 flex items-start gap-1.5 rounded-xl border border-danger/20 bg-danger/10 px-3 py-2.5 text-xs leading-5 text-danger">
                      <AlertCircle size={14} className="mt-0.5 shrink-0" />
                      <span>{t("translation.fetchModelsFailed", { error: modelFetchError })}</span>
                    </p>
                  )}
                </div>
              </div>
            )}

            {selectedProviderInfo.is_ai && (
              <div className="grid gap-4 lg:grid-cols-2">
                <label className="liquid-panel flex cursor-pointer items-start gap-3 rounded-2xl border border-border-subtle p-4 lg:col-span-2">
                  <input
                    type="checkbox"
                    checked={enableThinking}
                    onChange={(event) => setEnableThinking(event.target.checked)}
                    className="mt-0.5 h-4 w-4 accent-brand"
                  />
                  <span className="min-w-0">
                    <span className="flex items-center gap-2 text-sm font-semibold text-text-primary">
                      <Sparkles size={15} className="text-brand" />
                      {t("translation.thinkingMode")}
                    </span>
                    <span className="mt-1 block text-xs leading-5 text-text-tertiary">{t("translation.thinkingModeDesc")}</span>
                    {isThinkingOnlyModelName(modelName) && !enableThinking && (
                      <span className="mt-2 block text-xs leading-5 text-warning">{t("translation.thinkingOnlyHint")}</span>
                    )}
                  </span>
                </label>
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
                      <ShieldCheck size={17} />
                    </span>
                    <div className="min-w-0">
                      <div className="flex flex-wrap items-center gap-2">
                        <h5 className="text-sm font-semibold text-text-primary">{t("translation.alignmentTitle")}</h5>
                        <span className="rounded-md border border-brand/15 bg-brand/10 px-2 py-0.5 font-mono text-[10px] uppercase tracking-wide text-brand-text">
                          {t("translation.alignmentBadge")}
                        </span>
                      </div>
                      <p className="mt-1 text-xs leading-5 text-text-tertiary">{t("translation.alignmentDesc")}</p>
                    </div>
                  </div>
                  <div className="grid gap-4 lg:grid-cols-2">
                    <div>
                      <label className="mb-1.5 block text-sm font-medium text-text-secondary">
                        {t("translation.structuredOutput")}
                      </label>
                      <Select
                        value={structuredOutput}
                        onChange={(event) => setStructuredOutput(event.target.value as TranslationStructuredOutputMode)}
                      >
                        <option value="json_schema">{t("translation.structuredJsonSchema")}</option>
                        <option value="json_object">{t("translation.structuredJsonObject")}</option>
                        <option value="disabled">{t("translation.structuredDisabled")}</option>
                      </Select>
                      <p className="mt-1.5 text-xs leading-5 text-text-tertiary">{t("translation.structuredFallback")}</p>
                    </div>
                    <label className="flex min-h-24 cursor-pointer items-start gap-3 rounded-xl border border-border-subtle bg-surface-overlay/45 p-3.5">
                      <input
                        type="checkbox"
                        checked={echoAnchoring}
                        onChange={(event) => setEchoAnchoring(event.target.checked)}
                        className="mt-0.5 h-4 w-4 accent-brand"
                      />
                      <span>
                        <span className="block text-sm font-semibold text-text-primary">{t("translation.echoAnchoring")}</span>
                        <span className="mt-1 block text-xs leading-5 text-text-tertiary">{t("translation.echoAnchoringDesc")}</span>
                      </span>
                    </label>
                  </div>
                </div>
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
                    {t("translation.toSaveSecretStore")}
                  </p>
                ) : secretConfigured[field] ? (
                  <p className="mt-1.5 text-[11px] text-success">
                    {t("translation.savedSecretStore")}
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
          <div className="mb-5 flex flex-wrap items-start justify-between gap-4">
            <div className="flex min-w-0 items-start gap-3">
              <span className="liquid-icon grid h-10 w-10 shrink-0 place-items-center rounded-xl text-brand">
                <BookOpen size={18} />
              </span>
              <div>
                <h3 className="font-display text-h2 font-semibold text-text-primary">{t("translation.glossaryTitle")}</h3>
                <p className="mt-1 max-w-3xl text-sm leading-6 text-text-tertiary">{t("translation.glossaryDesc")}</p>
              </div>
            </div>
            <div className="flex flex-wrap gap-2 font-mono text-[10px] uppercase tracking-wide">
              <span className="rounded-md border border-border-subtle bg-surface-overlay px-2.5 py-1.5 text-text-secondary">
                {t("translation.glossaryEnabledCount", { enabled: enabledGlossaryCount, total: glossaries.length })}
              </span>
              <span className={`rounded-md border px-2.5 py-1.5 ${
                glossaryConflicts.length > 0
                  ? "border-warning/25 bg-warning/10 text-warning"
                  : "border-success/20 bg-success/10 text-success"
              }`}>
                {t("translation.glossaryConflictCount", { count: glossaryConflicts.length })}
              </span>
            </div>
          </div>

          <div className="grid gap-5 xl:grid-cols-[17rem_minmax(0,1fr)]">
            <aside className="rounded-2xl border border-border-subtle bg-surface-overlay/35 p-3">
              <div className="mb-3 flex items-center justify-between gap-2 px-1">
                <span className="text-xs font-semibold uppercase tracking-wide text-text-tertiary">
                  {t("translation.glossaryList")}
                </span>
                <Button type="button" variant="secondary" size="sm" onClick={handleAddGlossary} disabled={glossaries.length >= 64}>
                  <Plus size={13} />
                  {t("translation.glossaryAdd")}
                </Button>
              </div>
              {glossaries.length === 0 ? (
                <button
                  type="button"
                  onClick={handleAddGlossary}
                  className="flex min-h-32 w-full flex-col items-center justify-center rounded-xl border border-dashed border-border-default px-4 text-center text-sm text-text-tertiary transition hover:border-brand/35 hover:bg-brand/5 hover:text-text-primary"
                >
                  <Plus className="mb-2" size={18} />
                  {t("translation.glossaryEmpty")}
                </button>
              ) : (
                <div className="max-h-80 space-y-2 overflow-y-auto pr-1">
                  {glossaries.map((glossary, index) => (
                    <div
                      key={glossary.id}
                      className={`rounded-xl border p-2 transition ${
                        glossary.id === activeGlossaryId
                          ? "border-brand/25 bg-brand/10"
                          : "border-border-subtle bg-surface-raised/55"
                      }`}
                    >
                      <button
                        type="button"
                        onClick={() => setActiveGlossaryId(glossary.id)}
                        className="w-full px-1 text-left"
                      >
                        <span className="flex items-center justify-between gap-2">
                          <span className="truncate text-sm font-semibold text-text-primary">{glossary.name}</span>
                          <span className={`h-2 w-2 shrink-0 rounded-full ${glossary.enabled ? "bg-success" : "bg-text-tertiary/45"}`} />
                        </span>
                        <span className="mt-1 block text-xs text-text-tertiary">
                          {t("translation.glossaryEntryCount", { count: glossary.entries.length })}
                        </span>
                      </button>
                      <div className="mt-2 flex justify-end gap-1 border-t border-border-subtle pt-1.5">
                        <button
                          type="button"
                          onClick={() => updateGlossaries(moveGlossary(glossaries, glossary.id, -1))}
                          disabled={index === 0}
                          className="rounded-md p-1.5 text-text-tertiary transition hover:bg-surface-overlay hover:text-text-primary disabled:opacity-30"
                          title={t("translation.glossaryMoveUp")}
                        >
                          <ArrowUp size={13} />
                        </button>
                        <button
                          type="button"
                          onClick={() => updateGlossaries(moveGlossary(glossaries, glossary.id, 1))}
                          disabled={index === glossaries.length - 1}
                          className="rounded-md p-1.5 text-text-tertiary transition hover:bg-surface-overlay hover:text-text-primary disabled:opacity-30"
                          title={t("translation.glossaryMoveDown")}
                        >
                          <ArrowDown size={13} />
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </aside>

            <section className="min-w-0 rounded-2xl border border-border-subtle bg-surface-raised/50 p-4 sm:p-5">
              {activeGlossary ? (
                <>
                  <div className="mb-5 flex flex-wrap items-center justify-between gap-3">
                    <label className="flex cursor-pointer items-center gap-2.5 text-sm font-semibold text-text-primary">
                      <input
                        type="checkbox"
                        checked={activeGlossary.enabled}
                        onChange={(event) => updateActiveGlossary({ enabled: event.target.checked })}
                        className="h-4 w-4 accent-brand"
                      />
                      {t("translation.glossaryEnabled")}
                    </label>
                    <div className="flex flex-wrap gap-2">
                      <Button type="button" variant="secondary" size="sm" onClick={handleCopyGlossary} disabled={glossaries.length >= 64}>
                        <Copy size={13} /> {t("translation.glossaryCopy")}
                      </Button>
                      <Button type="button" variant="danger" size="sm" onClick={handleDeleteGlossary}>
                        <Trash2 size={13} /> {t("translation.glossaryDelete")}
                      </Button>
                    </div>
                  </div>

                  <div className="grid gap-4 lg:grid-cols-2">
                    <div>
                      <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("translation.glossaryName")}</label>
                      <Input
                        value={activeGlossary.name}
                        maxLength={120}
                        onChange={(event) => updateActiveGlossary({ name: event.target.value })}
                      />
                    </div>
                    <div>
                      <label className="mb-1.5 block text-sm font-medium text-text-secondary">{t("translation.glossaryDescription")}</label>
                      <Input
                        value={activeGlossary.description}
                        maxLength={500}
                        onChange={(event) => updateActiveGlossary({ description: event.target.value })}
                        placeholder={t("translation.glossaryDescriptionPlaceholder")}
                      />
                    </div>
                  </div>

                  <div className="mt-5 flex flex-wrap items-center justify-between gap-3 border-t border-border-subtle pt-5">
                    <div>
                      <h4 className="text-sm font-semibold text-text-primary">{t("translation.glossaryEntries")}</h4>
                      <p className="mt-1 text-xs leading-5 text-text-tertiary">{t("translation.glossaryEntriesHint")}</p>
                    </div>
                    <div className="flex flex-wrap gap-2">
                      <Button type="button" variant="secondary" size="sm" onClick={handleImportGlossary}>
                        <Upload size={13} /> {t("translation.glossaryImport")}
                      </Button>
                      <Button type="button" variant="secondary" size="sm" onClick={handleExportGlossary}>
                        <Download size={13} /> {t("translation.glossaryExport")}
                      </Button>
                      <Button type="button" variant="secondary" size="sm" onClick={handleAddGlossaryEntry} disabled={activeGlossary.entries.length >= 10_000}>
                        <Plus size={13} /> {t("translation.glossaryAddEntry")}
                      </Button>
                    </div>
                  </div>

                  {activeGlossary.entries.length === 0 ? (
                    <button
                      type="button"
                      onClick={handleAddGlossaryEntry}
                      className="mt-4 flex min-h-28 w-full flex-col items-center justify-center rounded-xl border border-dashed border-border-default px-4 text-sm text-text-tertiary transition hover:border-brand/35 hover:bg-brand/5 hover:text-text-primary"
                    >
                      <Plus className="mb-2" size={18} />
                      {t("translation.glossaryEntriesEmpty")}
                    </button>
                  ) : (
                    <div className="mt-4 max-h-[30rem] space-y-3 overflow-y-auto pr-1">
                      <div className="hidden grid-cols-[2rem_minmax(8rem,1fr)_minmax(8rem,1fr)_minmax(8rem,1fr)_2rem] gap-2 px-3 font-mono text-[10px] uppercase tracking-wide text-text-tertiary lg:grid">
                        <span />
                        <span>{t("translation.glossarySource")}</span>
                        <span>{t("translation.glossaryTarget")}</span>
                        <span>{t("translation.glossaryNote")}</span>
                        <span />
                      </div>
                      {activeGlossary.entries.map((entry, index) => (
                        <div key={entry.id} className="grid gap-2 rounded-xl border border-border-subtle bg-surface-overlay/35 p-3 lg:grid-cols-[2rem_minmax(8rem,1fr)_minmax(8rem,1fr)_minmax(8rem,1fr)_2rem] lg:items-center">
                          <span className="font-mono text-xs text-text-tertiary">{String(index + 1).padStart(2, "0")}</span>
                          <label className="min-w-0">
                            <span className="mb-1 block text-xs font-medium text-text-tertiary lg:hidden">{t("translation.glossarySource")}</span>
                            <Input
                              value={entry.source}
                              maxLength={300}
                              onChange={(event) => handleUpdateGlossaryEntry(entry.id, { source: event.target.value })}
                              placeholder={t("translation.glossarySource")}
                              aria-label={t("translation.glossarySource")}
                            />
                          </label>
                          <label className="min-w-0">
                            <span className="mb-1 block text-xs font-medium text-text-tertiary lg:hidden">{t("translation.glossaryTarget")}</span>
                            <Input
                              value={entry.target}
                              maxLength={600}
                              onChange={(event) => handleUpdateGlossaryEntry(entry.id, { target: event.target.value })}
                              placeholder={t("translation.glossaryTarget")}
                              aria-label={t("translation.glossaryTarget")}
                            />
                          </label>
                          <label className="min-w-0">
                            <span className="mb-1 block text-xs font-medium text-text-tertiary lg:hidden">{t("translation.glossaryNote")}</span>
                            <Input
                              value={entry.note}
                              maxLength={1000}
                              onChange={(event) => handleUpdateGlossaryEntry(entry.id, { note: event.target.value })}
                              placeholder={t("translation.glossaryNote")}
                              aria-label={t("translation.glossaryNote")}
                            />
                          </label>
                          <button
                            type="button"
                            onClick={() => handleDeleteGlossaryEntry(entry.id)}
                            className="grid h-8 w-8 place-items-center rounded-lg text-text-tertiary transition hover:bg-danger/10 hover:text-danger"
                            title={t("translation.glossaryDeleteEntry")}
                          >
                            <Trash2 size={14} />
                          </button>
                        </div>
                      ))}
                    </div>
                  )}

                  {glossaryConflicts.length > 0 && (
                    <div className="mt-4 flex items-start gap-2 rounded-xl border border-warning/20 bg-warning/10 px-3.5 py-3 text-sm text-warning">
                      <AlertCircle className="mt-0.5 shrink-0" size={14} />
                      <span className="leading-5">{t("translation.glossaryConflictNotice", { count: glossaryConflicts.length })}</span>
                    </div>
                  )}
                  {glossaryError && (
                    <div className="mt-4 flex items-start gap-2 rounded-xl border border-danger/20 bg-danger/10 px-3.5 py-3 text-sm text-danger">
                      <AlertCircle className="mt-0.5 shrink-0" size={14} />
                      <span>{glossaryError}</span>
                    </div>
                  )}
                  <div className="mt-5 flex flex-wrap items-center gap-3 border-t border-border-subtle pt-5">
                    <Button type="button" variant="primary" size="sm" onClick={handleSaveGlossaries}>
                      <Save size={14} /> {t("translation.glossarySave")}
                    </Button>
                    {glossaryStatus && <span className="text-sm font-medium text-success">{glossaryStatus}</span>}
                    {glossarySaved && (
                      <span className="flex items-center gap-1.5 text-sm font-medium text-success">
                        <CheckCircle size={14} /> {t("translation.glossarySaved")}
                      </span>
                    )}
                  </div>
                </>
              ) : (
                <div className="grid min-h-64 place-items-center text-center">
                  <div>
                    <BookOpen className="mx-auto mb-3 text-text-tertiary" size={28} />
                    <p className="text-sm text-text-tertiary">{t("translation.glossaryEmptyDetail")}</p>
                    <Button className="mt-4" type="button" variant="primary" size="sm" onClick={handleAddGlossary}>
                      <Plus size={13} /> {t("translation.glossaryAdd")}
                    </Button>
                  </div>
                </div>
              )}
            </section>
          </div>
        </Card>
      )}

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
              {testThinkingStatus && (
                <span className={`ml-auto rounded-full px-2 py-0.5 text-[11px] font-medium ${testThinkingStatus === "disabled" ? "bg-success/10 text-success" : "bg-warning/10 text-warning"}`}>
                  {testThinkingStatus === "disabled" ? t("translation.thinkingDisabled") : t("translation.thinkingUnavailable")}
                </span>
              )}
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
