import { useEffect, useState } from "react";
import {
  CheckCircle2,
  ChevronDown,
  Cloud,
  Copy,
  Eye,
  EyeOff,
  KeyRound,
  LockKeyhole,
  Plus,
  RefreshCw,
  Save,
  SlidersHorizontal,
  Trash2,
} from "lucide-react";

import { useI18n } from "../lib/i18n";
import {
  getSettings,
  hasProviderSecret,
  saveSettingsCmd,
  setProviderSecret,
  testCloudAsrConnection,
  type CloudAsrProfile,
  type CloudAsrProtocol,
  type Settings,
} from "../lib/tauri";
import { Button } from "./ui/Button";
import { Card } from "./ui/Card";
import { Input, Select } from "./ui/Input";

type CloudPreset = "openai" | "groq" | "siliconflow" | "elevenlabs" | "deepgram" | "gladia" | "volcengine" | "tencent" | "aliyun" | "xfyun" | "custom";

interface CloudPresetConfig {
  protocol: CloudAsrProtocol;
  endpoint: string;
  model: string;
}

const PRESETS: Record<Exclude<CloudPreset, "custom">, CloudPresetConfig> = {
  openai: {
    protocol: "openai-compatible",
    endpoint: "https://api.openai.com/v1",
    model: "gpt-4o-transcribe",
  },
  groq: {
    protocol: "openai-compatible",
    endpoint: "https://api.groq.com/openai/v1",
    model: "whisper-large-v3-turbo",
  },
  siliconflow: {
    protocol: "openai-compatible",
    endpoint: "https://api.siliconflow.cn/v1",
    model: "FunAudioLLM/SenseVoiceSmall",
  },
  elevenlabs: {
    protocol: "elevenlabs",
    endpoint: "https://api.elevenlabs.io",
    model: "scribe_v2",
  },
  deepgram: {
    protocol: "deepgram",
    endpoint: "https://api.deepgram.com",
    model: "nova-3",
  },
  gladia: {
    protocol: "gladia",
    endpoint: "https://api.gladia.io",
    model: "solaria-1",
  },
  volcengine: {
    protocol: "volcengine",
    endpoint: "https://openspeech.bytedance.com",
    model: "bigmodel",
  },
  tencent: {
    protocol: "tencent",
    endpoint: "https://asr.cloud.tencent.com",
    model: "standard",
  },
  aliyun: {
    protocol: "aliyun",
    endpoint: "https://nls-gateway-cn-shanghai.aliyuncs.com",
    model: "flash",
  },
  xfyun: {
    protocol: "xfyun",
    endpoint: "https://office-api-ist-dx.iflyaisol.com",
    model: "autodialect",
  },
};

const LEGACY_PROFILE_ID = "legacy-default";

function newProfileId(): string {
  return `cloud-${crypto.randomUUID()}`;
}

function legacyProfile(settings: Settings): CloudAsrProfile {
  return {
    id: LEGACY_PROFILE_ID,
    name: "Cloud ASR 1",
    protocol: settings.cloud_asr_protocol,
    endpoint: settings.cloud_asr_endpoint,
    model: settings.cloud_asr_model,
    upload_consent: settings.cloud_asr_upload_consent,
    timeout_seconds: settings.cloud_asr_timeout_seconds,
    retry_times: settings.cloud_asr_retry_times,
    request_concurrency: settings.cloud_asr_request_concurrency,
    request_interval_ms: settings.cloud_asr_request_interval_ms,
  };
}

function secretProviderForProtocol(protocol: CloudAsrProtocol): string {
  if (protocol === "elevenlabs") return "cloud-asr-elevenlabs";
  if (protocol === "deepgram") return "cloud-asr-deepgram";
  if (protocol === "gladia") return "cloud-asr-gladia";
  if (protocol === "volcengine") return "cloud-asr-volcengine";
  if (protocol === "tencent") return "cloud-asr-tencent";
  if (protocol === "aliyun") return "cloud-asr-aliyun";
  if (protocol === "xfyun") return "cloud-asr-xfyun";
  return "cloud-asr-openai-compatible";
}

