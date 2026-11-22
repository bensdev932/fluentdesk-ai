import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { ProvidersSnapshot, RouterConfigPatch } from '../services/tauriService';
import { ProviderKeysPanel } from './ProviderKeysPanel';
import { ProviderSettingsCard } from './ProviderSettingsCard';
import { buildProviderActions, getProviderStatusBadge } from './useProviderSettings';

describe('useProviderSettings thin adapter', () => {
  it('forwards toggleCloudOptIn and selectProvider patches directly without client consent/reset rules', () => {
    const patches: Array<{ patch: RouterConfigPatch; clearDrafts?: boolean }> = [];
    const actions = buildProviderActions(
      async (patch, clearDrafts) => {
        patches.push({ patch, clearDrafts });
      },
      {
        keyDrafts: { groq: 'gsk_new_123', gemini: '   ' },
        modelDrafts: { groq: 'openai/gpt-oss-120b' },
      },
    );

    actions.toggleCloudOptIn(false);
    actions.selectProvider('groq');
    actions.saveKeysAndModels();
    actions.clearKey('gemini');
    actions.retryCloudNow();

    expect(patches).toEqual([
      { patch: { cloudOptIn: false }, clearDrafts: undefined },
      { patch: { preferredProvider: 'groq' }, clearDrafts: undefined },
      {
        patch: {
          apiKeys: { groq: 'gsk_new_123' },
          modelOverrides: { groq: 'openai/gpt-oss-120b' },
        },
        clearDrafts: true,
      },
      { patch: { clearKeys: ['gemini'] }, clearDrafts: undefined },
      { patch: { resetCooldowns: true }, clearDrafts: undefined },
    ]);
  });

  it('computes provider status badges and renders server snapshot directly', () => {
    const snap: ProvidersSnapshot = {
      cloudOptIn: false,
      preferredProvider: 'auto',
      providers: [
        { id: 'ollama', label: 'Ollama (Local)', isCloud: false, configured: true, keySource: 'none', defaultModel: 'llama3.2:3b', freeModels: [], selectedModel: 'llama3.2:3b', cooldownSecs: 0, authInvalid: false, available: true },
        { id: 'groq', label: 'Groq (Free Cloud)', isCloud: true, configured: true, keySource: 'runtime', defaultModel: 'openai/gpt-oss-20b', freeModels: [], selectedModel: 'openai/gpt-oss-20b', cooldownSecs: 0, authInvalid: false, available: false },
        { id: 'gemini', label: 'Gemini (Free Cloud)', isCloud: true, configured: true, keySource: 'env', defaultModel: 'gemini-3.5-flash-lite', freeModels: [], selectedModel: 'gemini-3.5-flash-lite', cooldownSecs: 30, authInvalid: false, available: false },
        { id: 'openrouter', label: 'OpenRouter (Free Cloud)', isCloud: true, configured: true, keySource: 'runtime', defaultModel: 'openrouter/free', freeModels: [], selectedModel: 'openrouter/free', cooldownSecs: 0, authInvalid: true, available: false },
      ],
    };
    expect(getProviderStatusBadge(snap.providers[0])).toEqual({ text: 'Not set', className: 'key-none' });
    expect(getProviderStatusBadge(snap.providers[1])).toEqual({ text: 'Saved', className: 'key-runtime' });
    expect(getProviderStatusBadge(snap.providers[2])).toEqual({ text: 'Cooldown 30s', className: 'key-cooldown' });
    expect(getProviderStatusBadge(snap.providers[3])).toEqual({ text: 'Invalid Key', className: 'key-invalid' });

    const html = renderToStaticMarkup(createElement(ProviderSettingsCard, {
      cfg: {
        snapshot: snap, statusNote: null, busy: false, keyDrafts: {}, modelDrafts: {},
        setKeyDraft: vi.fn(), setModelDraft: vi.fn(),
        ...buildProviderActions(async () => undefined, { keyDrafts: {}, modelDrafts: {} }),
      },
    }));
    expect(html).toContain('value="auto" selected=""');
  });

  it('renders Workers AI Built-in Free badge and hides API key input for workers_ai', () => {
    const snap: ProvidersSnapshot = {
      cloudOptIn: true,
      preferredProvider: 'auto',
      providers: [
        { id: 'workers_ai', label: 'Cloudflare Workers AI (Free Built-in)', isCloud: true, configured: true, keySource: 'env', defaultModel: '@cf/meta/llama-3.1-8b-instruct-fast', freeModels: ['@cf/meta/llama-3.1-8b-instruct-fast'], selectedModel: '@cf/meta/llama-3.1-8b-instruct-fast', cooldownSecs: 0, authInvalid: false, available: true },
        { id: 'groq', label: 'Groq (Free Cloud)', isCloud: true, configured: false, keySource: 'none', defaultModel: 'openai/gpt-oss-20b', freeModels: ['openai/gpt-oss-20b'], selectedModel: 'openai/gpt-oss-20b', cooldownSecs: 0, authInvalid: false, available: false },
      ],
    };
    expect(getProviderStatusBadge(snap.providers[0])).toEqual({ text: 'Built-in Free', className: 'key-env' });
    const cfg = {
      snapshot: snap, statusNote: null, busy: false, keyDrafts: {}, modelDrafts: {},
      setKeyDraft: vi.fn(), setModelDraft: vi.fn(),
      ...buildProviderActions(async () => undefined, { keyDrafts: {}, modelDrafts: {} }),
    };
    const cardHtml = renderToStaticMarkup(createElement(ProviderSettingsCard, { cfg }));
    expect(cardHtml).toContain('Auto Cycle (Workers AI → Cloud)');
    expect(cardHtml).toContain('Cloudflare Workers AI (Free Built-in)');
    const panelHtml = renderToStaticMarkup(createElement(ProviderKeysPanel, { cfg }));
    expect(panelHtml).toContain('Built-in Free');
    expect(panelHtml).not.toContain('Cloudflare Workers AI (Free Built-in) API key');
    expect(panelHtml).toContain('Groq (Free Cloud) API key');
  });
});
