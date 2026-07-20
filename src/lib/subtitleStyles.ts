import type { SubtitleStyle } from "./tauri";
import type { TranslationKey } from "./i18n";

export interface BuiltInSubtitleStylePreset {
  id: string;
  nameKey: TranslationKey;
  style: SubtitleStyle;
}

export const DEFAULT_SUBTITLE_STYLE: SubtitleStyle = {
  font_name: "PingFang SC",
  font_size: 24,
  font_color: "&H00FFFFFF",
  outline_color: "&H00000000",
  outline_width: 2,
  shadow: 0,
  background_color: "&H80000000",
  opaque_background: false,
  alignment: 2,
  margin_v: 30,
};

export const BUILT_IN_SUBTITLE_STYLE_PRESETS: BuiltInSubtitleStylePreset[] = [
  {
    id: "builtin:classic",
    nameKey: "merge.style.classic",
    style: { ...DEFAULT_SUBTITLE_STYLE },
  },
  {
    id: "builtin:movie",
    nameKey: "merge.style.movie",
    style: {
      ...DEFAULT_SUBTITLE_STYLE,
      font_size: 28,
      outline_width: 2.5,
      shadow: 0.8,
      margin_v: 46,
    },
  },
  {
    id: "builtin:youtube",
    nameKey: "merge.style.youtube",
    style: {
      ...DEFAULT_SUBTITLE_STYLE,
      font_size: 32,
      font_color: "&H0000FFFF",
      outline_width: 4,
      shadow: 1.5,
      margin_v: 38,
    },
  },
  {
    id: "builtin:minimal",
    nameKey: "merge.style.minimal",
    style: {
      ...DEFAULT_SUBTITLE_STYLE,
      font_size: 22,
      outline_color: "&H80000000",
      outline_width: 1,
      margin_v: 26,
    },
  },
  {
    id: "builtin:bold",
    nameKey: "merge.style.bold",
    style: {
      ...DEFAULT_SUBTITLE_STYLE,
      font_size: 36,
      outline_width: 4,
      shadow: 2,
      margin_v: 36,
    },
  },
  {
    id: "builtin:boxed",
    nameKey: "merge.style.boxed",
    style: {
      ...DEFAULT_SUBTITLE_STYLE,
      font_size: 28,
      outline_width: 0,
      background_color: "&H60000000",
      opaque_background: true,
      margin_v: 34,
    },
  },
];

export function assColorToCss(assColor: string): string {
  if (!assColor) return "rgb(255, 255, 255)";
  const cleanColor = assColor.trim().toUpperCase();
  const match = cleanColor.match(/^&H([0-9A-F]{2})([0-9A-F]{2})([0-9A-F]{2})([0-9A-F]{2})$/);
  if (match) {
    const alpha = (1 - parseInt(match[1], 16) / 255).toFixed(2);
    return `rgba(${parseInt(match[4], 16)}, ${parseInt(match[3], 16)}, ${parseInt(match[2], 16)}, ${alpha})`;
  }
  const matchNoAlpha = cleanColor.match(/^&H([0-9A-F]{2})([0-9A-F]{2})([0-9A-F]{2})$/);
  if (matchNoAlpha) {
    return `rgb(${parseInt(matchNoAlpha[3], 16)}, ${parseInt(matchNoAlpha[2], 16)}, ${parseInt(matchNoAlpha[1], 16)})`;
  }
  return "rgb(255, 255, 255)";
}

export function subtitleStylesEqual(left: SubtitleStyle, right: SubtitleStyle): boolean {
  return left.font_name === right.font_name
    && left.font_size === right.font_size
    && left.font_color === right.font_color
    && left.outline_color === right.outline_color
    && left.outline_width === right.outline_width
    && left.shadow === right.shadow
    && left.background_color === right.background_color
    && left.opaque_background === right.opaque_background
    && left.alignment === right.alignment
    && left.margin_v === right.margin_v;
}
