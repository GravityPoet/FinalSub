import { Outlet, Link, useLocation } from "react-router-dom";
import { Bot, Edit3, FileVideo2, Film, Languages, ListTodo, Settings } from "lucide-react";

const navItems = [
  { to: "/", label: "任务", icon: FileVideo2 },
  { to: "/tasks", label: "任务队列", icon: ListTodo },
  { to: "/models", label: "模型管理", icon: Bot },
  { to: "/translation", label: "翻译管理", icon: Languages },
  { to: "/proofread", label: "字幕校对", icon: Edit3 },
  { to: "/subtitle-merge", label: "视频合字幕", icon: Film },
  { to: "/settings", label: "设置", icon: Settings },
];

export default function Layout() {
  const location = useLocation();

  return (
    <div className="flex min-h-screen flex-col bg-gray-50 dark:bg-gray-900 sm:flex-row">
      <aside className="w-full shrink-0 border-b border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-800 sm:flex sm:h-screen sm:w-56 sm:flex-col sm:border-b-0 sm:border-r">
        <div className="border-b border-gray-200 p-4 dark:border-gray-700">
          <h1 className="text-lg font-bold text-gray-900 dark:text-white">FinalSub</h1>
          <p className="text-xs text-gray-500">Tauri 预览版</p>
        </div>
        <nav className="flex gap-2 overflow-x-auto p-2 sm:block sm:flex-1 sm:space-y-1">
          {navItems.map(({ to, label, icon: Icon }) => (
            <Link
              key={to}
              to={to}
              className={`flex shrink-0 items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors ${
                location.pathname === to
                  ? "bg-blue-50 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300"
                  : "text-gray-600 hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-700"
              }`}
            >
              <Icon size={18} />
              {label}
            </Link>
          ))}
        </nav>
      </aside>
      <main className="flex-1 overflow-auto p-4 sm:p-6">
        <Outlet />
      </main>
    </div>
  );
}
