import { useEffect, useMemo, useState } from "react";
import {
  AlertCircle,
  ArrowDown,
  ArrowUp,
  CheckCircle2,
  Loader2,
  RefreshCcw,
  Save,
  Trash2,
} from "lucide-react";

import { useI18n } from "../lib/i18n";
import {
  deleteSubtitleStylePreset,
  listSubtitleStylePresets,
  reorderSubtitleStylePresets,
  saveSubtitleStylePreset,
  type SubtitleStyle,
  type SubtitleStylePreset,
} from "../lib/tauri";
import {
  assColorToCss,
  BUILT_IN_SUBTITLE_STYLE_PRESETS,
  subtitleStylesEqual,
} from "../lib/subtitleStyles";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";

interface SubtitleStylePresetManagerProps {
  currentStyle: SubtitleStyle;
  activePresetId: string | null;
  disabled?: boolean;
  onApply: (style: SubtitleStyle, presetId: string) => void;
  onActivePresetRemoved: () => void;
}

type PresetDialog =
  | { mode: "save" }
  | { mode: "delete"; preset: SubtitleStylePreset };

function StyleSwatch({ style, sample }: { style: SubtitleStyle; sample: string }) {
  return (
    <span className="relative flex h-12 w-[5.5rem] shrink-0 items-center justify-center overflow-hidden rounded-lg border border-white/10 bg-[#111827] px-2">
      <span className="absolute inset-0 bg-[radial-gradient(circle_at_30%_20%,rgba(59,130,246,.3),transparent_58%)]" />
      <span
        className="relative z-10 max-w-full truncate text-center font-bold leading-none"
        style={{
          fontFamily: style.font_name,
          fontSize: `${Math.min(18, Math.max(11, style.font_size * 0.48))}px`,
          color: assColorToCss(style.font_color),
          WebkitTextStroke: `${Math.min(2, style.outline_width * 0.55)}px ${assColorToCss(style.outline_color)}`,
          paintOrder: "stroke fill",
          textShadow: style.shadow > 0 ? "1px 1px 2px rgba(0,0,0,.9)" : "none",
          background: style.opaque_background ? assColorToCss(style.background_color) : "transparent",
          borderRadius: style.opaque_background ? "0.3rem" : undefined,
          padding: style.opaque_background ? "0.2rem 0.35rem" : undefined,
        }}
      >
        {sample}
      </span>
    </span>
  );
}

