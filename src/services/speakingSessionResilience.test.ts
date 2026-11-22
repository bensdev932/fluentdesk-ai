import { afterEach, describe, expect, it, vi } from 'vitest';
import { startBrowserSpeechRecognition } from './browserVoice';
import { createSpeakingSessionClient } from './speakingSessionClient';
import type { VoiceLoopEvent } from './tauriService';

class MockWs {
  static OPEN = 1;
  static CLOSED = 3;
  static instances: MockWs[] = [];
  readyState = MockWs.OPEN;
  sent: Array<string | ArrayBuffer | ArrayBufferView> = [];
  onopen: ((ev: unknown) => void) | null = null;
  onmessage: ((ev: { data: string }) => void) | null = null;
  onerror: ((ev: unknown) => void) | null = null;
  onclose: ((ev: unknown) => void) | null = null;
  closed = false;

  constructor(public readonly url: string) {
    MockWs.instances.push(this);
  }
  send(data: string | ArrayBuffer | ArrayBufferView) {
    this.sent.push(data);
  }
  close() {
    this.readyState = MockWs.CLOSED;
    this.closed = true;
    this.onclose?.({});
  }
  triggerMessage(payload: unknown) {
    this.onmessage?.({ data: JSON.stringify(payload) });
  }
}

afterEach(() => {
  MockWs.instances = [];
  vi.unstubAllGlobals();
  vi.useRealTimers();
  delete (globalThis as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
});

describe('speakingSessionClient timeout, error-recovery, and WebView2 STT resilience', () => {
  it('avoids double-SSE retry and offline lockout on pre-sentence WS timeout, allowing immediate retry', async () => {
    vi.useFakeTimers();
    vi.stubGlobal('location', { protocol: 'http:', host: 'localhost:5173' });
    vi.stubGlobal('WebSocket', MockWs as unknown as typeof WebSocket);
    const fetchMock = vi.fn(async () => new Response(JSON.stringify({
      sessionId: 'sess-to', state: 'Listening', correctionMode: 'Natural', pushToTalk: false,
    }), { status: 200 }));
    vi.stubGlobal('fetch', fetchMock);

    const client = createSpeakingSessionClient({
      speakUtterance: () => ({ done: Promise.resolve(), stop: () => undefined }),
    });
    const events: VoiceLoopEvent[] = [];
    client.subscribeVoiceEvents((ev) => events.push(ev));

    await client.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    const ws = MockWs.instances[0];

    const turn1 = client.submitUtteranceText('Cold Ollama turn');
    await vi.waitFor(() => expect(ws.sent).toHaveLength(1));
    await vi.advanceTimersByTimeAsync(45_001);
    await turn1;

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(ws.closed).toBe(false);
    expect(events.at(-1)).toMatchObject({ type: 'session-state-changed', state: 'Listening' });

    const turn2 = client.submitUtteranceText('Second turn succeeds');
    await vi.waitFor(() => expect(ws.sent.filter((m) => String(m).includes('start_turn'))).toHaveLength(2));
    ws.triggerMessage({
      type: 'turn_complete',
      sessionId: 'sess-to',
      userTurn: { id: 'u2', sessionId: 'sess-to', role: 'User', text: 'Second turn succeeds', timestampUnixMs: 10 },
      tutorTurn: { id: 't2', sessionId: 'sess-to', role: 'Tutor', text: 'Welcome back!', timestampUnixMs: 20 },
      reply: { spokenReply: 'Welcome back!' },
      provider: 'ollama', model: 'qwen3.5:4b', isCloud: false, state: 'Listening',
    });
    await turn2;
    expect(events.some((e) => e.type === 'tutor-reply-ready' && e.reply.spokenReply === 'Welcome back!' && e.replySource?.kind === 'live-local')).toBe(true);
  });

  it('clears activeTurnAbort after a turn error so subsequent WS state_changed events are delivered', async () => {
    vi.stubGlobal('location', { protocol: 'http:', host: 'localhost:5173' });
    vi.stubGlobal('WebSocket', MockWs as unknown as typeof WebSocket);
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
      sessionId: 'sess-err', state: 'Listening', correctionMode: 'Natural', pushToTalk: false,
    }), { status: 200 })));

    const client = createSpeakingSessionClient({
      speakUtterance: () => ({ done: Promise.resolve(), stop: () => undefined }),
    });
    const events: VoiceLoopEvent[] = [];
    client.subscribeVoiceEvents((ev) => events.push(ev));

    await client.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    const ws = MockWs.instances[0];

    const turn = client.submitUtteranceText('Will fail');
    await vi.waitFor(() => expect(ws.sent).toHaveLength(1));
    ws.triggerMessage({ type: 'error', sessionId: 'sess-err', error: 'Provider failed.', kind: 'AiInferenceFailed', recoverable: true });
    await turn;

    ws.triggerMessage({ type: 'state_changed', sessionId: 'sess-err', state: 'Listening' });
    expect(events.at(-1)).toMatchObject({ type: 'session-state-changed', state: 'Listening' });
  });

  it('stops restarting SpeechRecognition after fatal or repeated network errors (WebView2)', () => {
    vi.useFakeTimers();
    let startCalls = 0;
    let instance: {
      onerror: ((ev: { error?: string }) => void) | null;
      onend: (() => void) | null;
      start(): void;
      stop(): void;
    } | null = null;
    class FakeRec {
      lang = '';
      continuous = false;
      interimResults = false;
      onresult = null;
      onerror: ((ev: { error?: string }) => void) | null = null;
      onend: (() => void) | null = null;
      constructor() { instance = this; }
      start() { startCalls += 1; }
      stop() { /* noop */ }
    }
    vi.stubGlobal('webkitSpeechRecognition', FakeRec);
    const notes: string[] = [];
    const stop = startBrowserSpeechRecognition({
      onInterim: () => undefined,
      onFinal: () => undefined,
      onStatus: (m) => notes.push(m),
    });

    expect(startCalls).toBe(1);
    instance!.onerror?.({ error: 'network' });
    instance!.onend?.();
    vi.advanceTimersByTime(500);
    expect(startCalls).toBe(2);

    instance!.onerror?.({ error: 'network' });
    instance!.onend?.();
    vi.advanceTimersByTime(5_000);
    expect(startCalls).toBe(2);
    expect(notes.at(-1)?.toLowerCase()).toContain('unavailable');
    stop();
  });

  it('halts immediately on service-not-allowed while still restarting after transient no-speech', () => {
    vi.useFakeTimers();
    let startCalls = 0;
    let instance: {
      onerror: ((ev: { error?: string }) => void) | null;
      onend: (() => void) | null;
      start(): void;
      stop(): void;
    } | null = null;
    class FakeRec {
      lang = '';
      continuous = false;
      interimResults = false;
      onresult = null;
      onerror: ((ev: { error?: string }) => void) | null = null;
      onend: (() => void) | null = null;
      constructor() { instance = this; }
      start() { startCalls += 1; }
      stop() { /* noop */ }
    }
    vi.stubGlobal('webkitSpeechRecognition', FakeRec);
    const notes: string[] = [];
    const stop = startBrowserSpeechRecognition({
      onInterim: () => undefined,
      onFinal: () => undefined,
      onStatus: (m) => notes.push(m),
    });

    instance!.onerror?.({ error: 'no-speech' });
    instance!.onend?.();
    vi.advanceTimersByTime(300);
    expect(startCalls).toBe(2);

    instance!.onerror?.({ error: 'service-not-allowed' });
    instance!.onend?.();
    vi.advanceTimersByTime(5_000);
    expect(startCalls).toBe(2);
    expect(notes.at(-1)?.toLowerCase()).toContain('unavailable');
    stop();
  });
});
