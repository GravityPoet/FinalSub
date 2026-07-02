import { Outlet, Link, useLocation } from "react-router-dom";
import { Bot, Edit3, FileVideo2, Film, Languages, ListTodo, Settings, Sun, Moon, Laptop } from "lucide-react";
import { type TranslationKey, useI18n } from "../lib/i18n";
import { type Theme, useTheme } from "../lib/theme";

const navItems = [
  { to: "/", key: "nav.tasks", icon: FileVideo2 },
  { to: "/tasks", key: "nav.queue", icon: ListTodo },
  { to: "/models", key: "nav.models", icon: Bot },
  { to: "/translation", key: "nav.translation", icon: Languages },
  { to: "/proofread", key: "nav.proofread", icon: Edit3 },
  { to: "/subtitle-merge", key: "nav.merge", icon: Film },
  { to: "/settings", key: "nav.settings", icon: Settings },
] as const;

const themeOptions: Array<{
  value: Theme;
  labelKey: TranslationKey;
  icon: typeof Sun;
}> = [
  { value: "light", labelKey: "settings.themeLight", icon: Sun },
  { value: "dark", labelKey: "settings.themeDark", icon: Moon },
  { value: "system", labelKey: "settings.themeSystem", icon: Laptop },
];

const Logo = () => (
  <svg className="size-5 text-brand" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
    <path d="M12 2L2 7L12 12L22 7L12 2Z" fill="currentColor" opacity="0.85" />
    <path d="M2 17L12 22L22 17" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
    <path d="M2 12L12 17L22 12" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);

export default function Layout() {
  const location = useLocation();
  const { t } = useI18n();
  const { theme, setTheme } = useTheme();

  return (
    <div className="flex min-h-screen flex-col bg-app-bg text-text-primary sm:h-screen sm:overflow-hidden sm:flex-row">
      <aside className="glass-panel w-full shrink-0 rounded-none border-x-0 border-t-0 sm:flex sm:h-screen sm:w-[16rem] sm:flex-col sm:border-b-0 sm:border-l-0">
        <div className="flex min-h-14 items-center gap-3 border-b border-border-subtle px-4 py-3">
          <Logo />
          <h1 className="font-display text-base font-bold tracking-tight text-text-primary">FinalSub</h1>
        </div>
        <nav className="flex gap-1 overflow-x-auto p-2.5 sm:block sm:flex-1 sm:space-y-1 sm:p-3">
          {navItems.map(({ to, key, icon: Icon }) => {
            const isActive = location.pathname === to;
            return (
              <Link
                key={to}
                to={to}
                className={`relative flex min-h-10 shrink-0 items-center gap-3 rounded-xl px-3 py-2 text-[14px] font-semibold transition-all duration-150 ${
                  isActive
                    ? "liquid-selected text-brand-text before:absolute before:left-0 before:top-1/4 before:h-1/2 before:w-1 before:rounded-full before:bg-brand"
                    : "text-text-secondary hover:bg-surface-overlay hover:text-text-primary"
                }`}
              >
                <Icon size={18} className={isActive ? "text-brand" : "text-text-tertiary"} />
                <span>{t(key)}</span>
              </Link>
            );
          })}
        </nav>
        <div className="mt-auto space-y-2.5 border-t border-border-subtle p-3.5">
          <div className="flex items-center justify-between gap-2">
            <span className="text-sm text-text-secondary">{t("settings.theme")}</span>
            <span className="truncate text-sm font-semibold text-text-primary">
              {t(themeOptions.find((option) => option.value === theme)?.labelKey ?? "settings.themeDark")}
            </span>
          </div>
          <div className="glass-control grid grid-cols-3 rounded-xl p-1">
            {themeOptions.map(({ value, labelKey, icon: Icon }) => {
              const isActive = theme === value;
              const label = t(labelKey);
              return (
                <button
                  key={value}
                  type="button"
                  aria-pressed={isActive}
                  onClick={() => setTheme(value)}
                  className={`flex h-9 min-w-0 items-center justify-center gap-1.5 rounded-lg px-2 text-sm font-semibold transition ${
                    isActive
                      ? "bg-surface-raised text-brand shadow-sm"
                      : "text-text-tertiary hover:text-text-secondary"
                  }`}
                  title={label}
                >
                  <Icon size={14} className="shrink-0" />
                  <span className="truncate">{label}</span>
                </button>
              );
            })}
          </div>
        </div>
      </aside>
      <main className="min-h-0 min-w-0 flex-1 overflow-y-auto overflow-x-hidden px-5 py-5 sm:px-7 sm:py-6">
        <Outlet />
      </main>
    </div>
  );
}
