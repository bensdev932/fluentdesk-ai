import { afterEach, describe, expect, it, vi } from 'vitest';
import type { VoiceLoopEvent } from './tauriService';
import { createSpeakingSessionClient } from './speakingSessionClient';

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('createSpeakingSessionClient SSE streaming turn execution', () => {
  it('speaks sentence 1 immediately via SSE before done event arrives', async () => {
    const encoder = new TextEncoder();
    let releaseSecondChunk: () => void = () => undefined;
    const secondChunkGate = new Promise<void>((res) => { releaseSecondChunk = res; });
    let firstSpokenResolve: () => void = () => undefined;
    const firstSpoken = new Promise<void>((res) => { firstSpokenResolve = res; });
    const spokenCalls: string[] = [];

    vi.stubGlobal('fetch', vi.fn(async (url: RequestInfo | URL, init?: RequestInit) => {
      const target = String(url);
      if (target === '/api/sessions') {
        return new Response(JSON.stringify({ sessionId: 'sess-1', state: 'Listening', correctionMode: 'Natural', pushToTalk: false }), { status: 200 });
      }
      if (target === '/api/sessions/sess-1/turn') {
        expect((init?.headers as Record<string, string>)?.Accept).toContain('text/event-stream');
        const stream = new ReadableStream<Uint8Array>({
          async start(controller) {
            controller.enqueue(encoder.encode('event: sentence\ndata: {"text":"First sentence!"}\n\n'));
            await secondChunkGate;
            const donePayload = JSON.stringify({
              userTurn: { id: 'u1', sessionId: 'sess-1', role: 'User', text: 'Hello', correction: null, followUpPrompt: null, timestampUnixMs: 10 },
              tutorTurn: { id: 't1', sessionId: 'sess-1', role: 'Tutor', text: 'First sentence! Second part?', correction: 'Try saying hello.', followUpPrompt: 'Second part?', timestampUnixMs: 20 },
              reply: { spokenReply: 'First sentence! Second part?', correction: 'Try saying hello.', followUpPrompt: 'Second part?' },
              provider: 'groq', model: 'openai/gpt-oss-20b', isCloud: true, state: 'Listening',
            });
            controller.enqueue(encoder.encode(`event: sentence\ndata: {"text":"Second part?"}\n\nevent: done\ndata: ${donePayload}\n\n`));
            controller.close();
          },
        });
        return new Response(stream, { status: 200, headers: { 'Content-Type': 'text/event-stream' } });
      }
      return new Response('{}', { status: 200 });
    }));

    const client = createSpeakingSessionClient({
      speakUtterance: (text) => {
        spokenCalls.push(text);
        if (spokenCalls.length === 1) firstSpokenResolve();
        return { done: Promise.resolve(), stop: () => undefined };
      },
    });
    const events: VoiceLoopEvent[] = [];
    client.subscribeVoiceEvents((ev) => events.push(ev));

    await client.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    const turnPromise = client.submitUtteranceText('Hello', 0.96);

    await firstSpoken;
    expect(spokenCalls).toEqual(['First sentence!']);
    expect(events.some((e) => e.type === 'session-state-changed' && e.state === 'Speaking')).toBe(true);
    expect(events.some((e) => e.type === 'tutor-reply-streaming' && e.partialText === 'First sentence!')).toBe(true);

    releaseSecondChunk();
    await turnPromise;

    expect(spokenCalls).toEqual(['First sentence!', 'Second part?']);
    expect(events.some((e) => e.type === 'tutor-reply-streaming' && e.partialText === 'First sentence! Second part?')).toBe(true);
    const replyEv = events.find((e) => e.type === 'tutor-reply-ready');
    expect(replyEv?.type).toBe('tutor-reply-ready');
    if (replyEv?.type === 'tutor-reply-ready') {
      expect(replyEv.reply.correction).toBe('Try saying hello.');
      expect(replyEv.replySource?.isCloud).toBe(true);
    }
    expect(events.at(-1)).toMatchObject({ type: 'session-state-changed', state: 'Listening' });
  });

  it('calls cancelBrowserSpeech on turn start/stop without cancelling between queued sentences', async () => {
    const encoder = new TextEncoder();
    const cancel = vi.fn();
    const spoken: Array<{ text: string; onend: (() => void) | null }> = [];
    vi.stubGlobal('speechSynthesis', {
      speak: vi.fn((u: { text: string; onend: (() => void) | null }) => { spoken.push(u); }),
      cancel,
    });
    vi.stubGlobal('SpeechSynthesisUtterance', class {
      text: string;
      lang = '';
      onend: (() => void) | null = null;
      onerror: (() => void) | null = null;
      constructor(t: string) { this.text = t; }
    });
    vi.stubGlobal('fetch', vi.fn(async (url: RequestInfo | URL) => {
      if (String(url) === '/api/sessions') {
        return new Response(JSON.stringify({ sessionId: 'sess-q', state: 'Listening', correctionMode: 'Natural', pushToTalk: false }), { status: 200 });
      }
      if (String(url) === '/api/sessions/sess-q/turn') {
        const done = JSON.stringify({
          userTurn: { id: 'u1', sessionId: 'sess-q', role: 'User', text: 'Hi', timestampUnixMs: 1 },
          tutorTurn: { id: 't1', sessionId: 'sess-q', role: 'Tutor', text: 'One. Two.', timestampUnixMs: 2 },
          reply: { spokenReply: 'One. Two.' },
          state: 'Listening',
        });
        const stream = new ReadableStream<Uint8Array>({
          start(c) {
            c.enqueue(encoder.encode(`event: sentence\ndata: {"text":"One."}\n\nevent: sentence\ndata: {"text":"Two."}\n\nevent: done\ndata: ${done}\n\n`));
            c.close();
          },
        });
        return new Response(stream, { status: 200, headers: { 'Content-Type': 'text/event-stream' } });
      }
      return new Response(JSON.stringify({ sessionId: 'sess-q', state: 'Ended', correctionMode: 'Natural' }), { status: 200 });
    }));

    const client = createSpeakingSessionClient();
    await client.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    const cancelsAtStart = cancel.mock.calls.length;
    const turnDone = client.submitUtteranceText('Hi');
    await vi.waitFor(() => expect(cancel.mock.calls.length).toBeGreaterThan(cancelsAtStart));
    const cancelsBeforeQueue = cancel.mock.calls.length;

    await vi.waitFor(() => expect(spoken).toHaveLength(1));
    spoken[0].onend?.();
    await vi.waitFor(() => expect(spoken).toHaveLength(2));
    expect(cancel.mock.calls.length).toBe(cancelsBeforeQueue);
    spoken[1].onend?.();
    await turnDone;
    expect(cancel.mock.calls.length).toBe(cancelsBeforeQueue);
  });
});
