import type {
  ConversationTurn, CorrectionMode, LearnerPreferencesView, LearnerProgress, ScaffoldProgress, SessionError, SessionId, SessionReview, SessionState, TutorReply,
} from '../types/domain';
import { apiUrl, requestJson } from './apiTransport';
import { cancelBrowserSpeech, type SpeakUtteranceOptions } from './browserVoice';
import {
  buildOfflineScaffold, buildOfflineSessionReview, readLocalPreferences, readLocalProgress, recordLocalSessionProgress,
} from './offlineSessionHelpers';
import { buildOfflineScenarioOpener } from './scenariosFallback';
import { OFFLINE_SOURCE, canTransitionOffline, extractScaffold, normalizeReply, offlineWarning } from './speakingSessionCodec';
import type { ReplySourceStatus, SessionStatusSnapshot, StartSpeakingSessionConfig, VoiceLoopEvent } from './tauriService';
import type { SpeechPlaybackProgress } from './speakingSessionStream';
import type { WsSessionHandle } from './speakingSessionWs';

export interface ActiveContext {
  sessionId: SessionId; state: SessionState; correctionMode: CorrectionMode; pushToTalk: boolean;
  offline: boolean; startedAtMs: number; scaffold?: ScaffoldProgress; recentTurns: ConversationTurn[];
}

export interface SessionRuntime {
  baseUrl: string; selectedDeviceId: string | undefined; speakingSpeed: number; englishVoiceId: string;
  active: ActiveContext | null; activeTurnAbort: AbortController | null; stopSpeech: (() => void) | null;
  getSpeechProgress?: (() => SpeechPlaybackProgress) | null; ws: WsSessionHandle | null; offlineSeq: number;
  offlineProgress: LearnerProgress; offlinePrefs: LearnerPreferencesView;
  speak: (text: string, opts?: SpeakUtteranceOptions) => { done: Promise<void>; stop(): void };
  emit: (ev: VoiceLoopEvent) => void; onSessionStarted?: (sessionId: SessionId) => void;
}

export function toSnapshot(ctx: ActiveContext, sessionReview?: SessionReview): SessionStatusSnapshot {
  return {
    sessionId: ctx.sessionId, state: ctx.state, correctionMode: ctx.correctionMode, pushToTalk: ctx.pushToTalk,
    ...(ctx.scaffold ?? {}), ...(sessionReview ? { sessionReview } : {}),
  };
}

export function speakWithRuntimePrefs(rt: SessionRuntime, text: string, rateOverride?: number): { done: Promise<void>; stop(): void } {
  return rt.speak(text, { rate: rateOverride ?? rt.speakingSpeed, englishVoiceId: rt.englishVoiceId });
}

export function abortRuntimeInFlight(rt: SessionRuntime, notifyWsBargeIn = true): boolean {
  rt.stopSpeech?.();
  const progress = rt.getSpeechProgress?.() ?? undefined;
  rt.stopSpeech = null;
  rt.getSpeechProgress = null;
  cancelBrowserSpeech();
  const wsTurnActive = Boolean(rt.ws?.hasActiveTurn());
  rt.activeTurnAbort?.abort();
  rt.activeTurnAbort = null;
  let sentBargeIn = wsTurnActive;
  if (notifyWsBargeIn && !wsTurnActive && rt.active && (rt.active.state === 'Processing' || rt.active.state === 'Thinking' || rt.active.state === 'Speaking')) {
    rt.ws?.sendBargeIn(progress);
    sentBargeIn = true;
  }
  return sentBargeIn;
}

type TurnReadyPayload = {
  userTurn: ConversationTurn; tutorTurn: ConversationTurn; reply: TutorReply; replySource: ReplySourceStatus;
  playbackWarning?: SessionError; finalState?: SessionState; scaffold?: ScaffoldProgress; sessionReview?: SessionReview;
};

function emitTurnReadyEvents(
  rt: SessionRuntime, ctx: ActiveContext, out: TurnReadyPayload, confidence: number, emitSpeakingState: boolean,
): void {
  ctx.recentTurns = [...ctx.recentTurns, out.userTurn, out.tutorTurn].slice(-20);
  rt.emit({ type: 'user-transcript-ready', sessionId: ctx.sessionId, turn: out.userTurn, confidence });
  if (emitSpeakingState) {
    ctx.state = 'Speaking';
    rt.emit({ type: 'session-state-changed', sessionId: ctx.sessionId, state: 'Speaking' });
  }
  rt.emit({
    type: 'tutor-reply-ready', sessionId: ctx.sessionId, turn: out.tutorTurn, reply: out.reply,
    replySource: out.replySource, ...(out.playbackWarning ? { playbackWarning: out.playbackWarning } : {}),
  });
}

