import { useSyncExternalStore } from 'react';
interface ComposeActivity { progress: number | null; outputPath: string }
let activity: ComposeActivity | null = null;
const listeners = new Set<() => void>();
export function setComposeActivity(next: ComposeActivity | null) {
  activity = next;
  listeners.forEach((listener) => listener());
}
export function useComposeActivity() {
  return useSyncExternalStore((listener) => { listeners.add(listener); return () => { listeners.delete(listener); }; }, () => activity);
}
