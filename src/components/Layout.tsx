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
      <aside className="w-full shrink-0 border-b border-border-subtle bg-surface sm:flex sm:h-screen sm:w-60 sm:flex-col sm:border-b-0 sm:border-r">
        <div className="flex items-center gap-3 border-b border-border-subtle p-4">
          <Logo />
          <h1 className="text-md font-bold tracking-tight text-text-primary">FinalSub</h1>
        </div>
        <nav className="flex gap-1 overflow-x-auto p-2 sm:block sm:flex-1 sm:space-y-1">
          {navItems.map(({ to, key, icon: Icon }) => {
            const isActive = location.pathname === to;
            return (
              <Link
                key={to}
                to={to}
                className={`relative flex shrink-0 items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium transition-all duration-150 ${
                  isActive
                    ? "bg-brand-subtle text-brand-text font-semibold before:absolute before:left-0 before:top-1/4 before:h-1/2 before:w-0.5 before:bg-brand before:rounded-full"
                    : "text-text-secondary hover:bg-surface-overlay hover:text-text-primary"
                }`}
              >
                <Icon size={16} className={isActive ? "text-brand" : "text-text-tertiary"} />
                <span>{t(key)}</span>
              </Link>
            );
          })}
        </nav>
        <div className="mt-auto space-y-2 border-t border-border-subtle p-3">
          <div className="flex items-center justify-between gap-2">
            <span className="text-xs text-text-secondary">{t("settings.theme")}</span>
            <span className="truncate text-xs font-semibold text-text-primary">
              {t(themeOptions.find((option) => option.value === theme)?.labelKey ?? "settings.themeDark")}
            </span>
          </div>
          <div className="grid grid-cols-3 rounded-lg border border-border-default bg-surface-raised p-0.5">
            {themeOptions.map(({ value, labelKey, icon: Icon }) => {
              const isActive = theme === value;
              const label = t(labelKey);
              return (
                <button
                  key={value}
                  type="button"
                  aria-pressed={isActive}
                  onClick={() => setTheme(value)}
                  className={`flex h-8 min-w-0 items-center justify-center gap-1.5 rounded-md px-1.5 text-xs font-medium transition ${
                    isActive
                      ? "bg-surface text-brand shadow-sm"
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
      <main className="min-h-0 min-w-0 flex-1 overflow-y-auto overflow-x-hidden p-4 sm:p-6">
        <Outlet />
      </main>
    </div>
  );
}
