import test, { after } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import ts from 'typescript';

const temp = await fs.mkdtemp(path.join(os.tmpdir(), 'finalsub-document-tests-'));
after(() => fs.rm(temp, { recursive: true, force: true }));
for (const name of ['subtitleFormats', 'subtitleDocument']) {
  const source = await fs.readFile(new URL(`../src/pages/proofread/${name}.ts`, import.meta.url), 'utf8');
  const compiled = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext } }).outputText.replace("'./subtitleFormats'", "'./subtitleFormats.mjs'");
  await fs.writeFile(path.join(temp, `${name}.mjs`), compiled);
}
const { editSubtitleDocument } = await import(pathToFileURL(path.join(temp, 'subtitleDocument.mjs')));
const { parseSubtitleEntries } = await import(pathToFileURL(path.join(temp, 'subtitleFormats.mjs')));
const entries = (content, format) => parseSubtitleEntries(content, format).map((entry) => ({ ...entry, originalId: entry.id, text: entry.content.join('\n') }));
const ass = '\uFEFF[Script Info]\r\nTitle: Original\r\nPlayResX: 640\r\nPlayResY: 360\r\n\r\n[V4+ Styles]\r\nFormat: Name, Fontname, Fontsize\r\nStyle: ReviewStyle,Helvetica,28\r\n\r\n[Events]\r\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\r\nComment: 0,0:00:00.00,0:00:02.00,ReviewStyle,,0,0,0,,Keep this comment\r\nDialogue: 0,0:00:00.00,0:00:02.00,ReviewStyle,Speaker,10,20,30,,{\\pos(100,80)}Hello {\\i1}world{\\i0}\r\n';

test('unchanged ASS remains byte-identical, including BOM and CRLF', () => {
  assert.equal(editSubtitleDocument(ass, 'ass', entries(ass, 'ass')), ass);
});
test('ASS edits keep original styles, layout, speaker, comment and inline overrides', () => {
  const edited = entries(ass, 'ass');
  edited[0].text = 'Hello everyone';
  edited[0].startEndTime = '00:00:00,500 --> 00:00:02,500';
  const result = editSubtitleDocument(ass, 'ass', edited);
  for (const original of ['Title: Original', 'PlayResX: 640', 'Style: ReviewStyle,Helvetica,28', 'Keep this comment', 'ReviewStyle,Speaker,10,20,30', '{\\pos(100,80)}', '{\\i1}everyone{\\i0}']) assert.ok(result.includes(original), original);
  assert.ok(result.includes('0:00:00.50,0:00:02.50'));
  assert.ok(result.startsWith('\uFEFF'));
  assert.equal(result.includes('Style: Default'), false);
});
test('splitting an ASS cue inherits its original styling without flattening the document', () => {
  const [original] = entries(ass, 'ass');
  const result = editSubtitleDocument(ass, 'ass', [
    { ...original, text: 'Hello', startEndTime: '00:00:00,000 --> 00:00:01,000' },
    { ...original, id: '2', text: 'world', startEndTime: '00:00:01,000 --> 00:00:02,000' },
  ]);
  assert.equal((result.match(/^Dialogue:/gm) ?? []).length, 2);
  assert.equal((result.match(/ReviewStyle,Speaker,10,20,30/g) ?? []).length, 2);
  assert.equal(parseSubtitleEntries(result, 'ass').length, 2);
});
test('VTT edits retain identifiers, cue positioning, STYLE, REGION and NOTE metadata', () => {
  const original = 'WEBVTT Title\n\nSTYLE\n::cue { color: yellow; }\n\nREGION\nid:review\n\nNOTE Keep this note\n\noriginal-cue\n00:00:00.000 --> 00:00:02.000 line:10% align:start region:review\nHello\n';
  const edited = entries(original, 'vtt'); edited[0].text = 'Edited';
  const result = editSubtitleDocument(original, 'vtt', edited);
  for (const fragment of ['WEBVTT Title', 'STYLE\n::cue', 'REGION\nid:review', 'NOTE Keep this note', 'original-cue', 'line:10% align:start region:review', 'Edited']) assert.ok(result.includes(fragment), fragment);
});
test('LRC edits preserve tags and compensate the original offset exactly once', () => {
  const original = '[ti:Original song]\n[ar:Original artist]\n[offset:200]\n[00:01.00]Hello\n';
  const edited = entries(original, 'lrc'); edited[0].text = 'Edited'; edited[0].startEndTime = '00:00:01,500 --> 00:00:05,500';
  const result = editSubtitleDocument(original, 'lrc', edited);
  assert.ok(result.includes('[ar:Original artist]'));
  assert.ok(result.includes('[offset:200]'));
  assert.ok(result.includes('[00:01.30]Edited'));
  assert.equal(parseSubtitleEntries(result, 'lrc')[0].startEndTime, '00:00:01,500 --> 00:00:05,500');
  edited[0].startEndTime = '00:00:00,000 --> 00:00:01,000';
  assert.throws(() => editSubtitleDocument(original, 'lrc', edited), /offset/);
});

test('successive saves after deleting a cue retain the surviving original style anchors', () => {
  const original = '[Script Info]\nPlayResX: 640\n\n[V4+ Styles]\nStyle: First,Arial,20\nStyle: Last,Helvetica,30\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,0:00:01.00,First,A,0,0,10,,One\nDialogue: 0,0:00:01.00,0:00:02.00,First,B,0,0,20,,Two\nDialogue: 0,0:00:02.00,0:00:03.00,Last,C,0,0,30,,Three\n';
  const [first, , last] = entries(original, 'ass');
  const saved = [{ ...first }, { ...last, id: '2' }];
  const firstSave = editSubtitleDocument(original, 'ass', saved);
  const secondSave = editSubtitleDocument(original, 'ass', saved.map((cue, index) => index === 1 ? { ...cue, text: 'Edited three' } : cue));
  assert.ok(firstSave.includes('Last,C,0,0,30,,Three'));
  assert.ok(secondSave.includes('Last,C,0,0,30,,Edited three'));
});
