import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useNavigate } from "react-router-dom";
import {
  Bot,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  CircleGauge,
  Command,
  Edit3,
  FileVideo2,
  Film,
  Languages,
  ListTodo,
  Search,
  Settings,
  Sparkles,
  X,
} from "lucide-react";
import { TASK_DELETED_EVENT, TASK_UPDATED_EVENT, listen, listTasks, type Task } from "../lib/tauri";
import { type TranslationKey, useI18n } from "../lib/i18n";
import { Button } from "./ui/Button";
import { Card } from "./ui/Card";

const commands: Array<{
  path: string;
  label: TranslationKey;
  hint: TranslationKey;
  icon: typeof FileVideo2;
}> = [
  { path: "/", label: "nav.tasks", hint: "command.newTask", icon: FileVideo2 },
  { path: "/tasks", label: "nav.queue", hint: "command.openQueue", icon: ListTodo },
  { path: "/models", label: "nav.models", hint: "command.manageModels", icon: Bot },
  { path: "/translation", label: "nav.translation", hint: "command.configureTranslation", icon: Languages },
  { path: "/proofread", label: "nav.proofread", hint: "command.openProofread", icon: Edit3 },
  { path: "/subtitle-merge", label: "nav.merge", hint: "command.openMerge", icon: Film },
  { path: "/settings", label: "nav.settings", hint: "command.openSettings", icon: Settings },
];

