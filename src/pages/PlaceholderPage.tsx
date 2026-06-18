import type { LucideIcon } from "lucide-react";

interface PlaceholderPageProps {
  title: string;
  icon: LucideIcon;
}

export default function PlaceholderPage({ title, icon: Icon }: PlaceholderPageProps) {
  return (
    <div className="max-w-4xl">
      <h2 className="mb-6 text-2xl font-bold text-gray-900 dark:text-white">{title}</h2>
      <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-700 dark:bg-gray-800">
        <div className="flex items-center gap-3">
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-300">
            <Icon size={20} />
          </div>
          <div>
            <h3 className="font-semibold text-gray-900 dark:text-white">待迁移</h3>
            <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
              当前 Tauri 预览版已保留入口，完整功能将在后续迁移阶段接入。
            </p>
          </div>
        </div>
      </section>
    </div>
  );
}
