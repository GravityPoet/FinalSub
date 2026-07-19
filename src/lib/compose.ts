export type ComposeAudioMode = "replace" | "mix" | "add-track";

export function composeRequiresMkv(
  softSubtitle: boolean,
  audioPath: string,
  audioMode: ComposeAudioMode,
): boolean {
  return softSubtitle || (!!audioPath && audioMode === "add-track");
}

export function replaceMediaExtension(path: string, extension: "mp4" | "mkv"): string {
  if (!path) return path;
  const withoutExtension = path.replace(/\.[^./\\]+$/, "");
  return `${withoutExtension}.${extension}`;
}

export function defaultComposeOutputPath(
  videoPath: string,
  requiresMkv: boolean,
): string {
  const extension = requiresMkv ? "mkv" : "mp4";
  if (!videoPath) return `output.${extension}`;
  return `${videoPath.replace(/\.[^./\\]+$/, "")}-final.${extension}`;
}
