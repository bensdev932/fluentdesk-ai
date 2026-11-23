import { sanitizeUntrustedText, type LearnerPreferencesView, type LearnerProgress, type RoleplayScenario } from '../types/domain';
import { apiUrl, requestJson } from './apiTransport';
import { cancelBrowserSpeech, speakTutorUtterance, type SpeakUtteranceOptions } from './browserVoice';
import {
  advanceOfflineScaffold, buildOfflineSessionReview, readLocalPreferences, readLocalProgress, recordLocalSessionProgress, saveLocalPreferences,
} from './offlineSessionHelpers';
import { SAMPLE_UTTERANCES } from './sampleUtterances';
import { DEFAULT_ROLEPLAY_SCENARIOS } from './scenariosFallback';
import { clearAllLearnerFacts, fetchLearnerFacts, fetchSessionReview, removeLearnerFact } from './reviewApi';
import {
  OFFLINE_SOURCE, buildOfflineTurnOutcome, buildOnlineTurnFailureEvent, extractOnlineTurnOutcome, normalizeTurn, offlineWarning,
} from './speakingSessionCodec';
import {
  abortRuntimeInFlight, createInitialOfflineState, finishStreamedReplyAndReturn, playReplyAndReturn,
  runLifecycleOp, speakWithRuntimePrefs, startSessionOp, toSnapshot, type SessionRuntime,
} from './speakingSessionLifecycle';
import { createQueuedSpeechHandle, postTurnWithOptionalSse } from './speakingSessionStream';
import { openSessionWs } from './speakingSessionWs';
import { createWsEventHandler } from './speakingSessionWsEvents';
import {
  resolveApiBaseUrl, type SessionHistoryView, type SessionStatusSnapshot, type SessionSummary,
  type SpeakingSessionPort, type VoiceLoopEvent,
} from './tauriService';

const TURN_TIMEOUT_MS = 45_000;

export interface SpeakingSessionClientOptions {
  baseUrl?: string;
  hasVoiceCaptureUi?: boolean;
  speakUtterance?: (text: string, opts?: SpeakUtteranceOptions) => { done: Promise<void>; stop(): void };
  createWebSocket?: (url: string) => WebSocket;
  wsConnectTimeoutMs?: number;
}

