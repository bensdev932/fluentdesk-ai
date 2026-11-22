import type { ApiJsonResult } from './apiTransport';
import type { SessionTurnHttpResponse } from './speakingSessionCodec';
import type { SpeechPlaybackProgress } from './speakingSessionStream';
import type {
  ConversationTurn,
  CorrectionMode,
  ScaffoldProgress,
  SessionErrorKind,
  SessionId,
  SessionReview,
  SessionState,
  TutorReply,
} from '../types/domain';

export type WsServerMessage =
  | ({
      type: 'state_changed';
      sessionId: SessionId;
      state: SessionState;
      correctionMode?: CorrectionMode;
      pushToTalk?: boolean;
      openerReply?: TutorReply;
      sessionReview?: SessionReview;
    } & Partial<ScaffoldProgress>)
  | { type: 'user_transcript'; sessionId: SessionId; turn: ConversationTurn; confidence?: number }
  | { type: 'sentence'; sessionId: SessionId; turnId: string; text: string }
  | ({ type: 'turn_complete'; sessionId: SessionId; turnId?: string } & SessionTurnHttpResponse)
  | { type: 'error'; sessionId: SessionId; turnId?: string; error: string; kind?: SessionErrorKind; recoverable?: boolean };

export type WsTurnResult = ApiJsonResult<SessionTurnHttpResponse> & { hadSentences?: boolean };

export interface WsSessionHandle {
  isClosed(): boolean;
  hasActiveTurn(): boolean;
  awaitReady(timeoutMs: number, signal: AbortSignal): Promise<boolean>;
  runTextTurn(
    body: { userText: string; correctionMode?: CorrectionMode },
    opts: { signal: AbortSignal; timeoutMs: number; onSentence: (s: string) => void; getSpeechProgress?: () => SpeechPlaybackProgress | undefined },
  ): Promise<WsTurnResult>;
  sendAudioFrame(pcm: Int16Array | Float32Array): void;
  sendPttHold(held: boolean): void;
  sendBargeIn(progress?: SpeechPlaybackProgress): void;
  sendStop(): void;
  close(): void;
}

export function resolveSessionWsUrl(baseUrl: string, sessionId: string): string | null {
  const path = `/api/sessions/${encodeURIComponent(sessionId)}/ws`;
  if (baseUrl.startsWith('http://')) return `ws://${baseUrl.slice(7)}${path}`;
  if (baseUrl.startsWith('https://')) return `wss://${baseUrl.slice(8)}${path}`;
  const loc = (globalThis as { location?: { protocol?: string; host?: string } }).location;
  if (!loc?.host) return null;
  const proto = loc.protocol === 'https:' ? 'wss:' : 'ws:';
  return `${proto}//${loc.host}${path}`;
}

export function openSessionWs(
  baseUrl: string,
  sessionId: string,
  onUnsolicited: (msg: WsServerMessage) => void,
  createWs?: (url: string) => WebSocket,
): WsSessionHandle | null {
  const url = resolveSessionWsUrl(baseUrl, sessionId);
  const ctor = createWs ?? ((u: string) => (typeof WebSocket === 'function' ? new WebSocket(u) : null));
  if (!url) return null;
  let ws: WebSocket | null = null;
  try { ws = ctor(url); } catch { return null; }
  return ws ? createWsHandle(ws, onUnsolicited) : null;
}

function float32ToInt16(src: Float32Array): Int16Array {
  const out = new Int16Array(src.length);
  for (let i = 0; i < src.length; i += 1) {
    out[i] = Math.trunc(Math.max(-1, Math.min(1, src[i])) * 32767);
  }
  return out;
}

