import { formatAssTime, formatLrcTime, formatSrtTime, formatVttTime, parseStartEndTime, parseSubtitleEntries, type SubtitleFormat } from './subtitleFormats';

export interface DocumentEntry { id: string; originalId?: string; startEndTime: string; text: string }

// Keep override tags at their text boundaries when the surrounding words change.
function assText(raw: string, text: string): string {
  let plain = '';
  const tags: Array<{ at: number; value: string }> = [];
  for (const match of raw.matchAll(/\{[^}]*\}|\\[Nnh]|[^{}\\]+|./g)) {
    if (match[0].startsWith('{')) tags.push({ at: plain.length, value: match[0] });
    else plain += /^[\\][Nn]$/.test(match[0]) ? '\n' : match[0] === '\\h' ? ' ' : match[0];
  }
  const trimmed = plain.trim();
  if (trimmed === text) return raw;
  const leading = plain.length - plain.trimStart().length;
  let prefix = 0;
  while (prefix < trimmed.length && prefix < text.length && trimmed[prefix] === text[prefix]) prefix++;
  let suffix = 0;
  while (suffix < trimmed.length - prefix && suffix < text.length - prefix && trimmed[trimmed.length - suffix - 1] === text[text.length - suffix - 1]) suffix++;
  const map = (offset: number) => {
    const at = Math.max(0, Math.min(trimmed.length, offset - leading));
    if (at <= prefix) return at;
    if (at >= trimmed.length - suffix) return text.length - (trimmed.length - at);
    return prefix + Math.round((at - prefix) * (text.length - prefix - suffix) / Math.max(1, trimmed.length - prefix - suffix));
  };
  let result = text;
  for (const tag of [...tags].reverse()) {
    const at = map(tag.at);
    result = result.slice(0, at) + tag.value + result.slice(at);
  }
  return result.replace(/\n/g, '\\N');
}

/** Edit text/times while retaining the imported document's styles and metadata. */
export function editSubtitleDocument(original: string, format: SubtitleFormat, entries: DocumentEntry[]): string {
  const oldEntries = parseSubtitleEntries(original, format);
  if (oldEntries.length === entries.length && entries.every((entry, index) => entry.startEndTime === oldEntries[index].startEndTime && entry.text === oldEntries[index].content.join('\n'))) return original;
  const newline = original.includes('\r\n') ? '\r\n' : '\n';
  const bom = original.startsWith('\uFEFF') ? '\uFEFF' : '';
  const normalized = original.replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n');
  let output: string;
  if (format === 'ass') {
    const lines = normalized.split('\n');
    let inEvents = false;
    let fields: string[] = [];
    const events: Array<{ index: number; parts: string[]; prefix: string; fields: string[] }> = [];
    lines.forEach((line, index) => {
      if (/^\[.*\]\s*$/.test(line.trim())) inEvents = /^\[events\]\s*$/i.test(line.trim());
      if (!inEvents) return;
      if (/^Format\s*:/i.test(line.trim())) fields = line.slice(line.indexOf(':') + 1).split(',').map((field) => field.trim().toLowerCase());
      if (/^Dialogue\s*:/i.test(line.trim())) {
        if (fields.indexOf('text') !== fields.length - 1 || !fields.includes('start') || !fields.includes('end')) throw new Error('Unsupported ASS event format; export a new subtitle instead.');
        const prefix = line.slice(0, line.indexOf(':') + 1);
        const parts = line.slice(line.indexOf(':') + 1).split(',');
        const fixed = parts.slice(0, fields.length - 1);
        fixed.push(parts.slice(fields.length - 1).join(','));
        // Empty dialogue rows are not editor cues; preserve them in place.
        if (fixed[fields.length - 1].replace(/\{[^}]*\}/g, '').trim()) events.push({ index, parts: fixed, prefix, fields: [...fields] });
      }
    });
    if (events.length !== oldEntries.length || !events.length) throw new Error('Unable to preserve ASS structure; export a new subtitle instead.');
    const rendered = entries.map((entry) => {
      const template = events[Number(entry.originalId ?? entry.id) - 1] ?? events[0];
      const parts = [...template.parts];
      const times = parseStartEndTime(entry.startEndTime);
      parts[template.fields.indexOf('start')] = formatAssTime(times.startMs);
      parts[template.fields.indexOf('end')] = formatAssTime(times.endMs);
      const textAt = template.fields.indexOf('text');
      parts[textAt] = assText(parts[textAt], entry.text);
      return template.prefix + parts.join(',');
    });
    const eventLines = new Set(events.map((event) => event.index));
    output = lines.flatMap((line, index) => index === events[0].index ? rendered : eventLines.has(index) ? [] : [line]).join('\n');
  } else if (format === 'vtt' || format === 'srt') {
    const blocks = normalized.split(/\n{2,}/);
    const cueBlocks = blocks.map((block, index) => ({ index, lines: block.split('\n') })).filter(({ lines }) => !/^(NOTE|STYLE|REGION)\b/.test(lines[0]) && lines.some((line) => line.includes('-->')));
    if (cueBlocks.length !== oldEntries.length || !cueBlocks.length) throw new Error('Unable to preserve subtitle structure; export a new subtitle instead.');
    const rendered = entries.map((entry, index) => {
      const template = cueBlocks[Number(entry.originalId ?? entry.id) - 1] ?? cueBlocks[0];
      const timingIndex = template.lines.findIndex((line) => line.includes('-->'));
      const suffix = template.lines[timingIndex].split('-->')[1].trim().replace(/^\S+/, '');
      const times = parseStartEndTime(entry.startEndTime);
      const time = format === 'vtt' ? formatVttTime : formatSrtTime;
      const prefix = format === 'vtt' ? template.lines.slice(0, timingIndex) : [String(index + 1)];
      return [...prefix, `${time(times.startMs)} --> ${time(times.endMs)}${suffix}`, entry.text].join('\n');
    });
    const cueIndexes = new Set(cueBlocks.map((block) => block.index));
    output = blocks.flatMap((block, index) => index === cueBlocks[0].index ? rendered : cueIndexes.has(index) ? [] : [block]).join('\n\n');
  } else if (format === 'lrc') {
    const lines = normalized.split('\n');
    const offset = Number(normalized.match(/^\[offset:\s*([+-]?\d+)\s*\]$/im)?.[1] ?? 0);
    const isLyric = (line: string) => /\[\d{1,3}:\d{1,2}(?:[.:]\d{1,3})?\]/.test(line);
    const first = lines.findIndex(isLyric);
    if (first < 0) throw new Error('Unable to preserve LRC structure; export a new subtitle instead.');
    const rendered = entries.map((entry) => {
      const start = parseStartEndTime(entry.startEndTime).startMs - offset;
      if (start < 0) throw new Error('LRC offset cannot represent this time; export SRT instead.');
      return `[${formatLrcTime(start)}]${entry.text.replace(/\n/g, ' ')}`;
    });
    output = lines.flatMap((line, index) => index === first ? rendered : isLyric(line) ? [] : [line]).join('\n');
  } else {
    throw new Error('Original subtitle format cannot be edited safely. Export a new file instead.');
  }
  return bom + output.replace(/\n/g, newline);
}
