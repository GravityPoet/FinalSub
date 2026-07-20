export interface SubtitlePairingResult {
  pairedByMedia: Map<string, string>;
  unpairedMedia: string[];
  unpairedSubtitles: string[];
}

function fileName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function fileStem(path: string): string {
  const name = fileName(path);
  const dot = name.lastIndexOf(".");
  return (dot > 0 ? name.slice(0, dot) : name).toLowerCase();
}

/**
 * Pair media with timed subtitle files. Explicit assignments win; media set to
 * an empty assignment deliberately falls back to ASR. Remaining media use a
 * deterministic exact-name match, then a language-suffix match such as
 * `episode.zh.srt` for `episode.mp4`.
 */
export function pairMediaWithSubtitles(
  mediaPaths: readonly string[],
  subtitlePaths: readonly string[],
  manualPairs: Readonly<Record<string, string>>,
): SubtitlePairingResult {
  const subtitleByPath = new Map(subtitlePaths.map((path) => [path, path]));
  const taken = new Set<string>();
  const pairedByMedia = new Map<string, string>();
  const autoMedia: string[] = [];

  for (const mediaPath of mediaPaths) {
    if (!Object.prototype.hasOwnProperty.call(manualPairs, mediaPath)) {
      autoMedia.push(mediaPath);
      continue;
    }
    const subtitlePath = manualPairs[mediaPath];
    if (!subtitlePath) continue;
    if (subtitleByPath.has(subtitlePath) && !taken.has(subtitlePath)) {
      taken.add(subtitlePath);
      pairedByMedia.set(mediaPath, subtitlePath);
    } else {
      autoMedia.push(mediaPath);
    }
  }

  const remainingSubtitles = subtitlePaths
    .filter((path) => !taken.has(path))
    .map((path) => ({ path, stem: fileStem(path) }))
    .sort((left, right) => left.stem.localeCompare(right.stem) || left.path.localeCompare(right.path));

  for (const mediaPath of autoMedia) {
    const stem = fileStem(mediaPath);
    const exactIndex = remainingSubtitles.findIndex((subtitle) => subtitle.stem === stem);
    const suffixIndex = exactIndex >= 0
      ? -1
      : remainingSubtitles.findIndex((subtitle) => subtitle.stem.startsWith(`${stem}.`));
    const matchIndex = exactIndex >= 0 ? exactIndex : suffixIndex;
    if (matchIndex < 0) continue;
    const [match] = remainingSubtitles.splice(matchIndex, 1);
    pairedByMedia.set(mediaPath, match.path);
    taken.add(match.path);
  }

  return {
    pairedByMedia,
    unpairedMedia: mediaPaths.filter((path) => !pairedByMedia.has(path)),
    unpairedSubtitles: subtitlePaths.filter((path) => !taken.has(path)),
  };
}