function CommandPalette({ compact = false }: { compact?: boolean }) {
  const { t } = useI18n();
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const visibleCommands = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return commands;
    return commands.filter((command) =>
      `${t(command.label)} ${t(command.hint)}`.toLowerCase().includes(normalized),
    );
  }, [query, t]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setOpen((value) => !value);
        return;
      }
      if ((event.metaKey || event.ctrlKey) && /^[1-7]$/.test(event.key)) {
        event.preventDefault();
        navigate(commands[Number(event.key) - 1].path);
      }
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [navigate]);

  useEffect(() => {
    if (open) {
      setQuery("");
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        className={`sidebar-command-trigger liquid-control flex items-center rounded-xl text-xs font-semibold text-text-secondary transition hover:text-text-primary ${compact ? "mx-auto h-11 w-11 justify-center p-0" : "h-10 w-full justify-between px-3"}`}
        aria-label={t("command.open")}
      >
        <span className="flex items-center gap-2"><Command size={14} />{!compact && t("command.open")}</span>
        {!compact && <kbd className="rounded-md border border-border-subtle bg-surface-overlay px-1.5 py-0.5 font-mono text-[10px] text-text-tertiary">⌘K</kbd>}
      </button>
      {open && createPortal(
        <div className="fixed inset-0 z-[80] flex items-start justify-center bg-black/45 px-2 pt-[8vh] backdrop-blur-md sm:px-4 sm:pt-[12vh]" role="presentation" onMouseDown={() => setOpen(false)}>
          <Card
            className="liquid-shell max-h-[84vh] w-full max-w-xl overflow-hidden border border-border-strong p-0 shadow-2xl"
            role="dialog"
            aria-modal="true"
            aria-label={t("command.title")}
            onMouseDown={(event) => event.stopPropagation()}
          >
            <div className="flex items-center gap-2 border-b border-border-subtle px-3 py-3 sm:gap-3 sm:px-4 sm:py-3.5">
              <div className="surface-control flex h-10 min-w-0 flex-1 items-center gap-2 rounded-xl px-3 focus-within:border-brand/65 focus-within:ring-2 focus-within:ring-brand/20">
                <Search size={17} className="shrink-0 text-brand" />
                <input
                  ref={inputRef}
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder={t("command.search")}
                  className="command-search-input h-full min-w-0 flex-1 appearance-none border-0 bg-transparent p-0 text-sm text-text-primary shadow-none outline-none ring-0 placeholder:text-text-tertiary focus:border-0 focus:outline-none focus:ring-0"
                />
              </div>
              <button type="button" onClick={() => setOpen(false)} className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl text-text-tertiary transition hover:bg-surface-overlay hover:text-text-primary" aria-label={t("common.close")}>
                <X size={16} />
              </button>
            </div>
            <div className="max-h-[calc(84vh-4.75rem)] overscroll-contain overflow-y-auto p-2 sm:max-h-[55vh] sm:p-2.5">
              {visibleCommands.map((command, index) => {
                const Icon = command.icon;
                return (
                  <button
                    key={command.path}
                    type="button"
                    onClick={() => {
                      navigate(command.path);
                      setOpen(false);
                    }}
                    className="grid w-full grid-cols-[2rem_minmax(0,1fr)_auto] items-center gap-x-2.5 rounded-xl px-2.5 py-3 text-left transition hover:bg-surface-overlay sm:gap-x-3 sm:px-3"
                  >
                    <span className="nav-icon"><Icon size={16} /></span>
                    <span className="min-w-0 flex-1">
                      <span className="block truncate whitespace-nowrap text-sm font-semibold text-text-primary">{t(command.label)}</span>
                      <span className="mt-0.5 block truncate text-xs text-text-tertiary">{t(command.hint)}</span>
                    </span>
                    <kbd className="whitespace-nowrap font-mono text-[10px] text-text-tertiary">⌘{index + 1}</kbd>
                  </button>
                );
              })}
              {visibleCommands.length === 0 && <p className="px-3 py-8 text-center text-sm text-text-tertiary">{t("command.empty")}</p>}
            </div>
          </Card>
        </div>,
        document.body,
      )}
    </>
  );
}

export function ActivityCenter({ compact = false }: { compact?: boolean }) {
  const { t } = useI18n();
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const [tasks, setTasks] = useState<Task[]>([]);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void listTasks().then(setTasks).catch(() => undefined);
    let stopUpdated: (() => void) | undefined;
    let stopDeleted: (() => void) | undefined;
    void listen<Task>(TASK_UPDATED_EVENT, (event) => {
      setTasks((current) => {
        const next = current.filter((task) => task.id !== event.payload.id);
        return [event.payload, ...next].slice(0, 50);
      });
    }).then((stop) => { stopUpdated = stop; });
    void listen<{ task_id: string }>(TASK_DELETED_EVENT, (event) => {
      setTasks((current) => current.filter((task) => task.id !== event.payload.task_id));
    }).then((stop) => { stopDeleted = stop; });
    return () => {
      stopUpdated?.();
      stopDeleted?.();
    };
  }, []);

  useEffect(() => {
    if (!open) return;
    const closeOnOutsidePress = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", closeOnOutsidePress);
    window.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeOnOutsidePress);
      window.removeEventListener("keydown", closeOnEscape);
    };
  }, [open]);

  const activeCount = tasks.filter((task) => task.status === "running" || task.status === "pending").length;
  const recent = [...tasks]
    .sort((a, b) => Date.parse(b.updated_at) - Date.parse(a.updated_at))
    .slice(0, 6);

  return (
    <div ref={rootRef} className="relative z-[55] shrink-0 sm:w-full">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className={`sidebar-activity-trigger relative flex items-center rounded-xl text-xs font-semibold text-text-secondary transition hover:text-text-primary ${
          compact
            ? "mx-auto h-11 w-11 justify-center bg-surface-overlay"
            : "liquid-control h-9 w-9 justify-center shadow-sm sm:h-10 sm:w-full sm:justify-between sm:px-3"
        }`}
        aria-label={t("activity.title")}
        aria-expanded={open}
        aria-controls="activity-center-panel"
      >
        <span className={`flex items-center gap-2 ${activeCount > 0 ? "text-brand" : ""}`}>
          <CircleGauge size={16} />
          {!compact && <span className="hidden sm:inline">{t("activity.title")}</span>}
        </span>
        {!compact && activeCount > 0 && (
          <span className="absolute -right-1 -top-1 inline-flex min-w-4 items-center justify-center rounded-full bg-brand px-1 text-center text-[10px] font-bold leading-4 text-white sm:static">
            {activeCount}
          </span>
        )}
      </button>
      {open && (
        <Card
          id="activity-center-panel"
          role="dialog"
          aria-label={t("activity.title")}
          className="activity-popover absolute right-0 top-full mt-2 w-[min(21.5rem,calc(100vw-2rem))] border border-border-strong p-3 sm:left-[calc(100%+0.75rem)] sm:right-auto sm:top-0 sm:mt-0"
        >
          <div className="flex items-center justify-between px-1 pb-2">
            <div>
              <h3 className="font-display text-sm font-bold text-text-primary">{t("activity.title")}</h3>
              <p className="mt-0.5 text-xs text-text-tertiary">{t("activity.active", { count: activeCount })}</p>
            </div>
            <button type="button" onClick={() => setOpen(false)} className="rounded-lg p-1 text-text-tertiary hover:bg-surface-overlay" aria-label={t("common.close")}><X size={15} /></button>
          </div>
          <div className="space-y-1">
            {recent.map((task) => (
              <button
                key={task.id}
                type="button"
                onClick={() => { navigate("/tasks"); setOpen(false); }}
                className="flex w-full items-center gap-3 rounded-xl px-2.5 py-2.5 text-left transition hover:bg-surface-overlay"
              >
                <span className={`h-2.5 w-2.5 shrink-0 rounded-full ${task.status === "done" ? "bg-success" : task.status === "error" ? "bg-danger" : task.status === "running" ? "bg-brand animate-pulse" : "bg-warning"}`} />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-xs font-semibold text-text-primary">{task.media_name}</span>
                  <span className="mt-0.5 block truncate text-[11px] text-text-tertiary">{task.status_message}</span>
                </span>
                <span className="font-mono text-[10px] text-text-tertiary">{Math.round(task.progress * 100)}%</span>
              </button>
            ))}
            {recent.length === 0 && <p className="px-2 py-7 text-center text-xs text-text-tertiary">{t("activity.empty")}</p>}
          </div>
          <Button type="button" onClick={() => { navigate("/tasks"); setOpen(false); }} variant="secondary" size="sm" className="mt-2 w-full">
            {t("activity.openQueue")}
          </Button>
        </Card>
      )}
    </div>
  );
}

