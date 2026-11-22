import { isTauriRuntime, type AiProviderId } from './tauriService';

export type ByokProviderId = 'groq' | 'gemini' | 'openrouter';
export type ByokKeys = Partial<Record<ByokProviderId, string>>;

const LEARNER_STORAGE_KEY = 'fluentdesk.learnerId.v1';
const BYOK_STORAGE_KEY = 'fluentdesk.byok.v1';
const LEARNER_ID_RE = /^[A-Za-z0-9_-]{8,80}$/;

let memoryLearnerId: string | null = null;
let memoryByok: ByokKeys = {};

function safeStorage(kind: 'local' | 'session'): Storage | null {
  try {
    const store = kind === 'local' ? globalThis.localStorage : globalThis.sessionStorage;
    return store ?? null;
  } catch {
    return null;
  }
}

function randomLearnerId(): string {
  const uuid = globalThis.crypto?.randomUUID?.();
  return uuid ?? `lrn-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

export function getOrCreateLearnerId(): string {
  const store = safeStorage('local');
  const existing = store?.getItem(LEARNER_STORAGE_KEY)?.trim() ?? '';
  if (LEARNER_ID_RE.test(existing)) return existing;
  if (!memoryLearnerId || !LEARNER_ID_RE.test(memoryLearnerId)) {
    memoryLearnerId = randomLearnerId();
  }
  try {
    store?.setItem(LEARNER_STORAGE_KEY, memoryLearnerId);
  } catch { /* ignore storage quota/privacy restrictions */ }
  return memoryLearnerId;
}

export function readByokKeys(): ByokKeys {
  const store = safeStorage('session');
  if (!store) return { ...memoryByok };
  try {
    const raw = store.getItem(BYOK_STORAGE_KEY);
    return raw ? (JSON.parse(raw) as ByokKeys) : {};
  } catch {
    return { ...memoryByok };
  }
}

export function saveByokKeys(keys?: Partial<Record<AiProviderId, string>>): ByokKeys {
  const next: ByokKeys = { ...readByokKeys() };
  if (keys) {
    for (const id of ['groq', 'gemini', 'openrouter'] as const) {
      const val = keys[id]?.trim();
      if (val) next[id] = val;
    }
  }
  memoryByok = next;
  try {
    safeStorage('session')?.setItem(BYOK_STORAGE_KEY, JSON.stringify(next));
  } catch { /* ignore */ }
  return next;
}

export function clearByokKey(id: AiProviderId): ByokKeys {
  const next: ByokKeys = { ...readByokKeys() };
  if (id === 'groq' || id === 'gemini' || id === 'openrouter') delete next[id];
  memoryByok = next;
  try {
    safeStorage('session')?.setItem(BYOK_STORAGE_KEY, JSON.stringify(next));
  } catch { /* ignore */ }
  return next;
}

export function buildClientHeaders(extra: Record<string, string> = {}): Record<string, string> {
  const headers: Record<string, string> = {
    ...extra,
    'X-Learner-Id': getOrCreateLearnerId(),
  };
  if (!isTauriRuntime()) {
    const keys = readByokKeys();
    if (keys.groq?.trim()) headers['X-Byok-Groq'] = keys.groq.trim();
    if (keys.gemini?.trim()) headers['X-Byok-Gemini'] = keys.gemini.trim();
    if (keys.openrouter?.trim()) headers['X-Byok-Openrouter'] = keys.openrouter.trim();
  }
  return headers;
}
