import { afterEach, describe, expect, it, vi } from 'vitest';
import { buildClientHeaders, clearByokKey, getOrCreateLearnerId, readByokKeys, saveByokKeys } from './clientIdentity';
import { updateProvidersConfig } from './tutorApi';

function makeStorage(): Storage {
  const map = new Map<string, string>();
  return {
    get length() { return map.size; },
    clear: () => map.clear(),
    getItem: (k) => map.get(k) ?? null,
    key: (i) => [...map.keys()][i] ?? null,
    removeItem: (k) => { map.delete(k); },
    setItem: (k, v) => { map.set(k, v); },
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
  delete (globalThis as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
});

describe('clientIdentity & BYOK header isolation', () => {
  it('persists X-Learner-Id and sends BYOK headers only on web runtime', () => {
    vi.stubGlobal('localStorage', makeStorage());
    vi.stubGlobal('sessionStorage', makeStorage());

    const id1 = getOrCreateLearnerId();
    const id2 = getOrCreateLearnerId();
    expect(id1).toBe(id2);
    expect(id1.length).toBeGreaterThanOrEqual(8);

    saveByokKeys({ groq: 'gsk_web_only', gemini: 'AIza_web_only' });
    expect(readByokKeys()).toEqual({ groq: 'gsk_web_only', gemini: 'AIza_web_only' });
    const webHeaders = buildClientHeaders({ 'Content-Type': 'application/json' });
    expect(webHeaders['X-Learner-Id']).toBe(id1);
    expect(webHeaders['X-Byok-Groq']).toBe('gsk_web_only');
    expect(webHeaders['X-Byok-Gemini']).toBe('AIza_web_only');

    (globalThis as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    expect(buildClientHeaders()['X-Byok-Groq']).toBeUndefined();

    clearByokKey('groq');
    expect(readByokKeys()).toEqual({ gemini: 'AIza_web_only' });
  });

  it('strips apiKeys and clearKeys from web /api/providers/config POST body', async () => {
    vi.stubGlobal('localStorage', makeStorage());
    vi.stubGlobal('sessionStorage', makeStorage());
    const calls: Array<{ body: string; headers: Record<string, string> }> = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: RequestInfo | URL, init?: RequestInit) => {
      calls.push({ body: String(init?.body ?? ''), headers: (init?.headers ?? {}) as Record<string, string> });
      return new Response(JSON.stringify({ cloudOptIn: true, preferredProvider: 'groq', providers: [] }), { status: 200 });
    }));

    await updateProvidersConfig({ preferredProvider: 'groq', apiKeys: { groq: 'gsk_session_123' } });
    expect(calls[0].body).toBe(JSON.stringify({ preferredProvider: 'groq' }));
    expect(calls[0].headers['X-Byok-Groq']).toBe('gsk_session_123');
  });
});
