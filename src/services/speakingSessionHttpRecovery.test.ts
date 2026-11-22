import { afterEach, describe, expect, it, vi } from 'vitest';
import { createSpeakingSessionClient } from './speakingSessionClient';
import type { VoiceLoopEvent } from './tauriService';

const instantSpeech = (): { done: Promise<void>; stop(): void } => ({
  done: Promise.resolve(),
  stop: () => undefined,
});

const sessionResp = (sessionId: string): Response => new Response(JSON.stringify({
  sessionId, state: 'Listening', correctionMode: 'Natural', pushToTalk: false,
}), { status: 200, headers: { 'Content-Type': 'application/json' } });

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('HTTP SSE/REST 502/503 recovery without permanent offline lockout', () => {
  it('surfaces reachable bridge 502/503 JSON turn errors without locking ctx.offline and stays live after resume', async () => {
    let turnAttempt = 0;
    vi.stubGlobal('fetch', vi.fn(async (url: RequestInfo | URL) => {
      const target = String(url);
      if (target === '/api/sessions' || target === '/api/sessions/sess-live/resume') return sessionResp('sess-live');
      if (target === '/api/sessions/sess-live/turn') {
        turnAttempt += 1;
        if (turnAttempt === 1) return new Response(JSON.stringify({ error: 'Ollama error: CUDA out of memory' }), { status: 502 });
        if (turnAttempt === 2) return new Response(JSON.stringify({ error: 'All AI providers are cooling down' }), { status: 503 });
        return new Response(JSON.stringify({
          userTurn: { id: 'u3', sessionId: 'sess-live', role: 'User', text: 'Try again', timestampUnixMs: 10 },
          tutorTurn: { id: 't3', sessionId: 'sess-live', role: 'Tutor', text: 'Back online!', timestampUnixMs: 20 },
          reply: { spokenReply: 'Back online!' }, provider: 'ollama', model: 'qwen3.5:4b', isCloud: false, state: 'Listening',
        }), { status: 200 });
      }
      return new Response('{}', { status: 200 });
    }));

    const client = createSpeakingSessionClient({ speakUtterance: instantSpeech });
    const events: VoiceLoopEvent[] = [];
    client.subscribeVoiceEvents((ev) => events.push(ev));

    await client.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    await client.submitUtteranceText('First attempt');
    expect(events.some((e) => e.type === 'tutor-reply-ready')).toBe(false);
    expect(events.at(-1)).toMatchObject({ type: 'session-error', error: { kind: 'AiInferenceFailed', userMessage: 'Ollama error: CUDA out of memory' } });

    await client.resumeSpeakingSession('sess-live');
    await client.submitUtteranceText('Second attempt');
    expect(events.at(-1)).toMatchObject({ type: 'session-error', error: { kind: 'AiProviderUnavailable', userMessage: 'All AI providers are cooling down' } });

    await client.resumeSpeakingSession('sess-live');
    await client.submitUtteranceText('Try again');
    expect(turnAttempt).toBe(3);
    expect(events.some((e) => e.type === 'tutor-reply-ready' && e.reply.spokenReply === 'Back online!' && e.replySource?.kind === 'live-local')).toBe(true);
  });

  it('still falls back to offline coach when a proxy/gateway returns non-JSON 502 HTML on turn', async () => {
    vi.stubGlobal('fetch', vi.fn(async (url: RequestInfo | URL) => (
      String(url) === '/api/sessions'
        ? sessionResp('sess-gw')
        : new Response('<html><body>502 Bad Gateway</body></html>', { status: 502, headers: { 'Content-Type': 'text/html' } })
    )));
    const client = createSpeakingSessionClient({ speakUtterance: instantSpeech });
    const events: VoiceLoopEvent[] = [];
    client.subscribeVoiceEvents((ev) => events.push(ev));

    await client.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    await client.submitUtteranceText('Hello from behind a dead proxy.');

    const replyEv = events.find((e) => e.type === 'tutor-reply-ready');
    expect(replyEv?.type).toBe('tutor-reply-ready');
    if (replyEv?.type === 'tutor-reply-ready') expect(replyEv.replySource?.kind).toBe('offline-fallback');
  });

  it('parses SSE frames when CRLF delimiters and UTF-8 bytes split across chunks', async () => {
    const encoder = new TextEncoder();
    const done = JSON.stringify({
      userTurn: { id: 'u1', sessionId: 'sess-crlf', role: 'User', text: 'Hi', timestampUnixMs: 1 },
      tutorTurn: { id: 't1', sessionId: 'sess-crlf', role: 'Tutor', text: 'Café 😊!', timestampUnixMs: 2 },
      reply: { spokenReply: 'Café 😊!' }, provider: 'ollama', model: 'qwen3.5:4b', isCloud: false, state: 'Listening',
    });
    const part1 = encoder.encode('event: sentence\r\ndata: {"text":"Caf\u00e9 \u{1f60a}!"}\r\n\r');
    const part2 = encoder.encode(`\nevent: done\r\ndata: ${done}\r\n\r\n`);
    vi.stubGlobal('fetch', vi.fn(async (url: RequestInfo | URL) => {
      if (String(url) === '/api/sessions') return sessionResp('sess-crlf');
      const stream = new ReadableStream<Uint8Array>({ start(c) { c.enqueue(part1); c.enqueue(part2); c.close(); } });
      return new Response(stream, { status: 200, headers: { 'Content-Type': 'text/event-stream' } });
    }));

    const client = createSpeakingSessionClient({ speakUtterance: instantSpeech });
    const events: VoiceLoopEvent[] = [];
    client.subscribeVoiceEvents((ev) => events.push(ev));
    await client.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    await client.submitUtteranceText('Hi');

    const replyEv = events.find((e) => e.type === 'tutor-reply-ready');
    expect(replyEv?.type).toBe('tutor-reply-ready');
    if (replyEv?.type === 'tutor-reply-ready') {
      expect(replyEv.reply.spokenReply).toBe('Café 😊!');
      expect(replyEv.replySource?.kind).toBe('live-local');
    }
  });

  it('preserves 503 AiProviderUnavailable from SSE event: error and WS error kind', async () => {
    const encoder = new TextEncoder();
    vi.stubGlobal('fetch', vi.fn(async (url: RequestInfo | URL) => {
      if (String(url) === '/api/sessions') return sessionResp('sess-503');
      const stream = new ReadableStream<Uint8Array>({
        start(c) { c.enqueue(encoder.encode('event: error\ndata: {"error":"Local Ollama offline","status":503}\n\n')); c.close(); },
      });
      return new Response(stream, { status: 200, headers: { 'Content-Type': 'text/event-stream' } });
    }));
    const client = createSpeakingSessionClient({ speakUtterance: instantSpeech });
    const events: VoiceLoopEvent[] = [];
    client.subscribeVoiceEvents((ev) => events.push(ev));
    await client.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    await client.submitUtteranceText('Hello');
    expect(events.at(-1)).toMatchObject({ type: 'session-error', error: { kind: 'AiProviderUnavailable', userMessage: 'Local Ollama offline' } });

    class StubWs {
      static OPEN = 1;
      readyState = 1;
      onmessage: ((ev: { data: string }) => void) | null = null;
      send() {
        queueMicrotask(() => this.onmessage?.({
          data: JSON.stringify({ type: 'error', sessionId: 'sess-503', error: 'All cloud providers unavailable', kind: 'AiProviderUnavailable', recoverable: true }),
        }));
      }
      close() { this.readyState = 3; }
    }
    vi.stubGlobal('fetch', vi.fn(async () => sessionResp('sess-503')));
    const wsClient = createSpeakingSessionClient({ baseUrl: 'http://127.0.0.1:3420', speakUtterance: instantSpeech, createWebSocket: () => new StubWs() as unknown as WebSocket });
    const wsEvents: VoiceLoopEvent[] = [];
    wsClient.subscribeVoiceEvents((ev) => wsEvents.push(ev));
    await wsClient.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    await wsClient.submitUtteranceText('Hello via WS');
    expect(wsEvents.at(-1)).toMatchObject({ type: 'session-error', error: { kind: 'AiProviderUnavailable', userMessage: 'All cloud providers unavailable' } });
  });
});
