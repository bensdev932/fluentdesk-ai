import { readErrorDetail, withTimeoutSignal, type ApiJsonResult } from './apiTransport';
import { buildClientHeaders } from './clientIdentity';
import type { SessionTurnHttpResponse } from './speakingSessionCodec';

export interface SpeechPlaybackProgress {
  heardSentences: string[];
  playedMs: number;
}

export interface QueuedSpeechHandle {
  enqueue(sentence: string): void;
  hasQueued(): boolean;
  flush(): Promise<void>;
  stop(): void;
  getProgress(): SpeechPlaybackProgress;
}

export function createQueuedSpeechHandle(
  speak: (text: string) => { done: Promise<void>; stop(): void },
): QueuedSpeechHandle {
  let stopped = false;
  let count = 0;
  let elapsedMs = 0;
  let activeStartMs: number | null = null;
  const startedSentences: string[] = [];
  let current: { done: Promise<void>; stop(): void } | null = null;
  let chain: Promise<void> = Promise.resolve();

  return {
    enqueue(sentence: string) {
      const clean = sentence.trim();
      if (!clean || stopped) return;
      count += 1;
      chain = chain.then(async () => {
        if (stopped) return;
        startedSentences.push(clean);
        activeStartMs = Date.now();
        current = speak(clean);
        try {
          await current.done;
        } finally {
          if (activeStartMs !== null) {
            elapsedMs += Math.max(0, Date.now() - activeStartMs);
            activeStartMs = null;
          }
          current = null;
        }
      });
    },
    hasQueued: () => count > 0,
    flush: () => chain,
    stop: () => {
      if (activeStartMs !== null) {
        elapsedMs += Math.max(0, Date.now() - activeStartMs);
        activeStartMs = null;
      }
      stopped = true;
      current?.stop();
      current = null;
    },
    getProgress: () => ({
      heardSentences: [...startedSentences],
      playedMs: Math.round(elapsedMs),
    }),
  };
}

export async function postTurnWithOptionalSse(
  url: string,
  body: unknown,
  opts: { signal: AbortSignal; timeoutMs: number; onSentence: (sentence: string) => void },
): Promise<ApiJsonResult<SessionTurnHttpResponse>> {
  const { signal, cleanup } = withTimeoutSignal(opts.signal, opts.timeoutMs);
  try {
    const res = await fetch(url, {
      method: 'POST',
      headers: buildClientHeaders({
        'Content-Type': 'application/json',
        Accept: 'text/event-stream, application/json',
      }),
      body: JSON.stringify(body),
      signal,
    });
    if (!res.ok) {
      return { ok: false, unreachable: res.status >= 500, status: res.status, message: await readErrorDetail(res) };
    }
    const contentType = res.headers.get('content-type')?.toLowerCase() ?? '';
    if (contentType.includes('text/event-stream') && res.body) {
      return await consumeTurnSseStream(res.body, opts.onSentence);
    }
    return { ok: true, data: (await res.json()) as SessionTurnHttpResponse };
  } catch (err) {
    const message = err instanceof Error && err.message ? err.message : 'network error';
    return { ok: false, unreachable: true, message };
  } finally {
    cleanup();
  }
}

async function consumeTurnSseStream(
  body: ReadableStream<Uint8Array>,
  onSentence: (sentence: string) => void,
): Promise<ApiJsonResult<SessionTurnHttpResponse>> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buf = '';
  let donePayload: SessionTurnHttpResponse | null = null;
  let streamError: string | null = null;

  const processFrame = (frame: string): void => {
    const parsed = parseSseFrame(frame);
    if (!parsed) return;
    if (parsed.event === 'sentence') {
      const obj = JSON.parse(parsed.data) as { text?: unknown };
      if (typeof obj.text === 'string' && obj.text.trim()) onSentence(obj.text.trim());
    } else if (parsed.event === 'done') {
      donePayload = JSON.parse(parsed.data) as SessionTurnHttpResponse;
    } else if (parsed.event === 'error') {
      const obj = JSON.parse(parsed.data) as { error?: string; userMessage?: string };
      streamError = obj.userMessage ?? obj.error ?? 'Tutor streaming failed.';
    }
  };

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true }).replace(/\r\n/g, '\n');
    let split = buf.indexOf('\n\n');
    while (split >= 0) {
      processFrame(buf.slice(0, split));
      buf = buf.slice(split + 2);
      split = buf.indexOf('\n\n');
    }
  }
  if (buf.trim()) processFrame(buf);
  if (streamError) return { ok: false, unreachable: false, status: 502, message: streamError };
  if (donePayload) return { ok: true, data: donePayload };
  return { ok: false, unreachable: true, message: 'SSE turn stream ended before completion.' };
}

function parseSseFrame(raw: string): { event: string; data: string } | null {
  let event = 'message';
  const dataLines: string[] = [];
  for (const line of raw.split('\n')) {
    if (line.startsWith('event:')) event = line.slice(6).trim();
    else if (line.startsWith('data:')) dataLines.push(line.slice(5).trim());
  }
  return dataLines.length > 0 ? { event, data: dataLines.join('\n') } : null;
}
