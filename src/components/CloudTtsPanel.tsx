import { useEffect, useMemo, useState } from "react";
import {
  CheckCircle2,
  Cloud,
  Eye,
  EyeOff,
  KeyRound,
  Plus,
  Save,
  Sparkles,
  Trash2,
  Volume2,
} from "lucide-react";

import { useI18n } from "../lib/i18n";
import {
  deleteTtsProvider,
  hasProviderSecret,
  listTtsProviders,
  saveTtsProvider,
  setProviderSecret,
  testTtsProvider,
  type SaveTtsProviderRequest,
  type TtsProviderProfile,
  type TtsProviderProtocol,
} from "../lib/tauri";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { Card } from "./ui/Card";
import { Input, Select } from "./ui/Input";

const DEFAULTS: Record<TtsProviderProtocol, Pick<SaveTtsProviderRequest, "endpoint" | "model" | "voice" | "region">> = {
  "openai-compatible": {
    endpoint: "https://api.openai.com/v1",
    model: "gpt-4o-mini-tts",
    voice: "alloy",
    region: "",
  },
  "azure-speech": {
    endpoint: "",
    model: "azure-neural",
    voice: "zh-CN-XiaoxiaoNeural",
    region: "japaneast",
  },
  elevenlabs: {
    endpoint: "https://api.elevenlabs.io/v1",
    model: "eleven_multilingual_v2",
    voice: "21m00Tcm4TlvDq8ikWAM",
    region: "",
  },
};

function blankDraft(): SaveTtsProviderRequest {
  return {
    name: "OpenAI TTS 1",
    protocol: "openai-compatible",
    ...DEFAULTS["openai-compatible"],
    text_upload_consent: false,
    timeout_seconds: 60,
    request_concurrency: 1,
  };
}

function secretProviderId(id: string): string {
  return `tts-provider-${id}`;
}

function resolvedEndpoint(profile: Pick<TtsProviderProfile, "protocol" | "endpoint" | "region">): string {
  if (profile.protocol === "azure-speech" && !profile.endpoint.trim()) {
    return `https://${profile.region.trim()}.tts.speech.microsoft.com/cognitiveservices/v1`;
  }
  return profile.endpoint.trim().replace(/\/+$/, "");
}

