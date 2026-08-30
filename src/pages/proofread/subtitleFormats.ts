/**
 * 字幕格式编解码模块（零依赖，纯函数）。
 *
 * 统一支持常见字幕格式的解析（导入）与序列化（导出）：
 *   - srt：SubRip，应用内部规范格式
 *   - vtt：WebVTT
 *   - ass/ssa：Advanced SubStation Alpha
 *   - lrc：歌词格式（仅有起始时间）
 *   - txt：纯文本（无时间轴，仅用于导出）
 */

export type SubtitleFormat = 'srt' | 'vtt' | 'ass' | 'lrc' | 'txt';

/** 播放器预览时的字幕轨道布局。导出文件仍保持原始格式与内容。 */
export type PlayerSubtitleLayout = 'auto' | 'source' | 'target' | 'bilingual';

export interface SubtitleCue {
  startMs: number;
  endMs: number;
  text: string; // 多行以 \n 连接
}

export interface SubtitleEntry {
  id: string;
  startEndTime: string;
  content: string[];
}

export const SUPPORTED_SUBTITLE_FORMATS: SubtitleFormat[] = [
  'srt',
  'vtt',
  'ass',
  'lrc',
  'txt',
];

export const IMPORTABLE_SUBTITLE_FORMATS: SubtitleFormat[] = [
  'srt',
  'vtt',
  'ass',
  'lrc',
];

const EXT_TO_FORMAT: Record<string, SubtitleFormat> = {
  '.srt': 'srt',
  '.vtt': 'vtt',
  '.ass': 'ass',
  '.ssa': 'ass',
  '.lrc': 'lrc',
  '.txt': 'txt',
};

const FORMAT_TO_EXT: Record<SubtitleFormat, string> = {
  srt: '.srt',
  vtt: '.vtt',
  ass: '.ass',
  lrc: '.lrc',
  txt: '.txt',
};

/** 根据文件路径/扩展名推断字幕格式，未知时回退为 srt。 */
export function detectSubtitleFormat(filePath: string): SubtitleFormat {
  const match = /\.[^.\\/]+$/.exec(filePath || '');
  const ext = match ? match[0].toLowerCase() : '';
  return EXT_TO_FORMAT[ext] || 'srt';
}

/** 获取某格式对应的文件扩展名（含点）。 */
export function getFormatExtension(format: SubtitleFormat): string {
  return FORMAT_TO_EXT[format] || '.srt';
}

export function isSupportedSubtitleFormat(
  format: string,
): format is SubtitleFormat {
  return (SUPPORTED_SUBTITLE_FORMATS as string[]).includes(format);
}

// ----------------------------- 时间处理 -----------------------------

function pad(n: number, len = 2): string {
  return String(Math.max(0, Math.floor(n))).padStart(len, '0');
}

interface TimeParts {
  h: number;
  m: number;
  s: number;
  ms: number;
}

function splitMs(input: number): TimeParts {
  let ms = Math.max(0, Math.round(input));
  const h = Math.floor(ms / 3600000);
  ms -= h * 3600000;
  const m = Math.floor(ms / 60000);
  ms -= m * 60000;
  const s = Math.floor(ms / 1000);
  ms -= s * 1000;
  return { h, m, s, ms };
}

/**
 * 将各种字幕时间字符串解析为毫秒。
 * 支持：HH:MM:SS,mmm | HH:MM:SS.mmm | H:MM:SS.cc(ASS 厘秒) | MM:SS.xx | [mm:ss.xx](LRC)
 */
export function parseTimeToMs(raw: string): number {
  if (!raw) return 0;
  let s = raw.trim();
  s = s.replace(/^\[/, '').replace(/\]$/, '');
  s = s.replace(',', '.');
  const parts = s.split(':');
  let h = 0;
  let m = 0;
  let sec = 0;
  if (parts.length === 3) {
    h = parseInt(parts[0], 10) || 0;
    m = parseInt(parts[1], 10) || 0;
    sec = parseFloat(parts[2]) || 0;
  } else if (parts.length === 2) {
    m = parseInt(parts[0], 10) || 0;
    sec = parseFloat(parts[1]) || 0;
  } else {
    sec = parseFloat(parts[0]) || 0;
  }
  return Math.round((h * 3600 + m * 60 + sec) * 1000);
}

