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
import { useI18n } from '../../lib/i18n';

interface ProofreadImportProps {
  onImportComplete: (files: PendingFile[], type: 'video' | 'subtitle') => void;
}

export default function ProofreadImport({
  onImportComplete,
}: ProofreadImportProps) {
  const { t } = useI18n();
  const { showToast } = useToast();

  // 导入视频文件
  const handleImportVideos = useCallback(async () => {
    try {
      const selected = await open({
        multiple: true,
        directory: false,
        filters: [
          {
            name: t('proofread.import.videoFilterName'),
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
            name: t('proofread.import.subtitleFilterName'),
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
        showToast('error', t('proofread.import.noFilesFound'));
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
    <div className="mx-auto max-w-6xl space-y-8 py-8">
      <div className="text-center mb-10">
        <h2 className="font-display text-display font-bold tracking-tight text-text-primary">{t('proofread.import.selectMethod')}</h2>
        <p className="mx-auto mt-2.5 max-w-2xl text-sm leading-6 text-text-secondary">
          {t('proofread.import.selectMethodDesc')}
        </p>
      </div>

      <div className="grid grid-cols-1 gap-6 md:grid-cols-3">
        <div
          className="glass-panel group flex min-h-56 cursor-pointer flex-col items-center justify-center rounded-xl p-5 text-center transition-all duration-150 hover:border-brand hover:bg-surface-raised hover:shadow-brand-glow"
          onClick={handleImportVideos}
        >
          <div className="mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-brand-subtle transition-transform duration-150 group-hover:scale-105">
            <Video className="h-8 w-8 text-brand" />
          </div>
          <h3 className="mb-3 font-display text-h2 font-semibold text-text-primary">{t('proofread.import.importVideos')}</h3>
          <p className="text-sm leading-6 text-text-secondary">
            {t('proofread.import.importVideosDesc')}
          </p>
        </div>

        <div
          className="glass-panel group flex min-h-56 cursor-pointer flex-col items-center justify-center rounded-xl p-5 text-center transition-all duration-150 hover:border-brand hover:bg-surface-raised hover:shadow-brand-glow"
          onClick={handleImportSubtitles}
        >
          <div className="mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-success/10 transition-transform duration-150 group-hover:scale-105">
            <FileText className="h-8 w-8 text-success" />
          </div>
          <h3 className="mb-3 font-display text-h2 font-semibold text-text-primary">{t('proofread.import.importSubtitles')}</h3>
          <p className="text-sm leading-6 text-text-secondary">
            {t('proofread.import.importSubtitlesDesc')}
          </p>
        </div>

        <div
          className="glass-panel group flex min-h-56 cursor-pointer flex-col items-center justify-center rounded-xl p-5 text-center transition-all duration-150 hover:border-brand hover:bg-surface-raised hover:shadow-brand-glow"
          onClick={handleImportFolder}
        >
          <div className="mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-warning/10 transition-transform duration-150 group-hover:scale-105">
            <FolderOpen className="h-8 w-8 text-warning" />
          </div>
          <h3 className="mb-3 font-display text-h2 font-semibold text-text-primary">{t('proofread.import.importFolder')}</h3>
          <p className="text-sm leading-6 text-text-secondary">
            {t('proofread.import.importFolderDesc')}
          </p>
        </div>
      </div>
    </div>
  );
}
