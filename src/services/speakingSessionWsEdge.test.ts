import { afterEach, describe, expect, it, vi } from 'vitest';
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

  constructor(public readonly url: string) {
    MockWs.instances.push(this);
  }
  send(data: string | ArrayBuffer | ArrayBufferView) {
    this.sent.push(data);
  }
  close() {
    this.readyState = MockWs.CLOSED;
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
});

describe('speakingSessionWs edge cases', () => {
  it('accumulates spoken-turn sentences by turnId, forwards scaffold, and recovers from WS errors', async () => {
    vi.stubGlobal('location', { protocol: 'http:', host: 'localhost:5173' });
    vi.stubGlobal('WebSocket', MockWs as unknown as typeof WebSocket);
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
      sessionId: 'sess-e', state: 'Listening', correctionMode: 'Natural', pushToTalk: false,
      phase: 'Opener', turnCount: 0, maxTurns: 6, cefrLevel: 'B1', topic: 'Travel',
    }), { status: 200 })));

    const client = createSpeakingSessionClient();
    const events: VoiceLoopEvent[] = [];
    client.subscribeVoiceEvents((ev) => events.push(ev));
    await client.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    const ws = MockWs.instances[0];

    ws.triggerMessage({ type: 'sentence', sessionId: 'sess-e', turnId: 'ws-1', text: 'First part.' });
    ws.triggerMessage({ type: 'sentence', sessionId: 'sess-e', turnId: 'ws-1', text: 'Second part?' });
    const streams = events.filter((e): e is Extract<VoiceLoopEvent, { type: 'tutor-reply-streaming' }> => e.type === 'tutor-reply-streaming');
    expect(streams.map((s) => s.partialText)).toEqual(['First part.', 'First part. Second part?']);

    ws.triggerMessage({
      type: 'state_changed', sessionId: 'sess-e', state: 'Listening',
      phase: 'TopicExchange', turnCount: 2, maxTurns: 6, cefrLevel: 'B1', topic: 'Travel',
    });
    const stateWithScaffold = events.find((e) => e.type === 'session-state-changed' && e.scaffold?.phase === 'TopicExchange');
    expect(stateWithScaffold).toBeDefined();

    ws.triggerMessage({ type: 'error', sessionId: 'sess-e', error: 'Turn failed.', kind: 'AiInferenceFailed', recoverable: true });
    ws.triggerMessage({ type: 'state_changed', sessionId: 'sess-e', state: 'Listening' });
    expect(events.at(-1)).toMatchObject({ type: 'session-state-changed', state: 'Listening' });
  });

  it('sends barge_in once during active turn, avoids double-SSE on post-sentence timeout, and encodes Float32 as PCM16', async () => {
    vi.useFakeTimers();
    vi.stubGlobal('location', { protocol: 'http:', host: 'localhost:5173' });
    vi.stubGlobal('WebSocket', MockWs as unknown as typeof WebSocket);
    const fetchMock = vi.fn(async (url: RequestInfo | URL) => {
      if (String(url) === '/api/sessions') {
        return new Response(JSON.stringify({ sessionId: 'sess-b', state: 'Listening', correctionMode: 'Natural', pushToTalk: false }), { status: 200 });
      }
      return new Response('{}', { status: 200 });
    });
    vi.stubGlobal('fetch', fetchMock);

    const spoken: string[] = [];
    const client = createSpeakingSessionClient({
      speakUtterance: (t) => { spoken.push(t); return { done: Promise.resolve(), stop: () => undefined }; },
    });
    await client.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    const ws = MockWs.instances[0];

    const turn1 = client.submitUtteranceText('Cancel me');
    await vi.waitFor(() => expect(ws.sent).toHaveLength(1));
    await client.bargeIn?.();
    await turn1;
    expect(ws.sent.filter((x) => typeof x === 'string' && JSON.parse(x).type === 'barge_in')).toHaveLength(1);

    ws.sent = [];
    const turn2 = client.submitUtteranceText('Timeout after sentence');
    await vi.waitFor(() => expect(ws.sent).toHaveLength(1));
    ws.triggerMessage({ type: 'sentence', sessionId: 'sess-b', turnId: 't-2', text: 'Already streamed.' });
    await vi.advanceTimersByTimeAsync(45_001);
    await turn2;
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(spoken).toEqual(['Already streamed.']);

    client.sendAudioFrame?.(new Float32Array([0.5, -0.5]));
    const sentBin = ws.sent.find((x) => x instanceof ArrayBuffer) as ArrayBuffer;
    expect(sentBin.byteLength).toBe(4);
    expect(Array.from(new Int16Array(sentBin))).toEqual([16383, -16383]);
  });
});
