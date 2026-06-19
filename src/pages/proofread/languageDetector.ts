/**
 * 语言代码检测器
 * 从文件名中自动检测语言代码
 */

// ISO 639-1 语言代码映射表
const LANGUAGE_MAP: Record<string, string> = {
  zh: '中文',
  en: '英语',
  ja: '日语',
  ko: '韩语',
  fr: '法语',
  de: '德语',
  es: '西班牙语',
  ru: '俄语',
  pt: '葡萄牙语',
  it: '意大利语',
  nl: '荷兰语',
  pl: '波兰语',
  tr: '土耳其语',
  sv: '瑞典语',
  cs: '捷克语',
  da: '丹麦语',
  fi: '芬兰语',
  el: '希腊语',
  hu: '匈牙利语',
  no: '挪威语',
  ro: '罗马尼亚语',
  sk: '斯洛伐克语',
  hr: '克罗地亚语',
  sr: '塞尔维亚语',
  sl: '斯洛文尼亚语',
  bg: '保加利亚语',
  uk: '乌克兰语',
  et: '爱沙尼亚语',
  lv: '拉脱维亚语',
  lt: '立陶宛语',
  hi: '印地语',
  th: '泰语',
  vi: '越南语',
  id: '印度尼西亚语',
  ms: '马来语',
  ta: '泰米尔语',
  ur: '乌尔都语',
  mr: '马拉地语',
  ar: '阿拉伯语',
  he: '希伯来语',
  fa: '波斯语',
  af: '阿非利堪斯语',
  ca: '加泰罗尼亚语',
  gl: '加利西亚语',
  tl: '塔加洛语',
  sw: '斯瓦希里语',
  cy: '威尔士语',
  mn: '蒙古语',
};

const LANGUAGE_ALIASES: Record<string, string> = {
  'zh-cn': 'zh',
  'zh-tw': 'zh',
  'zh-hk': 'zh',
  'zh-hans': 'zh',
  'zh-hant': 'zh',
  chs: 'zh',
  cht: 'zh',
  chi: 'zh',
  chinese: 'zh',
  cn: 'zh',
  'en-us': 'en',
  'en-gb': 'en',
  'en-au': 'en',
  eng: 'en',
  english: 'en',
  jpn: 'ja',
  jap: 'ja',
  japanese: 'ja',
  jp: 'ja',
  kor: 'ko',
  korean: 'ko',
  kr: 'ko',
  fra: 'fr',
  fre: 'fr',
  french: 'fr',
  ger: 'de',
  deu: 'de',
  german: 'de',
  spa: 'es',
  spanish: 'es',
  rus: 'ru',
  russian: 'ru',
  por: 'pt',
  'pt-br': 'pt',
  portuguese: 'pt',
  ita: 'it',
  italian: 'it',
};

const LANGUAGE_PATTERNS = [
  /\.([a-z]{2}(?:-[a-z]{2,4})?)\.(?:srt|vtt|ass|ssa|lrc)$/i,
  /_([a-z]{2,10})\.(?:srt|vtt|ass|ssa|lrc)$/i,
  /\[([a-z]{2,10})\]\.(?:srt|vtt|ass|ssa|lrc)$/i,
  /\(([a-z]{2,10})\)\.(?:srt|vtt|ass|ssa|lrc)$/i,
  /\.([a-z]{2,10})\.(?:srt|vtt|ass|ssa|lrc)$/i,
];

function getBasename(filePath: string): string {
  const parts = filePath.split('/');
  return parts[parts.length - 1] || '';
}

export function detectLanguageFromFilename(
  filePath: string,
): { code: string; name: string; confidence: number } | null {
  const fileName = getBasename(filePath).toLowerCase();

  for (const pattern of LANGUAGE_PATTERNS) {
    const match = fileName.match(pattern);
    if (match) {
      const detected = match[1].toLowerCase();
      const normalized = normalizeLanguageCode(detected);

      if (normalized && LANGUAGE_MAP[normalized]) {
        return {
          code: normalized,
          name: LANGUAGE_MAP[normalized],
          confidence: 90,
        };
      }
    }
  }

  return null;
}

export function normalizeLanguageCode(code: string): string | null {
  const lower = code.toLowerCase();
  if (LANGUAGE_MAP[lower]) {
    return lower;
  }
  if (LANGUAGE_ALIASES[lower]) {
    return LANGUAGE_ALIASES[lower];
  }
  const baseLang = lower.split('-')[0];
  if (LANGUAGE_MAP[baseLang]) {
    return baseLang;
  }
  return null;
}

export function getLanguageName(code: string): string {
  const normalized = normalizeLanguageCode(code);
  return normalized ? LANGUAGE_MAP[normalized] || code : code;
}

export function getSupportedLanguages(): Array<{ code: string; name: string }> {
  return Object.entries(LANGUAGE_MAP).map(([code, name]) => ({
    code,
    name,
  }));
}

export function detectLanguagePair(subtitleFiles: string[]): {
  source?: string;
  target?: string;
} {
  const languages: Array<{ file: string; lang: { code: string; name: string; confidence: number } }> = [];

  for (const file of subtitleFiles) {
    const detected = detectLanguageFromFilename(file);
    if (detected) {
      languages.push({ file, lang: detected });
    }
  }

  if (languages.length >= 2) {
    const enIndex = languages.findIndex((l) => l.lang.code === 'en');
    const zhIndex = languages.findIndex((l) => l.lang.code === 'zh');

    if (enIndex >= 0 && zhIndex >= 0) {
      return {
        source: 'en',
        target: 'zh',
      };
    }
    return {
      source: languages[0].lang.code,
      target: languages[1].lang.code,
    };
  }

  if (languages.length === 1) {
    return {
      source: languages[0].lang.code,
    };
  }

  return {};
}

export function isValidLanguageCode(code: string): boolean {
  return normalizeLanguageCode(code) !== null;
}
