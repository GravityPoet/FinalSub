import { useState, useCallback, useEffect } from 'react';
import {
  Sparkles,
  Loader2,
  CheckCircle2,
  XCircle,
  ChevronDown,
  ChevronUp,
  Play,
  RotateCcw,
  Check,
} from 'lucide-react';
import { Subtitle } from '../useStandaloneSubtitles';
import { listTranslationProviders, testTranslation } from '../../../lib/tauri';
import { useToast } from '../Toast';
import { useI18n } from '../../../lib/i18n';
import { Button } from '../../../components/ui/Button';
import { Input, Select, Textarea } from '../../../components/ui/Input';

interface OptimizationResult {
  id: string;
  index: number;
  sourceContent: string;
  originalTarget: string;
  optimizedTarget: string;
  status: 'success' | 'error' | 'skipped';
  error?: string;
  selected: boolean;
}

interface BatchAiOptimizeDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  subtitles: Subtitle[];
  onApplyOptimizations: (
    optimizations: Array<{ index: number; targetContent: string }>,
  ) => void;
  shouldShowTranslation: boolean;
}

export default function BatchAiOptimizeDialog({
  open,
  onOpenChange,
  subtitles,
  onApplyOptimizations,
  shouldShowTranslation: _shouldShowTranslation,
}: BatchAiOptimizeDialogProps) {
  const { showToast } = useToast();
  const { t } = useI18n();
  const [step, setStep] = useState<'config' | 'running' | 'review'>('config');
  const [aiProviders, setAiProviders] = useState<any[]>([]);
  const [selectedProviderId, setSelectedProviderId] = useState('');
  const [batchSize, setBatchSize] = useState(5);
  const [showCustomPrompt, setShowCustomPrompt] = useState(false);
  const [customPrompt, setCustomPrompt] = useState('');

  // 进度状态
  const [progress, setProgress] = useState(0);
  const [currentBatch, setCurrentBatch] = useState(0);
  const [totalBatches, setTotalBatches] = useState(0);
  const [processedCount, setProcessedCount] = useState(0);

  // 结果状态
  const [results, setResults] = useState<OptimizationResult[]>([]);
  const [summary, setSummary] = useState<{
    total: number;
    success: number;
    error: number;
    skipped: number;
  } | null>(null);

  const BATCH_PROMPT_CACHE_KEY = 'ai_batch_optimize_prompt';

  const defaultBatchPrompt = `You are a professional subtitle translator and proofreader.

For each subtitle in the input JSON, optimize the translation based on the original text.
1. More accurately convey the original meaning.
2. Use natural and fluent expressions.
3. Be appropriate for subtitle display.
4. Maintain the original tone and style.

Input JSON format:
{
  "id": { "source": "original text", "translation": "current translation" }
}

Output JSON format:
{
  "id": "optimized translation string"
}

IMPORTANT: Return ONLY a valid JSON object. Do not include any markdown format tags like \`\`\`json.`;

  // 加载 AI 服务商
  const loadAiProviders = useCallback(async () => {
    try {
      const providers = await listTranslationProviders();
      const aiOnly = providers.filter((p: any) => p.is_ai);
      setAiProviders(aiOnly);
      if (aiOnly.length > 0 && !selectedProviderId) {
        setSelectedProviderId(aiOnly[0].id);
      }
    } catch (error) {
      console.error('Failed to load AI providers:', error);
    }
  }, [selectedProviderId]);

  // 加载缓存的提示词
  const loadCachedPrompt = useCallback(() => {
    try {
      const cached = localStorage.getItem(BATCH_PROMPT_CACHE_KEY);
      if (cached) {
        setCustomPrompt(cached);
        setShowCustomPrompt(true);
      } else {
        setCustomPrompt(defaultBatchPrompt);
      }
    } catch {
      setCustomPrompt(defaultBatchPrompt);
    }
  }, [defaultBatchPrompt]);

  // 保存提示词到缓存
  const savePromptToCache = useCallback(
    (prompt: string) => {
      try {
        if (prompt.trim() !== defaultBatchPrompt.trim()) {
          localStorage.setItem(BATCH_PROMPT_CACHE_KEY, prompt);
        } else {
          localStorage.removeItem(BATCH_PROMPT_CACHE_KEY);
        }
      } catch {}
    },
    [defaultBatchPrompt],
  );

  useEffect(() => {
    if (open) {
      loadAiProviders();
      loadCachedPrompt();
      setStep('config');
      setProgress(0);
      setResults([]);
      setSummary(null);
    }
  }, [open, loadAiProviders, loadCachedPrompt]);

  // 执行批量优化的主线程 (前端分批控制逻辑)
  const handleStartOptimization = useCallback(async (retryOnly = false) => {
    if (aiProviders.length === 0) {
      showToast('error', t('proofread.batchAi.toastNoService'));
      return;
    }

    const subs = retryOnly
      ? results
          .filter((r) => r.status === 'error')
          .map((r) => ({
            id: r.id,
            index: r.index,
            sourceContent: r.sourceContent,
            targetContent: r.originalTarget,
          }))
      : subtitles
          .map((sub, index) => ({
            id: sub.id || String(index + 1),
            index,
            sourceContent: sub.sourceContent || '',
            targetContent: sub.targetContent || '',
          }))
          .filter((sub) => sub.sourceContent.trim());

    if (subs.length === 0) {
      showToast('error', retryOnly ? t('proofread.batchAi.toastNoFailedSubs') : t('proofread.batchAi.toastNoSubs'));
      return;
    }

    setStep('running');
    setProgress(0);
    setProcessedCount(0);

    savePromptToCache(customPrompt);

    const total = subs.length;
    const batches: (typeof subs)[] = [];
    for (let i = 0; i < total; i += batchSize) {
      batches.push(subs.slice(i, i + batchSize));
    }

    setTotalBatches(batches.length);
    setCurrentBatch(0);

    const optimizationResults: OptimizationResult[] = [];
    let successCount = 0;
    let errorCount = 0;
    let skippedCount = 0;

    for (let b = 0; b < batches.length; b++) {
      setCurrentBatch(b + 1);
      const currentBatchItems = batches[b];

      // 拼装该批次的 JSON 结构
      const batchInputMap: Record<string, { source: string; translation: string }> = {};
      currentBatchItems.forEach(item => {
        batchInputMap[item.id] = {
          source: item.sourceContent,
          translation: item.targetContent
        };
      });

      const promptText = `${customPrompt}\n\nInput JSON:\n${JSON.stringify(batchInputMap, null, 2)}`;

      let batchSuccess = false;
      let batchOutputs: Record<string, string> = {};

      try {
        const res = await testTranslation({
          text: promptText,
          source_language: 'auto',
          target_language: 'auto',
          provider: selectedProviderId
        });

        if (res.success && res.translated_text) {
          // 移除可能存在的 Markdown 围栏
          let cleanJson = res.translated_text.trim();
          if (cleanJson.startsWith('```')) {
            cleanJson = cleanJson.replace(/^```[a-zA-Z]*\n/, '').replace(/\n```$/, '');
          }
          cleanJson = cleanJson.trim();

          try {
            batchOutputs = JSON.parse(cleanJson);
            batchSuccess = true;
          } catch (e) {
            console.error('Failed to parse batch JSON response:', cleanJson, e);
          }
        }
      } catch (e) {
        console.error('Batch request failed:', e);
      }

      // If the whole batch fails, fall back to per-line optimization with backoff to guarantee completion.
      if (!batchSuccess) {
        for (const item of currentBatchItems) {
          try {
            const singlePrompt = `You are a professional subtitle translator and proofreader.
Optimize the following translation based on the original text:
Original: ${item.sourceContent}
Current Translation: ${item.targetContent}

Only output the optimized translation, nothing else.`;

            const singleRes = await testTranslation({
              text: singlePrompt,
              source_language: 'auto',
              target_language: 'auto',
              provider: selectedProviderId
            });

            if (singleRes.success && singleRes.translated_text) {
              const val = singleRes.translated_text.trim();
              optimizationResults.push({
                id: item.id,
                index: item.index,
                sourceContent: item.sourceContent,
                originalTarget: item.targetContent,
                optimizedTarget: val,
                status: 'success',
                selected: val !== item.targetContent,
              });
              successCount++;
            } else {
              optimizationResults.push({
                id: item.id,
                index: item.index,
                sourceContent: item.sourceContent,
                originalTarget: item.targetContent,
                optimizedTarget: item.targetContent,
                status: 'error',
                error: singleRes.error || t('proofread.batchAi.toastSingleFailed'),
                selected: false,
              });
              errorCount++;
            }
          } catch (err: any) {
            optimizationResults.push({
              id: item.id,
              index: item.index,
              sourceContent: item.sourceContent,
              originalTarget: item.targetContent,
              optimizedTarget: item.targetContent,
              status: 'error',
              error: err.toString(),
              selected: false,
            });
            errorCount++;
          }
          setProcessedCount((prev) => prev + 1);
          setProgress((((b * batchSize) + currentBatchItems.indexOf(item) + 1) / total) * 100);
        }
      } else {
        // 整批提取成功
        currentBatchItems.forEach((item) => {
          const optimizedVal = (batchOutputs[item.id] || item.targetContent).trim();
          optimizationResults.push({
            id: item.id,
            index: item.index,
            sourceContent: item.sourceContent,
            originalTarget: item.targetContent,
            optimizedTarget: optimizedVal,
            status: 'success',
            selected: optimizedVal !== item.targetContent,
          });
          successCount++;
        });
        setProcessedCount((prev) => prev + currentBatchItems.length);
        setProgress(((b + 1) / batches.length) * 100);
      }
    }

    if (retryOnly) {
      const mergedResults = results.map((r) => {
        const newRes = optimizationResults.find((nr) => nr.id === r.id);
        return newRes ? newRes : r;
      });
      setResults(mergedResults);
      const sCount = mergedResults.filter((r) => r.status === 'success').length;
      const eCount = mergedResults.filter((r) => r.status === 'error').length;
      setSummary({
        total: mergedResults.length,
        success: sCount,
        error: eCount,
        skipped: 0,
      });
    } else {
      setResults(optimizationResults);
      setSummary({
        total,
        success: successCount,
        error: errorCount,
        skipped: skippedCount,
      });
    }
    setStep('review');
  }, [subtitles, results, selectedProviderId, customPrompt, batchSize, aiProviders, savePromptToCache, showToast]);

  const toggleResultSelection = (id: string) => {
    setResults((prev) =>
      prev.map((r) => (r.id === id ? { ...r, selected: !r.selected } : r)),
    );
  };

  const toggleSelectAll = (selected: boolean) => {
    setResults((prev) =>
      prev.map((r) =>
        r.status === 'success' && r.optimizedTarget !== r.originalTarget
          ? { ...r, selected }
          : r,
      ),
    );
  };

  const handleApply = () => {
    const selectedResults = results.filter((r) => r.selected);
    if (selectedResults.length === 0) {
      showToast('error', t('proofread.batchAi.toastNoSelections'));
      return;
    }

    const optimizations = selectedResults.map((r) => ({
      index: r.index,
      targetContent: r.optimizedTarget,
    }));

    onApplyOptimizations(optimizations);
    onOpenChange(false);
    showToast('success', t('proofread.batchAi.toastApplySuccess').replace('{count}', String(optimizations.length)));
  };

  const selectedCount = results.filter((r) => r.selected).length;
  const selectableCount = results.filter(
    (r) => r.status === 'success' && r.optimizedTarget !== r.originalTarget,
  ).length;

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 bg-black/40 backdrop-blur-md flex items-center justify-center p-4">
      <div className="bg-surface border border-border-default rounded-xl w-full max-w-4xl max-h-[85vh] flex flex-col shadow-lg overflow-hidden text-text-primary">
        {/* Header */}
        <div className="px-6 py-5 border-b border-border-subtle flex items-center gap-2.5 flex-shrink-0">
          <Sparkles className="h-5.5 w-5.5 text-brand animate-pulse" />
          <div>
            <h3 className="text-lg font-bold text-text-primary">
              {step === 'config' && t('proofread.batchAi.titleConfig')}
              {step === 'running' && t('proofread.batchAi.titleRunning')}
              {step === 'review' && t('proofread.batchAi.titleReview')}
            </h3>
            <p className="text-xs text-text-secondary mt-1">
              {step === 'config' && t('proofread.batchAi.descConfig')}
              {step === 'running' && t('proofread.batchAi.descRunning')}
              {step === 'review' && t('proofread.batchAi.descReview')}
            </p>
          </div>
        </div>

        {/* Content Body */}
        <div className="flex-1 overflow-y-auto p-6 min-h-0 bg-app-bg/30">
          {step === 'config' && (
            <div className="space-y-5">
              <div className="space-y-2">
                <label className="text-xs font-semibold text-text-secondary">{t('proofread.batchAi.selectService')}</label>
                {aiProviders.length === 0 ? (
                  <div className="p-3 bg-surface-overlay border border-border-subtle text-xs text-text-tertiary italic rounded-lg">
                    {t('proofread.batchAi.noServiceAlert')}
                  </div>
                ) : (
                  <Select
                    value={selectedProviderId}
                    onChange={(e) => setSelectedProviderId(e.target.value)}
                  >
                    {aiProviders.map((provider) => (
                      <option key={provider.id} value={provider.id}>
                        {provider.name}
                      </option>
                    ))}
                  </Select>
                )}
              </div>

              <div className="space-y-2">
                <label className="text-xs font-semibold text-text-secondary">{t('proofread.batchAi.batchSize')}</label>
                <div className="flex items-center gap-3">
                  <Input
                    type="number"
                    min={1}
                    max={20}
                    value={batchSize}
                    onChange={(e) => setBatchSize(Math.max(1, Math.min(20, parseInt(e.target.value) || 5)))}
                    className="w-24 text-center font-mono"
                  />
                  <span className="text-xs text-text-secondary">
                    {t('proofread.batchAi.batchSizeDesc')}
                  </span>
                </div>
              </div>

              <div className="p-4 bg-surface border border-border-subtle rounded-xl flex items-center justify-between shadow-sm">
                <div>
                  <span className="text-xs text-text-secondary">{t('proofread.batchAi.totalSubs')}</span>
                  <div className="text-xl font-bold text-text-primary mt-1 font-mono">
                    {t('proofread.batchAi.totalSubsCount').replace('{count}', String(subtitles.filter((s) => s.sourceContent?.trim()).length))}
                  </div>
                </div>
                <div className="text-right">
                  <span className="text-xs text-text-secondary">{t('proofread.batchAi.expectedBatches')}</span>
                  <div className="text-xl font-bold text-brand mt-1 font-mono">
                    {t('proofread.batchAi.expectedBatchesCount').replace('{count}', String(Math.ceil(subtitles.filter((s) => s.sourceContent?.trim()).length / batchSize)))}
                  </div>
                </div>
              </div>

              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <label className="text-xs font-semibold text-text-secondary">{t('proofread.batchAi.customPrompt')}</label>
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => {
                        setCustomPrompt(defaultBatchPrompt);
                        localStorage.removeItem(BATCH_PROMPT_CACHE_KEY);
                      }}
                      className="text-xs text-text-secondary hover:text-text-primary transition-colors cursor-pointer"
                    >
                      {t('proofread.batchAi.resetDefault')}
                    </button>
                    <button
                      onClick={() => setShowCustomPrompt(!showCustomPrompt)}
                      className="text-text-secondary hover:text-text-primary transition-colors cursor-pointer"
                    >
                      {showCustomPrompt ? <ChevronUp className="h-4.5 w-4.5" /> : <ChevronDown className="h-4.5 w-4.5" />}
                    </button>
                  </div>
                </div>
                {showCustomPrompt && (
                  <Textarea
                    value={customPrompt}
                    onChange={(e) => setCustomPrompt(e.target.value)}
                    className="font-mono min-h-[160px]"
                  />
                )}
              </div>
            </div>
          )}

          {step === 'running' && (
            <div className="py-12 flex flex-col items-center justify-center space-y-6">
              <Loader2 className="h-14 w-14 animate-spin text-brand" />
              <div className="text-center space-y-1.5">
                <p className="text-base font-semibold text-text-primary">{t('proofread.batchAi.optimizing')}</p>
                <p className="text-xs text-text-secondary font-mono">
                  {t('proofread.batchAi.currentBatch').replace('{current}', String(currentBatch)).replace('{total}', String(totalBatches))}
                </p>
              </div>

              <div className="w-full max-w-md bg-surface border border-border-subtle rounded-full h-2.5 overflow-hidden shadow-sm">
                <div
                  className="bg-brand h-full rounded-full transition-all duration-300 ease-out"
                  style={{ width: `${progress}%` }}
                />
              </div>
              <span className="text-xs font-medium text-text-secondary font-mono">
                {t('proofread.batchAi.processed').replace('{processed}', String(processedCount)).replace('{total}', String(subtitles.filter((s) => s.sourceContent?.trim()).length)).replace('{percent}', String(Math.round(progress)))}
              </span>
            </div>
          )}

          {step === 'review' && (
            <div className="flex flex-col h-[52vh] space-y-4">
              {summary && (
                <div className="flex items-center justify-between p-3.5 bg-surface border border-border-subtle rounded-xl flex-shrink-0 shadow-sm">
                  <div className="flex items-center gap-4 text-xs">
                    <span className="flex items-center gap-1 text-text-secondary font-medium">
                      <CheckCircle2 className="h-4 w-4 text-success" />
                      {t('proofread.batchAi.successCount').replace('{count}', String(summary.success))}
                    </span>
                    <span className="flex items-center gap-1 text-text-secondary font-medium">
                      <XCircle className="h-4 w-4 text-danger" />
                      {t('proofread.batchAi.errorCount').replace('{count}', String(summary.error))}
                    </span>
                  </div>

                  <div className="flex items-center gap-2">
                    <input
                      type="checkbox"
                      id="select-all-checkbox"
                      checked={selectedCount > 0 && selectedCount === selectableCount}
                      onChange={(e) => toggleSelectAll(e.target.checked)}
                      className="rounded bg-surface border-border-strong text-brand focus:ring-0 w-4 h-4 cursor-pointer"
                    />
                    <label htmlFor="select-all-checkbox" className="text-xs font-medium text-text-secondary cursor-pointer">
                      {t('proofread.batchAi.selectAll').replace('{selected}', String(selectedCount)).replace('{total}', String(selectableCount))}
                    </label>
                  </div>
                </div>
              )}

              <div className="flex-1 border border-border-subtle bg-surface rounded-xl overflow-y-auto divide-y divide-border-subtle shadow-sm">
                {results.map((result) => (
                  <div
                    key={result.id}
                    className={`p-4 transition-colors ${
                      result.status === 'error'
                        ? 'bg-danger/5 border-l-2 border-l-danger'
                        : 'hover:bg-surface-raised/40'
                    }`}
                  >
                    <div className="flex items-start gap-3">
                      <input
                        type="checkbox"
                        checked={result.selected}
                        onChange={() => toggleResultSelection(result.id)}
                        disabled={
                          result.status !== 'success' ||
                          result.optimizedTarget === result.originalTarget
                        }
                        className="rounded bg-surface border-border-strong text-brand focus:ring-0 w-4 h-4 mt-1 cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
                      />

                      <div className="flex-1 min-w-0 space-y-2.5">
                        <div className="flex items-center gap-2">
                          <span className="text-[10px] font-mono bg-surface-overlay text-text-secondary px-2 py-0.5 rounded border border-border-subtle">
                            #{result.index + 1}
                          </span>
                          {result.status === 'success' ? (
                            result.optimizedTarget !== result.originalTarget ? (
                              <span className="text-[10px] bg-success/10 text-success border border-success/20 px-2 py-0.5 rounded font-medium">
                                {t('proofread.batchAi.optimized')}
                              </span>
                            ) : (
                              <span className="text-[10px] bg-surface-overlay text-text-tertiary px-2 py-0.5 rounded font-medium border border-border-subtle">
                                {t('proofread.batchAi.noChange')}
                              </span>
                            )
                          ) : (
                            <span className="text-[10px] bg-danger/10 text-danger border border-danger/20 px-2 py-0.5 rounded font-medium">
                              {t('proofread.batchAi.failed')}
                            </span>
                          )}
                        </div>

                        <div className="text-xs text-text-secondary italic">
                          {result.sourceContent}
                        </div>

                        {result.status === 'success' &&
                          result.optimizedTarget !== result.originalTarget && (
                            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                              <div className="p-3 bg-surface-raised rounded-xl text-xs border border-border-subtle">
                                <div className="text-[10px] text-text-tertiary mb-1.5">{t('proofread.batchAi.originalTarget')}</div>
                                <div className="text-text-secondary font-medium">{result.originalTarget || t('proofread.batchAi.emptyText')}</div>
                              </div>
                              <div className="p-3 bg-brand-subtle rounded-xl text-xs border border-brand/15">
                                <div className="text-[10px] text-brand-text mb-1.5">{t('proofread.batchAi.optimizedTarget')}</div>
                                <div className="text-brand-text font-medium">{result.optimizedTarget}</div>
                              </div>
                            </div>
                          )}

                        {result.error && (
                          <div className="text-xs text-danger bg-danger/5 border border-danger/10 rounded-lg p-2 font-mono">
                            {result.error}
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="px-6 py-4.5 border-t border-border-subtle flex justify-end gap-2.5 flex-shrink-0 bg-surface">
          {step === 'config' && (
            <>
              <Button
                variant="ghost"
                onClick={() => onOpenChange(false)}
              >
                {t('proofread.batchAi.cancel')}
              </Button>
              <Button
                variant="primary"
                onClick={() => handleStartOptimization(false)}
                disabled={
                  aiProviders.length === 0 ||
                  subtitles.filter((s) => s.sourceContent?.trim()).length === 0
                }
              >
                <Play className="h-4 w-4 mr-1.5" />
                {t('proofread.batchAi.start')}
              </Button>
            </>
          )}

          {step === 'running' && (
            <Button
              variant="secondary"
              disabled
            >
              <Loader2 className="h-4 w-4 mr-1.5 animate-spin" />
              {t('proofread.batchAi.processing')}
            </Button>
          )}

          {step === 'review' && (
            <>
              {summary && summary.error > 0 && (
                <Button
                  variant="danger"
                  onClick={() => handleStartOptimization(true)}
                  className="mr-auto"
                >
                  <RotateCcw className="h-4 w-4 mr-1.5" />
                  {t('proofread.batchAi.retryFailed').replace('{count}', String(summary.error))}
                </Button>
              )}
              <Button
                variant="secondary"
                onClick={() => setStep('config')}
              >
                <RotateCcw className="h-4 w-4 mr-1.5" />
                {t('proofread.batchAi.reconfigure')}
              </Button>
              <Button
                variant="secondary"
                onClick={() => onOpenChange(false)}
              >
                {t('proofread.batchAi.cancelClose')}
              </Button>
              <Button
                variant="primary"
                onClick={handleApply}
                disabled={selectedCount === 0}
              >
                <Check className="h-4 w-4 mr-1.5" />
                {t('proofread.batchAi.applySelected').replace('{count}', String(selectedCount))}
              </Button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
