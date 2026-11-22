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
import type { ReplySourceStatus } from './tauriService';
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

export function normalizeTurn(turn: ConversationTurn): ConversationTurn {
  const correction = cleanOptionalText(turn.correction);
  const followUpPrompt = cleanOptionalText(turn.followUpPrompt);
  return {
    id: turn.id,
    sessionId: turn.sessionId,
    role: turn.role,
    text: sanitizeUntrustedText(turn.text, 2000),
    timestampUnixMs: turn.timestampUnixMs,
    ...(correction ? { correction } : {}),
    ...(followUpPrompt ? { followUpPrompt } : {}),
    ...(turn.stageMetadata ? { stageMetadata: turn.stageMetadata } : {}),
  };
}

export function normalizeReply(reply: TutorReply): TutorReply {
  const correction = cleanOptionalText(reply.correction);
  const followUpPrompt = cleanOptionalText(reply.followUpPrompt);
  return {
    spokenReply: sanitizeUntrustedText(reply.spokenReply, 2000),
    ...(correction ? { correction } : {}),
    ...(followUpPrompt ? { followUpPrompt } : {}),
  };
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
      ...(reply.correction ? { correction: reply.correction } : {}),
      ...(reply.followUpPrompt ? { followUpPrompt: reply.followUpPrompt } : {}),
      timestampUnixMs: now + 1,
    },
  };
}
