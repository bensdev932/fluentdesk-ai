import { sanitizeUntrustedText } from '../types/domain';
import { extractOnlineTurnOutcome, extractScaffold, normalizeTurn } from './speakingSessionCodec';
import type { SessionRuntime } from './speakingSessionLifecycle';
import type { WsServerMessage } from './speakingSessionWs';

export function createWsEventHandler(rt: SessionRuntime): {
  onWsEvent: (msg: WsServerMessage) => void;
  clearStreamBuf: () => void;
} {
  const wsStreamBuf = new Map<string, string>();
  const clearStreamBuf = (): void => { wsStreamBuf.clear(); };

  const onWsEvent = (msg: WsServerMessage): void => {
    const active = rt.active;
    if (!active || active.sessionId !== msg.sessionId) return;
    if (msg.type === 'state_changed') {
      if (rt.activeTurnAbort && msg.state !== 'Ended' && msg.state !== 'Error') return;
      const scaffold = extractScaffold(msg);
      if (active.state !== msg.state || scaffold || msg.sessionReview) {
        active.state = msg.state;
        if (msg.correctionMode) active.correctionMode = msg.correctionMode;
        rt.emit({
          type: 'session-state-changed',
          sessionId: msg.sessionId,
          state: msg.state,
          correctionMode: msg.correctionMode,
          ...(scaffold ? { scaffold } : {}),
          ...(msg.sessionReview ? { sessionReview: msg.sessionReview } : {}),
        });
      }
      return;
    }
    if (msg.type === 'user_transcript') {
      rt.emit({
        type: 'user-transcript-ready',
        sessionId: msg.sessionId,
        turn: normalizeTurn(msg.turn),
        confidence: msg.confidence ?? 0.95,
      });
      return;
    }
    if (msg.type === 'sentence' && !rt.activeTurnAbort) {
      const clean = sanitizeUntrustedText(msg.text, 2000);
      if (!clean) return;
      const prev = wsStreamBuf.get(msg.turnId) ?? '';
      const next = prev ? `${prev} ${clean}` : clean;
      wsStreamBuf.set(msg.turnId, next);
      rt.emit({ type: 'tutor-reply-streaming', sessionId: msg.sessionId, turnId: msg.turnId, partialText: next });
      return;
    }
    if (msg.type === 'turn_complete') {
      wsStreamBuf.clear();
      const out = extractOnlineTurnOutcome(msg);
      active.state = out.finalState;
      active.recentTurns = [...active.recentTurns, out.userTurn, out.tutorTurn].slice(-20);
      rt.emit({
        type: 'tutor-reply-ready',
        sessionId: msg.sessionId,
        turn: out.tutorTurn,
        reply: out.reply,
        replySource: out.replySource,
        ...(out.playbackWarning ? { playbackWarning: out.playbackWarning } : {}),
      });
      if (out.scaffold || out.finalState === 'Ended' || out.sessionReview) {
        rt.emit({
          type: 'session-state-changed',
          sessionId: msg.sessionId,
          state: out.finalState,
          ...(out.scaffold ? { scaffold: out.scaffold } : {}),
          ...(out.sessionReview ? { sessionReview: out.sessionReview } : {}),
        });
      }
      return;
    }
    if (msg.type === 'error') {
      wsStreamBuf.clear();
      if (msg.recoverable !== false && !msg.kind && active.state === 'Listening') {
        rt.emit({
          type: 'session-state-changed',
          sessionId: msg.sessionId,
          state: active.state,
          warning: { kind: 'InvalidStateTransition', userMessage: msg.error, recoverable: true },
        });
        return;
      }
      active.state = 'Error';
      rt.emit({
        type: 'session-error',
        sessionId: msg.sessionId,
        error: { kind: msg.kind ?? 'AiInferenceFailed', userMessage: msg.error, recoverable: msg.recoverable ?? true },
      });
    }
  };

  return { onWsEvent, clearStreamBuf };
}
