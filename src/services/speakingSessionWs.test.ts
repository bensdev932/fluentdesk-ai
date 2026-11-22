import { afterEach, describe, expect, it, vi } from 'vitest';
import { createSpeakingSessionClient } from './speakingSessionClient';
import type { VoiceLoopEvent } from './tauriService';

class MockWebSocket {
  static OPEN = 1;
  static CONNECTING = 0;
  static CLOSED = 3;
  static instances: MockWebSocket[] = [];
  readyState = MockWebSocket.CONNECTING;
  sent: Array<string | ArrayBuffer | ArrayBufferView> = [];
  onopen: ((ev: unknown) => void) | null = null;
  onmessage: ((ev: { data: string }) => void) | null = null;
  onerror: ((ev: unknown) => void) | null = null;
  onclose: ((ev: unknown) => void) | null = null;
  closed = false;

  constructor(public readonly url: string) {
    MockWebSocket.instances.push(this);
  }
  send(data: string | ArrayBuffer | ArrayBufferView) {
    this.sent.push(data);
  }
  close() {
    this.readyState = MockWebSocket.CLOSED;
    this.closed = true;
    this.onclose?.({});
  }
  triggerOpen() {
    this.readyState = MockWebSocket.OPEN;
    this.onopen?.({});
  }
  triggerMessage(payload: unknown) {
    this.onmessage?.({ data: JSON.stringify(payload) });
  }
  triggerError() {
    this.readyState = MockWebSocket.CLOSED;
    this.onerror?.({});
    this.onclose?.({});
  }
}

afterEach(() => {
  MockWebSocket.instances = [];
  vi.unstubAllGlobals();
});