export function formatSrtTime(ms: number): string {
  const t = splitMs(ms);
  return `${pad(t.h)}:${pad(t.m)}:${pad(t.s)},${pad(t.ms, 3)}`;
}

export function formatVttTime(ms: number): string {
  const t = splitMs(ms);
  return `${pad(t.h)}:${pad(t.m)}:${pad(t.s)}.${pad(t.ms, 3)}`;
}

export function formatAssTime(ms: number): string {
  const t = splitMs(ms);
  const cs = Math.floor(t.ms / 10);
  return `${t.h}:${pad(t.m)}:${pad(t.s)}.${pad(cs, 2)}`;
}

export function formatLrcTime(ms: number): string {
  const total = Math.max(0, Math.round(ms));
  const minutes = Math.floor(total / 60000);
  const seconds = Math.floor((total % 60000) / 1000);
  const cs = Math.floor((total % 1000) / 10);
  return `${pad(minutes)}:${pad(seconds)}.${pad(cs, 2)}`;
}

/** 解析 "HH:MM:SS,mmm --> HH:MM:SS,mmm" 形式的起止时间。 */
export function parseStartEndTime(startEndTime: string): {
  startMs: number;
  endMs: number;
} {
  const parts = (startEndTime || '').split('-->');
  return {
    startMs: parseTimeToMs(parts[0] || ''),
    endMs: parseTimeToMs(parts[1] || ''),
  };
}

/** 生成 SRT 风格的起止时间字符串。 */
export function toSrtTimeRange(startMs: number, endMs: number): string {
  return `${formatSrtTime(startMs)} --> ${formatSrtTime(endMs)}`;
}

// ----------------------------- 解析（导入） -----------------------------

const TIMING_LINE_REGEX = /-->/;

function stripBom(text: string): string {
  return text.charCodeAt(0) === 0xfeff ? text.slice(1) : text;
}

function normalizeLineEndings(text: string): string {
  return stripBom(text).replace(/\r\n?/g, '\n');
}

/** 解析 SRT / VTT。 */
function parseSrtVtt(content: string): SubtitleCue[] {
  let text = normalizeLineEndings(content);
  if (/^WEBVTT/.test(text)) {
    const firstBlank = text.indexOf('\n\n');
    text = firstBlank >= 0 ? text.slice(firstBlank + 2) : '';
  }
  const blocks = text.split(/\n{2,}/);
  const cues: SubtitleCue[] = [];
  for (const block of blocks) {
    const lines = block.split('\n').filter((l) => l.trim() !== '');
    if (lines.length === 0) continue;
    if (/^(NOTE|STYLE|REGION)\b/.test(lines[0])) continue;
    const timingIndex = lines.findIndex((l) => TIMING_LINE_REGEX.test(l));
    if (timingIndex === -1) continue;
    const timingLine = lines[timingIndex];
    const [startPart, endPartRaw] = timingLine.split('-->');
    if (endPartRaw === undefined) continue;
    const endPart = endPartRaw.trim().split(/\s+/)[0];
    const startMs = parseTimeToMs(startPart);
    const endMs = parseTimeToMs(endPart);
    const textLines = lines.slice(timingIndex + 1);
    if (textLines.length === 0) continue;
    cues.push({ startMs, endMs, text: textLines.join('\n') });
  }
  return cues;
}

/** 清理 ASS 文本中的覆盖标签与转义。 */
function cleanAssText(raw: string): string {
  return raw
    .replace(/\{[^}]*\}/g, '')
    .replace(/\\N/gi, '\n')
    .replace(/\\h/g, ' ')
    .trim();
}

