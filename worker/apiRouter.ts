import type { RouterConfigPatch } from '../src/services/tauriService';
import type { LearnerPreferencesPatch } from '../src/types/domain';
import { DEFAULT_ROLEPLAY_SCENARIOS } from '../src/services/scenariosFallback';
import { buildProvidersSnapshot } from './aiCatalog';
import { handleGetSessionDetails, handleSessionLifecycle, handleSessionTurn, handleStartSession } from './sessionHandlers';
import type { BridgeClient, ByokKeys, WorkerAiProviderId, WorkerEnv } from './types';

const LEARNER_ID_RE = /^[A-Za-z0-9_-]{8,80}$/;

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store' },
  });
}

function readLearnerId(req: Request, allowDefault: boolean): string | null {
  const raw = (req.headers.get('X-Learner-Id') ?? '').trim();
  if (LEARNER_ID_RE.test(raw)) return raw;
  return allowDefault ? 'anon-guest' : null;
}

function readByokKeys(req: Request): ByokKeys {
  const keys: ByokKeys = {};
  const g = req.headers.get('X-Byok-Groq')?.trim();
  const gem = req.headers.get('X-Byok-Gemini')?.trim();
  const or = req.headers.get('X-Byok-Openrouter')?.trim();
  if (g) keys.groq = g;
  if (gem) keys.gemini = gem;
  if (or) keys.openrouter = or;
  return keys;
}

async function hashClientIp(req: Request): Promise<string | undefined> {
  const ip = req.headers.get('CF-Connecting-IP')?.trim();
  if (!ip) return undefined;
  const buf = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(ip));
  return [...new Uint8Array(buf)].slice(0, 12).map((b) => b.toString(16).padStart(2, '0')).join('');
}

function toPreferencesView(s: Awaited<ReturnType<BridgeClient['getSettings']>>) {
  return {
    correctionMode: s.correctionMode, speakingSpeed: s.speakingSpeed,
    pushToTalkEnabled: s.pushToTalkEnabled, cefrLevel: s.cefrLevel, englishVoiceId: s.englishVoiceId,
  };
}

export async function handleApiRequest(
  req: Request,
  env: WorkerEnv,
  bridge: BridgeClient,
  fetchImpl: typeof fetch = fetch,
): Promise<Response> {
  const path = new URL(req.url).pathname;
  const method = req.method.toUpperCase();
  if (method === 'OPTIONS') return new Response(null, { status: 204 });

  if (path === '/api/health' && method === 'GET') {
    const snap = buildProvidersSnapshot(env, await bridge.getSettings(readLearnerId(req, true)!), readByokKeys(req));
    const configured = snap.providers.filter((p) => p.configured).map((p) => p.id);
    return json(200, { status: configured.length > 0 ? 'available' : 'degraded', mode: 'cloudflare-worker', configuredProviders: configured });
  }
  if (path === '/api/scenarios' && method === 'GET') return json(200, DEFAULT_ROLEPLAY_SCENARIOS);

  const learnerId = readLearnerId(req, method === 'GET');
  if (!learnerId) return json(400, { error: 'Missing or invalid X-Learner-Id header.' });
  const byok = readByokKeys(req);

  if (path === '/api/providers' && method === 'GET') {
    return json(200, buildProvidersSnapshot(env, await bridge.getSettings(learnerId), byok));
  }
  if (path === '/api/providers/config' && method === 'POST') {
    const patch = (await req.json().catch(() => null)) as RouterConfigPatch | null;
    if (!patch) return json(400, { error: 'Invalid provider configuration payload.' });
    const pref = patch.preferredProvider;
    const validPref = pref === 'auto' || pref === 'workers_ai' || pref === 'groq' || pref === 'gemini' || pref === 'openrouter' ? pref as 'auto' | WorkerAiProviderId : undefined;
    const updated = await bridge.patchSettings(learnerId, {
      ...(patch.cloudOptIn !== undefined ? { cloudOptIn: Boolean(patch.cloudOptIn) } : {}),
      ...(validPref ? { preferredProvider: validPref } : {}),
      ...(patch.modelOverrides ? { modelOverrides: patch.modelOverrides as Record<string, string> } : {}),
    });
    return json(200, buildProvidersSnapshot(env, updated, byok));
  }
  if (path === '/api/preferences' && method === 'GET') return json(200, toPreferencesView(await bridge.getSettings(learnerId)));
  if (path === '/api/preferences' && method === 'PATCH') {
    const patch = (await req.json().catch(() => null)) as LearnerPreferencesPatch | null;
    if (!patch) return json(400, { error: 'Invalid learner preferences payload.' });
    return json(200, toPreferencesView(await bridge.patchSettings(learnerId, patch)));
  }
  if (path === '/api/learner/progress' && method === 'GET') return json(200, await bridge.getProgress(learnerId));
  if (path === '/api/learner/facts' && method === 'GET') return json(200, await bridge.listFacts(learnerId));
  if (path === '/api/learner/facts' && method === 'DELETE') { await bridge.clearFacts(learnerId); return json(200, { ok: true }); }
  const factMatch = path.match(/^\/api\/learner\/facts\/([^/]+)$/);
  if (factMatch && method === 'DELETE') { await bridge.deleteFact(learnerId, decodeURIComponent(factMatch[1])); return json(200, { ok: true }); }

  if (path === '/api/sessions' && method === 'POST') return handleStartSession(req, bridge, learnerId);
  if (path === '/api/sessions' && method === 'GET') return json(200, await bridge.listSessions(learnerId));

  const sessSub = path.match(/^\/api\/sessions\/([^/]+)(?:\/(turn|turn\/stream|pause|resume|stop|status|review))?$/);
  if (sessSub) {
    const sid = decodeURIComponent(sessSub[1]);
    const sub = sessSub[2];
    if (!sub && method === 'GET') return handleGetSessionDetails(sid, 'history', bridge, learnerId);
    if (sub === 'status' && method === 'GET') return handleGetSessionDetails(sid, 'status', bridge, learnerId);
    if (sub === 'review' && method === 'GET') return handleGetSessionDetails(sid, 'review', bridge, learnerId);
    if ((sub === 'pause' || sub === 'resume' || sub === 'stop') && method === 'POST') return handleSessionLifecycle(sid, sub, bridge, learnerId);
    if ((sub === 'turn' || sub === 'turn/stream') && method === 'POST') {
      const sse = sub === 'turn/stream' || (req.headers.get('Accept') ?? '').toLowerCase().includes('text/event-stream');
      return handleSessionTurn({ req, sessionId: sid, streamSse: sse, learnerId, ipHash: await hashClientIp(req), byok, env, bridge, fetchImpl });
    }
  }
  return json(404, { error: 'Not found' });
}
