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
  const handleStartOptimization = useCallback(async () => {
    if (aiProviders.length === 0) {
      alert('请先在设置中配置并开启至少一个 AI 翻译服务');
      return;
    }

    const subs = subtitles
      .map((sub, index) => ({
        id: sub.id || String(index + 1),
        index,
        sourceContent: sub.sourceContent || '',
        targetContent: sub.targetContent || '',
      }))
      .filter((sub) => sub.sourceContent.trim());

    if (subs.length === 0) {
      alert('没有可优化的字幕');
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

      // 如果整批优化失败，启动退避的单条优化 fallback，确保任务百分之百能跑完！
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
                error: singleRes.error || '优化接口未返回内容',
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

    setResults(optimizationResults);
    setSummary({
      total,
      success: successCount,
      error: errorCount,
      skipped: skippedCount,
    });
    setStep('review');
  }, [subtitles, selectedProviderId, customPrompt, batchSize, aiProviders, savePromptToCache]);

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
      alert('请先选择要采纳的优化结果');
      return;
    }

    const optimizations = selectedResults.map((r) => ({
      index: r.index,
      targetContent: r.optimizedTarget,
    }));

    onApplyOptimizations(optimizations);
    onOpenChange(false);
    alert(`成功应用了 ${optimizations.length} 条字幕优化`);
  };

  const selectedCount = results.filter((r) => r.selected).length;
  const selectableCount = results.filter(
    (r) => r.status === 'success' && r.optimizedTarget !== r.originalTarget,
  ).length;

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 bg-black/75 backdrop-blur-sm flex items-center justify-center p-4">
      <div className="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-4xl max-h-[85vh] flex flex-col shadow-2xl overflow-hidden text-slate-100">
        {/* Header */}
        <div className="px-6 py-5 border-b border-slate-800 flex items-center gap-2.5 flex-shrink-0">
          <Sparkles className="h-5.5 w-5.5 text-blue-500" />
          <div>
            <h3 className="text-lg font-bold text-slate-100">
              {step === 'config' && '全文 AI 优化'}
              {step === 'running' && '批量优化处理中...'}
              {step === 'review' && '审核 AI 优化结果'}
            </h3>
            <p className="text-xs text-slate-400 mt-1">
              {step === 'config' && '使用 AI 批量优化所有字幕翻译，优化后可逐条审核并选择采纳'}
              {step === 'running' && '正在分批提交字幕优化请求，请耐心等待...'}
              {step === 'review' && '请审核优化结果，仅勾选并应用你满意的条目'}
            </p>
          </div>
        </div>

        {/* Content Body */}
        <div className="flex-1 overflow-y-auto p-6 min-h-0">
          {step === 'config' && (
            <div className="space-y-5">
              <div className="space-y-2">
                <label className="text-xs font-semibold text-slate-400">选择 AI 翻译服务</label>
                {aiProviders.length === 0 ? (
                  <div className="p-3 bg-slate-950/50 border border-slate-800 text-xs text-slate-400 italic rounded-lg">
                    未在系统设置中找到并配置开启的 AI 服务，请先前往“翻译设置”添加
                  </div>
                ) : (
                  <select
                    value={selectedProviderId}
                    onChange={(e) => setSelectedProviderId(e.target.value)}
                    className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-blue-500"
                  >
                    {aiProviders.map((provider) => (
                      <option key={provider.id} value={provider.id}>
                        {provider.name}
                      </option>
                    ))}
                  </select>
                )}
              </div>

              <div className="space-y-2">
                <label className="text-xs font-semibold text-slate-400">每批处理数量</label>
                <div className="flex items-center gap-3">
                  <input
                    type="number"
                    min={1}
                    max={20}
                    value={batchSize}
                    onChange={(e) => setBatchSize(Math.max(1, Math.min(20, parseInt(e.target.value) || 5)))}
                    className="bg-slate-950 border border-slate-800 rounded-lg px-3 py-1.5 text-sm text-slate-200 w-24 text-center focus:outline-none focus:border-blue-500"
                  />
                  <span className="text-xs text-slate-400">
                    建议 3-8 条。利用整批大模型打包机制，降低 token 消耗并提升速度。
                  </span>
                </div>
              </div>

              <div className="p-4 bg-slate-950/40 border border-slate-800 rounded-xl flex items-center justify-between">
                <div>
                  <span className="text-xs text-slate-400">有效翻译字幕总数</span>
                  <div className="text-xl font-bold text-slate-200 mt-1">
                    {subtitles.filter((s) => s.sourceContent?.trim()).length} 条
                  </div>
                </div>
                <div className="text-right">
                  <span className="text-xs text-slate-400">预计拆分批次</span>
                  <div className="text-xl font-bold text-slate-200 mt-1 text-blue-500">
                    {Math.ceil(subtitles.filter((s) => s.sourceContent?.trim()).length / batchSize)} 批
                  </div>
                </div>
              </div>

              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <label className="text-xs font-semibold text-slate-400">自定义提示词</label>
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => {
                        setCustomPrompt(defaultBatchPrompt);
                        localStorage.removeItem(BATCH_PROMPT_CACHE_KEY);
                      }}
                      className="text-xs text-slate-400 hover:text-slate-200"
                    >
                      重置默认
                    </button>
                    <button
                      onClick={() => setShowCustomPrompt(!showCustomPrompt)}
                      className="text-slate-400 hover:text-slate-200"
                    >
                      {showCustomPrompt ? <ChevronUp className="h-4.5 w-4.5" /> : <ChevronDown className="h-4.5 w-4.5" />}
                    </button>
                  </div>
                </div>
                {showCustomPrompt && (
                  <textarea
                    value={customPrompt}
                    onChange={(e) => setCustomPrompt(e.target.value)}
                    className="w-full bg-slate-955 font-mono border border-slate-800 text-xs text-slate-300 p-3 rounded-xl min-h-[160px] focus:outline-none focus:border-blue-500"
                  />
                )}
              </div>
            </div>
          )}

          {step === 'running' && (
            <div className="py-12 flex flex-col items-center justify-center space-y-6">
              <Loader2 className="h-14 w-14 animate-spin text-blue-500" />
              <div className="text-center space-y-1.5">
                <p className="text-base font-semibold">正在批量翻译和优化中</p>
                <p className="text-xs text-slate-400">
                  当前批次: {currentBatch} / {totalBatches}
                </p>
              </div>

              <div className="w-full max-w-md bg-slate-950 rounded-full h-3 overflow-hidden border border-slate-800">
                <div
                  className="bg-blue-600 h-full rounded-full transition-all duration-300"
                  style={{ width: `${progress}%` }}
                />
              </div>
              <span className="text-xs font-medium text-slate-400">
                已处理: {processedCount} / {subtitles.filter((s) => s.sourceContent?.trim()).length} ({Math.round(progress)}%)
              </span>
            </div>
          )}

          {step === 'review' && (
            <div className="flex flex-col h-[52vh] space-y-4">
              {summary && (
                <div className="flex items-center justify-between p-3.5 bg-slate-950/40 border border-slate-800 rounded-xl flex-shrink-0">
                  <div className="flex items-center gap-4 text-xs">
                    <span className="flex items-center gap-1 text-slate-300 font-medium">
                      <CheckCircle2 className="h-4 w-4 text-emerald-500" />
                      优化成功: {summary.success}
                    </span>
                    <span className="flex items-center gap-1 text-slate-300 font-medium">
                      <XCircle className="h-4 w-4 text-red-500" />
                      优化失败: {summary.error}
                    </span>
                  </div>

                  <div className="flex items-center gap-2">
                    <input
                      type="checkbox"
                      id="select-all-checkbox"
                      checked={selectedCount > 0 && selectedCount === selectableCount}
                      onChange={(e) => toggleSelectAll(e.target.checked)}
                      className="rounded bg-slate-950 border-slate-700 text-blue-500 focus:ring-0 w-4 h-4 cursor-pointer"
                    />
                    <label htmlFor="select-all-checkbox" className="text-xs font-medium text-slate-300 cursor-pointer">
                      采纳全选 ({selectedCount}/{selectableCount})
                    </label>
                  </div>
                </div>
              )}

              <div className="flex-1 border border-slate-800 bg-slate-950/30 rounded-xl overflow-y-auto divide-y divide-slate-850">
                {results.map((result) => (
                  <div
                    key={result.id}
                    className={`p-4 transition-colors ${
                      result.status === 'error'
                        ? 'bg-red-950/5'
                        : ''
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
                        className="rounded bg-slate-950 border-slate-700 text-blue-500 focus:ring-0 w-4 h-4 mt-1 cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
                      />

                      <div className="flex-1 min-w-0 space-y-2.5">
                        <div className="flex items-center gap-2">
                          <span className="text-[10px] font-mono bg-slate-800 text-slate-300 px-2 py-0.5 rounded">
                            #{result.index + 1}
                          </span>
                          {result.status === 'success' ? (
                            result.optimizedTarget !== result.originalTarget ? (
                              <span className="text-[10px] bg-emerald-950 text-emerald-400 border border-emerald-900/30 px-2 py-0.5 rounded font-medium">
                                已优化
                              </span>
                            ) : (
                              <span className="text-[10px] bg-slate-800 text-slate-400 px-2 py-0.5 rounded font-medium">
                                无变化
                              </span>
                            )
                          ) : (
                            <span className="text-[10px] bg-red-950 text-red-400 border border-red-900/30 px-2 py-0.5 rounded font-medium">
                              优化失败
                            </span>
                          )}
                        </div>

                        <div className="text-xs text-slate-400 italic">
                          {result.sourceContent}
                        </div>

                        {result.status === 'success' &&
                          result.optimizedTarget !== result.originalTarget && (
                            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                              <div className="p-3 bg-slate-900/60 rounded-xl text-xs border border-slate-800">
                                <div className="text-[10px] text-slate-500 mb-1.5">原译文</div>
                                <div className="text-slate-300 font-medium">{result.originalTarget || '(空)'}</div>
                              </div>
                              <div className="p-3 bg-emerald-950/20 rounded-xl text-xs border border-emerald-900/20">
                                <div className="text-[10px] text-emerald-500 mb-1.5">优化后</div>
                                <div className="text-emerald-300 font-medium">{result.optimizedTarget}</div>
                              </div>
                            </div>
                          )}

                        {result.error && (
                          <div className="text-xs text-red-400 bg-red-950/20 border border-red-900/20 rounded-lg p-2 font-mono">
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
        <div className="px-6 py-4.5 border-t border-slate-800 flex justify-end gap-2.5 flex-shrink-0">
          {step === 'config' && (
            <>
              <button
                onClick={() => onOpenChange(false)}
                className="text-xs text-slate-300 hover:text-white bg-slate-800 hover:bg-slate-700 px-4 py-2 rounded-lg font-medium transition-colors border border-slate-700/50"
              >
                取消
              </button>
              <button
                onClick={handleStartOptimization}
                disabled={
                  aiProviders.length === 0 ||
                  subtitles.filter((s) => s.sourceContent?.trim()).length === 0
                }
                className="flex items-center text-xs bg-blue-600 hover:bg-blue-700 text-white px-5 py-2 rounded-lg transition-colors font-medium disabled:opacity-50 disabled:cursor-not-allowed shadow-lg shadow-blue-500/10"
              >
                <Play className="h-4 w-4 mr-1.5 text-blue-200" />
                开始优化
              </button>
            </>
          )}

          {step === 'running' && (
            <button
              disabled
              className="flex items-center text-xs bg-slate-800 text-slate-400 px-5 py-2 rounded-lg font-medium opacity-65 border border-slate-700/50"
            >
              <Loader2 className="h-4 w-4 mr-1.5 animate-spin" />
              正在处理...
            </button>
          )}

          {step === 'review' && (
            <>
              <button
                onClick={() => setStep('config')}
                className="flex items-center text-xs text-slate-300 hover:text-white bg-slate-800 hover:bg-slate-700 px-4 py-2 rounded-lg font-medium transition-colors border border-slate-700/50"
              >
                <RotateCcw className="h-4 w-4 mr-1.5 text-slate-400" />
                重新配置
              </button>
              <button
                onClick={() => onOpenChange(false)}
                className="text-xs text-slate-350 hover:text-white bg-slate-800 hover:bg-slate-750 px-4 py-2 rounded-lg font-medium transition-colors border border-slate-700/50"
              >
                取消关闭
              </button>
              <button
                onClick={handleApply}
                disabled={selectedCount === 0}
                className="flex items-center text-xs bg-blue-600 hover:bg-blue-700 text-white px-5 py-2 rounded-lg transition-colors font-medium disabled:opacity-50 disabled:cursor-not-allowed shadow-lg shadow-blue-500/10"
              >
                <Check className="h-4 w-4 mr-1.5" />
                应用选中已优化字幕 ({selectedCount})
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
