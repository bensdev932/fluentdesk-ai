import type { BridgeClient, WorkerEnv } from './types';

const BRIDGE_TIMEOUT_MS = 10_000;

export function createConvexBridgeClient(
  env: WorkerEnv,
  fetchImpl: typeof fetch = fetch,
): BridgeClient {
  const baseUrl = (env.CONVEX_SITE_URL ?? '').trim().replace(/\/+$/, '');
  const secret = (env.CONVEX_BRIDGE_SECRET ?? '').trim();

  async function call<T>(action: string, learnerId: string, payload: Record<string, unknown> = {}): Promise<T> {
    if (!baseUrl || !secret) {
      throw new Error('Convex bridge environment variables (CONVEX_SITE_URL, CONVEX_BRIDGE_SECRET) are not configured.');
    }
    const ctrl = new AbortController();
    const timer = setTimeout(() => ctrl.abort(), BRIDGE_TIMEOUT_MS);
    try {
      const res = await fetchImpl(`${baseUrl}/bridge`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'X-Convex-Bridge-Secret': secret,
        },
        body: JSON.stringify({ action, learnerId, payload }),
        signal: ctrl.signal,
      });
      if (!res.ok) {
        const text = await res.text().catch(() => '');
        throw new Error(`Convex bridge ${action} failed (HTTP ${res.status}): ${text.slice(0, 180)}`);
      }
      return (await res.json()) as T;
    } finally {
      clearTimeout(timer);
    }
  }

  return {
    getSettings: (learnerId) => call('preferences.get', learnerId),
    patchSettings: (learnerId, patch) => call('preferences.patch', learnerId, patch),
    createSession: (learnerId, args) => call('sessions.create', learnerId, args),
    getSession: (learnerId, sessionId) => call('sessions.get', learnerId, { sessionId }),
    listSessions: (learnerId) => call('sessions.list', learnerId),
    recordTurn: (learnerId, args) => call('sessions.recordTurn', learnerId, args),
    updateSessionState: (learnerId, args) => call('sessions.updateState', learnerId, args),
    getReview: (learnerId, sessionId) => call('learner.getReview', learnerId, { sessionId }),
    listFacts: (learnerId) => call('learner.listFacts', learnerId),
    deleteFact: (learnerId, factId) => call('learner.deleteFact', learnerId, { factId }),
    clearFacts: (learnerId) => call('learner.clearFacts', learnerId),
    getProgress: (learnerId, nowUnixMs) => call('learner.getProgress', learnerId, { nowUnixMs }),
    consumeRateLimit: (learnerId, bucketKeys, nowUnixMs) =>
      call('learner.consumeRateLimit', learnerId, { bucketKeys, nowUnixMs }),
  };
}
