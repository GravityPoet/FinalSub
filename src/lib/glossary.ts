import type { TranslationGlossary, TranslationGlossaryEntry } from "./tauri";

export interface GlossaryImportEntry {
  source: string;
  target: string;
  note: string;
}

export interface GlossaryConflict {
  source: string;
  keptGlossary: string;
  ignoredGlossary: string;
}

export function createGlossaryId(prefix: "glossary" | "entry"): string {
  const uuid = typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.random().toString(16).slice(2)}`;
  return `${prefix}-${uuid}`;
}

export function normalizeGlossaryOrder(glossaries: TranslationGlossary[]): TranslationGlossary[] {
  return glossaries
    .map((glossary, index) => ({ glossary, index }))
    .sort((left, right) => left.glossary.order - right.glossary.order || left.index - right.index)
    .map(({ glossary }, order) => ({ ...glossary, order }));
}

export function moveGlossary(
  glossaries: TranslationGlossary[],
  id: string,
  direction: -1 | 1,
): TranslationGlossary[] {
  const ordered = normalizeGlossaryOrder(glossaries);
  const index = ordered.findIndex((glossary) => glossary.id === id);
  const target = index + direction;
  if (index < 0 || target < 0 || target >= ordered.length) return ordered;
  [ordered[index], ordered[target]] = [ordered[target], ordered[index]];
  return ordered.map((glossary, order) => ({ ...glossary, order }));
}

export function glossarySourceKey(source: string): string {
  return Array.from(source.trim(), (character) => {
    const codePoint = character.codePointAt(0) ?? 0;
    if (codePoint === 0x3000) return " ";
    if (codePoint >= 0xff01 && codePoint <= 0xff5e) {
      return String.fromCodePoint(codePoint - 0xfee0);
    }
    return character;
  }).join("").toLowerCase();
}

export function findGlossaryConflicts(glossaries: TranslationGlossary[]): GlossaryConflict[] {
  const first = new Map<string, { source: string; glossary: string }>();
  const conflicts: GlossaryConflict[] = [];
  for (const glossary of normalizeGlossaryOrder(glossaries)) {
    if (!glossary.enabled) continue;
    for (const entry of glossary.entries) {
      const key = glossarySourceKey(entry.source);
      if (!key) continue;
      const kept = first.get(key);
      if (kept) {
        conflicts.push({
          source: entry.source,
          keptGlossary: kept.glossary,
          ignoredGlossary: glossary.name,
        });
      } else {
        first.set(key, { source: entry.source, glossary: glossary.name });
      }
    }
  }
  return conflicts;
}

function parseCsv(content: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let quoted = false;
  const pushRow = () => {
    row.push(field);
    if (row.some((value) => value.trim())) rows.push(row);
    row = [];
    field = "";
  };
  for (let index = 0; index < content.length; index += 1) {
    const char = content[index];
    if (char === '"') {
      if (quoted && content[index + 1] === '"') {
        field += '"';
        index += 1;
      } else {
        quoted = !quoted;
      }
    } else if (!quoted && char === ",") {
      row.push(field);
      field = "";
    } else if (!quoted && (char === "\n" || char === "\r")) {
      if (char === "\r" && content[index + 1] === "\n") index += 1;
      pushRow();
    } else {
      field += char;
    }
  }
  if (field || row.length) pushRow();
  return rows;
}

function headerType(value: string): "source" | "target" | "note" | null {
  const normalized = value.trim().toLowerCase();
  if (["source", "original", "term", "原文", "术语"].includes(normalized)) return "source";
  if (["target", "translation", "译文", "期望译文"].includes(normalized)) return "target";
  if (["note", "notes", "comment", "备注"].includes(normalized)) return "note";
  return null;
}

function rowsToEntries(rows: string[][]): GlossaryImportEntry[] {
  if (rows.length === 0) return [];
  const header = rows[0].map(headerType);
  const hasHeader = header.includes("source") && header.includes("target");
  const sourceIndex = hasHeader ? header.indexOf("source") : 0;
  const targetIndex = hasHeader ? header.indexOf("target") : 1;
  const noteIndex = hasHeader ? header.indexOf("note") : 2;
  return rows
    .slice(hasHeader ? 1 : 0)
    .map((row) => ({
      source: (row[sourceIndex] ?? "").trim().slice(0, 300),
      target: (row[targetIndex] ?? "").trim().slice(0, 600),
      note: noteIndex >= 0 ? (row[noteIndex] ?? "").trim().slice(0, 1000) : "",
    }))
    .filter((entry) => entry.source && entry.target)
    .slice(0, 10_000);
}

export function parseGlossaryEntries(content: string, format: "csv" | "txt"): GlossaryImportEntry[] {
  const normalized = content.replace(/^\uFEFF/, "");
  if (format === "csv") return rowsToEntries(parseCsv(normalized));
  const separators = ["=>", "->", "→", "="];
  const rows = normalized
    .split(/\r?\n/)
    .filter((line) => line.trim())
    .map((line) => {
      if (line.includes("\t")) return line.split("\t");
      const trimmed = line.trim();
      for (const separator of separators) {
        const index = trimmed.indexOf(separator);
        if (index > 0) return [trimmed.slice(0, index), trimmed.slice(index + separator.length)];
      }
      return [trimmed];
    });
  return rowsToEntries(rows);
}

function quoteCsv(value: string): string {
  return `"${value.replace(/"/g, '""')}"`;
}

export function serializeGlossaryEntries(
  entries: TranslationGlossaryEntry[],
  format: "csv" | "txt",
): string {
  if (format === "txt") {
    return [
      "source\ttarget\tnote",
      ...entries.map((entry) => [entry.source, entry.target, entry.note]
        .map((value) => value.replace(/[\t\r\n]+/g, " "))
        .join("\t")),
    ].join("\n");
  }
  return [
    "source,target,note",
    ...entries.map((entry) => [entry.source, entry.target, entry.note].map(quoteCsv).join(",")),
  ].join("\r\n");
}

export function mergeImportedEntries(
  current: TranslationGlossaryEntry[],
  incoming: GlossaryImportEntry[],
): { entries: TranslationGlossaryEntry[]; added: number; updated: number } {
  const entries = current.map((entry) => ({ ...entry }));
  const bySource = new Map(entries.map((entry, index) => [glossarySourceKey(entry.source), index]));
  let added = 0;
  let updated = 0;
  for (const item of incoming) {
    const key = glossarySourceKey(item.source);
    const existing = bySource.get(key);
    if (existing === undefined) {
      bySource.set(key, entries.length);
      entries.push({ id: createGlossaryId("entry"), ...item });
      added += 1;
    } else {
      entries[existing] = { ...entries[existing], ...item };
      updated += 1;
    }
  }
  return { entries, added, updated };
}