function parseAss(content: string): SubtitleCue[] {
  const lines = normalizeLineEndings(content).split('\n');
  const cues: SubtitleCue[] = [];
  let inEvents = false;
  let formatFields: string[] = [];
  let idxStart = -1;
  let idxEnd = -1;
  let idxText = -1;

  for (const line of lines) {
    const trimmed = line.trim();
    if (/^\[.*\]$/.test(trimmed)) {
      inEvents = /^\[events\]$/i.test(trimmed);
      continue;
    }
    if (!inEvents) continue;

    if (/^Format\s*:/i.test(trimmed)) {
      formatFields = trimmed
        .slice(trimmed.indexOf(':') + 1)
        .split(',')
        .map((f) => f.trim().toLowerCase());
      idxStart = formatFields.indexOf('start');
      idxEnd = formatFields.indexOf('end');
      idxText = formatFields.indexOf('text');
      continue;
    }

    if (/^Dialogue\s*:/i.test(trimmed)) {
      if (idxText === -1) continue;
      const body = trimmed.slice(trimmed.indexOf(':') + 1);
      const parts = splitWithLimit(body, ',', formatFields.length);
      const startMs = parseTimeToMs(parts[idxStart] || '');
      const endMs = parseTimeToMs(parts[idxEnd] || '');
      const text = cleanAssText(parts[idxText] || '');
      if (!text) continue;
      cues.push({ startMs, endMs, text });
    }
  }
  return cues;
}

function splitWithLimit(str: string, sep: string, limit: number): string[] {
  if (limit <= 0) return [str];
  const result: string[] = [];
  let rest = str;
  for (let i = 0; i < limit - 1; i++) {
    const idx = rest.indexOf(sep);
    if (idx === -1) {
      result.push(rest);
      rest = '';
      return result;
    }
    result.push(rest.slice(0, idx));
    rest = rest.slice(idx + 1);
  }
  result.push(rest);
  return result;
}

function parseLrc(content: string): SubtitleCue[] {
  const lines = normalizeLineEndings(content).split('\n');
  const tagRegex = /\[(\d{1,3}):(\d{1,2}(?:[.:]\d{1,3})?)\]/g;
  let offsetMs = 0;
  const entries: { startMs: number; text: string }[] = [];

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const offsetMatch = /^\[offset:\s*([+-]?\d+)\s*\]$/i.exec(trimmed);
    if (offsetMatch) {
      offsetMs = parseInt(offsetMatch[1], 10) || 0;
      continue;
    }
    if (/^\[[a-z]+:.*\]$/i.test(trimmed)) continue;

    tagRegex.lastIndex = 0;
    const times: number[] = [];
    let m: RegExpExecArray | null;
    while ((m = tagRegex.exec(trimmed)) !== null) {
      const min = parseInt(m[1], 10) || 0;
      const sec = parseFloat(m[2].replace(':', '.')) || 0;
      times.push(min * 60000 + Math.round(sec * 1000));
    }
    if (times.length === 0) continue;
    const lyric = trimmed.replace(tagRegex, '').trim();
    for (const t of times) {
      entries.push({ startMs: t, text: lyric });
    }
  }

  entries.sort((a, b) => a.startMs - b.startMs);
  const cues: SubtitleCue[] = [];
  for (let i = 0; i < entries.length; i++) {
    const startMs = Math.max(0, entries[i].startMs + offsetMs);
    const endMs =
      i + 1 < entries.length
        ? Math.max(startMs, entries[i + 1].startMs + offsetMs)
        : startMs + 4000;
    if (entries[i].text === '') continue;
    cues.push({ startMs, endMs, text: entries[i].text });
  }
  return cues;
}

/** 将字幕内容解析为时间轴 cue 列表。 */
export function parseSubtitleCues(
  content: string,
  format: SubtitleFormat,
): SubtitleCue[] {
  switch (format) {
    case 'ass':
      return parseAss(content);
    case 'lrc':
      return parseLrc(content);
    case 'txt':
      return [];
    case 'srt':
    case 'vtt':
    default:
      return parseSrtVtt(content);
  }
}

/** 将字幕内容解析为应用内部的 SubtitleEntry 列表。 */
export function parseSubtitleEntries(
  content: string,
  format: SubtitleFormat,
): SubtitleEntry[] {
  const cues = parseSubtitleCues(content, format);
  return cues.map((cue, index) => ({
    id: String(index + 1),
    startEndTime: toSrtTimeRange(cue.startMs, cue.endMs),
    content: cue.text.split('\n'),
  }));
}

// ----------------------------- 序列化（导出） -----------------------------

