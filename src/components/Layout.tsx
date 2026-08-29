import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Outlet, Link, useLocation } from "react-router-dom";
import { AudioLines, Bot, ChevronDown, Edit3, FileVideo2, Film, Languages, ListTodo, ScrollText, Settings, PanelLeftClose, PanelLeftOpen, UserRound, Wrench } from "lucide-react";
import { useI18n } from "../lib/i18n";
import { ActivityCenter, CommandPalette, WorkspaceOverlays } from "./WorkspaceOverlays";
import { Card } from "./ui/Card";
import brandIcon from "../../src-tauri/icons/icon.png";

const navItems = [
  { to: "/", key: "nav.tasks", icon: FileVideo2 },
  { to: "/tasks", key: "nav.queue", icon: ListTodo },
  { to: "/models", key: "nav.models", icon: Bot },
  { to: "/translation", key: "nav.translation", icon: Languages },
  { to: "/voices", key: "nav.voices", icon: UserRound },
  { to: "/logs", key: "nav.logs", icon: ScrollText },
  { to: "/dubbing", key: "nav.dubbing", icon: AudioLines },
  { to: "/proofread", key: "nav.proofread", icon: Edit3 },
  { to: "/subtitle-merge", key: "nav.merge", icon: Film },
  { to: "/settings", key: "nav.settings", icon: Settings },
] as const;

const primaryNavItems = [navItems[0], navItems[1], navItems[7]] as const;
const toolNavItems = [navItems[6], navItems[8], navItems[2], navItems[3], navItems[4], navItems[5]] as const;
const settingsNavItem = navItems[9];
const mobilePrimaryNavItems = [navItems[0], navItems[1], navItems[7]] as const;
const mobileMoreNavItems = [navItems[6], navItems[8], navItems[2], navItems[3], navItems[4], navItems[5], navItems[9]] as const;

const Logo = () => <img className="brand-logo" src={brandIcon} alt="" aria-hidden="true" />;

