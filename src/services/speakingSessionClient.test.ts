import { afterEach, describe, expect, it, vi } from 'vitest';
import type { VoiceLoopEvent } from './tauriService';
import { createSpeakingSessionClient } from './speakingSessionClient';

function instantSpeech(): { done: Promise<void>; stop(): void } {
  return { done: Promise.resolve(), stop: () => undefined };
}

afterEach(() => {
  vi.unstubAllGlobals();
  delete (globalThis as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
});

describe('createSpeakingSessionClient (/api/sessions port + offline fallback)', () => {
  it('drives /api/sessions lifecycle, turns, and review coaching notes', async () => {
    const calls: Array<{ url: string; method: string; body?: unknown }> = [];
    vi.stubGlobal('fetch', vi.fn(async (url: RequestInfo | URL, init?: RequestInit) => {
      const target = String(url);
      const method = init?.method ?? 'GET';
      const body = init?.body ? JSON.parse(String(init.body)) : undefined;
      calls.push({ url: target, method, body });
      if (method === 'POST' && target === '/api/sessions') {
        return new Response(JSON.stringify({ sessionId: 'sess-1', state: 'Listening', correctionMode: 'Natural', pushToTalk: true }), { status: 200 });
      }
      if (method === 'POST' && target === '/api/sessions/sess-1/turn') {
        return new Response(JSON.stringify({
          userTurn: { id: 'u1', sessionId: 'sess-1', role: 'User', text: body.userText, correction: null, followUpPrompt: null, timestampUnixMs: 10 },
          tutorTurn: { id: 't1', sessionId: 'sess-1', role: 'Tutor', text: 'Nice! Did you enjoy it?', correction: 'Use past tense "went".', followUpPrompt: 'What did you order?', timestampUnixMs: 20 },
          reply: { spokenReply: 'Nice! Did you enjoy it?\u0007', correction: null, followUpPrompt: null },
          provider: 'ollama', model: 'qwen3.5:4b', isCloud: false, state: 'Listening',
        }), { status: 200 });
      }
      if (method === 'POST' && target === '/api/sessions/sess-1/pause') {
        return new Response(JSON.stringify({ sessionId: 'sess-1', state: 'Paused', correctionMode: 'Natural', pushToTalk: true }), { status: 200 });
      }
      if (method === 'POST' && target === '/api/sessions/sess-1/resume') {
        return new Response(JSON.stringify({ sessionId: 'sess-1', state: 'Listening', correctionMode: 'Natural', pushToTalk: true }), { status: 200 });
      }
      if (method === 'POST' && target === '/api/sessions/sess-1/stop') {
        return new Response(JSON.stringify({ sessionId: 'sess-1', state: 'Ended', correctionMode: 'Natural', pushToTalk: true }), { status: 200 });
      }
      if (method === 'GET' && target === '/api/sessions') {
        return new Response(JSON.stringify([{ sessionId: 'sess-1', startedAtUnixMs: 10, endedAtUnixMs: 30, correctionMode: 'Natural', turnCount: 2 }]), { status: 200 });
      }
      if (method === 'GET' && target === '/api/sessions/sess-1') {
        return new Response(JSON.stringify({
          sessionId: 'sess-1', startedAtUnixMs: 10, endedAtUnixMs: 30,
          turns: [
            { id: 'u1', sessionId: 'sess-1', role: 'User', text: 'Yesterday I go.', correction: null, followUpPrompt: null, timestampUnixMs: 10 },
            { id: 't1', sessionId: 'sess-1', role: 'Tutor', text: 'Nice!', correction: 'Use past tense "went".', followUpPrompt: 'What did you order?', timestampUnixMs: 20 },
          ],
        }), { status: 200 });
      }
      return new Response(JSON.stringify({ sessionId: 'sess-1', state: 'Ended', correctionMode: 'Natural', pushToTalk: true }), { status: 200 });
    }));

    const port = createSpeakingSessionClient({ speakUtterance: instantSpeech });
    const events: VoiceLoopEvent[] = [];
    port.subscribeVoiceEvents((ev) => events.push(ev));

    const started = await port.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: true });
    expect(started).toMatchObject({ sessionId: 'sess-1', state: 'Listening', correctionMode: 'Natural', pushToTalk: true });
    await port.submitUtteranceText('Yesterday I go.', 0.95);
    await port.pauseSpeakingSession('sess-1');
    await port.resumeSpeakingSession('sess-1');
    await port.stopSpeakingSession('sess-1');
    const summary = await port.listSessionHistory();
    const review = await port.getSessionHistory('sess-1');

    expect(calls.find((c) => c.url === '/api/sessions/sess-1/turn')?.body).toEqual({ userText: 'Yesterday I go.', correctionMode: 'Natural' });
    expect(summary).toHaveLength(1);
    expect(review.turns[0].correction).toBeUndefined();
    expect(review.turns[1].correction).toBe('Use past tense "went".');
    expect(review.turns[1].followUpPrompt).toBe('What did you order?');
    const replyEv = events.find((e) => e.type === 'tutor-reply-ready');
    expect(replyEv?.type).toBe('tutor-reply-ready');
    if (replyEv?.type === 'tutor-reply-ready') {
      expect(replyEv.reply.spokenReply).toBe('Nice! Did you enjoy it?');
      expect(replyEv.reply.correction).toBeUndefined();
      expect(replyEv.reply.followUpPrompt).toBeUndefined();
    }
    expect(events.some((e) => e.type === 'tutor-reply-ready' && e.replySource?.kind === 'live-local')).toBe(true);
  });

  it('activates explicit offline coaching only when unreachable and preserves 4xx rejections', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ error: 'Ollama down' }), { status: 503 })));
    const port = createSpeakingSessionClient({ speakUtterance: instantSpeech });
    const events: VoiceLoopEvent[] = [];
    port.subscribeVoiceEvents((ev) => events.push(ev));

    expect(await port.listSessionHistory()).toEqual([]);
    const offlineStart = await port.startSpeakingSession({ correctionMode: 'Gentle', pushToTalk: false });
    expect(offlineStart.state).toBe('Listening');
    expect(events[0]).toMatchObject({ type: 'session-state-changed', state: 'Listening', replySource: { kind: 'offline-fallback' } });
    if (events[0].type === 'session-state-changed') {
      expect(events[0].warning?.recoverable).toBe(true);
    }

    await port.submitUtteranceText('Yesterday I go to coffee shop.');
    const replyEv = events.find((e) => e.type === 'tutor-reply-ready');
    expect(replyEv?.type).toBe('tutor-reply-ready');
    if (replyEv?.type === 'tutor-reply-ready') {
      expect(replyEv.replySource?.kind).toBe('offline-fallback');
      expect(replyEv.reply.correction).toContain('simple past');
    }

    await port.stopSpeakingSession(offlineStart.sessionId);
    await expect(port.resumeSpeakingSession(offlineStart.sessionId)).rejects.toThrow();

    vi.stubGlobal('fetch', vi.fn(async (url: RequestInfo | URL) => {
      if (String(url) === '/api/sessions') {
        return new Response(JSON.stringify({ sessionId: 'sess-2', state: 'Listening', correctionMode: 'Natural', pushToTalk: false }), { status: 200 });
      }
      return new Response(JSON.stringify({ error: 'Session is paused' }), { status: 409 });
    }));
    await port.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    await port.submitUtteranceText('Hello there.');
    expect(events.some((e) => e.type === 'session-error' && e.error.userMessage.includes('Session is paused'))).toBe(true);
  });

  it('aborts in-flight turn + TTS on stop and uses 127.0.0.1:3420 under Tauri', async () => {
    let stopCalled = false;
    let releaseTurn: () => void = () => undefined;
    const calledUrls: string[] = [];
    (globalThis as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke: async () => undefined };
    vi.stubGlobal('fetch', vi.fn(async (url: RequestInfo | URL, init?: RequestInit) => {
      const target = String(url);
      calledUrls.push(target);
      if (target.endsWith('/api/sessions')) {
        return new Response(JSON.stringify({ sessionId: 'sess-t', state: 'Listening', correctionMode: 'Natural', pushToTalk: false }), { status: 200 });
      }
      if (target.endsWith('/turn')) {
        await new Promise<void>((resolve) => {
          releaseTurn = resolve;
          init?.signal?.addEventListener('abort', () => resolve(), { once: true });
        });
        throw new DOMException('Aborted', 'AbortError');
      }
      return new Response(JSON.stringify({ sessionId: 'sess-t', state: 'Ended', correctionMode: 'Natural', pushToTalk: false }), { status: 200 });
    }));

    const port = createSpeakingSessionClient({
      speakUtterance: () => ({ done: new Promise(() => undefined), stop: () => { stopCalled = true; } }),
    });
    const events: VoiceLoopEvent[] = [];
    port.subscribeVoiceEvents((ev) => events.push(ev));

    await port.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    const pendingTurn = port.submitUtteranceText('Tell me about Seattle.');
    await port.stopSpeakingSession('sess-t');
    releaseTurn();
    await pendingTurn;

    expect(calledUrls[0]).toBe('http://127.0.0.1:3420/api/sessions');
    expect(events.some((e) => e.type === 'tutor-reply-ready')).toBe(false);
    expect(stopCalled).toBe(false);
  });

  it('treats empty-body Workers 404 on /api/* as static-host offline mode with full 8-scenario loop and preserves JSON 4xx errors', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('', { status: 404 })));
    const spoken: string[] = [];
    const port = createSpeakingSessionClient({
      speakUtterance: (t) => { spoken.push(t); return instantSpeech(); },
    });
    const events: VoiceLoopEvent[] = [];
    port.subscribeVoiceEvents((ev) => events.push(ev));

    const started = await port.startSpeakingSession({ correctionMode: 'Gentle', pushToTalk: false, scenarioId: 'cafe_order' });
    expect(started.state).toBe('Listening');
    expect(started.openerReply?.spokenReply).toContain('Sunrise Cafe');
    expect(spoken[0]).toContain('Sunrise Cafe');
    const scenarios = await port.listScenarios!();
    expect(scenarios).toHaveLength(8);

    await port.submitUtteranceText('Could I order an iced latte with oat milk please?');
    const ended = await port.stopSpeakingSession(started.sessionId);
    expect(ended.state).toBe('Ended');
    expect(ended.sessionReview?.estimateLabel).toBe('AI Estimate');
    expect(events.some((e) => e.type === 'session-error')).toBe(false);
    expect((await port.getLearnerProgress!()).totalSessions).toBeGreaterThanOrEqual(1);

    vi.stubGlobal('fetch', vi.fn(async (url: RequestInfo | URL) => {
      if (String(url) === '/api/sessions') return new Response(JSON.stringify({ sessionId: 's-j', state: 'Listening', correctionMode: 'Natural', pushToTalk: false }), { status: 200 });
      return new Response(JSON.stringify({ error: 'Session is paused' }), { status: 409, headers: { 'Content-Type': 'application/json' } });
    }));
    await port.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    await port.submitUtteranceText('Hello.');
    expect(events.some((e) => e.type === 'session-error' && e.error.userMessage.includes('Session is paused'))).toBe(true);
  });
});