export function createSpeakingSessionClient(opts: SpeakingSessionClientOptions = {}): SpeakingSessionPort {
  const listeners = new Set<(event: VoiceLoopEvent) => void>();
  let sampleIdx = 0;
  let streamSeq = 0;
  const offlineSeed = createInitialOfflineState();
  const rt: SessionRuntime = {
    baseUrl: opts.baseUrl ?? resolveApiBaseUrl(), selectedDeviceId: undefined,
    speakingSpeed: offlineSeed.prefs.speakingSpeed, englishVoiceId: offlineSeed.prefs.englishVoiceId,
    active: null, activeTurnAbort: null, stopSpeech: null, ws: null, offlineSeq: 0,
    offlineProgress: offlineSeed.progress, offlinePrefs: offlineSeed.prefs,
    speak: opts.speakUtterance ?? speakTutorUtterance,
    emit: (ev) => { listeners.forEach((fn) => fn(ev)); },
  };
  const { onWsEvent, clearStreamBuf } = createWsEventHandler(rt);
  rt.onSessionStarted = (sid) => { clearStreamBuf(); rt.ws = openSessionWs(rt.baseUrl, sid, onWsEvent, opts.createWebSocket); };

  const submitUtteranceText = async (rawText: string, confidence = 0.96): Promise<void> => {
    if (!rt.active || rt.active.state !== 'Listening') return;
    const text = sanitizeUntrustedText(rawText, 1000);
    if (!text) return;
    abortRuntimeInFlight(rt);
    const ctrl = new AbortController();
    rt.activeTurnAbort = ctrl;
    const ctx = rt.active;
    ctx.state = 'Thinking';
    rt.emit({ type: 'session-state-changed', sessionId: ctx.sessionId, state: 'Thinking' });

    let offlineReason = ctx.offline ? 'backend offline' : '';
    if (!ctx.offline) {
      const queue = createQueuedSpeechHandle((sentence) => speakWithRuntimePrefs(rt, sentence));
      rt.stopSpeech = queue.stop;
      rt.getSpeechProgress = queue.getProgress;
      let streamedText = '';
      const streamTurnId = `stream-${ctx.sessionId}-${++streamSeq}`;
      const onSentence = (s: string): void => {
        if (ctrl.signal.aborted || rt.active?.sessionId !== ctx.sessionId) return;
        const clean = sanitizeUntrustedText(s, 2000);
        if (!clean) return;
        streamedText = streamedText ? `${streamedText} ${clean}` : clean;
        if (ctx.state === 'Thinking') {
          ctx.state = 'Speaking';
          rt.emit({ type: 'session-state-changed', sessionId: ctx.sessionId, state: 'Speaking' });
        }
        rt.emit({ type: 'tutor-reply-streaming', sessionId: ctx.sessionId, turnId: streamTurnId, partialText: streamedText });
        queue.enqueue(clean);
      };
      const turnOpts = { signal: ctrl.signal, timeoutMs: TURN_TIMEOUT_MS, onSentence, getSpeechProgress: queue.getProgress };
      const body = { userText: text, correctionMode: ctx.correctionMode };
      const res = await (async () => {
        if (rt.ws && !rt.ws.isClosed() && (await rt.ws.awaitReady(opts.wsConnectTimeoutMs ?? 250, ctrl.signal))) {
          const wsRes = await rt.ws.runTextTurn(body, turnOpts);
          if (wsRes.ok || ctrl.signal.aborted || !wsRes.unreachable || wsRes.hadSentences) return wsRes;
          rt.ws.close();
          rt.ws = null;
        } else if (rt.ws?.isClosed()) rt.ws = null;
        return postTurnWithOptionalSse(apiUrl(`/api/sessions/${encodeURIComponent(ctx.sessionId)}/turn`, rt.baseUrl), body, turnOpts);
      })();
      if (ctrl.signal.aborted || rt.active?.sessionId !== ctx.sessionId) { queue.stop(); if (rt.activeTurnAbort === ctrl) rt.activeTurnAbort = null; return; }
      if (res.ok) {
        const outcome = extractOnlineTurnOutcome(res.data);
        if (queue.hasQueued()) await finishStreamedReplyAndReturn(rt, ctx, ctrl, queue, outcome, confidence);
        else await playReplyAndReturn(rt, ctx, ctrl, outcome, confidence);
        return;
      }
      queue.stop();
      rt.stopSpeech = null;
      rt.getSpeechProgress = null;
      if (rt.activeTurnAbort === ctrl) rt.activeTurnAbort = null;
      if (!res.unreachable) {
        const failure = buildOnlineTurnFailureEvent(ctx.sessionId, res.status, res.message);
        ctx.state = failure.nextState;
        rt.emit(failure.event);
        return;
      }
      ctx.offline = true;
      offlineReason = res.message;
    }
    const off = buildOfflineTurnOutcome(ctx.sessionId, text, ctx.correctionMode, ctx.recentTurns);
    const nextScaffold = ctx.scaffold ? advanceOfflineScaffold(ctx.scaffold, text) : undefined;
    if (nextScaffold) ctx.scaffold = nextScaffold;
    const ended = nextScaffold?.phase === 'Completed';
    const allTurns = [...ctx.recentTurns, off.userTurn, off.tutorTurn];
    const sessionReview = ended ? buildOfflineSessionReview(ctx.sessionId, nextScaffold, allTurns) : undefined;
    if (sessionReview) recordLocalSessionProgress(sessionReview, ctx.startedAtMs, rt.offlineProgress);
    await playReplyAndReturn(rt, ctx, ctrl, { ...off, replySource: OFFLINE_SOURCE, playbackWarning: offlineWarning(offlineReason), ...(nextScaffold ? { scaffold: nextScaffold } : {}), ...(ended ? { finalState: 'Ended' } : {}), ...(sessionReview ? { sessionReview } : {}) }, confidence);
  };

  return {
    hasVoiceCaptureUi: opts.hasVoiceCaptureUi ?? true,
    startSpeakingSession: (cfg) => startSessionOp(rt, cfg),
    pauseSpeakingSession: (id) => runLifecycleOp(rt, id, 'pause', 'Paused'),
    resumeSpeakingSession: (id) => runLifecycleOp(rt, id, 'resume', 'Listening'),
    stopSpeakingSession: (id) => runLifecycleOp(rt, id, 'stop', 'Ended'),
    bargeIn: async () => {
      clearStreamBuf();
      const progress = rt.getSpeechProgress?.();
      if (!abortRuntimeInFlight(rt, true)) rt.ws?.sendBargeIn(progress);
      if (rt.active && (rt.active.state === 'Processing' || rt.active.state === 'Thinking' || rt.active.state === 'Speaking')) {
        rt.active.state = 'Listening';
        rt.emit({ type: 'session-state-changed', sessionId: rt.active.sessionId, state: 'Listening' });
      }
    },
    setPushToTalkHeld: async (held) => { if (rt.active && !rt.active.offline) rt.ws?.sendPttHold(held); },
    sendAudioFrame: (pcm) => { if (rt.active && !rt.active.offline) rt.ws?.sendAudioFrame(pcm); },
    submitUtteranceText,
    simulateSampleTurn: async () => {
      if (!rt.active || rt.active.state !== 'Listening') return;
      const sample = SAMPLE_UTTERANCES[sampleIdx % SAMPLE_UTTERANCES.length];
      sampleIdx += 1;
      await submitUtteranceText(sample.userText, sample.confidence);
    },
    listAudioInputDevices: async () => [{ id: 'default', name: 'System Default Microphone', isDefault: true }],
    setAudioInputDevice: async (deviceId) => { rt.selectedDeviceId = deviceId || undefined; },
    listSessionHistory: async () => { const r = await requestJson<SessionSummary[]>(apiUrl('/api/sessions', rt.baseUrl)); return r.ok ? r.data : []; },
    getSessionHistory: async (sessionId) => {
      const res = await requestJson<SessionHistoryView>(apiUrl(`/api/sessions/${encodeURIComponent(sessionId)}`, rt.baseUrl));
      if (!res.ok) throw new Error(res.message);
      return { ...res.data, turns: res.data.turns.map(normalizeTurn) };
    },
    updateCorrectionMode: async (mode) => {
      if (rt.active && rt.active.state !== 'Ended' && rt.active.state !== 'Idle') rt.active.correctionMode = mode;
      const r = await requestJson<LearnerPreferencesView>(apiUrl('/api/preferences', rt.baseUrl), { method: 'PATCH', body: { correctionMode: mode } });
      if (!r.ok && r.unreachable) saveLocalPreferences({ correctionMode: mode }, rt.offlinePrefs);
      return mode;
    },
    getSessionStatus: async (sessionId) => {
      if (rt.active?.offline && rt.active.sessionId === sessionId) return toSnapshot(rt.active);
      const res = await requestJson<SessionStatusSnapshot>(apiUrl(`/api/sessions/${encodeURIComponent(sessionId)}/status`, rt.baseUrl));
      if (res.ok) return res.data;
      if (res.unreachable && rt.active?.sessionId === sessionId) return toSnapshot(rt.active);
      throw new Error(res.message);
    },
    subscribeVoiceEvents: (handler) => { listeners.add(handler); return () => { listeners.delete(handler); }; },
    listScenarios: async () => {
      const res = await requestJson<RoleplayScenario[]>(apiUrl('/api/scenarios', rt.baseUrl));
      return res.ok && res.data.length > 0 ? res.data : DEFAULT_ROLEPLAY_SCENARIOS;
    },
    getSessionReview: (sid) => fetchSessionReview(rt.baseUrl, sid),
    listLearnerFacts: () => fetchLearnerFacts(rt.baseUrl),
    deleteLearnerFact: (id) => removeLearnerFact(rt.baseUrl, id),
    clearLearnerFacts: () => clearAllLearnerFacts(rt.baseUrl),
    getLearnerProgress: async () => {
      const res = await requestJson<LearnerProgress>(apiUrl('/api/learner/progress', rt.baseUrl));
      return res.ok ? res.data : readLocalProgress(rt.offlineProgress);
    },
    getPreferences: async () => {
      const res = await requestJson<LearnerPreferencesView>(apiUrl('/api/preferences', rt.baseUrl));
      const prefs = res.ok ? res.data : res.unreachable ? readLocalPreferences(rt.offlinePrefs) : null;
      if (prefs) { rt.speakingSpeed = prefs.speakingSpeed; rt.englishVoiceId = prefs.englishVoiceId || 'en-US'; }
      return prefs;
    },
    patchPreferences: async (patch) => {
      if (patch.speakingSpeed !== undefined) rt.speakingSpeed = patch.speakingSpeed;
      if (patch.englishVoiceId !== undefined) rt.englishVoiceId = patch.englishVoiceId;
      if (patch.correctionMode && rt.active && rt.active.state !== 'Ended' && rt.active.state !== 'Idle') rt.active.correctionMode = patch.correctionMode;
      if (patch.cefrLevel && rt.active?.scaffold && rt.active.state !== 'Ended' && rt.active.state !== 'Idle') rt.active.scaffold = { ...rt.active.scaffold, cefrLevel: patch.cefrLevel };
      const res = await requestJson<LearnerPreferencesView>(apiUrl('/api/preferences', rt.baseUrl), { method: 'PATCH', body: patch });
      const updated = res.ok ? res.data : res.unreachable ? saveLocalPreferences(patch, rt.offlinePrefs) : null;
      if (updated) { rt.speakingSpeed = updated.speakingSpeed; rt.englishVoiceId = updated.englishVoiceId || 'en-US'; }
      return updated;
    },
    replayTutorSpeech: (text, rateOverride) => { rt.stopSpeech?.(); cancelBrowserSpeech(); rt.stopSpeech = speakWithRuntimePrefs(rt, text, rateOverride).stop; },
  };
}
