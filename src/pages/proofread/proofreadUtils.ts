/**
 * 字幕校对相关的工具函数
 * 封装公共的字幕检测、创建 PendingFile 等逻辑
 */

import {
  detectSubtitlesForVideo,
  scanDirectoryForSubtitles,
} from './subtitleDetector';
import { detectLanguageFromFilename } from './languageDetector';
import { DetectedSubtitle, ProofreadItem } from './types';
import { authorizeSubtitleDirectory } from '../../lib/tauri';

// 生成 UUID
function generateUUID(): string {
  if (typeof crypto !== 'undefined' && crypto.randomUUID) {
    return crypto.randomUUID();
  }
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function (c) {
    const r = (Math.random() * 16) | 0;
    const v = c === 'x' ? r : (r & 0x3) | 0x8;
    return v.toString(16);
  });
}

// 待校对文件项
export interface PendingFile {
  id: string;
  videoPath?: string;
  fileName: string;
  detectedSubtitles: DetectedSubtitle[];
  selectedSource?: string;
  selectedTarget?: string;
  sourceLanguage?: string;
  targetLanguage?: string;
  status: 'pending' | 'proofreading' | 'completed';
  isSubtitleOnlyMode?: boolean; // 字幕导入模式，源字幕不可切换
}

// 支持的字幕类型
export type SubtitleType = 'source' | 'translated' | 'bilingual' | 'unknown';

/**
 * 从检测到的字幕列表中选择最佳的源字幕和翻译字幕
 */
export function selectBestSubtitles(
  detectedSubtitles: DetectedSubtitle[],
  excludeSource?: string,
): {
  bestSource: DetectedSubtitle | undefined;
  bestTarget: DetectedSubtitle | undefined;
} {
  // 源字幕优先选择 source 或 unknown 类型
  const sourceSubtitles = detectedSubtitles
    .filter((s) => s.type === 'source' || s.type === 'unknown')
    .sort((a, b) => b.confidence - a.confidence);

  // 翻译字幕优先选择 translated 类型
  const translatedSubtitles = detectedSubtitles
    .filter(
      (s) =>
        s.type === 'translated' &&
        (!excludeSource || s.filePath !== excludeSource),
    )
    .sort((a, b) => b.confidence - a.confidence);

  return {
    bestSource: sourceSubtitles[0],
    bestTarget: translatedSubtitles[0],
  };
}

/**
 * 尝试授权指定路径所在目录，使 plugin-fs 能读取该文件并扫描同目录字幕。
 * 失败（如敏感目录被拒）时仅降级，不抛出，由后续读取/检测的 try/catch 兜底。
 */
async function tryAuthorizeDir(path: string | undefined): Promise<void> {
  if (!path) return;
  try {
    await authorizeSubtitleDirectory(path);
  } catch (e) {
    console.warn('Failed to authorize subtitle directory, auto-detection may degrade:', e);
  }
}

/**
 * 从视频路径创建 PendingFile
 */
export async function createPendingFileFromVideo(
  videoPath: string,
): Promise<PendingFile> {
  // 先授权视频所在目录，确保 plugin-fs 可扫描同目录字幕
  await tryAuthorizeDir(videoPath);
  // 检测关联的字幕
  const detectResult = await detectSubtitlesForVideo(videoPath, '', '');
  const detectedSubtitles = detectResult.detectedSubtitles || [];

  const { bestSource, bestTarget } = selectBestSubtitles(detectedSubtitles);

  return {
    id: generateUUID(),
    videoPath,
    fileName: videoPath.split('/').pop() || '',
    detectedSubtitles,
    selectedSource: bestSource?.filePath,
    selectedTarget: bestTarget?.filePath,
    sourceLanguage: bestSource?.language,
    targetLanguage: bestTarget?.language,
    status: 'pending',
  };
}

/**
 * 从字幕文件路径创建 PendingFile
 * @param sourceFilePath 源字幕文件路径
 * @param detectRelated 是否检测关联字幕（同目录下的其他字幕）
 */