const ASS_HEADER = `[Script Info]
ScriptType: v4.00+
Collisions: Normal
PlayResX: 1920
PlayResY: 1080
WrapStyle: 0
ScaledBorderAndShadow: yes

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Arial,72,&H00FFFFFF,&H000000FF,&H00000000,&H64000000,0,0,0,0,100,100,0,0,1,3,1,2,30,30,40,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
`;

export function getSubtitleFileHeader(format: SubtitleFormat): string {
  if (format === 'vtt') return 'WEBVTT\n\n';
  if (format === 'ass') return ASS_HEADER;
  return '';
}

export function serializeCue(
  cue: { id?: string; startMs: number; endMs: number; text: string },
  format: SubtitleFormat,
): string {
  const text = (cue.text || '').trim();
  switch (format) {
    case 'vtt':
      return `${formatVttTime(cue.startMs)} --> ${formatVttTime(cue.endMs)}\n${text}\n\n`;
    case 'ass':
      return `Dialogue: 0,${formatAssTime(cue.startMs)},${formatAssTime(
        cue.endMs,
      )},Default,,0,0,0,,${text.replace(/\n/g, '\\N')}\n`;
    case 'lrc':
      return `[${formatLrcTime(cue.startMs)}]${text.replace(/\n/g, ' ')}\n`;
    case 'txt':
      return `${text}\n\n`;
    case 'srt':
    default:
      return `${cue.id ?? ''}\n${formatSrtTime(cue.startMs)} --> ${formatSrtTime(
        cue.endMs,
      )}\n${text}\n\n`;
  }
}

export function serializeSubtitleCues(
  cues: SubtitleCue[],
  format: SubtitleFormat,
): string {
  const header = getSubtitleFileHeader(format);
  const body = cues
    .map((cue, index) =>
      serializeCue({ ...cue, id: String(index + 1) }, format),
    )
    .join('');
  return header + body;
}

export function serializeSubtitleEntries(
  entries: { id?: string; startEndTime: string; text: string }[],
  format: SubtitleFormat,
): string {
  const header = getSubtitleFileHeader(format);
  const body = entries
    .map((entry, index) => {
      const { startMs, endMs } = parseStartEndTime(entry.startEndTime);
      return serializeCue(
        { id: entry.id ?? String(index + 1), startMs, endMs, text: entry.text },
        format,
      );
    })
    .join('');
  return header + body;
}

export function convertSubtitleContent(
  content: string,
  fromFormat: SubtitleFormat,
  toFormat: SubtitleFormat,
): string {
  const cues = parseSubtitleCues(content, fromFormat);
  return serializeSubtitleCues(cues, toFormat);
}

// ----------------------------- 播放器布局 -----------------------------

// 不使用 Unicode property escape，确保 WebView/ES2020 运行时也能识别常见中日韩文字。
const CJK_CHAR_REGEX = /[\u2e80-\u2fff\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uac00-\ud7af\uf900-\ufaff]/;
const LATIN_CHAR_REGEX = /[A-Za-z]/;
const BILINGUAL_FILE_MARKER_REGEX = /(?:bilingual|双语)/i;

const PLAYER_SOURCE_LINE = 'line:78% position:50% size:90% align:center';
const PLAYER_TARGET_LINE = 'line:86% position:50% size:90% align:center';
const PLAYER_LATIN_MAX_LINE_CHARS = 42;
const PLAYER_CJK_MAX_LINE_CHARS = 16;
const PLAYER_MAX_EVENT_MS = 7000;

interface BilingualTextPair {
  source: string;
  target: string;
  sourceFirst: boolean;
}

function hasCjk(text: string): boolean {
  return CJK_CHAR_REGEX.test(text);
}

function hasLatin(text: string): boolean {
  return LATIN_CHAR_REGEX.test(text);
}

/**
 * 识别一条字幕中的英文/中文两块。
 *
 * 除了规范的两行形式，也兼容某些播放器/导出器把两块拼成一行的结果，
 * 例如 “English.中文”。这只用于播放器布局，不会改写用户导出的 SRT。
 */