export function SubtitleStylePresetManager({
  currentStyle,
  activePresetId,
  disabled = false,
  onApply,
  onActivePresetRemoved,
}: SubtitleStylePresetManagerProps) {
  const { t } = useI18n();
  const [presets, setPresets] = useState<SubtitleStylePreset[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [dialog, setDialog] = useState<PresetDialog | null>(null);
  const [presetName, setPresetName] = useState("");
  const [notice, setNotice] = useState<{ tone: "success" | "error"; text: string } | null>(null);

  useEffect(() => {
    let active = true;
    listSubtitleStylePresets()
      .then((loaded) => {
        if (active) setPresets(loaded);
      })
      .catch((error) => {
        if (active) setNotice({ tone: "error", text: error instanceof Error ? error.message : String(error) });
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  const activeUserPreset = useMemo(
    () => presets.find((preset) => preset.id === activePresetId),
    [activePresetId, presets],
  );

  const openSaveDialog = () => {
    setPresetName("");
    setNotice(null);
    setDialog({ mode: "save" });
  };

  const handleSave = async () => {
    const name = presetName.trim();
    if (!name) return;
    if (presets.some((preset) => preset.name.trim().toLocaleLowerCase() === name.toLocaleLowerCase())) {
      setNotice({ tone: "error", text: t("merge.stylePresetDuplicate") });
      return;
    }
    setBusyId("save");
    setNotice(null);
    try {
      const saved = await saveSubtitleStylePreset({ name, style: currentStyle });
      setPresets((current) => [...current, saved]);
      onApply(saved.style, saved.id);
      setDialog(null);
      setNotice({ tone: "success", text: t("merge.stylePresetSaved", { name: saved.name }) });
    } catch (error) {
      setNotice({ tone: "error", text: error instanceof Error ? error.message : String(error) });
    } finally {
      setBusyId(null);
    }
  };

  const handleUpdate = async () => {
    if (!activeUserPreset) return;
    setBusyId(activeUserPreset.id);
    setNotice(null);
    try {
      const updated = await saveSubtitleStylePreset({
        id: activeUserPreset.id,
        name: activeUserPreset.name,
        style: currentStyle,
      });
      setPresets((current) => current.map((preset) => preset.id === updated.id ? updated : preset));
      onApply(updated.style, updated.id);
      setNotice({ tone: "success", text: t("merge.stylePresetUpdated", { name: updated.name }) });
    } catch (error) {
      setNotice({ tone: "error", text: error instanceof Error ? error.message : String(error) });
    } finally {
      setBusyId(null);
    }
  };

  const handleMove = async (index: number, offset: -1 | 1) => {
    const target = index + offset;
    if (target < 0 || target >= presets.length) return;
    const previous = presets;
    const next = [...presets];
    [next[index], next[target]] = [next[target], next[index]];
    setPresets(next);
    setBusyId(next[target].id);
    setNotice(null);
    try {
      const persisted = await reorderSubtitleStylePresets(next.map((preset) => preset.id));
      setPresets(persisted);
      setNotice({ tone: "success", text: t("merge.stylePresetReordered") });
    } catch (error) {
      setPresets(previous);
      setNotice({ tone: "error", text: error instanceof Error ? error.message : String(error) });
    } finally {
      setBusyId(null);
    }
  };

  const handleDelete = async (preset: SubtitleStylePreset) => {
    setBusyId(preset.id);
    setNotice(null);
    try {
      await deleteSubtitleStylePreset(preset.id);
      setPresets((current) => current.filter((item) => item.id !== preset.id));
      if (activePresetId === preset.id) onActivePresetRemoved();
      setDialog(null);
      setNotice({ tone: "success", text: t("merge.stylePresetDeleted", { name: preset.name }) });
    } catch (error) {
      setNotice({ tone: "error", text: error instanceof Error ? error.message : String(error) });
    } finally {
      setBusyId(null);
    }
  };

  return (
    <div data-testid="subtitle-style-presets" className="space-y-5">
      <section>
        <div className="mb-2.5 flex items-center justify-between gap-3">
          <div>
            <p className="text-sm font-semibold text-text-primary">{t("merge.builtInStyles")}</p>
            <p className="mt-0.5 text-xs leading-5 text-text-tertiary">{t("merge.builtInStylesDesc")}</p>
          </div>
        </div>
        <div className="grid gap-2.5 sm:grid-cols-2 xl:grid-cols-3">
          {BUILT_IN_SUBTITLE_STYLE_PRESETS.map((preset) => {
            const selected = activePresetId === preset.id && subtitleStylesEqual(currentStyle, preset.style);
            return (
              <button
                key={preset.id}
                type="button"
                aria-pressed={selected}
                disabled={disabled}
                data-testid={`style-preset-${preset.id.replace(":", "-")}`}
                onClick={() => onApply(preset.style, preset.id)}
                className={`flex min-w-0 items-center gap-3 rounded-xl border p-2.5 text-left transition disabled:cursor-not-allowed disabled:opacity-50 ${selected ? "liquid-selected" : "border-border-default bg-surface-overlay/35 hover:border-border-strong hover:bg-surface-overlay"}`}
              >
                <StyleSwatch style={preset.style} sample={t("merge.styleSample")} />
                <span className="min-w-0">
                  <span className="block truncate text-sm font-semibold text-text-primary">{t(preset.nameKey)}</span>
                  <span className="mt-1 block text-xs text-text-tertiary">{preset.style.font_size}px · {preset.style.outline_width}px</span>
                </span>
              </button>
            );
          })}
        </div>
      </section>

      <section className="rounded-xl border border-border-subtle bg-surface-overlay/35 p-3.5 sm:p-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <p className="text-sm font-semibold text-text-primary">{t("merge.myStyles")}</p>
            <p className="mt-0.5 text-xs leading-5 text-text-tertiary">{t("merge.myStylesDesc")}</p>
          </div>
          <div className="flex flex-wrap gap-2">
            {activeUserPreset && (
              <Button
                type="button"
                variant="secondary"
                size="sm"
                disabled={disabled || busyId !== null}
                onClick={() => void handleUpdate()}
                data-testid="style-preset-update"
              >
                {busyId === activeUserPreset.id ? <Loader2 size={13} className="animate-spin" /> : <RefreshCcw size={13} />}
                {t("merge.updateCurrentStyle")}
              </Button>
            )}
            <Button
              type="button"
              variant="secondary"
              size="sm"
              disabled={disabled || loading || busyId !== null || presets.length >= 64}
              onClick={openSaveDialog}
              data-testid="style-preset-save"
            >
              <Save size={13} />
              {t("merge.saveCurrentStyle")}
            </Button>
          </div>
        </div>

        {loading ? (
          <div className="flex min-h-20 items-center justify-center text-sm text-text-tertiary"><Loader2 size={15} className="mr-2 animate-spin" />{t("merge.stylePresetLoading")}</div>
        ) : presets.length === 0 ? (
          <div className="mt-3 rounded-lg border border-dashed border-border-default px-4 py-5 text-center text-xs leading-5 text-text-tertiary">{t("merge.noSavedStyles")}</div>
        ) : (
          <div className="mt-3 space-y-2" data-testid="style-preset-user-list">
            {presets.map((preset, index) => {
              const selected = activePresetId === preset.id && subtitleStylesEqual(currentStyle, preset.style);
              return (
                <div
                  key={preset.id}
                  data-testid="style-preset-user-row"
                  data-preset-name={preset.name}
                  className={`flex min-w-0 flex-col gap-2.5 rounded-xl border p-2.5 sm:flex-row sm:items-center ${selected ? "border-brand/35 bg-brand/[0.06]" : "border-border-subtle bg-surface-elevated/60"}`}
                >
                  <button
                    type="button"
                    disabled={disabled}
                    onClick={() => onApply(preset.style, preset.id)}
                    className="flex min-w-0 flex-1 items-center gap-3 rounded-lg text-left outline-none focus-visible:ring-2 focus-visible:ring-brand/45 disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    <StyleSwatch style={preset.style} sample={t("merge.styleSample")} />
                    <span className="min-w-0">
                      <span className="block truncate text-sm font-semibold text-text-primary" title={preset.name}>{preset.name}</span>
                      <span className="mt-1 block text-xs text-text-tertiary">{preset.style.font_size}px · {t("merge.stylePosition", { value: preset.style.alignment })}</span>
                    </span>
                  </button>
                  <div className="flex shrink-0 items-center justify-end gap-1">
                    <button type="button" disabled={disabled || busyId !== null || index === 0} onClick={() => void handleMove(index, -1)} title={t("merge.moveStyleUp")} aria-label={t("merge.moveStyleUp")} className="inline-flex h-8 w-8 items-center justify-center rounded-lg text-text-tertiary transition hover:bg-surface-overlay hover:text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/45 disabled:cursor-not-allowed disabled:opacity-35"><ArrowUp size={14} /></button>
                    <button type="button" disabled={disabled || busyId !== null || index === presets.length - 1} onClick={() => void handleMove(index, 1)} title={t("merge.moveStyleDown")} aria-label={t("merge.moveStyleDown")} className="inline-flex h-8 w-8 items-center justify-center rounded-lg text-text-tertiary transition hover:bg-surface-overlay hover:text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/45 disabled:cursor-not-allowed disabled:opacity-35"><ArrowDown size={14} /></button>
                    <button type="button" disabled={disabled || busyId !== null} onClick={() => { setNotice(null); setDialog({ mode: "delete", preset }); }} title={t("merge.deleteStylePreset")} aria-label={t("merge.deleteStylePreset")} className="inline-flex h-8 w-8 items-center justify-center rounded-lg text-danger transition hover:bg-danger/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-danger/40 disabled:cursor-not-allowed disabled:opacity-35"><Trash2 size={14} /></button>
                  </div>
                </div>
              );
            })}
          </div>
        )}

        {notice && !dialog && (
          <div data-testid="style-preset-notice" className={`mt-3 flex items-start gap-2 rounded-lg border px-3 py-2 text-xs leading-5 ${notice.tone === "success" ? "border-success/20 bg-success/8 text-success" : "border-danger/20 bg-danger/8 text-danger"}`}>
            {notice.tone === "success" ? <CheckCircle2 size={13} className="mt-0.5 shrink-0" /> : <AlertCircle size={13} className="mt-0.5 shrink-0" />}
            <span>{notice.text}</span>
          </div>
        )}
      </section>

      {dialog && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/35 p-4 backdrop-blur-sm"
          role="dialog"
          aria-modal="true"
          aria-labelledby="style-preset-dialog-title"
          data-testid="style-preset-dialog"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget && busyId === null) setDialog(null);
          }}
        >
          <div className="glass-card w-full max-w-md p-5 shadow-xl sm:p-6">
            <h3 id="style-preset-dialog-title" className="font-display text-h2 font-bold text-text-primary">
              {dialog.mode === "save" ? t("merge.saveStyleTitle") : t("merge.deleteStyleTitle")}
            </h3>
            {dialog.mode === "save" ? (
              <div className="mt-4">
                <label htmlFor="style-preset-name" className="mb-1.5 block text-sm font-medium text-text-secondary">{t("merge.stylePresetName")}</label>
                <Input
                  id="style-preset-name"
                  autoFocus
                  maxLength={64}
                  value={presetName}
                  onChange={(event) => setPresetName(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" && presetName.trim() && busyId === null) void handleSave();
                  }}
                  placeholder={t("merge.stylePresetNamePlaceholder")}
                  data-testid="style-preset-name"
                />
              </div>
            ) : (
              <p className="mt-3 text-sm leading-6 text-text-secondary">{t("merge.deleteStyleDesc", { name: dialog.preset.name })}</p>
            )}
            {notice?.tone === "error" && (
              <div className="mt-3 flex items-start gap-2 rounded-lg border border-danger/20 bg-danger/8 px-3 py-2 text-xs leading-5 text-danger">
                <AlertCircle size={13} className="mt-0.5 shrink-0" />
                <span>{notice.text}</span>
              </div>
            )}
            <div className="mt-5 flex justify-end gap-2">
              <Button type="button" variant="secondary" disabled={busyId !== null} onClick={() => setDialog(null)}>{t("common.cancel")}</Button>
              <Button
                type="button"
                variant={dialog.mode === "delete" ? "danger" : "primary"}
                disabled={busyId !== null || (dialog.mode === "save" && !presetName.trim())}
                onClick={() => dialog.mode === "save" ? void handleSave() : void handleDelete(dialog.preset)}
                data-testid="style-preset-dialog-confirm"
              >
                {busyId !== null && <Loader2 size={14} className="animate-spin" />}
                {dialog.mode === "save" ? t("merge.saveStyleConfirm") : t("merge.deleteStyleConfirm")}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