function createWsHandle(ws: WebSocket, onUnsolicited: (msg: WsServerMessage) => void): WsSessionHandle {
  const OPEN = 1;
  const CLOSED = 3;
  let closed = false;
  let turnSeq = 0;
  let readyWaiters: Array<(ok: boolean) => void> = [];
  const abandonedTurns = new Set<string>();
  let activeTurn: { turnId: string; seenTurnId?: string; onSentence: (s: string) => void; hadSentences: boolean; resolve: (r: WsTurnResult) => void } | null = null;
  const abandonTurn = (t: { turnId: string; seenTurnId?: string }): void => {
    for (const id of [t.turnId, t.seenTurnId]) {
      if (!id) continue;
      abandonedTurns.add(id);
      if (abandonedTurns.size > 16) {
        const oldest = abandonedTurns.values().next().value;
        if (oldest) abandonedTurns.delete(oldest);
      }
    }
  };
  const flushReady = (ok: boolean): void => { const w = readyWaiters; readyWaiters = []; w.forEach((fn) => fn(ok)); };
  const failActive = (message: string): void => {
    if (!activeTurn) return;
    const t = activeTurn;
    activeTurn = null;
    t.resolve(t.hadSentences ? { ok: false, unreachable: false, status: 502, hadSentences: true, message } : { ok: false, unreachable: true, hadSentences: false, message });
  };

  ws.onopen = () => flushReady(true);
  ws.onerror = () => { closed = true; flushReady(false); failActive('WebSocket error.'); };
  ws.onclose = () => { closed = true; flushReady(false); failActive('WebSocket closed.'); };
  ws.onmessage = (ev: MessageEvent) => {
    if (typeof ev.data !== 'string') return;
    let msg: WsServerMessage;
    try { msg = JSON.parse(ev.data) as WsServerMessage; } catch { return; }
    const msgTurnId = 'turnId' in msg ? msg.turnId : undefined;
    if (msgTurnId && abandonedTurns.has(msgTurnId)) return;
    if (activeTurn) {
      if (msg.type === 'sentence') {
        activeTurn.seenTurnId = msg.turnId;
        activeTurn.hadSentences = true;
        activeTurn.onSentence(msg.text);
        return;
      }
      if (msg.type === 'turn_complete') { const t = activeTurn; activeTurn = null; t.resolve({ ok: true, data: msg }); return; }
      if (msg.type === 'error') {
        const t = activeTurn; activeTurn = null;
        const status = msg.kind === 'AiProviderUnavailable' ? 503 : msg.kind === 'AiInferenceFailed' ? 502 : msg.kind === 'PersistenceFailed' ? 500 : 400;
        t.resolve({ ok: false, unreachable: false, status, hadSentences: t.hadSentences, message: msg.error }); return;
      }
      if (msg.type === 'state_changed' && (msg.state === 'Ended' || msg.state === 'Error')) {
        const t = activeTurn;
        activeTurn = null;
        t.resolve({ ok: false, unreachable: false, status: 409, hadSentences: t.hadSentences, message: `Session entered ${msg.state}.` });
      }
    }
    onUnsolicited(msg);
  };

  const sendJson = (payload: unknown): boolean => {
    if (closed || ws.readyState !== OPEN) return false;
    try { ws.send(JSON.stringify(payload)); return true; } catch { return false; }
  };

  return {
    isClosed: () => closed || ws.readyState === CLOSED,
    hasActiveTurn: () => activeTurn !== null,
    awaitReady: (timeoutMs, signal) => {
      if (closed || ws.readyState === CLOSED || signal.aborted) return Promise.resolve(false);
      if (ws.readyState === OPEN) return Promise.resolve(true);
      return new Promise<boolean>((res) => {
        const done = (ok: boolean): void => { clearTimeout(timer); signal.removeEventListener('abort', onAbort); readyWaiters = readyWaiters.filter((fn) => fn !== done); res(ok); };
        const onAbort = (): void => done(false);
        const timer = setTimeout(() => done(ws.readyState === OPEN), timeoutMs);
        signal.addEventListener('abort', onAbort, { once: true });
        readyWaiters.push(done);
      });
    },
    runTextTurn: (body, opts) => {
      const turnId = `ws-c-${++turnSeq}`;
      if (!sendJson({ type: 'start_turn', turnId, ...body })) {
        return Promise.resolve({ ok: false, unreachable: true, hadSentences: false, message: 'WebSocket not open.' });
      }
      const bargeInFrame = (p?: SpeechPlaybackProgress) =>
        p && p.heardSentences.length > 0 ? { type: 'barge_in', heardSentences: p.heardSentences, playedMs: p.playedMs } : { type: 'barge_in' };
      return new Promise<WsTurnResult>((resolve) => {
        const cleanup = (): void => { clearTimeout(timer); opts.signal.removeEventListener('abort', onAbort); };
        const onAbort = (): void => {
          if (!activeTurn) return;
          const t = activeTurn;
          activeTurn = null;
          abandonTurn(t);
          sendJson(bargeInFrame(opts.getSpeechProgress?.()));
          cleanup();
          resolve({ ok: false, unreachable: false, status: 499, hadSentences: t.hadSentences, message: 'Turn aborted.' });
        };
        const timer = setTimeout(() => {
          if (!activeTurn) return;
          const t = activeTurn;
          activeTurn = null;
          abandonTurn(t);
          sendJson({ type: 'barge_in' });
          cleanup();
          resolve({ ok: false, unreachable: false, status: 504, hadSentences: t.hadSentences, message: 'AI model timed out. Check your selected provider or retry.' });
        }, opts.timeoutMs);
        opts.signal.addEventListener('abort', onAbort, { once: true });
        activeTurn = { turnId, onSentence: opts.onSentence, hadSentences: false, resolve: (r) => { cleanup(); resolve(r); } };
      });
    },
    sendAudioFrame: (pcm) => {
      if (closed || ws.readyState !== OPEN) return;
      const frame = pcm instanceof Float32Array ? float32ToInt16(pcm) : pcm;
      ws.send(frame.buffer.slice(frame.byteOffset, frame.byteOffset + frame.byteLength));
    },
    sendPttHold: (held) => { sendJson({ type: 'ptt_hold', held }); },
    sendBargeIn: (progress) => { sendJson(progress && progress.heardSentences.length > 0 ? { type: 'barge_in', heardSentences: progress.heardSentences, playedMs: progress.playedMs } : { type: 'barge_in' }); },
    sendStop: () => { sendJson({ type: 'stop' }); },
    close: () => { closed = true; flushReady(false); try { ws.close(); } catch { /* ignore */ } },
  };
}
