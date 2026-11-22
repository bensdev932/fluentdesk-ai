import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { VoiceOrbStage } from '../components/VoiceOrbStage';
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

  constructor(public readonly url: string) { MockWs.instances.push(this); }
  send(data: string | ArrayBuffer | ArrayBufferView) { this.sent.push(data); }
  close() { this.readyState = MockWs.CLOSED; this.onclose?.({}); }
  triggerMessage(payload: unknown) { this.onmessage?.({ data: JSON.stringify(payload) }); }
}

afterEach(() => {
  MockWs.instances = [];
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe('SSE timeout recovery, WS stale-turn correlation, and Thinking cancel control', () => {
  it('recovers to Listening without offline lockout when HTTP SSE turn times out', async () => {
    vi.useFakeTimers();
    let turnAttempts = 0;
    vi.stubGlobal('fetch', vi.fn(async (url: RequestInfo | URL, init?: RequestInit) => {
      const target = String(url);
      if (target === '/api/sessions') {
        return new Response(JSON.stringify({
          sessionId: 'sess-sse-to', state: 'Listening', correctionMode: 'Natural', pushToTalk: false,
        }), { status: 200 });
      }
      if (target === '/api/sessions/sess-sse-to/turn') {
        turnAttempts += 1;
        if (turnAttempts === 1) {
          return await new Promise<Response>((_res, rej) => {
            init?.signal?.addEventListener('abort', () => rej(new DOMException('Aborted', 'AbortError')), { once: true });
          });
        }
        return new Response(JSON.stringify({
          userTurn: { id: 'u2', sessionId: 'sess-sse-to', role: 'User', text: 'Retry live', timestampUnixMs: 10 },
          tutorTurn: { id: 't2', sessionId: 'sess-sse-to', role: 'Tutor', text: 'Live backend still works!', timestampUnixMs: 20 },
          reply: { spokenReply: 'Live backend still works!' },
          provider: 'ollama', model: 'qwen3.5:4b', isCloud: false, state: 'Listening',
        }), { status: 200 });
      }
      return new Response('{}', { status: 200 });
    }));

    const client = createSpeakingSessionClient({
      speakUtterance: () => ({ done: Promise.resolve(), stop: () => undefined }),
    });
    const events: VoiceLoopEvent[] = [];
    client.subscribeVoiceEvents((ev) => events.push(ev));

    await client.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    const t1 = client.submitUtteranceText('Cold SSE turn');
    await vi.advanceTimersByTimeAsync(45_001);
    await t1;

    expect(events.some((e) => e.type === 'tutor-reply-ready')).toBe(false);
    expect(events.at(-1)).toMatchObject({ type: 'session-state-changed', state: 'Listening' });

    await client.submitUtteranceText('Retry live');
    expect(turnAttempts).toBe(2);
    expect(events.some((e) => e.type === 'tutor-reply-ready' && e.reply.spokenReply === 'Live backend still works!' && e.replySource?.kind === 'live-local')).toBe(true);
  });

  it('ignores stale turn_complete from a timed-out WS turn so it cannot corrupt turn 2', async () => {
    vi.useFakeTimers();
    vi.stubGlobal('location', { protocol: 'http:', host: 'localhost:5173' });
    vi.stubGlobal('WebSocket', MockWs as unknown as typeof WebSocket);
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
      sessionId: 'sess-stale', state: 'Listening', correctionMode: 'Natural', pushToTalk: false,
    }), { status: 200 })));

    const client = createSpeakingSessionClient({
      speakUtterance: () => ({ done: Promise.resolve(), stop: () => undefined }),
    });
    const events: VoiceLoopEvent[] = [];
    client.subscribeVoiceEvents((ev) => events.push(ev));

    await client.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    const ws = MockWs.instances[0];

    const turn1 = client.submitUtteranceText('Slow turn 1');
    await vi.waitFor(() => expect(ws.sent).toHaveLength(1));
    const t1Frame = JSON.parse(String(ws.sent[0])) as { turnId?: string };
    expect(typeof t1Frame.turnId).toBe('string');

    await vi.advanceTimersByTimeAsync(45_001);
    await turn1;

    const turn2 = client.submitUtteranceText('Fast turn 2');
    await vi.waitFor(() => expect(ws.sent.filter((m) => String(m).includes('start_turn'))).toHaveLength(2));
    const t2Frame = JSON.parse(String(ws.sent.filter((m) => String(m).includes('start_turn'))[1])) as { turnId?: string };
    expect(t2Frame.turnId).not.toBe(t1Frame.turnId);

    // Late stale response from turn 1 arrives while turn 2 is in flight!
    ws.triggerMessage({
      type: 'turn_complete',
      sessionId: 'sess-stale',
      turnId: t1Frame.turnId,
      userTurn: { id: 'u1', sessionId: 'sess-stale', role: 'User', text: 'Slow turn 1', timestampUnixMs: 1 },
      tutorTurn: { id: 't1', sessionId: 'sess-stale', role: 'Tutor', text: 'STALE REPLY', timestampUnixMs: 2 },
      reply: { spokenReply: 'STALE REPLY' },
      provider: 'ollama', model: 'qwen3.5:4b', isCloud: false, state: 'Listening',
    });

    ws.triggerMessage({
      type: 'turn_complete',
      sessionId: 'sess-stale',
      turnId: t2Frame.turnId,
      userTurn: { id: 'u2', sessionId: 'sess-stale', role: 'User', text: 'Fast turn 2', timestampUnixMs: 3 },
      tutorTurn: { id: 't2', sessionId: 'sess-stale', role: 'Tutor', text: 'Fresh turn 2 reply!', timestampUnixMs: 4 },
      reply: { spokenReply: 'Fresh turn 2 reply!' },
      provider: 'ollama', model: 'qwen3.5:4b', isCloud: false, state: 'Listening',
    });
    await turn2;

    const replies = events.filter((e): e is Extract<VoiceLoopEvent, { type: 'tutor-reply-ready' }> => e.type === 'tutor-reply-ready');
    expect(replies).toHaveLength(1);
    expect(replies[0].reply.spokenReply).toBe('Fresh turn 2 reply!');
  });

  it('renders a cancel control in VoiceOrbStage while Thinking so users are never trapped', () => {
    const html = renderToStaticMarkup(createElement(VoiceOrbStage, {
      state: 'Thinking',
      review: null,
      onBargeIn: () => undefined,
      onRestart: () => undefined,
      onReturnToHub: () => undefined,
    }));
    expect(html).toContain('Cancel');
  });
});
