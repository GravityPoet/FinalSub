/**
 * 字幕校对相关类型定义
 */

export type DetectedSubtitleType =
  | 'source'
  | 'translated'
  | 'bilingual'
  | 'unknown';

export interface DetectedSubtitle {
  type: DetectedSubtitleType;
  filePath: string;
  language?: string;
  confidence: number;
}

export interface SubtitleDetectionResult {
  videoFile: string;
  detectedSubtitles: DetectedSubtitle[];
}

export interface SubtitleMatchRule {
  id: string;
  name: string;
  sourcePattern: string;
  targetPattern: string;
  priority: number;
  isDefault?: boolean;
}

export interface SubtitleMatchResult {
  baseName: string;
  source?: string;
  target?: string;
  sourceLanguage?: string;
  targetLanguage?: string;
}

export interface ProofreadItem {
  id: string;
  videoPath?: string;
  sourceSubtitlePath: string;
  targetSubtitlePath?: string;
  sourceLanguage?: string;
  targetLanguage?: string;
  lastPosition?: number;
  totalCount?: number;
  modifiedCount?: number;
  status: 'pending' | 'in_progress' | 'completed';
  detectedSubtitles?: DetectedSubtitle[];
}

export interface ProofreadTask {
  id: string;
  name: string;
  createdAt: number;
  updatedAt: number;
  items: ProofreadItem[];
  currentItemIndex: number;
  status: 'in_progress' | 'completed';
}

export interface ProofreadHistory {
  id: string;
  createdAt: number;
  updatedAt: number;
  videoPath?: string;
  sourceSubtitlePath: string;
  targetSubtitlePath?: string;
  sourceLanguage: string;
  targetLanguage: string;
  lastPosition: number;
  modifiedCount: number;
  totalCount: number;
  status: 'in_progress' | 'completed';
  displayName?: string;
}

export interface StandaloneSubtitleConfig {
  videoPath?: string;
  sourceSubtitlePath: string;
  targetSubtitlePath?: string;
  sourceLanguage?: string;
  targetLanguage?: string;
}

export interface LanguageDetectionResult {
  code: string;
  name: string;
  confidence: number;
}
