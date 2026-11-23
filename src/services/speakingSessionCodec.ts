import {
  sanitizeUntrustedText,
  type CefrLevel,
  type CommunicativeGoal,
  type ConversationTurn,
  type CorrectionMode,
  type ScaffoldProgress,
  type SessionError,
  type SessionId,
  type SessionPhase,
  type SessionReview,
  type SessionState,
  type TutorReply,
} from '../types/domain';
import { buildReplySource } from './apiTransport';
import type { ReplySourceStatus, VoiceLoopEvent } from './tauriService';
import { generateTutorCoachingReply } from './tutorCoach';

export const OFFLINE_SOURCE: ReplySourceStatus = {
  kind: 'offline-fallback',
  label: 'Offline Local Coach (Fallback)',
  isCloud: false,
};

export interface SessionTurnHttpResponse {
  userTurn: ConversationTurn;
  tutorTurn: ConversationTurn;
  reply: TutorReply;
  provider?: string;
  model?: string;
  isCloud?: boolean;
  failoverNote?: string;
  state: SessionState;
  phase?: SessionPhase;
  turnCount?: number;
  maxTurns?: number;
  cefrLevel?: CefrLevel;
  topic?: string;
  scenarioId?: string;
  targetVocabulary?: string[];
  goals?: CommunicativeGoal[];
  completedGoalIds?: string[];
  sessionReview?: SessionReview;
}

export function extractScaffold(data: {
  phase?: SessionPhase;
  turnCount?: number;
  maxTurns?: number;
  cefrLevel?: CefrLevel;
  topic?: string;
  scenarioId?: string;
  targetVocabulary?: string[];
  goals?: CommunicativeGoal[];
  completedGoalIds?: string[];
}): ScaffoldProgress | undefined {
  if (!data.phase || data.turnCount === undefined || data.maxTurns === undefined || !data.cefrLevel || !data.topic) {
    return undefined;
  }
  return {
    phase: data.phase,
    turnCount: data.turnCount,
    maxTurns: data.maxTurns,
    cefrLevel: data.cefrLevel,
    topic: data.topic,
    ...(data.scenarioId ? { scenarioId: data.scenarioId } : {}),
    ...(data.targetVocabulary ? { targetVocabulary: data.targetVocabulary } : {}),
    ...(data.goals ? { goals: data.goals } : {}),
    ...(data.completedGoalIds ? { completedGoalIds: data.completedGoalIds } : {}),
  };
}

export function cleanOptionalText(raw: string | null | undefined): string | undefined {
  const cleaned = typeof raw === 'string' ? sanitizeUntrustedText(raw, 2000) : '';
  return cleaned || undefined;
}

function pickFeedbackFields(src: { correction?: string; properWay?: string; why?: string; followUpPrompt?: string }) {
  const correction = cleanOptionalText(src.correction);
  const properWay = cleanOptionalText(src.properWay);
  const why = cleanOptionalText(src.why);
  const followUpPrompt = cleanOptionalText(src.followUpPrompt);
  return { ...(correction ? { correction } : {}), ...(properWay ? { properWay } : {}), ...(why ? { why } : {}), ...(followUpPrompt ? { followUpPrompt } : {}) };
}

export function normalizeTurn(turn: ConversationTurn): ConversationTurn {
  return { id: turn.id, sessionId: turn.sessionId, role: turn.role, text: sanitizeUntrustedText(turn.text, 2000), timestampUnixMs: turn.timestampUnixMs, ...pickFeedbackFields(turn), ...(turn.stageMetadata ? { stageMetadata: turn.stageMetadata } : {}) };
}

function cleanSuggestedReplies(raw?: string[]): string[] | undefined {
  if (!Array.isArray(raw)) return undefined;
  const cleaned = raw.map((item) => sanitizeUntrustedText(String(item), 120).trim()).filter(Boolean).slice(0, 3);
  return cleaned.length > 0 ? cleaned : undefined;
}

export function normalizeReply(reply: TutorReply): TutorReply {
  const suggestedReplies = cleanSuggestedReplies(reply.suggestedReplies);
  return { spokenReply: sanitizeUntrustedText(reply.spokenReply, 2000), ...pickFeedbackFields(reply), ...(suggestedReplies ? { suggestedReplies } : {}) };
}

export function offlineWarning(detail: string): SessionError {
  return {
    kind: 'AiProviderUnavailable',
    userMessage: `Live AI unavailable (${detail}) — using offline local coach.`,
    recoverable: true,
  };
}

export function canTransitionOffline(current: SessionState, op: 'pause' | 'resume' | 'stop'): boolean {
  if (op === 'pause') return current === 'Listening' || current === 'Thinking' || current === 'Speaking';
  if (op === 'resume') return current === 'Paused' || current === 'Error';
  return current !== 'Idle' && current !== 'Ended';
}

export function extractOnlineTurnOutcome(data: SessionTurnHttpResponse): {
  userTurn: ConversationTurn;
  tutorTurn: ConversationTurn;
  reply: TutorReply;
  replySource: ReplySourceStatus;
  playbackWarning?: SessionError;
  finalState: SessionState;
  scaffold?: ScaffoldProgress;
  sessionReview?: SessionReview;
} {
  const note = cleanOptionalText(data.failoverNote);
  const scaffold = extractScaffold(data);
  return {
    userTurn: normalizeTurn(data.userTurn),
    tutorTurn: normalizeTurn(data.tutorTurn),
    reply: normalizeReply(data.reply),
    replySource: buildReplySource(data.provider, data.model, data.isCloud),
    finalState: data.state,
    ...(scaffold ? { scaffold } : {}),
    ...(data.sessionReview ? { sessionReview: data.sessionReview } : {}),
    ...(note ? { playbackWarning: { kind: 'AiProviderUnavailable', userMessage: note, recoverable: true } } : {}),
  };
}

export function buildOfflineTurnOutcome(
  sessionId: SessionId,
  text: string,
  mode: CorrectionMode,
  history: readonly ConversationTurn[],
): { userTurn: ConversationTurn; tutorTurn: ConversationTurn; reply: TutorReply } {
  const reply = normalizeReply(generateTutorCoachingReply(text, mode, history));
  const now = Date.now();
  return {
    reply,
    userTurn: { id: `off-u-${now}`, sessionId, role: 'User', text, timestampUnixMs: now },
    tutorTurn: {
      id: `off-t-${now + 1}`,
      sessionId,
      role: 'Tutor',
      text: reply.spokenReply,
      ...pickFeedbackFields(reply),
      timestampUnixMs: now + 1,
    },
  };
}

export function buildOnlineTurnFailureEvent(
  sessionId: SessionId,
  status: number | undefined,
  message: string,
): { nextState: SessionState; event: VoiceLoopEvent } {
  if (status === 504) {
    return {
      nextState: 'Listening',
      event: {
        type: 'session-state-changed',
        sessionId,
        state: 'Listening',
        warning: { kind: 'AiProviderUnavailable', userMessage: message, recoverable: true },
      },
    };
  }
  const kind = status === 503
    ? 'AiProviderUnavailable'
    : status === 502
      ? 'AiInferenceFailed'
      : status === 500
        ? 'PersistenceFailed'
        : 'InvalidStateTransition';
  return {
    nextState: 'Error',
    event: {
      type: 'session-error',
      sessionId,
      error: { kind, userMessage: message, recoverable: true },
    },
  };
}