export default function Layout() {
  const location = useLocation();
  const { t } = useI18n();
  const [collapsed, setCollapsed] = useState(() => localStorage.getItem("finalsub:nav-collapsed") === "true");
  const [toolsOpen, setToolsOpen] = useState(() => toolNavItems.some(({ to }) => location.pathname === to));
  const [mobileMoreOpen, setMobileMoreOpen] = useState(false);
  const mobileNavRef = useRef<HTMLElement>(null);

  useEffect(() => {
    const activeItem = mobileNavRef.current?.querySelector<HTMLElement>('[aria-current="page"]');
    activeItem?.scrollIntoView({ block: "nearest", inline: "center" });
    if (toolNavItems.some(({ to }) => location.pathname === to)) setToolsOpen(true);
    setMobileMoreOpen(false);
  }, [location.pathname]);

  useEffect(() => {
    if (!mobileMoreOpen) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMobileMoreOpen(false);
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [mobileMoreOpen]);

  const toggleCollapsed = () => {
    setCollapsed((value) => {
      const next = !value;
      localStorage.setItem("finalsub:nav-collapsed", String(next));
      return next;
    });
  };

  const renderNavLink = ({ to, key, icon: Icon }: (typeof navItems)[number], compact = false) => {
    const isActive = location.pathname === to;
    return (
      <Link
        key={to}
        to={to}
        aria-current={isActive ? "page" : undefined}
        className={`nav-item group relative flex shrink-0 items-center rounded-[0.9rem] text-[13px] font-semibold transition-all duration-200 ${collapsed ? "mx-auto h-11 min-h-11 w-11 justify-center p-0" : `${compact ? "min-h-9" : "min-h-11"} gap-3 px-2.5 py-2`} ${
          isActive
            ? "liquid-selected text-text-primary"
            : "text-text-secondary hover:bg-surface-overlay hover:text-text-primary"
        }`}
      >
        <span className={`nav-icon ${isActive ? "nav-icon-active" : ""}`}>
          <Icon size={16} />
        </span>
        <span className={collapsed ? "sr-only" : ""}>{t(key)}</span>
        {isActive && <span className="nav-active-rail" aria-hidden="true" />}
      </Link>
    );
  };

  return (
    <div className="app-frame flex min-h-screen flex-col text-text-primary sm:h-screen sm:overflow-hidden sm:flex-row sm:gap-3 sm:p-3">
      <div className="app-atmosphere" aria-hidden="true">
        <span className="ambient-orb ambient-orb-one" />
        <span className="ambient-orb ambient-orb-two" />
        <span className="ambient-orb ambient-orb-three" />
      </div>

      <aside
        className={`liquid-shell relative z-20 w-full shrink-0 border-x-0 border-t-0 sm:flex sm:h-[calc(100vh-1.5rem)] sm:flex-col sm:rounded-[1.75rem] sm:border ${collapsed ? "sm:w-[5.25rem]" : "sm:w-[16rem]"}`}
        data-sidebar-collapsed={collapsed}
      >
        <div
          className={`flex min-h-[4.5rem] items-center gap-3 border-b border-border-subtle px-4 py-3.5 sm:px-4.5 ${
            collapsed ? "sm:min-h-[6.75rem] sm:flex-col sm:justify-center sm:gap-1 sm:px-0 sm:py-2" : ""
          }`}
        >
          <Logo />
          <div className={`min-w-0 flex-1 ${collapsed ? "sm:hidden" : ""}`}>
            <h1 className="font-display text-[1.05rem] font-bold tracking-[-0.025em] text-text-primary">FinalSub</h1>
            <p className="mt-1 whitespace-nowrap text-[8.5px] font-semibold uppercase leading-none tracking-[0.11em] text-text-tertiary">
              Subtitle Studio
            </p>
          </div>
          <button
            type="button"
            onClick={toggleCollapsed}
            className={`sidebar-collapse-toggle hidden h-11 w-11 shrink-0 items-center justify-center rounded-xl text-text-tertiary transition hover:bg-surface-overlay hover:text-text-primary sm:flex ${collapsed ? "sm:mx-auto" : ""}`}
            aria-label={collapsed ? t("nav.expand") : t("nav.collapse")}
            title={collapsed ? t("nav.expand") : t("nav.collapse")}
          >
            {collapsed ? <PanelLeftOpen size={16} /> : <PanelLeftClose size={16} />}
          </button>
        </div>
        <div className={`absolute right-4 top-[1.1rem] z-[55] sm:static sm:block sm:border-b sm:border-border-subtle ${collapsed ? "sm:px-0 sm:py-2.5" : "sm:p-3"}`}>
          <ActivityCenter compact={collapsed} />
        </div>
        <nav className={`hidden gap-1 overflow-x-hidden p-2.5 sm:block sm:min-h-0 sm:flex-1 sm:space-y-1.5 ${collapsed ? "sidebar-nav-collapsed overflow-y-auto sm:px-0 sm:py-3" : "overflow-hidden sm:p-3"}`}>
          {collapsed ? navItems.map((item) => renderNavLink(item)) : (
            <>
              {primaryNavItems.map((item) => renderNavLink(item))}
              <details
                open={toolsOpen}
                onToggle={(event) => setToolsOpen(event.currentTarget.open)}
                className="group/tools pt-1"
              >
                <summary className={`flex min-h-10 cursor-pointer list-none items-center gap-3 rounded-[0.9rem] px-2.5 py-2 text-[13px] font-semibold transition hover:bg-surface-overlay hover:text-text-primary ${toolNavItems.some(({ to }) => location.pathname === to) ? "text-brand" : "text-text-secondary"}`}>
                  <span className="nav-icon"><Wrench size={16} /></span>
                  <span className="flex-1">{t("nav.tools")}</span>
                  <ChevronDown size={14} className="text-text-tertiary transition-transform group-open/tools:rotate-180" aria-hidden="true" />
                </summary>
                <div className="mt-1 space-y-0.5 border-l border-border-subtle pl-2">
                  {toolNavItems.map((item) => renderNavLink(item, true))}
                </div>
              </details>
              <div className="pt-1">{renderNavLink(settingsNavItem)}</div>
            </>
          )}
        </nav>
        <div className={`relative z-10 mt-auto hidden border-t border-border-subtle bg-surface-card sm:block ${collapsed ? "sm:px-0 sm:py-3.5" : "sm:p-3.5"}`}>
          <CommandPalette compact={collapsed} />
        </div>
      </aside>
      <main className="content-scroll relative z-10 min-h-0 min-w-0 flex-1 overflow-y-auto overflow-x-hidden px-4 pb-24 pt-5 sm:rounded-[1.75rem] sm:px-7 sm:py-6 lg:px-9">
        <div className="content-stage">
          <Outlet />
        </div>
      </main>
      {mobileMoreOpen && createPortal(
        <div className="fixed inset-0 z-[70] sm:hidden" role="presentation" onMouseDown={() => setMobileMoreOpen(false)}>
          <div className="absolute inset-0 bg-black/45 backdrop-blur-sm" />
          <Card role="dialog" aria-modal="true" aria-label={t("nav.mobileMore")} className="liquid-shell absolute inset-x-3 bottom-[5.75rem] max-h-[70vh] overflow-y-auto border border-border-strong p-3 shadow-2xl" onMouseDown={(event) => event.stopPropagation()}>
            <div className="flex items-center justify-between gap-3 px-1 pb-2">
              <div>
                <p className="text-xs font-bold uppercase tracking-[0.12em] text-brand">{t("nav.mobileMore")}</p>
                <p className="mt-1 text-xs text-text-tertiary">{t("home.advancedOptionsHint")}</p>
              </div>
              <button type="button" onClick={() => setMobileMoreOpen(false)} className="flex h-9 w-9 items-center justify-center rounded-xl text-text-tertiary hover:bg-surface-overlay hover:text-text-primary" aria-label={t("common.close")}>×</button>
            </div>
            <div className="grid grid-cols-2 gap-1.5 border-t border-border-subtle pt-2">
              {mobileMoreNavItems.map(({ to, key, icon: Icon }) => {
                const isActive = location.pathname === to;
                return (
                  <Link
                    key={to}
                    to={to}
                    aria-current={isActive ? "page" : undefined}
                    onClick={() => setMobileMoreOpen(false)}
                    className={`flex min-h-12 items-center gap-2 rounded-xl px-3 py-2 text-xs font-semibold transition ${isActive ? "liquid-selected text-text-primary" : "text-text-secondary hover:bg-surface-overlay hover:text-text-primary"}`}
                  >
                    <Icon size={16} className={isActive ? "text-brand" : "text-text-tertiary"} />
                    <span className="truncate">{t(key)}</span>
                  </Link>
                );
              })}
            </div>
          </Card>
        </div>,
        document.body,
      )}
      <nav ref={mobileNavRef} className="liquid-shell liquid-dock fixed inset-x-3 bottom-3 z-50 grid grid-cols-4 gap-1 rounded-[1.4rem] p-1.5 sm:hidden">
        {mobilePrimaryNavItems.map(({ to, key, icon: Icon }) => {
          const isActive = location.pathname === to;
          return (
            <Link
              key={to}
              to={to}
              aria-current={isActive ? "page" : undefined}
              className={`flex min-h-14 min-w-0 flex-col items-center justify-center gap-1 rounded-[1rem] px-2 text-[10px] font-semibold transition-all duration-200 ${
                isActive
                  ? "liquid-selected text-text-primary"
                  : "text-text-tertiary hover:bg-surface-overlay hover:text-text-primary"
              }`}
            >
              <Icon size={17} className={isActive ? "text-brand" : "text-text-tertiary"} />
              <span className="max-w-full truncate">{t(key)}</span>
            </Link>
          );
        })}
        <button
          type="button"
          onClick={() => setMobileMoreOpen(true)}
          aria-expanded={mobileMoreOpen}
          aria-haspopup="dialog"
          className={`flex min-h-14 min-w-0 flex-col items-center justify-center gap-1 rounded-[1rem] px-2 text-[10px] font-semibold transition-all duration-200 ${mobileMoreOpen || mobileMoreNavItems.some(({ to }) => location.pathname === to) ? "liquid-selected text-text-primary" : "text-text-tertiary hover:bg-surface-overlay hover:text-text-primary"}`}
        >
          <Wrench size={17} className={mobileMoreOpen || mobileMoreNavItems.some(({ to }) => location.pathname === to) ? "text-brand" : "text-text-tertiary"} />
          <span className="max-w-full truncate">{t("nav.mobileMore")}</span>
        </button>
      </nav>
      <WorkspaceOverlays />
    </div>
  );
}
