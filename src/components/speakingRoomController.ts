import type { TranscriptItem } from './TranscriptPanel';
import {
  isSessionActiveState,
  type CefrLevel,
  type CommunicativeGoal,
  type CorrectionMode,
  type SessionId,
  type SessionPhase,
  type SessionReview,
  type SessionState,
  type TutorReply,
} from '../types/domain';
import { DEFAULT_ROLEPLAY_SCENARIOS } from '../services/scenariosFallback';
import type { ReplySourceStatus, VoiceLoopEvent } from '../services/tauriService';
import {
  appendTutorReply,
  appendUserTranscript,
  applyStreamingTutorReply,
} from './speakingRoomTurns';

export type ReplySourceKind = ReplySourceStatus['kind'];
export type ReplySourceInfo = ReplySourceStatus;

export interface SpeakingRoomViewModel {
  sessionId: SessionId | null;
  state: SessionState;
  correctionMode: CorrectionMode;
  cefrLevel: CefrLevel;
  scenarioId: string;
  topic: string;
  targetVocabulary: string[];
  goals: CommunicativeGoal[];
  completedGoalIds: string[];
  phase: SessionPhase;
  turnCount: number;
  maxTurns: number;
  pushToTalk: boolean;
  pttHeld: boolean;
  speakingSpeed: number;
  englishVoiceId: string;
  turns: TranscriptItem[];
  streamingTurnId: string | null;
  latestReply: TutorReply | null;
  replySource: ReplySourceStatus | null;
  playbackWarning: string | null;
  errorMessage: string | null;
  sessionReview: SessionReview | null;
}

export interface SpeakingControlPermissions {
  canStart: boolean;
  canPause: boolean;
  canResume: boolean;
  canStop: boolean;
  canHoldPtt: boolean;
}

export function createInitialSpeakingRoomState(): SpeakingRoomViewModel {
  const def = DEFAULT_ROLEPLAY_SCENARIOS[0];
  return {
    sessionId: null,
    state: 'Idle',
    correctionMode: 'Gentle',
    cefrLevel: def.defaultCefr,
    scenarioId: def.id,
    topic: def.title,
    targetVocabulary: def.targetVocabulary,
    goals: def.goals,
    completedGoalIds: [],
    phase: 'Opener',
    turnCount: 0,
    maxTurns: def.defaultMaxTurns,
    pushToTalk: false,
    pttHeld: false,
    speakingSpeed: 1.0,
    englishVoiceId: 'en-US',
    turns: [],
    streamingTurnId: null,
    latestReply: null,
    replySource: null,
    playbackWarning: null,
    errorMessage: null,
    sessionReview: null,
  };
}

export function getSpeakingControlPermissions(
  vm: SpeakingRoomViewModel,
): SpeakingControlPermissions {
  const canStart = vm.state === 'Idle' || vm.state === 'Ended' || vm.state === 'Error';
  return {
    canStart,
    canPause: vm.state === 'Listening',
    canResume: vm.state === 'Paused' || vm.state === 'Error',
    canStop: isSessionActiveState(vm.state),
    canHoldPtt: vm.state === 'Listening' && vm.pushToTalk,
  };
}

export function reduceVoiceLoopEvent(
  vm: SpeakingRoomViewModel,
  event: VoiceLoopEvent,
): SpeakingRoomViewModel {
  switch (event.type) {
    case 'session-state-changed': {
      const isNewSession = vm.sessionId !== null && vm.sessionId !== event.sessionId;
      const isFirstStart = vm.sessionId === null && event.state !== 'Idle';
      const modePatch = event.correctionMode ? { correctionMode: event.correctionMode } : {};
      const scaffoldPatch = event.scaffold ?? {};
      const opener = event.openerReply;
      const openerTurns: TranscriptItem[] = opener
        ? [{ id: `${event.sessionId}-opener`, role: 'Tutor', text: opener.spokenReply, timestampUnixMs: Date.now() }]
        : [];
      if (isNewSession || isFirstStart) {
        return {
          ...vm,
          ...modePatch,
          ...scaffoldPatch,
          sessionId: event.sessionId,
          state: event.state,
          pttHeld: false,
          turns: opener ? openerTurns : isNewSession ? [] : vm.turns,
          streamingTurnId: null,
          latestReply: opener ?? (isNewSession ? null : vm.latestReply),
          replySource: event.replySource ?? (isNewSession ? null : vm.replySource),
          playbackWarning: event.warning?.userMessage ?? null,
          errorMessage: null,
          sessionReview: event.sessionReview ?? null,
        };
      }
      return {
        ...vm,
        ...modePatch,
        ...scaffoldPatch,
        sessionId: event.sessionId,
        state: event.state,
        ...(opener && vm.turns.length === 0 ? { turns: openerTurns, latestReply: opener } : {}),
        streamingTurnId: event.state === 'Speaking' ? vm.streamingTurnId : null,
        ...(event.replySource !== undefined ? { replySource: event.replySource } : {}),
        ...(event.warning !== undefined ? { playbackWarning: event.warning.userMessage } : {}),
        ...(event.sessionReview !== undefined ? { sessionReview: event.sessionReview } : {}),
        errorMessage: null,
      };
    }
    case 'user-transcript-ready':
      return appendUserTranscript(vm, event);
    case 'tutor-reply-streaming':
      return applyStreamingTutorReply(vm, event);
    case 'tutor-reply-ready':
      return appendTutorReply(vm, event);
    case 'session-error':
      return { ...vm, state: 'Error', streamingTurnId: null, errorMessage: event.error.userMessage };
  }
}

export function computeUsedVocabulary(
  targetVocabulary: readonly string[],
  turns: readonly TranscriptItem[],
  reviewVocab?: readonly string[],
): string[] {
  const userSpeech = turns
    .filter((t) => t.role === 'User')
    .map((t) => t.text.toLowerCase())
    .join(' ');
  const reviewSet = new Set((reviewVocab ?? []).map((w) => w.toLowerCase()));
  return targetVocabulary.filter(
    (word) => reviewSet.has(word.toLowerCase()) || userSpeech.includes(word.toLowerCase()),
  );
}

function normalizeFollowUp(text: string): string {
  return text.trim().replace(/\s+/g, ' ').replace(/[.!?]+$/, '').toLowerCase();
}

export function resolveDistinctFollowUp(reply: TutorReply): string | null {
  const candidate = reply.followUpPrompt?.trim();
  if (!candidate) return null;
  if (normalizeFollowUp(candidate) === normalizeFollowUp(reply.spokenReply)) return null;
  return candidate;
}

export function formatReplySourceBadge(
  reply: TutorReply | null,
  replySource?: ReplySourceStatus | null,
): { text: string; tone: 'live' | 'cloud' | 'fallback' | 'idle' } {
  if (!reply) return { text: 'Awaiting Speech', tone: 'idle' };
  if (!replySource) return { text: 'Latest Turn', tone: 'idle' };
  if (replySource.kind === 'offline-fallback') {
    return { text: replySource.label || 'Offline Fallback', tone: 'fallback' };
  }
  const base = replySource.label || (replySource.isCloud ? 'Cloud LLM' : 'Local LLM');
  const suffix = replySource.model && !base.includes(replySource.model) ? ` · ${replySource.model}` : '';
  return { text: `${base}${suffix}`, tone: replySource.isCloud || replySource.kind === 'live-cloud' ? 'cloud' : 'live' };
}