function finalizeSpeakingReturn(rt: SessionRuntime, ctx: ActiveContext, ctrl: AbortController, out: TurnReadyPayload): void {
  rt.stopSpeech = null;
  rt.getSpeechProgress = null;
  if (rt.activeTurnAbort === ctrl) rt.activeTurnAbort = null;
  if (!ctrl.signal.aborted && rt.active?.sessionId === ctx.sessionId && ctx.state === 'Speaking') {
    const nextState: SessionState = out.finalState === 'Ended' ? 'Ended' : 'Listening';
    ctx.state = nextState;
    rt.emit({
      type: 'session-state-changed', sessionId: ctx.sessionId, state: nextState,
      ...(out.scaffold ? { scaffold: out.scaffold } : {}), ...(out.sessionReview ? { sessionReview: out.sessionReview } : {}),
    });
  }
}

export async function playReplyAndReturn(
  rt: SessionRuntime, ctx: ActiveContext, ctrl: AbortController, out: TurnReadyPayload, confidence: number,
): Promise<void> {
  emitTurnReadyEvents(rt, ctx, out, confidence, true);
  const speech = speakWithRuntimePrefs(rt, out.reply.spokenReply);
  rt.stopSpeech = speech.stop;
  await speech.done;
  finalizeSpeakingReturn(rt, ctx, ctrl, out);
}

export async function finishStreamedReplyAndReturn(
  rt: SessionRuntime, ctx: ActiveContext, ctrl: AbortController, queue: { flush(): Promise<void>; stop(): void },
  out: TurnReadyPayload, confidence: number,
): Promise<void> {
  emitTurnReadyEvents(rt, ctx, out, confidence, ctx.state !== 'Speaking');
  await queue.flush();
  finalizeSpeakingReturn(rt, ctx, ctrl, out);
}

function startOfflineContext(rt: SessionRuntime, cfg: StartSpeakingSessionConfig, detail: string): SessionStatusSnapshot {
  rt.offlineSeq += 1;
  const sessionId = `offline-${rt.offlineSeq}`;
  const scaffold = buildOfflineScaffold(cfg);
  const openerReply = buildOfflineScenarioOpener(scaffold.scenarioId, scaffold.topic, scaffold.cefrLevel);
  rt.active = { sessionId, state: 'Listening', correctionMode: cfg.correctionMode, pushToTalk: cfg.pushToTalk, offline: true, startedAtMs: Date.now(), scaffold, recentTurns: [] };
  rt.emit({ type: 'session-state-changed', sessionId, state: 'Listening', correctionMode: cfg.correctionMode, scaffold, openerReply, warning: offlineWarning(detail), replySource: OFFLINE_SOURCE });
  rt.stopSpeech = speakWithRuntimePrefs(rt, openerReply.spokenReply).stop;
  return { ...toSnapshot(rt.active), openerReply };
}

export async function startSessionOp(rt: SessionRuntime, cfg: StartSpeakingSessionConfig): Promise<SessionStatusSnapshot> {
  abortRuntimeInFlight(rt, false);
  rt.ws?.close();
  rt.ws = null;
  const device = cfg.inputDeviceId ?? rt.selectedDeviceId;
  const body = {
    correctionMode: cfg.correctionMode, pushToTalk: cfg.pushToTalk,
    ...(device ? { inputDeviceId: device } : {}), ...(cfg.cefrLevel ? { cefrLevel: cfg.cefrLevel } : {}),
    ...(cfg.scenarioId ? { scenarioId: cfg.scenarioId } : {}), ...(cfg.topic ? { topic: cfg.topic } : {}),
    ...(cfg.targetVocabulary?.length ? { targetVocabulary: cfg.targetVocabulary } : {}),
    ...(cfg.maxTurns ? { maxTurns: cfg.maxTurns } : {}),
  };
  const res = await requestJson<SessionStatusSnapshot>(apiUrl('/api/sessions', rt.baseUrl), { method: 'POST', body });
  if (res.ok) {
    const scaffold = extractScaffold(res.data);
    rt.active = { sessionId: res.data.sessionId, state: res.data.state, correctionMode: res.data.correctionMode, pushToTalk: Boolean(res.data.pushToTalk ?? cfg.pushToTalk), offline: false, startedAtMs: Date.now(), ...(scaffold ? { scaffold } : {}), recentTurns: [] };
    const openerReply = res.data.openerReply ? normalizeReply(res.data.openerReply) : undefined;
    rt.emit({ type: 'session-state-changed', sessionId: res.data.sessionId, state: res.data.state, correctionMode: res.data.correctionMode, ...(scaffold ? { scaffold } : {}), ...(openerReply ? { openerReply } : {}) });
    if (openerReply) rt.stopSpeech = speakWithRuntimePrefs(rt, openerReply.spokenReply).stop;
    rt.onSessionStarted?.(res.data.sessionId);
    return res.data;
  }
  if (!res.unreachable) {
    rt.emit({ type: 'session-error', sessionId: rt.active?.sessionId ?? 'session', error: { kind: 'InvalidStateTransition', userMessage: res.message, recoverable: true } });
    throw new Error(res.message);
  }
  return startOfflineContext(rt, cfg, res.message);
}