export async function createPendingFileFromSubtitle(
  sourceFilePath: string,
  detectRelated: boolean = true,
): Promise<PendingFile> {
  // 先授权字幕所在目录，确保 plugin-fs 可读取并扫描同目录字幕
  await tryAuthorizeDir(sourceFilePath);
  const sourceFileName = sourceFilePath.split('/').pop() || '';
  const sourceBaseName = sourceFileName.replace(/\.[^.]+$/, '');

  // 检测源字幕语言
  const langInfo = detectLanguageFromFilename(sourceFilePath);
  const sourceLanguage = langInfo?.code;

  let detectedSubtitles: DetectedSubtitle[] = [];

  if (detectRelated) {
    // 使用检测逻辑获取同目录下的相关字幕，通过伪造视频路径复用检测逻辑
    const fakeVideoPath = sourceFilePath.replace(/\.[^.]+$/, '.mp4');
    const detectResult = await detectSubtitlesForVideo(fakeVideoPath, '', '');
    if (detectResult && detectResult.detectedSubtitles) {
      detectedSubtitles = detectResult.detectedSubtitles;
    }
  }

  // 确保源字幕在列表中（标记为 source，置信度 100%）
  const sourceInList = detectedSubtitles.find(
    (s) => s.filePath === sourceFilePath,
  );
  if (!sourceInList) {
    detectedSubtitles.unshift({
      filePath: sourceFilePath,
      type: 'source',
      language: sourceLanguage,
      confidence: 100,
    });
  } else {
    // 更新源字幕信息
    sourceInList.type = 'source';
    sourceInList.confidence = 100;
  }

  // 找到置信度最高的翻译字幕（排除源字幕）
  const translatedSubtitles = detectedSubtitles
    .filter((s) => s.filePath !== sourceFilePath && s.type !== 'source')
    .sort((a, b) => b.confidence - a.confidence);

  const bestTranslated = translatedSubtitles[0];

  return {
    id: generateUUID(),
    fileName: sourceBaseName,
    detectedSubtitles,
    selectedSource: sourceFilePath,
    selectedTarget: bestTranslated?.filePath,
    sourceLanguage,
    targetLanguage: bestTranslated?.language,
    status: 'pending',
    isSubtitleOnlyMode: true, // 标记为字幕导入模式
  };
}

/**
 * 获取字幕文件同目录下的可用字幕列表
 * @param subtitlePath 字幕文件路径
 */
export async function getAvailableSubtitles(
  subtitlePath: string,
): Promise<DetectedSubtitle[]> {
  await tryAuthorizeDir(subtitlePath);
  const lastSlash = subtitlePath.lastIndexOf('/');
  const dir = lastSlash === -1 ? '.' : subtitlePath.substring(0, lastSlash);

  const scanResult = await scanDirectoryForSubtitles(dir);

  // 对每个字幕文件进行语言检测和置信度计算
  const detectedSubtitles = await Promise.all(
    scanResult.map(async (filePath: string) => {
      const langInfo = detectLanguageFromFilename(filePath);
      const lang = langInfo?.code;

      // 计算置信度：与源字幕同名的文件置信度更高
      const sourceName = subtitlePath
        .split('/')
        .pop()
        ?.replace(/\.[^.]+$/, '')
        .replace(/\.[a-z]{2}(?:-[a-z]{2,4})?$/i, '');
      const fileName = filePath
        .split('/')
        .pop()
        ?.replace(/\.[^.]+$/, '')
        .replace(/\.[a-z]{2}(?:-[a-z]{2,4})?$/i, '');
      const isRelated = sourceName === fileName;
      const confidence = isRelated ? 90 : 70;

      return {
        filePath,
        type: (filePath === subtitlePath
          ? 'source'
          : lang === 'en'
            ? 'source'
            : lang
              ? 'translated'
              : 'unknown') as SubtitleType,
        language: lang,
        confidence,
      };
    }),
  );

  return detectedSubtitles;
}

