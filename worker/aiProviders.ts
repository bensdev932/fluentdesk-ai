import type { ConversationTurn, CorrectionMode, ScaffoldProgress, TutorReply } from '../src/types/domain';
import { generateTutorCoachingReply } from '../src/services/tutorCoach';
import {
  CLOUD_PROVIDER_ORDER,
  parseModelOverrides,
  resolveProviderEndpoint,
  resolveProviderKey,
  WORKER_PROVIDER_SPECS,
} from './aiCatalog';
import { assembleTutorPrompt, parseTutorOutput, type AssembledPrompt } from './tutorPrompt';
import type { ByokKeys, StoredSettings, WorkerAiProviderId, WorkerEnv, WorkersAiBinding } from './types';

const CLOUD_TIMEOUT_MS = 12_000;

export interface RoutedTurnResult {
  reply: TutorReply;
  completedGoalIds: string[];
  provider?: WorkerAiProviderId;
  model?: string;
  isCloud: boolean;
  failoverNote?: string;
}

export function redactSecrets(input: string, secrets: string[]): string {
  let out = input.replace(/\b(gsk_[A-Za-z0-9_-]+|AIza[A-Za-z0-9_-]+|sk-or-[A-Za-z0-9_-]+)\b/g, '[REDACTED]');
  for (const s of secrets) {
    if (s.length >= 6) out = out.split(s).join('[REDACTED]');
  }
  return out.slice(0, 180);
}

async function callWorkersAi(ai: WorkersAiBinding, model: string, prompt: AssembledPrompt): Promise<string> {
  const res = (await ai.run(model, {
    messages: [{ role: 'system', content: prompt.system }, ...prompt.messages],
    max_tokens: 260,
    temperature: 0.7,
  })) as { response?: unknown; choices?: Array<{ message?: { content?: unknown } }> };
  if (typeof res?.response === 'string' && res.response.trim()) return res.response;
  const choice = res?.choices?.[0]?.message?.content;
  if (typeof choice === 'string' && choice.trim()) return choice;
  throw new Error('Workers AI returned an empty response');
}

async function fetchJsonWithTimeout(
  url: string,
  init: RequestInit,
  fetchImpl: typeof fetch,
): Promise<unknown> {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), CLOUD_TIMEOUT_MS);
  try {
    const res = await fetchImpl(url, { ...init, signal: ctrl.signal });
    if (!res.ok) {
      const txt = await res.text().catch(() => '');
      throw new Error(`HTTP ${res.status}${txt ? `: ${txt.slice(0, 100)}` : ''}`);
    }
    return await res.json();
  } finally {
    clearTimeout(timer);
  }
}

async function callOpenAiCompat(
  id: 'groq' | 'openrouter',
  endpoint: string,
  apiKey: string,
  model: string,
  prompt: AssembledPrompt,
  fetchImpl: typeof fetch,
): Promise<string> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    Authorization: `Bearer ${apiKey}`,
    ...(id === 'openrouter' ? { 'HTTP-Referer': 'https://fluentdesk-ai.workers.dev', 'X-Title': 'FluentDesk AI' } : {}),
  };
  const json = (await fetchJsonWithTimeout(
    endpoint,
    {
      method: 'POST',
      headers,
      body: JSON.stringify({
        model,
        messages: [{ role: 'system', content: prompt.system }, ...prompt.messages],
        temperature: 0.7,
        max_tokens: 280,
      }),
    },
    fetchImpl,
  )) as { choices?: Array<{ message?: { content?: unknown } }> };
  const text = json?.choices?.[0]?.message?.content;
  if (typeof text !== 'string' || !text.trim()) throw new Error(`${id} returned an empty response`);
  return text;
}

async function callGemini(
  baseUrl: string,
  apiKey: string,
  model: string,
  prompt: AssembledPrompt,
  fetchImpl: typeof fetch,
): Promise<string> {
  const url = `${baseUrl}/${encodeURIComponent(model)}:generateContent`;
  const json = (await fetchJsonWithTimeout(
    url,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'x-goog-api-key': apiKey },
      body: JSON.stringify({
        systemInstruction: { parts: [{ text: prompt.system }] },
        contents: prompt.messages.map((m) => ({
          role: m.role === 'user' ? 'user' : 'model',
          parts: [{ text: m.content }],
        })),
        generationConfig: { temperature: 0.7, maxOutputTokens: 280 },
      }),
    },
    fetchImpl,
  )) as { candidates?: Array<{ content?: { parts?: Array<{ text?: unknown }> } }> };
  const parts = json?.candidates?.[0]?.content?.parts ?? [];
  const text = parts.map((p) => (typeof p.text === 'string' ? p.text : '')).join('').trim();
  if (!text) throw new Error('gemini returned an empty response');
  return text;
}

export async function routeCloudTurn(args: {
  env: WorkerEnv;
  settings: StoredSettings;
  byok: ByokKeys;
  history: readonly ConversationTurn[];
  userText: string;
  mode: CorrectionMode;
  scaffold: ScaffoldProgress;
  facts: readonly string[];
  fetchImpl?: typeof fetch;
}): Promise<RoutedTurnResult> {
  const { env, settings, byok, history, userText, mode, scaffold, facts, fetchImpl = fetch } = args;
  if (!settings.cloudOptIn) {
    return {
      reply: generateTutorCoachingReply(userText, mode, history),
      completedGoalIds: [],
      isCloud: false,
      failoverNote: 'Cloud AI Opt-In is disabled — using offline local coach.',
    };
  }
  const overrides = parseModelOverrides(settings.modelOverridesJson);
  const order: WorkerAiProviderId[] = settings.preferredProvider === 'auto'
    ? [...CLOUD_PROVIDER_ORDER]
    : [settings.preferredProvider, ...CLOUD_PROVIDER_ORDER.filter((p) => p !== settings.preferredProvider)];
  const prompt = assembleTutorPrompt(history, userText, mode, scaffold, facts);
  const secrets = [...Object.values(byok), env.GROQ_API_KEY, env.GEMINI_API_KEY, env.OPENROUTER_API_KEY]
    .filter((s): s is string => Boolean(s));
  const failures: string[] = [];

  for (const id of order) {
    const { key } = resolveProviderKey(id, env, byok);
    if (!key) continue;
    const spec = WORKER_PROVIDER_SPECS.find((s) => s.id === id)!;
    const model = (overrides[id] ?? '').trim() || spec.defaultModel;
    try {
      const raw =
        id === 'workers_ai'
          ? await callWorkersAi(env.AI!, model, prompt)
          : id === 'gemini'
            ? await callGemini(resolveProviderEndpoint('gemini', env), key, model, prompt, fetchImpl)
            : await callOpenAiCompat(id, resolveProviderEndpoint(id, env), key, model, prompt, fetchImpl);
      const parsed = parseTutorOutput(raw, mode);
      if (parsed) {
        return {
          ...parsed,
          provider: id,
          model,
          isCloud: true,
          ...(failures.length > 0 ? { failoverNote: `Recovered via ${id} after: ${failures.join('; ')}` } : {}),
        };
      }
      failures.push(`${id}: invalid output`);
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'request failed';
      failures.push(`${id}: ${redactSecrets(msg, secrets)}`);
    }
  }

  return {
    reply: generateTutorCoachingReply(userText, mode, history),
    completedGoalIds: [],
    isCloud: false,
    failoverNote: failures.length > 0
      ? `Live cloud AI unavailable (${failures.join('; ')}) — using offline local coach.`
      : 'No cloud AI provider configured — using offline local coach.',
  };
}
