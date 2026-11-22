import type { ProviderDescriptorView, ProviderKeySource, ProvidersSnapshot } from '../src/services/tauriService';
import type { ByokKeys, StoredSettings, WorkerAiProviderId, WorkerEnv } from './types';

export interface WorkerProviderSpec {
  id: WorkerAiProviderId;
  label: string;
  defaultModel: string;
  freeModels: string[];
  defaultEndpoint: string;
}

export const WORKER_PROVIDER_SPECS: readonly WorkerProviderSpec[] = [
  {
    id: 'workers_ai',
    label: 'Cloudflare Workers AI (Free Built-in)',
    defaultModel: '@cf/meta/llama-3.1-8b-instruct-fast',
    freeModels: [
      '@cf/meta/llama-3.1-8b-instruct-fast',
      '@cf/meta/llama-3.3-70b-instruct-fp8-fast',
      '@cf/qwen/qwen2.5-7b-instruct-fp8',
    ],
    defaultEndpoint: '',
  },
  {
    id: 'groq',
    label: 'Groq (Free Cloud)',
    defaultModel: 'openai/gpt-oss-20b',
    freeModels: ['openai/gpt-oss-20b', 'openai/gpt-oss-120b', 'llama-3.3-70b-versatile'],
    defaultEndpoint: 'https://api.groq.com/openai/v1/chat/completions',
  },
  {
    id: 'gemini',
    label: 'Gemini (Free Cloud)',
    defaultModel: 'gemini-2.5-flash-lite',
    freeModels: ['gemini-2.5-flash-lite', 'gemini-2.5-flash', 'gemini-3.5-flash-lite'],
    defaultEndpoint: 'https://generativelanguage.googleapis.com/v1beta/models',
  },
  {
    id: 'openrouter',
    label: 'OpenRouter (Free Cloud)',
    defaultModel: 'openrouter/free',
    freeModels: [
      'openrouter/free',
      'nvidia/nemotron-3.5-lightning:free',
      'meta-llama/llama-3.3-70b-instruct:free',
    ],
    defaultEndpoint: 'https://openrouter.ai/api/v1/chat/completions',
  },
];

export const CLOUD_PROVIDER_ORDER: readonly WorkerAiProviderId[] = [
  'workers_ai',
  'groq',
  'gemini',
  'openrouter',
];

export function parseModelOverrides(json: string): Partial<Record<WorkerAiProviderId, string>> {
  try {
    return JSON.parse(json || '{}') as Partial<Record<WorkerAiProviderId, string>>;
  } catch {
    return {};
  }
}

export function resolveProviderKey(
  id: WorkerAiProviderId,
  env: WorkerEnv,
  byok: ByokKeys,
): { key: string; source: ProviderKeySource } {
  if (id === 'workers_ai') {
    return env.AI ? { key: 'binding', source: 'env' } : { key: '', source: 'none' };
  }
  const userKey = (byok[id] ?? '').trim();
  if (userKey) return { key: userKey, source: 'runtime' };
  const envMap: Record<'groq' | 'gemini' | 'openrouter', string | undefined> = {
    groq: env.GROQ_API_KEY,
    gemini: env.GEMINI_API_KEY,
    openrouter: env.OPENROUTER_API_KEY,
  };
  const serverKey = (envMap[id] ?? '').trim();
  return serverKey ? { key: serverKey, source: 'env' } : { key: '', source: 'none' };
}

export function resolveProviderEndpoint(id: 'groq' | 'gemini' | 'openrouter', env: WorkerEnv): string {
  const custom =
    id === 'groq'
      ? env.FLUENTDESK_GROQ_BASE_URL
      : id === 'gemini'
        ? env.FLUENTDESK_GEMINI_BASE_URL
        : env.FLUENTDESK_OPENROUTER_BASE_URL;
  const spec = WORKER_PROVIDER_SPECS.find((s) => s.id === id)!;
  return (custom ?? '').trim().replace(/\/+$/, '') || spec.defaultEndpoint;
}

export function buildProvidersSnapshot(
  env: WorkerEnv,
  settings: StoredSettings,
  byok: ByokKeys,
): ProvidersSnapshot {
  const overrides = parseModelOverrides(settings.modelOverridesJson);
  const providers: ProviderDescriptorView[] = WORKER_PROVIDER_SPECS.map((spec) => {
    const { source } = resolveProviderKey(spec.id, env, byok);
    const configured = source !== 'none';
    return {
      id: spec.id,
      label: spec.label,
      isCloud: true,
      configured,
      available: configured && settings.cloudOptIn,
      keySource: source,
      defaultModel: spec.defaultModel,
      selectedModel: (overrides[spec.id] ?? '').trim() || spec.defaultModel,
      freeModels: [...spec.freeModels],
      cooldownSecs: 0,
      authInvalid: false,
      statusDetail: configured
        ? spec.id === 'workers_ai'
          ? 'Ready via Cloudflare Workers AI.'
          : `Ready (${source === 'runtime' ? 'browser session key' : 'server secret'}).`
        : 'Add a free API key in Models & Keys or use Workers AI.',
    };
  });
  return {
    cloudOptIn: settings.cloudOptIn,
    preferredProvider: settings.preferredProvider,
    providers,
  };
}
