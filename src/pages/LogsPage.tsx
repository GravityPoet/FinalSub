import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  CalendarDays,
  Check,
  CircleAlert,
  Copy,
  Info,
  RefreshCw,
  Trash2,
  XCircle,
} from "lucide-react";
import { useI18n } from "../lib/i18n";
import {
  clearLogs,
  getLogDates,
  getLogs,
  listen,
  listTasks,
  type LogEntry,
  type Task,
} from "../lib/tauri";
import { Badge } from "../components/ui/Badge";
import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";

type LevelFilter = "all" | "error" | "warn";

function localDateKey(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return year + "-" + month + "-" + day;
}

function formatTimestamp(timestamp: string, locale: string): string {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) return timestamp;
  return new Intl.DateTimeFormat(locale, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(date);
}

function levelIcon(level: LogEntry["level"]) {
  if (level === "error") return <XCircle size={15} className="text-danger" aria-hidden="true" />;
  if (level === "warn") return <AlertTriangle size={15} className="text-warning" aria-hidden="true" />;
  return <Info size={15} className="text-brand" aria-hidden="true" />;
}

export default function LogsPage() {
  const { t, locale } = useI18n();
  const today = localDateKey(new Date());
  const [entries, setEntries] = useState<LogEntry[]>([]);
  const [dates, setDates] = useState<string[]>([]);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [date, setDate] = useState(today);
  const [level, setLevel] = useState<LevelFilter>("all");
  const [limit, setLimit] = useState(100);
  const [taskId, setTaskId] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [clearTarget, setClearTarget] = useState<{ taskId?: string } | null>(null);
  const requestRevision = useRef(0);
  const logScroller = useRef<HTMLDivElement>(null);

  const taskNames = useMemo(
    () => new Map(tasks.map((task) => [task.id, task.media_name])),
    [tasks],
  );
  const visibleDates = useMemo(
    () => [...new Set([today, ...dates])].sort((left, right) => right.localeCompare(left)),
    [dates, today],
  );
  const selectedLevels = useMemo(
    () => level === "all" ? undefined : [level],
    [level],
  );
  const levelLabels = useMemo<Record<LevelFilter, string>>(() => ({
    all: t("logs.level.all"),
    error: t("logs.level.error"),
    warn: t("logs.level.warn"),
  }), [t]);
  const canClear = dates.length > 0 || entries.length > 0;

  const loadDates = useCallback(async () => {
    try {
      setDates(await getLogDates());
    } catch (error) {
      console.error("Failed to load log dates", error);
    }
  }, []);

  const loadEntries = useCallback(async () => {
    const revision = requestRevision.current + 1;
    requestRevision.current = revision;
    setLoading(true);
    try {
      const nextEntries = await getLogs({
        date,
        limit,
        levels: selectedLevels,
        task_id: taskId || undefined,
      });
      if (requestRevision.current === revision) setEntries(nextEntries);
    } catch (error) {
      console.error("Failed to load logs", error);
      if (requestRevision.current === revision) setNotice(t("logs.loadFailed"));
    } finally {
      if (requestRevision.current === revision) setLoading(false);
    }
  }, [date, limit, selectedLevels, t, taskId]);

  useEffect(() => {
    void listTasks().then(setTasks).catch(() => undefined);
    void loadDates();
  }, [loadDates]);

  useEffect(() => {
    void loadEntries();
  }, [loadEntries]);

  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    void listen<LogEntry>("new-log", (event) => {
      const entry = event.payload;
      if (date !== today || (level !== "all" && entry.level !== level)) return;
      if (taskId && entry.task_id !== taskId) return;
      setEntries((current) => [...current, entry].slice(-limit));
    }).then((cleanup) => {
      if (cancelled) cleanup();
      else stop = cleanup;
    }).catch(console.error);
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [date, level, limit, taskId, today]);

  useEffect(() => {
    if (loading || !entries.length) return;
    const frame = window.requestAnimationFrame(() => {
      if (logScroller.current) logScroller.current.scrollTop = logScroller.current.scrollHeight;
    });
    return () => window.cancelAnimationFrame(frame);
  }, [entries.length, loading]);

  useEffect(() => {
    if (!clearTarget) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) setClearTarget(null);
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [busy, clearTarget]);

  const copyLogs = async () => {
    if (!entries.length) return;
    const text = entries
      .map((entry) => "[" + entry.timestamp + "] [" + entry.level + "] " + entry.message)
      .join("\n");
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch (error) {
      console.error("Failed to copy logs", error);
      setNotice(t("logs.copyFailed"));
    }
  };

  const handleClear = async () => {
    if (!clearTarget) return;
    const targetTaskId = clearTarget.taskId;
    requestRevision.current += 1;
    setLoading(false);
    setBusy(true);
    try {
      await clearLogs(targetTaskId);
      setEntries([]);
      await loadDates();
      if (!targetTaskId) setDate(today);
      setNotice(targetTaskId ? t("logs.taskCleared") : t("logs.allCleared"));
      setClearTarget(null);
    } catch (error) {
      console.error("Failed to clear logs", error);
      setNotice(t("logs.clearFailed"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="space-y-5" data-testid="logs-page">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="text-xs font-bold uppercase tracking-[0.16em] text-brand">{t("logs.eyebrow")}</p>
          <h1 className="mt-2 font-display text-[clamp(1.75rem,4vw,2.55rem)] font-bold tracking-[-0.04em] text-text-primary">
            {t("logs.title")}
          </h1>
          <p className="mt-2 max-w-2xl text-sm leading-6 text-text-secondary">{t("logs.desc")}</p>
        </div>
        <div className="flex items-center gap-2">
          <Button type="button" variant="secondary" size="sm" onClick={() => { void loadDates(); void loadEntries(); }} disabled={loading}>
            <RefreshCw size={14} className={loading ? "animate-spin" : ""} />
            {t("logs.refresh")}
          </Button>
          <Button type="button" variant="secondary" size="sm" onClick={() => void copyLogs()} disabled={!entries.length}>
            {copied ? <Check size={14} className="text-success" /> : <Copy size={14} />}
            {copied ? t("logs.copied") : t("logs.copy")}
          </Button>
          <Button type="button" variant="danger" size="sm" onClick={() => setClearTarget({ taskId: taskId || undefined })} disabled={busy || !canClear}>
            <Trash2 size={14} />
            {taskId ? t("logs.clearTask") : t("logs.clearAll")}
          </Button>
        </div>
      </div>

      <Card className="border border-border-default p-4 sm:p-5">
        <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto_auto] md:items-end">
          <label className="block">
            <span className="mb-1.5 flex items-center gap-1.5 text-xs font-semibold text-text-secondary">
              <CalendarDays size={13} className="text-brand" />{t("logs.date")}
            </span>
            <select
              value={date}
              onChange={(event) => setDate(event.target.value)}
              className="surface-control h-10 w-full rounded-xl px-3 text-sm text-text-primary outline-none focus:border-brand/70"
              data-testid="logs-date"
            >
              {visibleDates.map((item) => <option key={item} value={item}>{item === today ? item + " · " + t("logs.today") : item}</option>)}
            </select>
          </label>
          <label className="block">
            <span className="mb-1.5 block text-xs font-semibold text-text-secondary">{t("logs.taskFilter")}</span>
            <select
              value={taskId}
              onChange={(event) => setTaskId(event.target.value)}
              className="surface-control h-10 w-full rounded-xl px-3 text-sm text-text-primary outline-none focus:border-brand/70"
              data-testid="logs-task"
            >
              <option value="">{t("logs.allTasks")}</option>
              {tasks.map((task) => <option key={task.id} value={task.id}>{task.media_name}</option>)}
            </select>
          </label>
          <label className="block">
            <span className="mb-1.5 block text-xs font-semibold text-text-secondary">{t("logs.limit")}</span>
            <select
              value={limit}
              onChange={(event) => setLimit(Number(event.target.value))}
              className="surface-control h-10 rounded-xl px-3 text-sm text-text-primary outline-none focus:border-brand/70"
              data-testid="logs-limit"
            >
              {[50, 100, 200].map((item) => <option key={item} value={item}>{item}</option>)}
            </select>
          </label>
          <div>
            <span className="mb-1.5 block text-xs font-semibold text-text-secondary">{t("logs.level")}</span>
            <div className="flex h-10 items-center gap-1 rounded-xl bg-surface-overlay p-1" role="group" aria-label={t("logs.level")}>
              {(["all", "error", "warn"] as LevelFilter[]).map((item) => (
                <button
                  key={item}
                  type="button"
                  onClick={() => setLevel(item)}
                  aria-pressed={level === item}
                  className={"h-8 rounded-lg px-2.5 text-xs font-semibold transition " + (level === item ? "bg-brand text-white shadow-sm" : "text-text-secondary hover:bg-surface-card hover:text-text-primary")}
                  data-testid={"logs-level-" + item}
                >
                  {levelLabels[item]}
                </button>
              ))}
            </div>
          </div>
        </div>
      </Card>

      {notice && (
        <div className="flex items-center justify-between gap-3 rounded-xl border border-border-default bg-surface-card px-3.5 py-2.5 text-sm text-text-secondary" role="status">
          <span>{notice}</span>
          <button type="button" onClick={() => setNotice(null)} className="text-text-tertiary hover:text-text-primary" aria-label={t("common.close")}>×</button>
        </div>
      )}

      <Card className="overflow-hidden border border-border-default" data-testid="logs-list">
        <div className="flex items-center justify-between border-b border-border-subtle px-4 py-3 sm:px-5">
          <div className="flex items-center gap-2">
            <CircleAlert size={16} className="text-brand" />
            <span className="text-sm font-semibold text-text-primary">{t("logs.entries")}</span>
          </div>
          <Badge variant="default">{entries.length}</Badge>
        </div>
        <div ref={logScroller} className="max-h-[min(60vh,42rem)] overflow-y-auto">
          {loading ? (
            <div className="px-5 py-14 text-center text-sm text-text-tertiary">{t("logs.loading")}</div>
          ) : entries.length === 0 ? (
            <div className="px-5 py-14 text-center text-sm text-text-tertiary">{t("logs.empty")}</div>
          ) : (
            <div className="divide-y divide-border-subtle" role="list">
              {entries.map((entry, index) => (
                <div key={entry.timestamp + "-" + index} role="listitem" className="grid gap-2 px-4 py-3 transition hover:bg-surface-overlay sm:grid-cols-[auto_7rem_minmax(0,1fr)_minmax(0,12rem)] sm:items-start sm:px-5">
                  <span className="mt-0.5">{levelIcon(entry.level)}</span>
                  <span className="font-mono text-[11px] text-text-tertiary">{formatTimestamp(entry.timestamp, locale)}</span>
                  <p className="min-w-0 whitespace-pre-wrap break-words text-sm leading-6 text-text-primary">{entry.message}</p>
                  <span className="truncate text-xs text-text-tertiary" title={entry.task_id ?? undefined}>
                    {entry.task_id ? taskNames.get(entry.task_id) ?? entry.task_id : t("logs.system")}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      </Card>

      {clearTarget && (
        <div
          className="fixed inset-0 z-[75] flex items-center justify-center bg-black/50 p-4 backdrop-blur-sm"
          role="dialog"
          aria-modal="true"
          aria-labelledby="clear-logs-title"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget && !busy) setClearTarget(null);
          }}
        >
          <Card className="w-full max-w-md border border-border-default bg-surface-raised p-5 shadow-2xl">
            <span className="grid h-10 w-10 place-items-center rounded-full bg-danger/10 text-danger">
              <Trash2 size={18} aria-hidden="true" />
            </span>
            <h2 id="clear-logs-title" className="mt-4 font-display text-h2 font-semibold text-text-primary">
              {clearTarget.taskId ? t("logs.clearTask") : t("logs.clearAll")}
            </h2>
            <p className="mt-2 text-sm leading-6 text-text-secondary">
              {clearTarget.taskId ? t("logs.clearTaskConfirm") : t("logs.clearAllConfirm")}
            </p>
            <div className="mt-5 flex justify-end gap-2">
              <Button type="button" variant="secondary" onClick={() => setClearTarget(null)} disabled={busy} autoFocus>
                {t("common.cancel")}
              </Button>
              <Button type="button" variant="danger" onClick={() => void handleClear()} disabled={busy} data-testid="logs-clear-confirm">
                <Trash2 size={14} aria-hidden="true" />
                {clearTarget.taskId ? t("logs.clearTask") : t("logs.clearAll")}
              </Button>
            </div>
          </Card>
        </div>
      )}
    </section>
  );
}
