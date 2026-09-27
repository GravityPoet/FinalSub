import { useEffect, useRef, useState } from 'react';
import { Download, Play, RefreshCw, X } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { useI18n, type TranslationKey } from '../../lib/i18n';
import { inspectSubtitleQuality, repairSubtitleTranslation, saveDialog, writeTextFilePath, type QualityReport } from '../../lib/tauri';
import type { Subtitle } from './useStandaloneSubtitles';

interface Props {
  subtitles: Subtitle[];
  sourceLanguage: string;
  targetLanguage: string;
  bilingual: boolean;
  onChange: (subtitles: Subtitle[]) => void;
  onLocate: (index: number, play: boolean) => void;
  onSplit: (index: number) => void;
  onMerge: (index: number) => void;
}

export default function QualityPanel({ subtitles, sourceLanguage, targetLanguage, bilingual, onChange, onLocate, onSplit, onMerge }: Props) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const [report, setReport] = useState<QualityReport | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState<string | null>(null);
  const [ignored, setIgnored] = useState<Set<string>>(new Set());
  const latest = useRef(subtitles);
  latest.current = subtitles;
  useEffect(() => {
    let active = true;
    const timer = setTimeout(() => {
      inspectSubtitleQuality(subtitles.map((cue) => ({ start_ms: Math.round((cue.startTimeInSeconds || 0) * 1000), end_ms: Math.round((cue.endTimeInSeconds || 0) * 1000), source: cue.sourceContent ?? cue.content.join('\n'), target: bilingual ? cue.targetContent ?? '' : null })), sourceLanguage, targetLanguage)
        .then((result) => { if (active) { setReport(result); setError(''); } })
        .catch((failure) => { if (active) setError(String(failure)); });
    }, 350);
    return () => { active = false; clearTimeout(timer); };
  }, [subtitles, sourceLanguage, targetLanguage, bilingual]);
  const issueKey = (index: number, code: string) => `${code}:${index}:${JSON.stringify(subtitles[index])}`;
  const issues = report?.issues.filter((issue) => !ignored.has(issueKey(issue.cue_index, issue.code))) ?? [];
  const repair = async (index: number) => {
    const snapshot = subtitles[index];
    setBusy(snapshot.id);
    setError('');
    try {
      const translated = await repairSubtitleTranslation(snapshot.sourceContent ?? snapshot.content.join('\n'), sourceLanguage, targetLanguage);
      const current = latest.current;
      if (current[index] !== snapshot) throw new Error(t('quality.changed'));
      onChange(current.map((cue, at) => at === index ? { ...cue, targetContent: translated } : cue));
    } catch (failure) { setError(String(failure)); }
    finally { setBusy(null); }
  };
  const exportReport = async () => {
    try {
      const path = await saveDialog({ defaultPath: 'FinalSub-quality.json', filters: [{ name: 'JSON', extensions: ['json'] }] });
      if (path) await writeTextFilePath(path, JSON.stringify({ ...report, ignored_issues: (report?.issues.length || 0) - issues.length }, null, 2));
    } catch (failure) { setError(String(failure)); }
  };
  return <section className="shrink-0 border-b border-border-subtle bg-surface-overlay/35 px-4 py-2" aria-label={t('quality.title')}>
    <div className="flex flex-wrap items-center justify-between gap-2">
      <button type="button" className="text-sm font-semibold text-text-primary" onClick={() => setOpen(!open)} aria-expanded={open}>{t('quality.title')} · {t('quality.count', { count: issues.length })}</button>
      <Button size="sm" variant="ghost" onClick={() => void exportReport()} disabled={!report}><Download size={13} />{t('quality.export')}</Button>
    </div>
    {error && <p role="alert" className="py-2 text-xs text-danger">{error}</p>}
    {open && <div className="max-h-52 overflow-y-auto py-2">
      <p className="mb-2 text-xs text-text-tertiary">{t('quality.hint')}</p>
      {issues.length === 0 && <p className="text-xs text-success">{t('quality.clear')}</p>}
      {issues.slice(0, 100).map((issue) => <div key={`${issue.cue_index}:${issue.code}`} className="mb-1 flex flex-wrap items-center gap-2 rounded-lg bg-surface px-3 py-2 text-xs">
        <button type="button" className="min-w-0 flex-1 text-left text-brand" onClick={() => onLocate(issue.cue_index, false)}>#{issue.cue_index + 1} · {t(`quality.${issue.code}` as TranslationKey)}{issue.value > 0 ? ` (${issue.value})` : ''}</button>
        <Button size="sm" variant="ghost" onClick={() => onLocate(issue.cue_index, true)}><Play size={12} />{t('quality.listen')}</Button>
        {issue.code === 'untranslated' && bilingual && <Button size="sm" variant="secondary" disabled={busy !== null} onClick={() => void repair(issue.cue_index)}><RefreshCw size={12} />{t('quality.retranslate')}</Button>}
        {(issue.code === 'long-line' || issue.code === 'fast-reading') && <Button size="sm" variant="secondary" onClick={() => onSplit(issue.cue_index)}>{t('quality.split')}</Button>}
        {(issue.code === 'short-duration' || issue.code === 'repeated') && issue.cue_index + 1 < subtitles.length && <Button size="sm" variant="secondary" onClick={() => onMerge(issue.cue_index)}>{t('quality.merge')}</Button>}
        <button type="button" title={t('quality.ignore')} aria-label={t('quality.ignore')} onClick={() => setIgnored((current) => new Set([...current, issueKey(issue.cue_index, issue.code)]))}><X size={14} /></button>
      </div>)}
      {issues.length > 100 && <p className="text-xs text-text-tertiary">{t('quality.more')}</p>}
    </div>}
  </section>;
}