const onboardingSteps: Array<{ title: TranslationKey; desc: TranslationKey; icon: typeof Sparkles }> = [
  { title: "onboarding.nativeTitle", desc: "onboarding.nativeDesc", icon: Sparkles },
  { title: "onboarding.workflowTitle", desc: "onboarding.workflowDesc", icon: FileVideo2 },
  { title: "onboarding.privateTitle", desc: "onboarding.privateDesc", icon: CheckCircle2 },
];

function Onboarding() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const [visible, setVisible] = useState(false);
  const [step, setStep] = useState(0);
  useEffect(() => {
    setVisible(localStorage.getItem("finalsub:onboarding:v2") !== "done");
  }, []);
  if (!visible) return null;
  const current = onboardingSteps[step];
  const Icon = current.icon;
  const finish = () => {
    localStorage.setItem("finalsub:onboarding:v2", "done");
    setVisible(false);
  };
  return (
    <div className="fixed inset-0 z-[90] flex items-center justify-center bg-black/55 p-4 backdrop-blur-xl">
      <Card className="liquid-shell relative w-full max-w-2xl overflow-hidden border border-border-strong p-7 shadow-2xl sm:p-9">
        <span className="pipeline-glow" aria-hidden="true" />
        <button type="button" onClick={finish} className="absolute right-4 top-4 rounded-lg p-1.5 text-text-tertiary hover:bg-surface-overlay hover:text-text-primary" aria-label={t("onboarding.skip")}><X size={17} /></button>
        <div className="relative z-10">
          <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-brand/14 text-brand shadow-[0_0_32px_color-mix(in_srgb,var(--color-brand)_22%,transparent)]"><Icon size={23} /></div>
          <p className="mt-6 text-xs font-bold uppercase tracking-[0.16em] text-brand">{t("onboarding.eyebrow", { current: step + 1, total: onboardingSteps.length })}</p>
          <h2 className="mt-3 max-w-xl font-display text-[clamp(1.8rem,5vw,3rem)] font-bold leading-tight tracking-[-0.04em] text-text-primary">{t(current.title)}</h2>
          <p className="mt-4 max-w-xl text-base leading-7 text-text-secondary">{t(current.desc)}</p>
          <div className="mt-8 flex items-center justify-between gap-4">
            <div className="flex gap-1.5">
              {onboardingSteps.map((_, index) => <span key={index} className={`h-1.5 rounded-full transition-all ${index === step ? "w-8 bg-brand" : "w-3 bg-border-strong"}`} />)}
            </div>
            <div className="flex gap-2">
              {step > 0 && <Button type="button" onClick={() => setStep((value) => value - 1)} variant="secondary" size="sm"><ChevronLeft size={14} />{t("onboarding.back")}</Button>}
              {step < onboardingSteps.length - 1 ? (
                <Button type="button" onClick={() => setStep((value) => value + 1)} variant="primary" size="sm">{t("onboarding.next")}<ChevronRight size={14} /></Button>
              ) : (
                <Button type="button" onClick={() => { finish(); navigate("/models"); }} variant="primary" size="sm">{t("onboarding.finish")}</Button>
              )}
            </div>
          </div>
        </div>
      </Card>
    </div>
  );
}

export function WorkspaceOverlays() {
  return (
    <>
      <Onboarding />
    </>
  );
}

export { CommandPalette };
