import { useCallback } from 'react';
import { Video, FileText, FolderOpen } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { useToast } from './Toast';
import {
  PendingFile,
  createPendingFileFromVideo,
  createPendingFileFromSubtitle,
} from './proofreadUtils';
import { DetectedSubtitle } from './types';
import {
  smartScanDirectory,
  matchSubtitlesByRules,
} from './subtitleDetector';
import { detectLanguageFromFilename } from './languageDetector';

interface ProofreadImportProps {
  onImportComplete: (files: PendingFile[], type: 'video' | 'subtitle') => void;
}

export default function ProofreadImport({
  onImportComplete,
}: ProofreadImportProps) {
  const { showToast } = useToast();

  // 导入视频文件
  const handleImportVideos = useCallback(async () => {
    try {
      const selected = await open({
        multiple: true,
        directory: false,
        filters: [
          {
            name: '视频文件',
            extensions: ['mp4', 'avi', 'mov', 'mkv', 'flv', 'wmv', 'webm', '3gp', 'ts', 'm4v'],
          },
        ],
      });

      if (!selected) return;
      const filePaths = Array.isArray(selected) ? selected : [selected];

      const files = await Promise.all(
        filePaths.map((videoPath: string) =>
          createPendingFileFromVideo(videoPath),
        ),
      );

      if (files.length > 0) {
        onImportComplete(files, 'video');
      }
    } catch (error) {
      console.error('Failed to import videos:', error);
    }
  }, [onImportComplete]);

  // 导入字幕文件
  const handleImportSubtitles = useCallback(async () => {
    try {
      const selected = await open({
        multiple: true,
        directory: false,
        filters: [
          {
            name: '字幕文件',
            extensions: ['srt', 'vtt', 'ass', 'ssa', 'lrc'],
          },
        ],
      });

      if (!selected) return;
      const filePaths = Array.isArray(selected) ? selected : [selected];

      const files = await Promise.all(
        filePaths.map((filePath: string) =>
          createPendingFileFromSubtitle(filePath),
        ),
      );

      if (files.length > 0) {
        onImportComplete(files, 'subtitle');
      }
    } catch (error) {
      console.error('Failed to import subtitles:', error);
    }
  }, [onImportComplete]);

  // 导入文件夹（智能检测）
  const handleImportFolder = useCallback(async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });
      if (!selected || Array.isArray(selected)) return;
      const directoryPath = selected;

      // 智能扫描目录
      const scanResult = await smartScanDirectory(directoryPath);
      const { videos, subtitles } = scanResult;

      if (videos.length === 0 && subtitles.length === 0) {
        showToast('error', '未找到视频或字幕文件');
        return;
      }

      // 智能检测：如果有视频，按视频模式处理
      if (videos.length > 0) {
        const files = await Promise.all(
          videos.map((videoPath: string) =>
            createPendingFileFromVideo(videoPath),
          ),
        );

        if (files.length > 0) {
          onImportComplete(files, 'video');
        }
      } else {
        // 没有视频，按字幕模式处理
        const allSubtitles: DetectedSubtitle[] = [];

        for (const filePath of subtitles) {
          const langInfo = detectLanguageFromFilename(filePath);
          const lang = langInfo?.code;
          const type =
            lang === 'en' ? 'source' : lang ? 'translated' : 'unknown';
          allSubtitles.push({
            filePath,
            type: type as 'source' | 'translated' | 'unknown',
            language: lang,
            confidence: lang ? 90 : 80,
          });
        }

        // 匹配字幕对
        const matches = await matchSubtitlesByRules(subtitles);

        const files: PendingFile[] = [];

        for (const match of matches) {
          if (match.source) {
            const baseName = match.baseName.toLowerCase();
            const relatedSubtitles = allSubtitles.filter((s) => {
              const fileName = s.filePath.split('/').pop()?.toLowerCase() || '';
              return (
                fileName.includes(baseName) ||
                baseName.includes(fileName.replace(/\.[^.]+$/, ''))
              );
            });

            // 简单防重复随机生成 ID
            const randomId = Math.random().toString(36).substring(2, 11);

            files.push({
              id: randomId,
              fileName: match.baseName,
              detectedSubtitles:
                relatedSubtitles.length > 0
                  ? relatedSubtitles
                  : [
                      {
                        filePath: match.source,
                        type: 'source' as const,
                        language: match.sourceLanguage,
                        confidence: 90,
                      },
                      ...(match.target
                        ? [
                            {
                              filePath: match.target,
                              type: 'translated' as const,
                              language: match.targetLanguage,
                              confidence: 90,
                            },
                          ]
                        : []),
                    ],
              selectedSource: match.source,
              selectedTarget: match.target,
              sourceLanguage: match.sourceLanguage,
              targetLanguage: match.targetLanguage,
              status: 'pending',
            });
          }
        }

        if (files.length > 0) {
          onImportComplete(files, 'subtitle');
        }
      }
    } catch (error) {
      console.error('Failed to import folder:', error);
    }
  }, [onImportComplete]);

  return (
    <div className="space-y-8 max-w-4xl mx-auto py-10">
      <div className="text-center mb-10">
        <h2 className="text-2xl font-bold mb-3 text-slate-100">选择导入方式</h2>
        <p className="text-slate-400 text-sm">
          支持导入视频或字幕文件开始批量字幕校对与双语合并
        </p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
        <div
          className="group cursor-pointer bg-slate-800/50 hover:bg-slate-800/80 transition-all border border-slate-700/60 hover:border-blue-500/50 rounded-xl p-6 flex flex-col items-center text-center shadow-lg hover:shadow-blue-500/5"
          onClick={handleImportVideos}
        >
          <div className="w-14 h-14 bg-blue-600/10 rounded-full flex items-center justify-center mb-4 group-hover:scale-110 transition-transform">
            <Video className="w-7 h-7 text-blue-500" />
          </div>
          <h3 className="text-lg font-semibold text-slate-200 mb-2">导入视频</h3>
          <p className="text-xs text-slate-400 leading-relaxed">
            导入视频文件，自动检测同目录下文件名关联的字幕文件
          </p>
        </div>

        <div
          className="group cursor-pointer bg-slate-800/50 hover:bg-slate-800/80 transition-all border border-slate-700/60 hover:border-blue-500/50 rounded-xl p-6 flex flex-col items-center text-center shadow-lg hover:shadow-blue-500/5"
          onClick={handleImportSubtitles}
        >
          <div className="w-14 h-14 bg-emerald-600/10 rounded-full flex items-center justify-center mb-4 group-hover:scale-110 transition-transform">
            <FileText className="w-7 h-7 text-emerald-500" />
          </div>
          <h3 className="text-lg font-semibold text-slate-200 mb-2">导入字幕</h3>
          <p className="text-xs text-slate-400 leading-relaxed">
            直接导入已有的字幕文件（如 .srt / .vtt / .ass），快速进行校对与编辑
          </p>
        </div>

        <div
          className="group cursor-pointer bg-slate-800/50 hover:bg-slate-800/80 transition-all border border-slate-700/60 hover:border-blue-500/50 rounded-xl p-6 flex flex-col items-center text-center shadow-lg hover:shadow-blue-500/5"
          onClick={handleImportFolder}
        >
          <div className="w-14 h-14 bg-amber-600/10 rounded-full flex items-center justify-center mb-4 group-hover:scale-110 transition-transform">
            <FolderOpen className="w-7 h-7 text-amber-500" />
          </div>
          <h3 className="text-lg font-semibold text-slate-200 mb-2">导入文件夹</h3>
          <p className="text-xs text-slate-400 leading-relaxed">
            批量导入选定文件夹下的所有视频和字幕，智能分类和匹配
          </p>
        </div>
      </div>
    </div>
  );
}