function credentialFieldsForProtocol(protocol: CloudAsrProtocol): string[] {
  return protocol === "tencent" || protocol === "aliyun" || protocol === "xfyun"
    ? ["accountId", "apiKey", "apiSecret"]
    : ["apiKey"];
}

function trimEndpoint(value: string): string {
  return value.trim().replace(/\/+$/, "");
}

function detectPreset(
  protocol: CloudAsrProtocol,
  endpoint: string,
  model: string,
): CloudPreset {
  const normalizedEndpoint = trimEndpoint(endpoint);
  const normalizedModel = model.trim();
  const preset = (Object.entries(PRESETS) as Array<[
    Exclude<CloudPreset, "custom">,
    CloudPresetConfig,
  ]>).find(([, value]) => (
    value.protocol === protocol
      && trimEndpoint(value.endpoint) === normalizedEndpoint
      && value.model === normalizedModel
  ));
  return preset?.[0] ?? "custom";
}

interface CloudAsrPanelProps {
  onSaved: () => void;
}

export function CloudAsrPanel({ onSaved }: CloudAsrPanelProps) {
  const { t } = useI18n();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [profiles, setProfiles] = useState<CloudAsrProfile[]>([]);
  const [activeProfileId, setActiveProfileId] = useState(LEGACY_PROFILE_ID);
  const [profileName, setProfileName] = useState("Cloud ASR 1");
  const [preset, setPreset] = useState<CloudPreset>("openai");
  const [protocol, setProtocol] = useState<CloudAsrProtocol>("openai-compatible");
  const [endpoint, setEndpoint] = useState(PRESETS.openai.endpoint);
  const [model, setModel] = useState(PRESETS.openai.model);
  const [apiKey, setApiKey] = useState("");
  const [apiSecret, setApiSecret] = useState("");
  const [accountId, setAccountId] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [keyConfigured, setKeyConfigured] = useState(false);
  const [checkingKey, setCheckingKey] = useState(false);
  const [consent, setConsent] = useState(false);
  const [timeoutSeconds, setTimeoutSeconds] = useState(120);
  const [retryTimes, setRetryTimes] = useState(1);
  const [requestConcurrency, setRequestConcurrency] = useState(1);
  const [requestIntervalMs, setRequestIntervalMs] = useState(0);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testStatus, setTestStatus] = useState("");

  useEffect(() => {
    setTestStatus("");
  }, [
    activeProfileId,
    profileName,
    protocol,
    endpoint,
    model,
    apiKey,
    apiSecret,
    accountId,
    consent,
    timeoutSeconds,
    retryTimes,
    requestConcurrency,
    requestIntervalMs,
  ]);

  const applyProfile = (profile: CloudAsrProfile) => {
    setActiveProfileId(profile.id);
    setProfileName(profile.name);
    setProtocol(profile.protocol);
    setEndpoint(profile.endpoint);
    setModel(profile.model);
    setPreset(detectPreset(profile.protocol, profile.endpoint, profile.model));
    setConsent(profile.upload_consent);
    setTimeoutSeconds(profile.timeout_seconds);
    setRetryTimes(profile.retry_times);
    setRequestConcurrency(profile.request_concurrency);
    setRequestIntervalMs(profile.request_interval_ms);
    setApiKey("");
    setApiSecret("");
    setAccountId("");
    setKeyConfigured(false);
    setSaved(false);
    setError("");
  };

  useEffect(() => {
    getSettings()
      .then((loaded) => {
        setSettings(loaded);
        const loadedProfiles = loaded.cloud_asr_profiles.length > 0
          ? loaded.cloud_asr_profiles
          : [legacyProfile(loaded)];
        const activeProfile = loadedProfiles.find(
          (profile) => profile.id === loaded.cloud_asr_active_profile_id,
        ) ?? loadedProfiles[0];
        setProfiles(loadedProfiles);
        applyProfile(activeProfile);
      })
      .catch((reason) => setError(String(reason)));
  }, []);

  useEffect(() => {
    const currentEndpoint = endpoint.trim();
    setKeyConfigured(false);
    if (!currentEndpoint) {
      setCheckingKey(false);
      return;
    }
    let active = true;
    setCheckingKey(true);
    const timer = window.setTimeout(() => {
      Promise.all(credentialFieldsForProtocol(protocol).map((field) => (
        hasProviderSecret(secretProviderForProtocol(protocol), currentEndpoint, field)
      )))
        .then((configuredFields) => {
          if (!active) return;
          setKeyConfigured(configuredFields.every(Boolean));
          setError("");
        })
        .catch(() => {
          if (!active) return;
          setKeyConfigured(false);
          setError(t("models.cloudSecretStoreError"));
        })
        .finally(() => {
          if (active) setCheckingKey(false);
        });
    }, 250);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [endpoint, protocol]);

  const currentProfile = (id = activeProfileId, name = profileName): CloudAsrProfile => ({
    id,
    name: name.trim(),
    protocol,
    endpoint: endpoint.trim(),
    model: model.trim(),
    upload_consent: consent,
    timeout_seconds: timeoutSeconds,
    retry_times: retryTimes,
    request_concurrency: requestConcurrency,
    request_interval_ms: requestIntervalMs,
  });

  const handleProfileChange = (profileId: string) => {
    const profile = profiles.find((candidate) => candidate.id === profileId);
    if (profile) applyProfile(profile);
  };

  const handleAddProfile = () => {
    const index = profiles.length + 1;
    const profile: CloudAsrProfile = {
      id: newProfileId(),
      name: t("models.cloudProfileUntitled", { index }),
      protocol: PRESETS.openai.protocol,
      endpoint: PRESETS.openai.endpoint,
      model: PRESETS.openai.model,
      upload_consent: false,
      timeout_seconds: 120,
      retry_times: 1,
      request_concurrency: 1,
      request_interval_ms: 0,
    };
    setProfiles((current) => [...current, profile]);
    applyProfile(profile);
  };

  const handleDuplicateProfile = () => {
    const profile = currentProfile(
      newProfileId(),
      t("models.cloudProfileCopy", { name: profileName.trim() || "Cloud ASR" }),
    );
    setProfiles((current) => [...current, profile]);
    applyProfile(profile);
  };

  const handleDeleteProfile = () => {
    if (profiles.length <= 1) return;
    const remaining = profiles.filter((profile) => profile.id !== activeProfileId);
    setProfiles(remaining);
    applyProfile(remaining[0]);
  };

  const handlePresetChange = (nextPreset: CloudPreset) => {
    setPreset(nextPreset);
    setSaved(false);
    setError("");
    setApiKey("");
    setApiSecret("");
    setAccountId("");
    setKeyConfigured(false);
    if (nextPreset !== "custom") {
      setProtocol(PRESETS[nextPreset].protocol);
      setEndpoint(PRESETS[nextPreset].endpoint);
      setModel(PRESETS[nextPreset].model);
      if (nextPreset === "xfyun") setTimeoutSeconds(300);
    }
  };

  const handleSave = async (): Promise<boolean> => {
    if (!settings) return false;
    setSaving(true);
    setSaved(false);
    setError("");
    try {
      const normalizedEndpoint = endpoint.trim();
      const normalizedModel = model.trim();
      const profile = currentProfile();
      const updatedProfiles = profiles.some((candidate) => candidate.id === activeProfileId)
        ? profiles.map((candidate) => candidate.id === activeProfileId ? profile : candidate)
        : [...profiles, profile];
      const updated: Settings = {
        ...settings,
        cloud_asr_protocol: protocol,
        cloud_asr_endpoint: normalizedEndpoint,
        cloud_asr_model: normalizedModel,
        cloud_asr_upload_consent: consent,
        cloud_asr_timeout_seconds: timeoutSeconds,
        cloud_asr_retry_times: retryTimes,
        cloud_asr_request_concurrency: requestConcurrency,
        cloud_asr_request_interval_ms: requestIntervalMs,
        cloud_asr_active_profile_id: activeProfileId,
        cloud_asr_profiles: updatedProfiles,
      };
      const savedSettings = await saveSettingsCmd(updated);
      setSettings(savedSettings);
      setProfiles(savedSettings.cloud_asr_profiles);
      if (apiKey.trim()) {
        await setProviderSecret(
          secretProviderForProtocol(protocol),
          normalizedEndpoint,
          "apiKey",
          apiKey.trim(),
        );
      }
      if (apiSecret.trim()) {
        await setProviderSecret(
          secretProviderForProtocol(protocol),
          normalizedEndpoint,
          "apiSecret",
          apiSecret.trim(),
        );
      }
      if (accountId.trim()) {
        await setProviderSecret(
          secretProviderForProtocol(protocol),
          normalizedEndpoint,
          "accountId",
          accountId.trim(),
        );
      }
      const configuredFields = await Promise.all(
        credentialFieldsForProtocol(protocol).map((field) => (
          hasProviderSecret(
            secretProviderForProtocol(protocol),
            normalizedEndpoint,
            field,
          )
        )),
      );
      setKeyConfigured(configuredFields.every(Boolean));
      setApiKey("");
      setApiSecret("");
      setAccountId("");
      setShowKey(false);
      setSaved(true);
      onSaved();
      window.setTimeout(() => setSaved(false), 3200);
      return true;
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      return false;
    } finally {
      setSaving(false);
    }
  };

  const handleTestConnection = async () => {
    if (!settings || testing) return;
    setTesting(true);
    setTestStatus("");
    setError("");
    try {
      if (!await handleSave()) return;
      const result = await testCloudAsrConnection(activeProfileId);
      setTestStatus(t("models.cloudTestSuccess", {
        provider: result.provider,
        model: result.model,
        elapsed: result.elapsed_ms,
      }));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setTesting(false);
    }
  };

  const ready = keyConfigured && consent;
  const usesThreePartCredentials = protocol === "tencent" || protocol === "aliyun" || protocol === "xfyun";
  const draftCredentialsComplete = usesThreePartCredentials
    ? Boolean(accountId.trim() && apiKey.trim() && apiSecret.trim())
    : Boolean(apiKey.trim());
  const canTest = consent && (keyConfigured || draftCredentialsComplete);

  return (
    <Card className="relative overflow-hidden border-brand/20 p-0">
      <div
        aria-hidden="true"
        className="pointer-events-none absolute -right-16 -top-20 h-52 w-52 rounded-full bg-brand/10 blur-3xl"
      />
      <form
        className="relative p-5 sm:p-6"
        onSubmit={(event) => {
          event.preventDefault();
          void handleSave();
        }}
      >
        <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
          <div className="flex min-w-0 gap-3.5">
            <div className="liquid-control flex h-11 w-11 shrink-0 items-center justify-center rounded-2xl text-brand">
              <Cloud size={21} strokeWidth={1.8} />
            </div>
            <div className="min-w-0">
              <p className="text-[11px] font-semibold uppercase tracking-[0.18em] text-brand">
                {t("models.cloudEyebrow")}
              </p>
              <h3 className="mt-1 font-display text-xl font-semibold tracking-tight text-text-primary">
                {t("models.cloudTitle")}
              </h3>
              <p className="mt-1.5 max-w-3xl text-sm leading-6 text-text-secondary">
                {t("models.cloudDesc")}
              </p>
            </div>
          </div>
          <div
            className={`inline-flex shrink-0 items-center gap-2 self-start rounded-full border px-3 py-1.5 text-xs font-semibold ${
              ready
                ? "border-success/25 bg-success/10 text-success"
                : "border-warning/20 bg-warning/10 text-warning"
            }`}
          >
            {ready ? <CheckCircle2 size={14} /> : <LockKeyhole size={14} />}
            {ready ? t("models.cloudReady") : t("models.cloudNotReady")}
          </div>
        </div>

        <div className="mt-6 rounded-2xl border border-border-subtle bg-surface-overlay/55 p-3.5">
          <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] md:items-end">
            <label className="space-y-2 text-sm font-medium text-text-secondary">
              <span>{t("models.cloudProfile")}</span>
              <Select
                value={activeProfileId}
                onChange={(event) => handleProfileChange(event.target.value)}
              >
                {profiles.map((profile) => (
                  <option key={profile.id} value={profile.id}>{profile.name}</option>
                ))}
              </Select>
            </label>
            <label className="space-y-2 text-sm font-medium text-text-secondary">
              <span>{t("models.cloudProfileName")}</span>
              <Input
                value={profileName}
                maxLength={80}
                onChange={(event) => {
                  setProfileName(event.target.value);
                  setSaved(false);
                }}
              />
            </label>
            <div className="flex items-center gap-2 md:pb-0.5">
              <Button
                type="button"
                variant="secondary"
                size="sm"
                onClick={handleAddProfile}
                aria-label={t("models.cloudProfileAdd")}
                title={t("models.cloudProfileAdd")}
              >
                <Plus size={15} />
              </Button>
              <Button
                type="button"
                variant="secondary"
                size="sm"
                onClick={handleDuplicateProfile}
                aria-label={t("models.cloudProfileDuplicate")}
                title={t("models.cloudProfileDuplicate")}
              >
                <Copy size={15} />
              </Button>
              <Button
                type="button"
                variant="secondary"
                size="sm"
                onClick={handleDeleteProfile}
                disabled={profiles.length <= 1}
                aria-label={t("models.cloudProfileDelete")}
                title={t("models.cloudProfileDelete")}
              >
                <Trash2 size={15} />
              </Button>
            </div>
          </div>
        </div>

        <div className="mt-6 grid gap-4 lg:grid-cols-3">
          <label className="space-y-2 text-sm font-medium text-text-secondary">
            <span>{t("models.cloudPreset")}</span>
            <Select
              value={preset}
              onChange={(event) => handlePresetChange(event.target.value as CloudPreset)}
            >
              <option value="openai">{t("models.cloudProviderOpenai")}</option>
              <option value="groq">{t("models.cloudProviderGroq")}</option>
              <option value="siliconflow">{t("models.cloudProviderSiliconflow")}</option>
              <option value="elevenlabs">{t("models.cloudProviderElevenlabs")}</option>
              <option value="deepgram">{t("models.cloudProviderDeepgram")}</option>
              <option value="gladia">{t("models.cloudProviderGladia")}</option>
              <option value="volcengine">{t("models.cloudProviderVolcengine")}</option>
              <option value="tencent">{t("models.cloudProviderTencent")}</option>
              <option value="aliyun">{t("models.cloudProviderAliyun")}</option>
              <option value="xfyun">{t("models.cloudProviderXfyun")}</option>
              <option value="custom">{t("models.cloudProviderCustom")}</option>
            </Select>
          </label>
          <label className="space-y-2 text-sm font-medium text-text-secondary">
            <span>{t("models.cloudProtocol")}</span>
            <Select
              value={protocol}
              onChange={(event) => {
                const value = event.target.value as CloudAsrProtocol;
                setProtocol(value);
                setPreset(detectPreset(value, endpoint, model));
                setApiKey("");
                setApiSecret("");
                setAccountId("");
                setKeyConfigured(false);
                setSaved(false);
              }}
            >
              <option value="openai-compatible">{t("models.cloudProtocolOpenai")}</option>
              <option value="elevenlabs">{t("models.cloudProtocolElevenlabs")}</option>
              <option value="deepgram">{t("models.cloudProtocolDeepgram")}</option>
              <option value="gladia">{t("models.cloudProtocolGladia")}</option>
              <option value="volcengine">{t("models.cloudProtocolVolcengine")}</option>
              <option value="tencent">{t("models.cloudProtocolTencent")}</option>
              <option value="aliyun">{t("models.cloudProtocolAliyun")}</option>
              <option value="xfyun">{t("models.cloudProtocolXfyun")}</option>
            </Select>
          </label>
          <label className="space-y-2 text-sm font-medium text-text-secondary">
            <span>{t("models.cloudModel")}</span>
            <Input
              value={model}
              onChange={(event) => {
                const value = event.target.value;
                setModel(value);
                setPreset(detectPreset(protocol, endpoint, value));
                setSaved(false);
              }}
              autoComplete="off"
              spellCheck={false}
            />
          </label>
          <label className="space-y-2 text-sm font-medium text-text-secondary lg:col-span-3">
            <span>{t("models.cloudEndpoint")}</span>
            <Input
              type="url"
              value={endpoint}
              onChange={(event) => {
                const value = event.target.value;
                setEndpoint(value);
                setPreset(detectPreset(protocol, value, model));
                setApiKey("");
                setApiSecret("");
                setAccountId("");
                setKeyConfigured(false);
                setSaved(false);
              }}
              autoCapitalize="none"
              autoComplete="url"
              spellCheck={false}
              className="font-mono text-[13px]"
            />
          </label>
          {usesThreePartCredentials && (
            <label className="space-y-2 text-sm font-medium text-text-secondary lg:col-span-3">
              <span>
                {protocol === "aliyun"
                  ? t("models.cloudAliyunAppKey")
                  : protocol === "xfyun"
                    ? t("models.cloudXfyunAppId")
                  : t("models.cloudTencentAppId")}
              </span>
              <Input
                value={accountId}
                onChange={(event) => {
                  setAccountId(event.target.value);
                  setSaved(false);
                }}
                placeholder={t("models.cloudCredentialPlaceholder")}
                autoComplete="off"
                spellCheck={false}
                className="font-mono text-[13px]"
              />
            </label>
          )}
          <label className="space-y-2 text-sm font-medium text-text-secondary lg:col-span-3">
            <span className="flex items-center justify-between gap-3">
              <span>
                {protocol === "tencent"
                  ? t("models.cloudTencentSecretId")
                  : protocol === "aliyun"
                    ? t("models.cloudAliyunAccessKeyId")
                    : protocol === "xfyun"
                      ? t("models.cloudXfyunApiKey")
                    : t("models.cloudApiKey")}
              </span>
              <span className={`text-xs font-normal ${keyConfigured ? "text-success" : "text-text-tertiary"}`}>
                {checkingKey
                  ? "…"
                  : keyConfigured
                    ? t("models.cloudApiKeySaved")
                    : t("models.cloudApiKeyMissing")}
              </span>
            </span>
            <span className="relative block">
              <Input
                type={showKey ? "text" : "password"}
                value={apiKey}
                onChange={(event) => {
                  setApiKey(event.target.value);
                  setSaved(false);
                }}
                placeholder={t("models.cloudApiKeyPlaceholder")}
                autoComplete="new-password"
                spellCheck={false}
                className="pr-11 font-mono text-[13px]"
              />
              <button
                type="button"
                onClick={() => setShowKey((value) => !value)}
                className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded-lg p-1.5 text-text-tertiary transition hover:bg-surface-overlay hover:text-text-primary"
                aria-label={showKey ? t("models.cloudHideKey") : t("models.cloudShowKey")}
              >
                {showKey ? <EyeOff size={16} /> : <Eye size={16} />}
              </button>
            </span>
          </label>
          {usesThreePartCredentials && (
            <label className="space-y-2 text-sm font-medium text-text-secondary lg:col-span-3">
              <span>
                {protocol === "aliyun"
                  ? t("models.cloudAliyunAccessKeySecret")
                  : protocol === "xfyun"
                    ? t("models.cloudXfyunApiSecret")
                  : t("models.cloudTencentSecretKey")}
              </span>
              <Input
                type={showKey ? "text" : "password"}
                value={apiSecret}
                onChange={(event) => {
                  setApiSecret(event.target.value);
                  setSaved(false);
                }}
                placeholder={t("models.cloudCredentialPlaceholder")}
                autoComplete="new-password"
                spellCheck={false}
                className="font-mono text-[13px]"
              />
            </label>
          )}
        </div>

        <label className="mt-5 flex cursor-pointer items-start gap-3 rounded-2xl border border-border-subtle bg-surface-overlay/60 p-4 transition hover:border-border-default">
          <input
            type="checkbox"
            checked={consent}
            onChange={(event) => {
              setConsent(event.target.checked);
              setSaved(false);
            }}
            className="mt-0.5 h-4 w-4 shrink-0 accent-[rgb(var(--color-brand))]"
          />
          <span className="min-w-0">
            <span className="block text-sm font-semibold text-text-primary">
              {t("models.cloudConsent")}
            </span>
            <span className="mt-1 block text-xs leading-5 text-text-tertiary">
              {t("models.cloudConsentHint")}
            </span>
          </span>
        </label>

        <div className="mt-4 border-t border-border-subtle pt-4">
          <button
            type="button"
            onClick={() => setAdvancedOpen((value) => !value)}
            className="flex w-full items-center justify-between gap-3 rounded-xl px-1 py-1.5 text-sm font-semibold text-text-secondary transition hover:text-text-primary"
            aria-expanded={advancedOpen}
          >
            <span className="flex items-center gap-2">
              <SlidersHorizontal size={15} />
              {t("models.cloudAdvanced")}
            </span>
            <ChevronDown
              size={16}
              className={`transition-transform ${advancedOpen ? "rotate-180" : ""}`}
            />
          </button>
          {advancedOpen && (
            <div className="mt-3 grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
              <label className="space-y-2 text-xs font-medium text-text-secondary">
                <span>{t("models.cloudTimeout")}</span>
                <Input
                  type="number"
                  min={10}
                  max={900}
                  value={timeoutSeconds}
                  onChange={(event) => setTimeoutSeconds(Number(event.target.value))}
                />
              </label>
              <label className="space-y-2 text-xs font-medium text-text-secondary">
                <span>{t("models.cloudRetries")}</span>
                <Input
                  type="number"
                  min={0}
                  max={5}
                  value={retryTimes}
                  onChange={(event) => setRetryTimes(Number(event.target.value))}
                />
              </label>
              <label className="space-y-2 text-xs font-medium text-text-secondary">
                <span>{t("models.cloudConcurrency")}</span>
                <Input
                  type="number"
                  min={1}
                  max={8}
                  value={requestConcurrency}
                  onChange={(event) => setRequestConcurrency(
                    Math.min(8, Math.max(1, Number(event.target.value) || 1)),
                  )}
                />
              </label>
              <label className="space-y-2 text-xs font-medium text-text-secondary">
                <span>{t("models.cloudInterval")}</span>
                <Input
                  type="number"
                  min={0}
                  max={60000}
                  step={100}
                  value={requestIntervalMs}
                  onChange={(event) => setRequestIntervalMs(Number(event.target.value))}
                />
              </label>
            </div>
          )}
        </div>

        {(error || testStatus || saved) && (
          <div
            role="status"
            className={`mt-4 rounded-xl border px-3.5 py-2.5 text-sm ${
              error
                ? "border-danger/20 bg-danger/10 text-danger"
                : "border-success/20 bg-success/10 text-success"
            }`}
          >
            {error || testStatus || t("models.cloudSaved")}
          </div>
        )}

        <div className="mt-5 flex items-center justify-between gap-3">
          <div className="hidden max-w-xl items-start gap-2 text-xs leading-5 text-text-tertiary sm:flex">
            <KeyRound size={14} className="mt-0.5 shrink-0" />
            <span>
              {t("models.cloudSecretStorage")}
              <span className="block">{t("models.cloudTestHint")}</span>
            </span>
          </div>
          <div className="ml-auto flex shrink-0 items-center gap-2">
            <Button
              type="button"
              disabled={testing || saving || !settings || !canTest}
              variant="secondary"
              onClick={() => void handleTestConnection()}
            >
              <RefreshCw size={15} className={testing ? "animate-spin" : ""} />
              {testing ? t("models.cloudTesting") : t("models.cloudTest")}
            </Button>
            <Button
              type="submit"
              disabled={saving || testing || !settings}
              variant="primary"
              className="min-w-[10rem]"
            >
              <Save size={15} />
              {saving ? t("models.cloudSaving") : t("models.cloudSave")}
            </Button>
          </div>
        </div>
      </form>
    </Card>
  );
}