export function createInitialOfflineState(): { progress: LearnerProgress; prefs: LearnerPreferencesView } {
  return { progress: readLocalProgress(), prefs: readLocalPreferences() };
}

export async function runLifecycleOp(
  rt: SessionRuntime, sessionId: SessionId, op: 'pause' | 'resume' | 'stop', fallbackState: SessionState,
): Promise<SessionStatusSnapshot> {
  if (op === 'stop') { rt.ws?.sendStop(); rt.ws?.close(); rt.ws = null; }
  if (op !== 'resume') abortRuntimeInFlight(rt, op !== 'stop');
  if (rt.active?.offline && rt.active.sessionId === sessionId) {
    if (!canTransitionOffline(rt.active.state, op)) {
      const msg = `Cannot ${op} session from state ${rt.active.state}`;
      rt.emit({ type: 'session-error', sessionId, error: { kind: 'InvalidStateTransition', userMessage: msg, recoverable: true } });
      throw new Error(msg);
    }
    rt.active.state = fallbackState;
    let sessionReview: SessionReview | undefined;
    if (op === 'stop') {
      sessionReview = buildOfflineSessionReview(sessionId, rt.active.scaffold, rt.active.recentTurns);
      recordLocalSessionProgress(sessionReview, rt.active.startedAtMs, rt.offlineProgress);
    }
    rt.emit({ type: 'session-state-changed', sessionId, state: fallbackState, ...(rt.active.scaffold ? { scaffold: rt.active.scaffold } : {}), ...(sessionReview ? { sessionReview } : {}) });
    return toSnapshot(rt.active, sessionReview);
  }
  const res = await requestJson<SessionStatusSnapshot>(apiUrl(`/api/sessions/${encodeURIComponent(sessionId)}/${op}`, rt.baseUrl), { method: 'POST' });
  if (res.ok) {
    if (rt.active?.sessionId === sessionId) rt.active = { ...rt.active, state: res.data.state, correctionMode: res.data.correctionMode };
    const scaffold = extractScaffold(res.data);
    rt.emit({ type: 'session-state-changed', sessionId: res.data.sessionId, state: res.data.state, correctionMode: res.data.correctionMode, ...(scaffold ? { scaffold } : {}), ...(res.data.sessionReview ? { sessionReview: res.data.sessionReview } : {}) });
    return res.data;
  }
  if (res.unreachable && rt.active?.sessionId === sessionId) {
    rt.active.offline = true;
    rt.active.state = fallbackState;
    const sessionReview = op === 'stop' ? buildOfflineSessionReview(sessionId, rt.active.scaffold, rt.active.recentTurns) : undefined;
    if (sessionReview) recordLocalSessionProgress(sessionReview, rt.active.startedAtMs, rt.offlineProgress);
    rt.emit({ type: 'session-state-changed', sessionId, state: fallbackState, warning: offlineWarning(res.message), replySource: OFFLINE_SOURCE, ...(sessionReview ? { sessionReview } : {}) });
    return toSnapshot(rt.active, sessionReview);
  }
  rt.emit({ type: 'session-error', sessionId, error: { kind: 'InvalidStateTransition', userMessage: res.message, recoverable: true } });
  throw new Error(res.message);
}
