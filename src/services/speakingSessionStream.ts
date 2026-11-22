import { readJsonErrorDetail, withTimeoutSignal, type ApiJsonResult } from './apiTransport';
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
  let hadSentences = false;
  const onSentence = (s: string): void => {
    hadSentences = true;
    opts.onSentence(s);
  };
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
      const jsonErr = await readJsonErrorDetail(res);
      const bridgeJsonErr = (res.status === 500 || res.status === 502 || res.status === 503) && jsonErr !== null;
      return {
        ok: false,
        unreachable: res.status >= 500 && !bridgeJsonErr,
        status: res.status,
        message: jsonErr ?? `HTTP ${res.status}`,
      };
    }
    const contentType = res.headers.get('content-type')?.toLowerCase() ?? '';
    if (contentType.includes('text/event-stream') && res.body) {
      return await consumeTurnSseStream(res.body, onSentence, () => hadSentences);
    }
    return { ok: true, data: (await res.json()) as SessionTurnHttpResponse };
  } catch (err) {
    if (signal.aborted && !opts.signal.aborted) {
      return { ok: false, unreachable: false, status: 504, message: 'AI model timed out. Check your selected provider or retry.' };
    }
    const message = err instanceof Error && err.message ? err.message : 'network error';
    return { ok: false, unreachable: !hadSentences, ...(hadSentences ? { status: 502 } : {}), message };
  } finally {
    cleanup();
  }
}

async function consumeTurnSseStream(
  body: ReadableStream<Uint8Array>,
  onSentence: (sentence: string) => void,
  hasSentences: () => boolean,
): Promise<ApiJsonResult<SessionTurnHttpResponse>> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buf = '';
  let donePayload: SessionTurnHttpResponse | null = null;
  let streamError: { message: string; status: number } | null = null;

  const processFrame = (frame: string): void => {
    const parsed = parseSseFrame(frame);
    if (!parsed) return;
    try {
      if (parsed.event === 'sentence') {
        const obj = JSON.parse(parsed.data) as { text?: unknown };
        if (typeof obj.text === 'string' && obj.text.trim()) onSentence(obj.text.trim());
      } else if (parsed.event === 'done') {
        donePayload = JSON.parse(parsed.data) as SessionTurnHttpResponse;
      } else if (parsed.event === 'error') {
        const obj = JSON.parse(parsed.data) as { error?: string; userMessage?: string; status?: unknown };
        const status = typeof obj.status === 'number' ? obj.status : 502;
        streamError = { message: obj.userMessage ?? obj.error ?? 'Tutor streaming failed.', status };
      }
    } catch { /* ignore malformed SSE payload */ }
  };

  const appendDecoded = (chunk: string): void => {
    if (!chunk) return;
    buf = (buf + chunk).replace(/\r\n/g, '\n');
    let split = buf.indexOf('\n\n');
    while (split >= 0) {
      processFrame(buf.slice(0, split));
      buf = buf.slice(split + 2);
      split = buf.indexOf('\n\n');
    }
  };

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    appendDecoded(decoder.decode(value, { stream: true }));
  }
  appendDecoded(decoder.decode());
  if (buf.trim()) processFrame(buf);
  if (streamError) {
    const err: { message: string; status: number } = streamError;
    return { ok: false, unreachable: false, status: err.status, message: err.message };
  }
  if (donePayload) return { ok: true, data: donePayload };
  const streamed = hasSentences();
  return { ok: false, unreachable: !streamed, ...(streamed ? { status: 502 } : {}), message: 'SSE turn stream ended before completion.' };
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
