import { useEffect, useState } from "react";
import { getAppInfo, getFfmpegVersion, type AppInfo } from "../lib/tauri";

export default function HomePage() {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [ffmpegVersion, setFfmpegVersion] = useState<string>("checking...");

  useEffect(() => {
    getAppInfo().then(setAppInfo).catch(console.error);
    getFfmpegVersion().then(setFfmpegVersion).catch(() => setFfmpegVersion("not found"));
  }, []);

  return (
    <div className="max-w-2xl">
      <h2 className="text-2xl font-bold text-gray-900 dark:text-white mb-6">Welcome</h2>

      <div className="bg-white dark:bg-gray-800 rounded-xl p-6 shadow-sm border border-gray-200 dark:border-gray-700">
        <h3 className="text-lg font-semibold mb-4">System Info</h3>
        <dl className="space-y-3">
          <div className="flex justify-between">
            <dt className="text-gray-500">App Name</dt>
            <dd className="font-mono text-sm">{appInfo?.name ?? "loading..."}</dd>
          </div>
          <div className="flex justify-between">
            <dt className="text-gray-500">Version</dt>
            <dd className="font-mono text-sm">{appInfo?.version ?? "loading..."}</dd>
          </div>
          <div className="flex justify-between">
            <dt className="text-gray-500">FFmpeg</dt>
            <dd className="font-mono text-sm">{ffmpegVersion}</dd>
          </div>
        </dl>
      </div>

      <div className="mt-6 bg-white dark:bg-gray-800 rounded-xl p-6 shadow-sm border border-gray-200 dark:border-gray-700">
        <h3 className="text-lg font-semibold mb-4">Quick Start</h3>
        <ol className="list-decimal list-inside space-y-2 text-gray-600 dark:text-gray-400 text-sm">
          <li>Go to <strong>Models</strong> and check available ASR engines</li>
          <li>Go to <strong>Tasks</strong> and import a media file</li>
          <li>Select an engine and model to generate subtitles</li>
        </ol>
      </div>
    </div>
  );
}