function splitBilingualText(text: string): BilingualTextPair | null {
  const lines = text
    .replace(/\r\n?/g, '\n')
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean);
  if (lines.length < 2) {
    const onlyLine = lines[0] || '';
    const chars = [...onlyLine];
    const firstCjk = chars.findIndex((character) => CJK_CHAR_REGEX.test(character));
    const firstLatin = chars.findIndex((character) => LATIN_CHAR_REGEX.test(character));
    if (firstLatin >= 0 && firstCjk > firstLatin) {
      const source = chars.slice(0, firstCjk).join('').trim();
      const target = chars.slice(firstCjk).join('').trim();
      if (hasLatin(source) && hasCjk(target)) {
        return { source, target, sourceFirst: true };
      }
    }
    if (firstCjk >= 0 && firstLatin > firstCjk) {
      const target = chars.slice(0, firstLatin).join('').trim();
      const source = chars.slice(firstLatin).join('').trim();
      if (hasCjk(target) && hasLatin(source)) {
        return { source, target, sourceFirst: false };
      }
    }
    return null;
  }

  const scriptLines = lines.map((line) => ({
    line,
    cjk: hasCjk(line),
    latin: hasLatin(line),
  }));
  const cjkLines = scriptLines.filter(({ cjk }) => cjk).map(({ line }) => line);
  const latinLines = scriptLines
    .filter(({ cjk, latin }) => latin && !cjk)
    .map(({ line }) => line);
  if (cjkLines.length === 0 || latinLines.length === 0) return null;

  const firstLanguageLine = scriptLines.find(({ cjk, latin }) => cjk || latin);
  const sourceFirst = firstLanguageLine?.latin === true && firstLanguageLine.cjk !== true;
  return {
    source: latinLines.join('\n'),
    target: cjkLines.join('\n'),
    sourceFirst,
  };
}

/** 判断字幕文本是否包含可分离的英文/中文双语内容。 */
export function isBilingualSubtitleText(text: string): boolean {
  return splitBilingualText(text) !== null;
}

/** 判断字幕文件是否大概率是双语轨道（供 UI/播放器选择布局）。 */
export function looksLikeBilingualSubtitle(
  content: string,
  filePath = '',
  format: SubtitleFormat = detectSubtitleFormat(filePath),
): boolean {
  const cues = parseSubtitleCues(content, format);
  if (cues.length === 0) return false;
  const pairCount = cues.filter((cue) => splitBilingualText(cue.text) !== null).length;
  return pairCount > 0 && (BILINGUAL_FILE_MARKER_REGEX.test(filePath) || pairCount / cues.length >= 0.5);
}

function playerCueSettings(layout: PlayerSubtitleLayout): string {
  if (layout === 'source') return PLAYER_SOURCE_LINE;
  if (layout === 'target') return PLAYER_TARGET_LINE;
  return '';
}

function isPreferredPlayerBreak(character: string): boolean {
  return /\s/.test(character) || '。．.！!？?…，,；;、：:—-'.includes(character);
}

/** Netflix 行长限制：拉丁文字 42 字符，简体中文 16 字符。 */
function splitPlayerLines(text: string, maxCharacters: number): string[] {
  const output: string[] = [];
  text
    .replace(/\\N/g, '\n')
    .replace(/\r/g, '')
    .split('\n')
    .forEach((rawLine) => {
      const characters = [...rawLine.trim()];
      let start = 0;
      while (start < characters.length) {
        while (start < characters.length && /\s/.test(characters[start])) start += 1;
        if (start >= characters.length) break;
        const limit = Math.min(characters.length, start + maxCharacters);
        let end = limit;
        if (limit < characters.length) {
          for (let index = limit - 1; index >= start; index -= 1) {
            if (isPreferredPlayerBreak(characters[index])) {
              end = index + 1;
              break;
            }
          }
        }
        const line = characters.slice(start, end).join('').trim();
        if (line) output.push(line);
        start = Math.max(end, start + 1);
      }
    });
  return output.length > 0 ? output : [''];
}

interface PlayerBilingualSegment {
  startMs: number;
  endMs: number;
  top: string;
  bottom: string;
}

/**
 * 双语预览严格保持两行总上限：一行原文、一行译文。超过语言行长时，
 * 拆到后续时间段，而不是让浏览器把某种语言再自动折成两行。
 */