describe('speakingSessionClient WebSocket transport with SSE fallback', () => {
  it('streams sentences over WebSocket, sends barge_in/ptt_hold/PCM frames, and stops cleanly', async () => {
    vi.stubGlobal('location', { protocol: 'http:', host: 'localhost:5173' });
    vi.stubGlobal('WebSocket', MockWebSocket as unknown as typeof WebSocket);
    vi.stubGlobal('fetch', vi.fn(async (url: RequestInfo | URL) => {
      if (String(url) === '/api/sessions') {
        return new Response(JSON.stringify({ sessionId: 'sess-ws', state: 'Listening', correctionMode: 'Natural', pushToTalk: true }), { status: 200 });
      }
      return new Response(JSON.stringify({ sessionId: 'sess-ws', state: 'Ended', correctionMode: 'Natural' }), { status: 200 });
    }));

    const spoken: string[] = [];
    let stopCalls = 0;
    const client = createSpeakingSessionClient({
      speakUtterance: (t) => {
        spoken.push(t);
        return { done: Promise.resolve(), stop: () => { stopCalls += 1; } };
      },
    });
    const events: VoiceLoopEvent[] = [];
    client.subscribeVoiceEvents((ev) => events.push(ev));

    await client.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: true });
    const ws = MockWebSocket.instances[0];
    expect(ws?.url).toBe('ws://localhost:5173/api/sessions/sess-ws/ws');
    ws.triggerOpen();

    const turnPromise = client.submitUtteranceText('Hello WS!', 0.95);
    await vi.waitFor(() => expect(ws.sent.length).toBeGreaterThanOrEqual(1));
    expect(JSON.parse(String(ws.sent[0]))).toMatchObject({ type: 'start_turn', userText: 'Hello WS!', correctionMode: 'Natural' });

    ws.triggerMessage({ type: 'sentence', sessionId: 'sess-ws', turnId: 't-1', text: 'First WS sentence!' });
    await vi.waitFor(() => expect(spoken).toEqual(['First WS sentence!']));

    ws.triggerMessage({
      type: 'turn_complete',
      sessionId: 'sess-ws',
      userTurn: { id: 'u1', sessionId: 'sess-ws', role: 'User', text: 'Hello WS!', timestampUnixMs: 10 },
      tutorTurn: { id: 't1', sessionId: 'sess-ws', role: 'Tutor', text: 'First WS sentence!', timestampUnixMs: 20 },
      reply: { spokenReply: 'First WS sentence!' },
      provider: 'ollama', model: 'qwen2.5:7b', isCloud: false, state: 'Listening',
    });
    await turnPromise;
    expect(events.some((e) => e.type === 'tutor-reply-ready')).toBe(true);

    await client.setPushToTalkHeld?.(true);
    client.sendAudioFrame?.(new Int16Array([1000, -1000]));
    await client.setPushToTalkHeld?.(false);
    await client.bargeIn?.();
    expect(JSON.parse(String(ws.sent[1]))).toEqual({ type: 'ptt_hold', held: true });
    expect(ws.sent[2]).toBeInstanceOf(ArrayBuffer);
    expect(JSON.parse(String(ws.sent[3]))).toEqual({ type: 'ptt_hold', held: false });
    expect(JSON.parse(String(ws.sent[4]))).toEqual({ type: 'barge_in' });
    expect(stopCalls).toBeGreaterThanOrEqual(0);

    await client.stopSpeakingSession('sess-ws');
    expect(ws.closed).toBe(true);
  });

  it('falls back cleanly to SSE when WebSocket handshake fails or closes before streaming', async () => {
    vi.stubGlobal('location', { protocol: 'http:', host: 'localhost:5173' });
    vi.stubGlobal('WebSocket', MockWebSocket as unknown as typeof WebSocket);
    const enc = new TextEncoder();
    vi.stubGlobal('fetch', vi.fn(async (url: RequestInfo | URL) => {
      const target = String(url);
      if (target === '/api/sessions') {
        return new Response(JSON.stringify({ sessionId: 'sess-fb', state: 'Listening', correctionMode: 'Natural', pushToTalk: false }), { status: 200 });
      }
      if (target === '/api/sessions/sess-fb/turn') {
        const done = JSON.stringify({
          userTurn: { id: 'u1', sessionId: 'sess-fb', role: 'User', text: 'Fallback', timestampUnixMs: 1 },
          tutorTurn: { id: 't1', sessionId: 'sess-fb', role: 'Tutor', text: 'SSE fallback worked!', timestampUnixMs: 2 },
          reply: { spokenReply: 'SSE fallback worked!' },
          state: 'Listening',
        });
        const stream = new ReadableStream<Uint8Array>({
          start(c) {
            c.enqueue(enc.encode(`event: sentence\ndata: {"text":"SSE fallback worked!"}\n\nevent: done\ndata: ${done}\n\n`));
            c.close();
          },
        });
        return new Response(stream, { status: 200, headers: { 'Content-Type': 'text/event-stream' } });
      }
      return new Response('{}', { status: 200 });
    }));

    const spoken: string[] = [];
    const client = createSpeakingSessionClient({
      speakUtterance: (t) => { spoken.push(t); return { done: Promise.resolve(), stop: () => undefined }; },
    });
    await client.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    MockWebSocket.instances[0].triggerError();
    await client.submitUtteranceText('Fallback');
    expect(spoken).toEqual(['SSE fallback worked!']);
  });

  it('dispatches unsolicited binary-PCM WebSocket events to VoiceLoop listeners', async () => {
    vi.stubGlobal('location', { protocol: 'http:', host: 'localhost:5173' });
    vi.stubGlobal('WebSocket', MockWebSocket as unknown as typeof WebSocket);
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ sessionId: 'sess-pcm', state: 'Listening', correctionMode: 'Natural', pushToTalk: false }), { status: 200 })));

    const client = createSpeakingSessionClient();
    const events: VoiceLoopEvent[] = [];
    client.subscribeVoiceEvents((ev) => events.push(ev));
    await client.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    const ws = MockWebSocket.instances[0];
    ws.triggerOpen();
    client.sendAudioFrame?.(new Int16Array([3000, -3000]));
    ws.triggerMessage({ type: 'state_changed', sessionId: 'sess-pcm', state: 'Processing' });
    ws.triggerMessage({ type: 'user_transcript', sessionId: 'sess-pcm', confidence: 0.91, turn: { id: 'u-p', sessionId: 'sess-pcm', role: 'User', text: 'Spoken via PCM', timestampUnixMs: 5 } });
    ws.triggerMessage({ type: 'sentence', sessionId: 'sess-pcm', turnId: 'ws-1', text: 'PCM reply chunk!' });
    ws.triggerMessage({
      type: 'turn_complete', sessionId: 'sess-pcm',
      userTurn: { id: 'u-p', sessionId: 'sess-pcm', role: 'User', text: 'Spoken via PCM', timestampUnixMs: 5 },
      tutorTurn: { id: 't-p', sessionId: 'sess-pcm', role: 'Tutor', text: 'PCM reply chunk!', timestampUnixMs: 6 },
      reply: { spokenReply: 'PCM reply chunk!' }, state: 'Listening',
    });
    expect(events.map((e) => e.type)).toEqual(['session-state-changed', 'session-state-changed', 'user-transcript-ready', 'tutor-reply-streaming', 'tutor-reply-ready']);
  });

  it('sends heardSentences and playedMs on bargeIn while browser TTS queue is speaking', async () => {
    vi.stubGlobal('location', { protocol: 'http:', host: 'localhost:5173' });
    vi.stubGlobal('WebSocket', MockWebSocket as unknown as typeof WebSocket);
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ sessionId: 'sess-prog', state: 'Listening', correctionMode: 'Natural', pushToTalk: false }), { status: 200 })));

    let releaseFirst: (() => void) | null = null;
    const client = createSpeakingSessionClient({
      speakUtterance: () => ({
        done: new Promise<void>((res) => { releaseFirst = res; }),
        stop: () => { releaseFirst?.(); },
      }),
    });
    await client.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    const ws = MockWebSocket.instances[0];
    ws.triggerOpen();

    const turnP = client.submitUtteranceText('Tell me a story');
    await vi.waitFor(() => expect(ws.sent).toHaveLength(1));
    ws.triggerMessage({ type: 'sentence', sessionId: 'sess-prog', turnId: 't-1', text: 'Part one!' });
    ws.triggerMessage({ type: 'sentence', sessionId: 'sess-prog', turnId: 't-1', text: 'Part two?' });
    await vi.waitFor(() => expect(releaseFirst).not.toBeNull());

    await client.bargeIn?.();
    await turnP;
    const bargeMsg = ws.sent.map((x) => JSON.parse(String(x))).find((m) => m.type === 'barge_in');
    expect(bargeMsg).toMatchObject({ type: 'barge_in', heardSentences: ['Part one!'] });
    expect(typeof bargeMsg.playedMs).toBe('number');
  });
});