export function CloudTtsPanel() {
  const { t } = useI18n();
  const [profiles, setProfiles] = useState<TtsProviderProfile[]>([]);
  const [selectedId, setSelectedId] = useState<string>("");
  const [draft, setDraft] = useState<SaveTtsProviderRequest>(blankDraft);
  const [apiKey, setApiKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [keyConfigured, setKeyConfigured] = useState(false);
  const [busy, setBusy] = useState<"save" | "test" | "delete" | null>(null);
  const [message, setMessage] = useState<{ type: "ok" | "err"; text: string } | null>(null);

  const load = () => {
    listTtsProviders()
      .then((loaded) => {
        setProfiles(loaded);
        if (loaded.length > 0 && !selectedId) {
          setSelectedId(loaded[0].id);
          setDraft({ ...loaded[0] });
        }
      })
      .catch((error) => setMessage({ type: "err", text: String(error) }));
  };

  useEffect(() => {
    load();
  }, []);

  const selected = useMemo(
    () => profiles.find((profile) => profile.id === selectedId) ?? null,
    [profiles, selectedId],
  );

  useEffect(() => {
    setApiKey("");
    setKeyConfigured(false);
    if (!selected?.id) return;
    const endpoint = resolvedEndpoint(selected);
    if (!endpoint || endpoint.includes("//.tts.")) return;
    hasProviderSecret(secretProviderId(selected.id), endpoint, "apiKey")
      .then(setKeyConfigured)
      .catch(() => setKeyConfigured(false));
  }, [selected]);

  const selectProfile = (id: string) => {
    const profile = profiles.find((item) => item.id === id);
    if (!profile) return;
    setSelectedId(id);
    setDraft({ ...profile });
    setMessage(null);
  };

  const createNew = () => {
    setSelectedId("");
    setDraft({ ...blankDraft(), name: t("models.ttsCloudDefaultName", { count: profiles.length + 1 }) });
    setApiKey("");
    setKeyConfigured(false);
    setMessage(null);
  };

  const changeProtocol = (protocol: TtsProviderProtocol) => {
    setDraft((current) => ({
      ...current,
      protocol,
      ...DEFAULTS[protocol],
    }));
    setApiKey("");
    setKeyConfigured(false);
  };

  const save = async () => {
    setBusy("save");
    setMessage(null);
    try {
      const saved = await saveTtsProvider({ ...draft, id: selectedId || undefined });
      const endpoint = resolvedEndpoint(saved);
      if (apiKey.trim()) {
        await setProviderSecret(secretProviderId(saved.id), endpoint, "apiKey", apiKey.trim());
      }
      const configured = apiKey.trim()
        ? true
        : await hasProviderSecret(secretProviderId(saved.id), endpoint, "apiKey");
      setKeyConfigured(configured);
      setProfiles((current) => [saved, ...current.filter((item) => item.id !== saved.id)]);
      setSelectedId(saved.id);
      setDraft({ ...saved });
      setApiKey("");
      setMessage({ type: "ok", text: t("models.ttsCloudSaved") });
    } catch (error) {
      setMessage({ type: "err", text: String(error) });
    } finally {
      setBusy(null);
    }
  };

  const test = async () => {
    if (!selectedId) return;
    setBusy("test");
    setMessage(null);
    try {
      const result = await testTtsProvider(selectedId);
      setMessage({
        type: "ok",
        text: t("models.ttsCloudTestSuccess", { duration: (result.duration_ms / 1000).toFixed(1) }),
      });
    } catch (error) {
      setMessage({ type: "err", text: String(error) });
    } finally {
      setBusy(null);
    }
  };

  const remove = async () => {
    if (!selectedId) return;
    setBusy("delete");
    setMessage(null);
    try {
      await deleteTtsProvider(selectedId);
      const remaining = profiles.filter((profile) => profile.id !== selectedId);
      setProfiles(remaining);
      if (remaining[0]) {
        setSelectedId(remaining[0].id);
        setDraft({ ...remaining[0] });
      } else {
        setSelectedId("");
        setDraft(blankDraft());
      }
      setMessage({ type: "ok", text: t("models.ttsCloudDeleted") });
    } catch (error) {
      setMessage({ type: "err", text: String(error) });
    } finally {
      setBusy(null);
    }
  };

  return (
    <Card className="p-0 overflow-hidden">
      <div className="border-b border-border-subtle bg-surface-overlay px-5 py-4">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex items-start gap-3">
            <span className="liquid-icon grid h-10 w-10 shrink-0 place-items-center rounded-xl text-brand">
              <Cloud size={18} />
            </span>
            <div>
              <div className="flex flex-wrap items-center gap-2">
                <h3 className="font-display text-h3 font-semibold text-text-primary">{t("models.ttsCloudTitle")}</h3>
                <Badge variant="warning">{t("models.cloudTab")}</Badge>
              </div>
              <p className="mt-1 text-sm leading-6 text-text-tertiary">{t("models.ttsCloudDesc")}</p>
            </div>
          </div>
          <Button type="button" variant="secondary" size="sm" onClick={createNew}>
            <Plus size={14} /> {t("models.ttsCloudNew")}
          </Button>
        </div>
      </div>

      <div className="grid min-h-[31rem] lg:grid-cols-[17rem_minmax(0,1fr)]">
        <aside className="border-b border-border-subtle p-3 lg:border-b-0 lg:border-r">
          {profiles.length === 0 ? (
            <div className="rounded-xl border border-dashed border-border-default px-3 py-6 text-center text-sm leading-6 text-text-tertiary">
              {t("models.ttsCloudEmpty")}
            </div>
          ) : (
            <div className="space-y-2">
              {profiles.map((profile) => (
                <button
                  key={profile.id}
                  type="button"
                  onClick={() => selectProfile(profile.id)}
                  className={`w-full rounded-xl border px-3 py-3 text-left transition ${
                    profile.id === selectedId
                      ? "liquid-selected border-brand/25"
                      : "border-border-subtle bg-surface-card hover:bg-surface-overlay"
                  }`}
                >
                  <span className="block truncate text-sm font-semibold text-text-primary">{profile.name}</span>
                  <span className="mt-1 flex items-center justify-between gap-2 text-xs text-text-tertiary">
                    <span>{profile.protocol}</span>
                    {profile.text_upload_consent && <CheckCircle2 size={13} className="text-success" />}
                  </span>
                </button>
              ))}
            </div>
          )}
        </aside>

        <div className="space-y-5 p-5">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <p className="text-xs font-semibold uppercase tracking-[0.14em] text-brand">{t("models.ttsCloudEditorEyebrow")}</p>
              <h4 className="mt-1 font-display text-lg font-semibold text-text-primary">
                {selected ? selected.name : t("models.ttsCloudNewTitle")}
              </h4>
            </div>
            <div className="flex flex-wrap gap-2">
              {selectedId && (
                <Button type="button" variant="ghost" size="sm" onClick={remove} disabled={busy !== null}>
                  <Trash2 size={14} /> {t("common.delete")}
                </Button>
              )}
              <Button type="button" variant="primary" size="sm" onClick={save} disabled={busy !== null}>
                <Save size={14} /> {busy === "save" ? t("models.ttsCloudSaving") : t("common.save")}
              </Button>
            </div>
          </div>

          {message && (
            <div className={`rounded-xl border px-4 py-3 text-sm font-semibold ${
              message.type === "ok"
                ? "border-success/20 bg-success/10 text-success"
                : "border-danger/20 bg-danger/10 text-danger"
            }`}>
              {message.text}
            </div>
          )}

          <div className="grid gap-4 sm:grid-cols-2">
            <label className="space-y-1.5 text-sm font-medium text-text-secondary">
              <span>{t("models.ttsCloudInstanceName")}</span>
              <Input value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} />
            </label>
            <label className="space-y-1.5 text-sm font-medium text-text-secondary">
              <span>{t("models.ttsCloudProtocol")}</span>
              <Select value={draft.protocol} onChange={(event) => changeProtocol(event.target.value as TtsProviderProtocol)}>
                <option value="openai-compatible">OpenAI Compatible</option>
                <option value="azure-speech">Azure Speech</option>
                <option value="elevenlabs">ElevenLabs</option>
              </Select>
            </label>
            {draft.protocol === "azure-speech" && (
              <label className="space-y-1.5 text-sm font-medium text-text-secondary">
                <span>Region</span>
                <Input value={draft.region} onChange={(event) => setDraft({ ...draft, region: event.target.value })} placeholder="japaneast" />
              </label>
            )}
            <label className={`space-y-1.5 text-sm font-medium text-text-secondary ${draft.protocol === "azure-speech" ? "" : "sm:col-span-2"}`}>
              <span>{draft.protocol === "azure-speech" ? t("models.ttsCloudEndpointOptional") : "Endpoint"}</span>
              <Input value={draft.endpoint} onChange={(event) => setDraft({ ...draft, endpoint: event.target.value })} placeholder={draft.protocol === "azure-speech" ? t("models.ttsCloudEndpointAuto") : "https://..."} />
            </label>
            {draft.protocol !== "azure-speech" && (
              <label className="space-y-1.5 text-sm font-medium text-text-secondary">
                <span>{t("models.ttsCloudModel")}</span>
                <Input value={draft.model} onChange={(event) => setDraft({ ...draft, model: event.target.value })} />
              </label>
            )}
            <label className="space-y-1.5 text-sm font-medium text-text-secondary">
              <span>{t("models.ttsCloudVoice")}</span>
              <Input value={draft.voice} onChange={(event) => setDraft({ ...draft, voice: event.target.value })} />
            </label>
          </div>

          <div className="rounded-2xl border border-border-subtle bg-surface-overlay p-4">
            <div className="flex items-center justify-between gap-3">
              <span className="inline-flex items-center gap-2 text-sm font-semibold text-text-primary">
                <KeyRound size={15} className="text-brand" /> API Key
              </span>
              <span className={`text-xs font-semibold ${keyConfigured ? "text-success" : "text-text-tertiary"}`}>
                {keyConfigured ? t("models.ttsCloudKeySaved") : t("models.ttsCloudKeyMissing")}
              </span>
            </div>
            <div className="relative mt-3">
              <Input
                type={showKey ? "text" : "password"}
                value={apiKey}
                onChange={(event) => setApiKey(event.target.value)}
                placeholder={keyConfigured ? t("models.ttsCloudKeyKeep") : t("models.ttsCloudKeyPlaceholder")}
                className="pr-11"
              />
              <button type="button" onClick={() => setShowKey((value) => !value)} className="absolute right-3 top-1/2 -translate-y-1/2 text-text-tertiary hover:text-text-primary">
                {showKey ? <EyeOff size={16} /> : <Eye size={16} />}
              </button>
            </div>
            <p className="mt-2 text-xs leading-5 text-text-tertiary">{t("models.ttsCloudKeychain")}</p>
          </div>

          <label className="flex cursor-pointer items-start gap-3 rounded-2xl border border-warning/20 bg-warning/10 p-4">
            <input
              type="checkbox"
              checked={draft.text_upload_consent}
              onChange={(event) => setDraft({ ...draft, text_upload_consent: event.target.checked })}
              className="mt-1 h-4 w-4 rounded border-border-strong accent-brand"
            />
            <span>
              <span className="flex items-center gap-2 text-sm font-semibold text-text-primary">
                <Sparkles size={15} className="text-warning" /> {t("models.ttsCloudConsentTitle")}
              </span>
              <span className="mt-1 block text-xs leading-5 text-text-tertiary">{t("models.ttsCloudConsentDesc")}</span>
            </span>
          </label>

          <div className="flex flex-wrap items-center justify-between gap-3 border-t border-border-subtle pt-4">
            <p className="max-w-xl text-xs leading-5 text-text-tertiary">{t("models.ttsCloudTestDesc")}</p>
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={test}
              disabled={!selectedId || !keyConfigured || !draft.text_upload_consent || busy !== null}
            >
              <Volume2 size={14} /> {busy === "test" ? t("models.ttsCloudTesting") : t("models.ttsCloudTest")}
            </Button>
          </div>
        </div>
      </div>
    </Card>
  );
}
