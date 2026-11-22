import { createElement, isValidElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { EmulatorVoiceBar } from '../components/EmulatorVoiceBar';
import { ProviderKeysPanel } from '../components/ProviderKeysPanel';
import { ProviderSettingsCard } from '../components/ProviderSettingsCard';
import { TutorReplyCard } from '../components/TutorReplyCard';
import { buildProviderActions, type ProviderSettingsHandle } from '../components/useProviderSettings';
import { stubSpeechRecognition } from '../test/voiceTestStubs';
import type { ConversationTurn } from '../types/domain';
import { startBrowserSpeechRecognition } from './browserVoice';
import { createSpeakingSessionClient } from './speakingSessionClient';
import type { ProvidersSnapshot, VoiceLoopEvent } from './tauriService';
import { updateProvidersConfig } from './tutorApi';
import { generateTutorCoachingReply } from './tutorCoach';

function findButtonHandler(node: ReactNode, label: string): (() => void) | undefined {
  if (Array.isArray(node)) return node.map((c) => findButtonHandler(c, label)).find(Boolean);
  if (!isValidElement(node)) return undefined;
  const props = node.props as { children?: ReactNode; onClick?: () => void };
  if (node.type === 'button' && props.children === label) return props.onClick;
  return findButtonHandler(props.children, label);
}

const instantSpeech = (): { done: Promise<void>; stop(): void } => ({ done: Promise.resolve(), stop: () => undefined });

afterEach(() => {
  vi.unstubAllGlobals();
  delete (globalThis as { SpeechRecognition?: unknown }).SpeechRecognition;
});

describe('Unified /api/sessions conversational pipeline, cloud routing status, and STT guard', () => {
  it('routes turns through /api/sessions/{id}/turn without uploading history and falls back when unreachable', async () => {
    const fetchMock = vi.fn(async (url: RequestInfo | URL, _init?: RequestInit) => {
      if (String(url) === '/api/sessions') {
        return new Response(JSON.stringify({ sessionId: 's1', state: 'Listening', correctionMode: 'Gentle', pushToTalk: false }), { status: 200 });
      }
      return new Response(JSON.stringify({
        userTurn: { id: 'u2', sessionId: 's1', role: 'User', text: 'Yes, it came out crispy!', timestampUnixMs: 10 },
        tutorTurn: { id: 't2', sessionId: 's1', role: 'Tutor', text: 'Crispy crust is worth every hour!', followUpPrompt: 'Did you add rosemary?', timestampUnixMs: 20 },
        reply: { spokenReply: 'Crispy crust is worth every hour!', followUpPrompt: 'Did you add rosemary?' },
        provider: 'groq',
        model: 'openai/gpt-oss-20b',
        isCloud: true,
        failoverNote: 'Failover: ollama offline',
        state: 'Listening',
      }), { status: 200 });
    });
    vi.stubGlobal('fetch', fetchMock);

    const client = createSpeakingSessionClient({ speakUtterance: instantSpeech });
    const events: VoiceLoopEvent[] = [];
    client.subscribeVoiceEvents((ev) => events.push(ev));

    await client.startSpeakingSession({ correctionMode: 'Gentle', pushToTalk: false });
    await client.submitUtteranceText('Yes, it came out crispy!');

    const turnCall = fetchMock.mock.calls.find(([u]) => String(u) === '/api/sessions/s1/turn');
    const posted = JSON.parse(String((turnCall?.[1] as RequestInit | undefined)?.body ?? '{}'));
    expect(posted).toEqual({ userText: 'Yes, it came out crispy!', correctionMode: 'Gentle' });
    expect(posted.history).toBeUndefined();

    const cloudReply = events.find((e) => e.type === 'tutor-reply-ready');
    expect(cloudReply?.type).toBe('tutor-reply-ready');
    if (cloudReply?.type === 'tutor-reply-ready') {
      expect(cloudReply.replySource).toMatchObject({ kind: 'live-cloud', provider: 'groq', model: 'openai/gpt-oss-20b', isCloud: true });
      expect(cloudReply.playbackWarning?.userMessage).toContain('Failover: ollama offline');
    }

    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('ECONNREFUSED'); }));
    await client.submitUtteranceText('I hiked Tiger Mountain with Sam.');
    const offlineReply = [...events].reverse().find((e) => e.type === 'tutor-reply-ready');
    expect(offlineReply?.type).toBe('tutor-reply-ready');
    if (offlineReply?.type === 'tutor-reply-ready') {
      expect(offlineReply.replySource?.kind).toBe('offline-fallback');
      expect(offlineReply.playbackWarning?.userMessage).toContain('Live AI unavailable');
      expect(offlineReply.playbackWarning?.recoverable).toBe(true);
    }
  });

  it('builds contextual offline replies and ignores post-stop STT events', () => {
    const history: ConversationTurn[] = [
      { id: 'u1', sessionId: 's1', role: 'User', text: 'I visited Kyoto with Elena last autumn.', timestampUnixMs: 1 },
    ];
    const r = generateTutorCoachingReply('We drank matcha near a bamboo grove.', 'Natural', history);
    expect(r.spokenReply).toMatch(/Kyoto|Elena/i);
    expect(r.spokenReply).toMatch(/bamboo|matcha/i);

    const getRec = stubSpeechRecognition();
    const interims: string[] = [];
    const finals: string[] = [];
    const stop = startBrowserSpeechRecognition({ onInterim: (t) => interims.push(t), onFinal: (t) => finals.push(t), onStatus: () => undefined });
    getRec()?.onresult?.({ resultIndex: 0, results: [{ isFinal: false, 0: { transcript: 'testing interim' } }] });
    stop();
    getRec()?.onresult?.({ resultIndex: 0, results: [{ isFinal: true, 0: { transcript: 'late final', confidence: 0.9 } }] });
    expect(interims).toEqual(['testing interim']);
    expect(finals).toEqual([]);
  });

  it('renders cloud vs local badges, STT previews, and gated cloud settings safely', () => {
    const interimBar = renderToStaticMarkup(createElement(EmulatorVoiceBar, {
      canSpeak: true,
      voice: { micLevel: 0.4, interimText: 'I missed the train', finalText: '', voiceNote: 'Listening' },
      onSubmitText: () => undefined,
      onSimulateSample: () => undefined,
    }));
    expect(interimBar).toContain('data-stt-stage="interim"');

    const cloudCard = renderToStaticMarkup(createElement(TutorReplyCard, {
      reply: { spokenReply: 'Oh no! Did you take the bus instead?', followUpPrompt: 'Did you take the bus instead?' },
      playbackWarning: null,
      replySource: { kind: 'live-cloud', label: 'Cloud · Gemini (gemini-3.5-flash-lite)', provider: 'gemini', model: 'gemini-3.5-flash-lite', isCloud: true },
    }));
    expect(cloudCard).toContain('data-source-tone="cloud"');
    expect(cloudCard).toContain('Cloud · Gemini (gemini-3.5-flash-lite)');

    const snap: ProvidersSnapshot = {
      cloudOptIn: false,
      preferredProvider: 'auto',
      providers: [
        { id: 'ollama', label: 'Ollama (Local)', isCloud: false, configured: true, keySource: 'none', defaultModel: 'qwen3.5:4b', freeModels: [], selectedModel: 'qwen3.5:4b', cooldownSecs: 0, available: true },
        { id: 'groq', label: 'Groq (Free Cloud)', isCloud: true, configured: true, keySource: 'env', defaultModel: 'openai/gpt-oss-20b', freeModels: ['openai/gpt-oss-20b'], selectedModel: 'openai/gpt-oss-20b', cooldownSecs: 0, available: false },
      ],
    };
    const settingsHtml = renderToStaticMarkup(createElement(ProviderSettingsCard, {
      cfg: { snapshot: snap, statusNote: null, busy: false, keyDrafts: {}, modelDrafts: {}, setKeyDraft: () => undefined, setModelDraft: () => undefined, toggleCloudOptIn: () => undefined, selectProvider: () => undefined, saveKeysAndModels: () => undefined, clearKey: () => undefined, retryCloudNow: () => undefined },
    }));
    expect(settingsHtml).toContain('Auto Cycle (Cloud → Local Ollama)');
    expect(settingsHtml).toContain('Cloud AI Opt-In');
    expect(settingsHtml).toContain('Groq (Free Cloud)');
    expect(settingsHtml).toContain('Gemini (Free Cloud)');
    expect(settingsHtml).toContain('OpenRouter (Free Cloud)');
  });

  it('renders Invalid Key / Cooldown 45s badges, posts resetCooldowns, and shows Ollama failover note', async () => {
    const snap: ProvidersSnapshot = {
      cloudOptIn: true,
      preferredProvider: 'auto',
      providers: [
        { id: 'ollama', label: 'Ollama (Local)', isCloud: false, configured: true, keySource: 'none', defaultModel: 'qwen3.5:4b', freeModels: [], selectedModel: 'qwen3.5:4b', cooldownSecs: 0, authInvalid: false, available: true },
        { id: 'groq', label: 'Groq (Free Cloud)', isCloud: true, configured: true, keySource: 'runtime', defaultModel: 'openai/gpt-oss-20b', freeModels: [], selectedModel: 'openai/gpt-oss-20b', cooldownSecs: 0, authInvalid: true, available: false },
        { id: 'gemini', label: 'Gemini (Free Cloud)', isCloud: true, configured: true, keySource: 'env', defaultModel: 'gemini-3.5-flash-lite', freeModels: [], selectedModel: 'gemini-3.5-flash-lite', cooldownSecs: 45, authInvalid: false, available: false },
      ],
    };
    const postedCalls: Array<{ url: string; method?: string; body: unknown }> = [];
    vi.stubGlobal('fetch', vi.fn(async (url: RequestInfo | URL, init?: RequestInit) => {
      postedCalls.push({ url: String(url), method: init?.method, body: JSON.parse(String(init?.body ?? '{}')) });
      return new Response(JSON.stringify(snap), { status: 200 });
    }));
    const actions = buildProviderActions(async (patch) => { await updateProvidersConfig(patch); }, {
      keyDrafts: {}, modelDrafts: {},
    });
    const cfg: ProviderSettingsHandle = {
      snapshot: snap, statusNote: null, busy: false, keyDrafts: {}, modelDrafts: {},
      setKeyDraft: () => undefined, setModelDraft: () => undefined, ...actions,
    };
    const drawerHtml = renderToStaticMarkup(createElement(ProviderSettingsCard, { cfg, defaultOpenKeys: true }));
    expect(drawerHtml).toContain('Invalid Key');
    expect(drawerHtml).toContain('Cooldown 45s');
    expect(drawerHtml).toContain('Retry Cloud Now');

    findButtonHandler(ProviderKeysPanel({ cfg }), 'Retry Cloud Now')?.();
    await vi.waitFor(() => expect(postedCalls).toEqual([{ url: '/api/providers/config', method: 'POST', body: { resetCooldowns: true } }]));

    vi.stubGlobal('fetch', vi.fn(async (url: RequestInfo | URL) => {
      if (String(url) === '/api/sessions') {
        return new Response(JSON.stringify({ sessionId: 's-ol', state: 'Listening', correctionMode: 'Gentle', pushToTalk: false }), { status: 200 });
      }
      return new Response(JSON.stringify({
        userTurn: { id: 'u1', sessionId: 's-ol', role: 'User', text: 'I visited a quiet bakery.', timestampUnixMs: 1 },
        tutorTurn: { id: 't1', sessionId: 's-ol', role: 'Tutor', text: 'That bakery sounds cozy! Which pastry did you try?', followUpPrompt: 'Which pastry did you try?', timestampUnixMs: 2 },
        reply: { spokenReply: 'That bakery sounds cozy! Which pastry did you try?', followUpPrompt: 'Which pastry did you try?' },
        provider: 'ollama',
        model: 'qwen3.5:4b',
        isCloud: false,
        failoverNote: 'Cloud providers cooling down; fell back to local Ollama.',
        state: 'Listening',
      }), { status: 200 });
    }));
    const client = createSpeakingSessionClient({ speakUtterance: instantSpeech });
    const replyEvents: Array<Extract<VoiceLoopEvent, { type: 'tutor-reply-ready' }>> = [];
    client.subscribeVoiceEvents((ev) => { if (ev.type === 'tutor-reply-ready') replyEvents.push(ev); });
    await client.startSpeakingSession({ correctionMode: 'Gentle', pushToTalk: false });
    await client.submitUtteranceText('I visited a quiet bakery.');

    const latest = replyEvents[0];
    const replyHtml = renderToStaticMarkup(createElement(TutorReplyCard, {
      reply: latest.reply,
      playbackWarning: latest.playbackWarning?.userMessage ?? null,
      replySource: latest.replySource ?? null,
    }));
    expect(replyHtml).toContain('data-source-tone="live"');
    expect(replyHtml).toContain('Local · Ollama (qwen3.5:4b)');
    expect(replyHtml).toContain('Cloud providers cooling down; fell back to local Ollama.');
  });
});