/**
 * 确保指定的字幕文件在列表中
 * @param subtitles 现有字幕列表
 * @param filePath 要确保存在的文件路径
 * @param type 字幕类型
 * @param language 语言代码
 * @returns 更新后的字幕列表
 */
export function ensureSubtitleInList(
  subtitles: DetectedSubtitle[],
  filePath: string | undefined,
  type: 'source' | 'translated',
  language?: string,
): DetectedSubtitle[] {
  if (!filePath) return subtitles;

  const exists = subtitles.some((s) => s.filePath === filePath);
  if (exists) return subtitles;

  return [
    ...subtitles,
    {
      filePath,
      type,
      language,
      confidence: 100, // 用户已选择的置信度设为最高
    },
  ];
}

/**
 * 从 ProofreadItem 加载 PendingFile（包括检测可用字幕）
 * @param item ProofreadItem 数据
 */
export async function loadPendingFileFromItem(item: ProofreadItem): Promise<PendingFile> {
  // 历史任务重新加载：运行时 scope 重启后已清空，重新授权已保存的文件路径所在目录
  await tryAuthorizeDir(item.videoPath);
  await tryAuthorizeDir(item.sourceSubtitlePath);
  await tryAuthorizeDir(item.targetSubtitlePath);

  let detectedSubtitles: DetectedSubtitle[] = [];
  const isSubtitleOnlyMode = !item.videoPath;

  // 如果任务中已保存了 detectedSubtitles，优先使用
  if (item.detectedSubtitles && item.detectedSubtitles.length > 0) {
    detectedSubtitles = item.detectedSubtitles.map((s) => ({
      filePath: s.filePath,
      type: s.type,
      language: s.language,
      confidence: s.confidence,
    }));
  } else {
    // 否则重新检测
    if (item.videoPath) {
      // 有视频：使用视频检测
      const detectResult = await detectSubtitlesForVideo(item.videoPath, '', '');
      detectedSubtitles = detectResult.detectedSubtitles || [];
    } else if (item.sourceSubtitlePath) {
      // 仅字幕：检测同目录下的其他字幕文件
      detectedSubtitles = await getAvailableSubtitles(item.sourceSubtitlePath);
    }
  }

  // 确保已选择的字幕在列表中
  detectedSubtitles = ensureSubtitleInList(
    detectedSubtitles,
    item.sourceSubtitlePath,
    'source',
    item.sourceLanguage,
  );
  detectedSubtitles = ensureSubtitleInList(
    detectedSubtitles,
    item.targetSubtitlePath,
    'translated',
    item.targetLanguage,
  );

  return {
    id: item.id,
    videoPath: item.videoPath,
    fileName: item.videoPath
      ? item.videoPath.split('/').pop() || ''
      : item.sourceSubtitlePath.split('/').pop() || '',
    detectedSubtitles,
    selectedSource: item.sourceSubtitlePath,
    selectedTarget: item.targetSubtitlePath,
    sourceLanguage: item.sourceLanguage,
    targetLanguage: item.targetLanguage,
    status: item.status === 'completed' ? 'completed' : 'pending',
    isSubtitleOnlyMode,
  };
}

/**
 * 将 PendingFile 转换为保存格式（用于创建/更新任务）
 */
export function pendingFileToSaveFormat(file: PendingFile): ProofreadItem {
  const itemStatus =
    file.status === 'completed'
      ? 'completed'
      : file.status === 'proofreading'
        ? 'in_progress'
        : 'pending';

  return {
    id: file.id,
    videoPath: file.videoPath,
    sourceSubtitlePath: file.selectedSource || '',
    targetSubtitlePath: file.selectedTarget,
    sourceLanguage: file.sourceLanguage,
    targetLanguage: file.targetLanguage,
    detectedSubtitles: file.detectedSubtitles.map((s) => ({
      filePath: s.filePath,
      type: s.type,
      language: s.language,
      confidence: s.confidence,
    })),
    status: itemStatus,
    lastPosition: 0,
    totalCount: 0,
    modifiedCount: 0,
  };
}