function splitPlayerBilingualCue(
  cue: SubtitleCue,
  pair: BilingualTextPair,
): PlayerBilingualSegment[] {
  const sourceLines = splitPlayerLines(
    pair.source,
    hasCjk(pair.source) ? PLAYER_CJK_MAX_LINE_CHARS : PLAYER_LATIN_MAX_LINE_CHARS,
  );
  const targetLines = splitPlayerLines(
    pair.target,
    hasCjk(pair.target) ? PLAYER_CJK_MAX_LINE_CHARS : PLAYER_LATIN_MAX_LINE_CHARS,
  );
  const duration = Math.max(1, cue.endMs - cue.startMs);
  const segmentCount = Math.max(
    sourceLines.length,
    targetLines.length,
    Math.ceil(duration / PLAYER_MAX_EVENT_MS),
    1,
  );
  const selectLine = (lines: string[], index: number): string => {
    const lineIndex = Math.min(
      lines.length - 1,
      Math.ceil(((index + 1) * lines.length) / segmentCount) - 1,
    );
    return lines[Math.max(0, lineIndex)] || '';
  };
  return Array.from({ length: segmentCount }, (_, index) => {
    const source = selectLine(sourceLines, index);
    const target = selectLine(targetLines, index);
    const startMs = cue.startMs + Math.floor((duration * index) / segmentCount);
    const endMs = index + 1 === segmentCount
      ? cue.endMs
      : cue.startMs + Math.floor((duration * (index + 1)) / segmentCount);
    return {
      startMs,
      endMs,
      top: pair.sourceFirst ? source : target,
      bottom: pair.sourceFirst ? target : source,
    };
  });
}

function serializePlayerCue(
  index: number,
  startMs: number,
  endMs: number,
  text: string,
  settings = '',
): string {
  const timing = `${formatVttTime(startMs)} --> ${formatVttTime(endMs)}${settings ? ` ${settings}` : ''}`;
  return `${index}\n${timing}\n${text.trim()}\n\n`;
}

/**
 * 为 HTMLVideoElement 的原生 WebVTT 轨道生成稳定布局。
 * 原始 SRT 的多行 cue 在不同 WebKit/播放器中可能被折叠成同一行；
 * 双语 cue 在这里拆成两个同时间段的 cue，并用 line/position 明确上下行。
 */
export function convertSubtitleContentForPlayer(
  content: string,
  fromFormat: SubtitleFormat,
  filePath = '',
  layout: PlayerSubtitleLayout = 'auto',
): string {
  const cues = parseSubtitleCues(content, fromFormat);
  const isMarkedBilingual = BILINGUAL_FILE_MARKER_REGEX.test(filePath);
  const output: string[] = [
    'WEBVTT\n\n',
    'STYLE\n::cue { font-size: min(3.2vh, 1.85vw); line-height: 1.15; }\n\n',
  ];
  let outputIndex = 1;

  cues.forEach((cue) => {
    const pair = splitBilingualText(cue.text);
    const genericPair = isMarkedBilingual
      ? cue.text
          .replace(/\r\n?/g, '\n')
          .split('\n')
          .map((line) => line.trim())
          .filter(Boolean)
      : [];
    const effectivePair = pair || (genericPair.length === 2
      ? { source: genericPair[0], target: genericPair[1], sourceFirst: true }
      : null);
    if (effectivePair && (layout === 'auto' || layout === 'bilingual')) {
      splitPlayerBilingualCue(cue, effectivePair).forEach((segment) => {
        output.push(
          serializePlayerCue(
            outputIndex++,
            segment.startMs,
            segment.endMs,
            segment.top,
            PLAYER_SOURCE_LINE,
          ),
        );
        output.push(
          serializePlayerCue(
            outputIndex++,
            segment.startMs,
            segment.endMs,
            segment.bottom,
            PLAYER_TARGET_LINE,
          ),
        );
      });
      return;
    }

    const selectedText = effectivePair
      ? layout === 'target'
        ? effectivePair.target
        : layout === 'source'
          ? effectivePair.source
          : effectivePair.sourceFirst
            ? `${effectivePair.source}\n${effectivePair.target}`
            : `${effectivePair.target}\n${effectivePair.source}`
      : cue.text;
    output.push(
      serializePlayerCue(
        outputIndex++,
        cue.startMs,
        cue.endMs,
        selectedText,
        playerCueSettings(layout),
      ),
    );
  });

  return output.join('');
}
