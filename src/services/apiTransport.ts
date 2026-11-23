import { buildClientHeaders } from './clientIdentity';
import { resolveApiBaseUrl, type AiProviderId, type ReplySourceStatus } from './tauriService';

const PROVIDER_TAGS: Record<AiProviderId, string> = {
  ollama: 'Local · Ollama',
  workers_ai: 'Cloud · Workers AI',
  groq: 'Cloud · Groq',
  gemini: 'Cloud · Gemini',
  openrouter: 'Cloud · OpenRouter',
  freellmapi: 'Cloud · FreeLLMAPI',
};

export type ApiJsonResult<T> =
  | { ok: true; data: T }
  | { ok: false; unreachable: boolean; status?: number; message: string };

export function apiUrl(path: string, baseUrl = resolveApiBaseUrl()): string {
  return `${baseUrl}${path}`;
}

export function withTimeoutSignal(
  parent: AbortSignal,
  ms: number,
): { signal: AbortSignal; cleanup: () => void } {
  const ctrl = new AbortController();
  const onParentAbort = (): void => ctrl.abort();
  if (parent.aborted) ctrl.abort();
  else parent.addEventListener('abort', onParentAbort, { once: true });
  const timer = setTimeout(() => ctrl.abort(), ms);
  return {
    signal: ctrl.signal,
    cleanup: () => {
      clearTimeout(timer);
      parent.removeEventListener('abort', onParentAbort);
    },
  };
}

export async function readJsonErrorDetail(res: Response): Promise<string | null> {
  try {
    const data = (await res.json()) as { error?: unknown };
    if (typeof data?.error === 'string' && data.error.trim()) return data.error.trim();
  } catch { /* ignore non-JSON body */ }
  return null;
}

export async function readErrorDetail(res: Response): Promise<string> {
  return (await readJsonErrorDetail(res)) ?? `HTTP ${res.status}`;
}

export function buildReplySource(
  provider: unknown,
  model: unknown,
  isCloud: unknown,
): ReplySourceStatus {
  const id = typeof provider === 'string' && provider in PROVIDER_TAGS ? (provider as AiProviderId) : undefined;
  const mod = typeof model === 'string' && model.trim() ? model.trim() : undefined;
  const cloud = typeof isCloud === 'boolean' ? isCloud : Boolean(id && id !== 'ollama');
  const tag = id ? PROVIDER_TAGS[id] : cloud ? 'Live Cloud LLM' : 'Live Local LLM';
  return {
    kind: cloud ? 'live-cloud' : 'live-local',
    label: mod ? `${tag} (${mod})` : tag,
    provider: id,
    model: mod,
    isCloud: cloud,
  };
}

export async function requestJson<T>(
  url: string,
  opts: { method?: 'GET' | 'POST' | 'PATCH' | 'DELETE'; body?: unknown; signal?: AbortSignal; timeoutMs?: number } = {},
): Promise<ApiJsonResult<T>> {
  const parent = opts.signal ?? new AbortController().signal;
  const { signal, cleanup } = withTimeoutSignal(parent, opts.timeoutMs ?? 10_000);
  try {
    const hasBody = opts.body !== undefined;
    const res = await fetch(url, {
      method: opts.method ?? 'GET',
      headers: buildClientHeaders(hasBody ? { 'Content-Type': 'application/json' } : undefined),
      ...(hasBody ? { body: JSON.stringify(opts.body) } : {}),
      signal,
    });
    if (res.ok) return { ok: true, data: (await res.json()) as T };
    const jsonErr = await readJsonErrorDetail(res);
    return {
      ok: false,
      unreachable: res.status >= 500 || ((res.status === 404 || res.status === 405) && jsonErr === null),
      status: res.status,
      message: jsonErr ?? `HTTP ${res.status}`,
    };
  } catch (err) {
    const message = err instanceof Error && err.message ? err.message : 'network error';
    return { ok: false, unreachable: true, message };
  } finally {
    cleanup();
  }
}
