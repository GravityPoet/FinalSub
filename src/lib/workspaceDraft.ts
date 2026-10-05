import { useCallback, useEffect, useRef, useState, type SetStateAction } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { loadWorkspaceDraft, saveWorkspaceDraft, type WorkspaceKey } from './tauri';

interface Envelope<T> { version: 1; updatedAt: number; value: T }
const flushers = new Map<WorkspaceKey, () => Promise<void>>();
const writes = new Map<WorkspaceKey, Promise<void>>();

function queueWrite(key: WorkspaceKey, data: string): Promise<void> {
  const next = (writes.get(key) ?? Promise.resolve()).catch(() => undefined).then(() => saveWorkspaceDraft(key, data));
  writes.set(key, next);
  return next;
}

export async function flushWorkspaceDrafts(): Promise<void> {
  await Promise.all(Array.from(flushers.values(), (flush) => flush()));
  await Promise.all(writes.values());
}

export function useWorkspaceDraft<T>(key: WorkspaceKey, initial: T, valid: (value: unknown) => value is T) {
  const initialRef = useRef(initial);
  const validRef = useRef(valid);
  validRef.current = valid;
  const [value, setValue] = useState(initial);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState('');
  const [attempt, setAttempt] = useState(0);
  const latest = useRef<Envelope<T>>({ version: 1, updatedAt: 0, value });
  const recoveryKey = `finalsub:recovery:${key}`;
  const parse = useCallback((raw: string | null): Envelope<T> | null => {
    if (!raw) return null;
    const data: unknown = JSON.parse(raw);
    if (!data || typeof data !== 'object' || !('version' in data) || data.version !== 1 || !('updatedAt' in data) || typeof data.updatedAt !== 'number' || !('value' in data) || !validRef.current(data.value)) throw new Error('Invalid workspace draft');
    return data as Envelope<T>;
  }, []);
  useEffect(() => {
    let active = true;
    setReady(false);
    setError('');
    loadWorkspaceDraft(key).then((raw) => {
      const saved = parse(raw);
      let recovery: Envelope<T> | null = null;
      try { recovery = parse(localStorage.getItem(recoveryKey)); } catch { /* A bad recovery copy cannot replace the valid native draft. */ }
      const restored = recovery && (!saved || recovery.updatedAt > saved.updatedAt) ? recovery : saved;
      if (active) {
        latest.current = restored ?? { version: 1, updatedAt: 0, value: initialRef.current };
        setValue(latest.current.value);
        setReady(true);
      }
    }).catch((reason) => { if (active) setError(String(reason)); });
    return () => { active = false; };
  }, [key, attempt, parse, recoveryKey]);
  const setDraft = useCallback((update: SetStateAction<T>) => setValue(update), []);
  const flush = useCallback(async () => {
    const envelope = latest.current;
    const data = JSON.stringify(envelope);
    try { await queueWrite(key, data); setError(''); }
    catch (reason) { setError(String(reason)); throw reason; }
  }, [key]);
  useEffect(() => {
    if (!ready) return;
    latest.current = { version: 1, updatedAt: Date.now(), value };
    // A small synchronous recovery copy also survives Cmd+Q or WebView reload.
    // Large drafts still use the native file and the awaited close flush.
    try { localStorage.setItem(recoveryKey, JSON.stringify(latest.current)); } catch { /* Native storage remains authoritative when WebView quota is full. */ }
    const timer = window.setTimeout(() => { void flush().catch(() => undefined); }, 150);
    return () => window.clearTimeout(timer);
  }, [value, ready, flush, recoveryKey]);
  useEffect(() => {
    if (!ready) return;
    flushers.set(key, flush);
    return () => { void flush().catch(() => undefined); flushers.delete(key); };
  }, [key, ready, flush]);
  return { draft: value, setDraft, ready, error, retry: () => setAttempt((count) => count + 1), flush };
}

export function useWorkspaceCloseFlush() {
  useEffect(() => {
    if (!('__TAURI_INTERNALS__' in window)) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    getCurrentWindow().onCloseRequested(async (event) => {
      try { await flushWorkspaceDrafts(); }
      catch { event.preventDefault(); }
    }).then((stop) => { if (disposed) stop(); else unlisten = stop; }).catch(console.error);
    return () => { disposed = true; unlisten?.(); };
  }, []);
}
