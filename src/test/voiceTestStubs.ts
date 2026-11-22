import { DEFAULT_ROLEPLAY_SCENARIOS } from '../services/scenariosFallback';
import type {
  SessionHistoryView,
  SessionStatusSnapshot,
  SessionSummary,
  SpeakingSessionPort,
  VoiceLoopEvent,
} from '../services/tauriService';
import type { ConversationTurn, CorrectionMode, SessionState, TutorReply } from '../types/domain';

export interface MockSpeechRec {
  onresult: ((ev: unknown) => void) | null;
  onerror: ((ev: { error: string }) => void) | null;
  onend: (() => void) | null;
}

export function stubSpeechRecognition(): () => MockSpeechRec | null {
  let active: MockSpeechRec | null = null;
  (globalThis as { SpeechRecognition?: unknown }).SpeechRecognition = class {
    lang = '';
    continuous = false;
    interimResults = false;
    onresult = null;
    onerror = null;
    onend = null;
    start() { active = this; }
    stop() { /* noop */ }
  };
  return () => active;
}

export interface FakePortOptions {
  hasVoiceCaptureUi?: boolean;
  turnGate?: () => Promise<void>;
  replyForTurn?: (text: string, mode: CorrectionMode) => TutorReply;
}

export function createFakeSpeakingSessionPort(opts: FakePortOptions = {}): SpeakingSessionPort {
  const listeners = new Set<(event: VoiceLoopEvent) => void>();
  const sessions = new Map<string, { startedAtUnixMs: number; endedAtUnixMs?: number; mode: CorrectionMode; turns: ConversationTurn[] }>();
  let seq = 0;
  let activeId = 'fake-sess-0';
  let state: SessionState = 'Idle';
  let mode: CorrectionMode = 'Natural';
  let pushToTalk = false;
  let aborted = false;

  const emit = (ev: VoiceLoopEvent): void => { listeners.forEach((fn) => fn(ev)); };
  const snap = (): SessionStatusSnapshot => ({ sessionId: activeId, state, correctionMode: mode, pushToTalk });

  const submitUtteranceText = async (text: string, confidence = 0.96): Promise<void> => {
    if (state !== 'Listening' || !text.trim()) return;
    aborted = false;
    const sid = activeId;
    state = 'Thinking';
    emit({ type: 'session-state-changed', sessionId: sid, state: 'Thinking' });
    if (opts.turnGate) await opts.turnGate();
    if (aborted || activeId !== sid) return;

    const rec = sessions.get(sid);
    const now = Date.now();
    const userTurn: ConversationTurn = { id: `u-${now}-${Math.random()}`, sessionId: sid, role: 'User', text, timestampUnixMs: now };
    const reply = opts.replyForTurn?.(text, mode) ?? {
      spokenReply: `Got it: ${text}`,
      ...(mode === 'Gentle' || mode === 'Detailed' ? { correction: 'Use simple past tense.' } : {}),
      followUpPrompt: 'What happened next?',
    };
    const tutorTurn: ConversationTurn = {
      id: `t-${now + 1}-${Math.random()}`, sessionId: sid, role: 'Tutor', text: reply.spokenReply,
      ...(reply.correction ? { correction: reply.correction } : {}),
      ...(reply.followUpPrompt ? { followUpPrompt: reply.followUpPrompt } : {}),
      timestampUnixMs: now + 1,
    };
    rec?.turns.push(userTurn, tutorTurn);
    emit({ type: 'user-transcript-ready', sessionId: sid, turn: userTurn, confidence });
    emit({ type: 'tutor-reply-ready', sessionId: sid, turn: tutorTurn, reply, replySource: { kind: 'live-local', label: 'Local · Fake' } });
    state = 'Listening';
    emit({ type: 'session-state-changed', sessionId: sid, state: 'Listening' });
  };

  return {
    hasVoiceCaptureUi: opts.hasVoiceCaptureUi ?? true,
    startSpeakingSession: async (cfg) => {
      aborted = true;
      seq += 1;
      activeId = `fake-sess-${seq}`;
      mode = cfg.correctionMode;
      pushToTalk = cfg.pushToTalk;
      state = 'Listening';
      sessions.set(activeId, { startedAtUnixMs: Date.now(), mode, turns: [] });
      emit({ type: 'session-state-changed', sessionId: activeId, state: 'Listening', correctionMode: mode });
      return snap();
    },
    pauseSpeakingSession: async () => { aborted = true; state = 'Paused'; emit({ type: 'session-state-changed', sessionId: activeId, state }); return snap(); },
    resumeSpeakingSession: async () => { state = 'Listening'; emit({ type: 'session-state-changed', sessionId: activeId, state }); return snap(); },
    stopSpeakingSession: async (id) => {
      aborted = true;
      state = 'Ended';
      const rec = sessions.get(id);
      if (rec) rec.endedAtUnixMs = Date.now();
      emit({ type: 'session-state-changed', sessionId: id, state: 'Ended' });
      return snap();
    },
    submitUtteranceText,
    simulateSampleTurn: () => submitUtteranceText('Yesterday I go to the park.', 0.95),
    listAudioInputDevices: async () => [{ id: 'default', name: 'System Default Microphone', isDefault: true }],
    setAudioInputDevice: async () => undefined,
    listSessionHistory: async (): Promise<SessionSummary[]> =>
      [...sessions.entries()].map(([sessionId, s]) => ({ sessionId, startedAtUnixMs: s.startedAtUnixMs, endedAtUnixMs: s.endedAtUnixMs, correctionMode: s.mode, turnCount: s.turns.length })),
    getSessionHistory: async (sessionId): Promise<SessionHistoryView> => {
      const s = sessions.get(sessionId);
      return { sessionId, startedAtUnixMs: s?.startedAtUnixMs ?? Date.now(), endedAtUnixMs: s?.endedAtUnixMs, turns: [...(s?.turns ?? [])] };
    },
    updateCorrectionMode: async (next) => { mode = next; return mode; },
    getSessionStatus: async () => snap(),
    listScenarios: async () => DEFAULT_ROLEPLAY_SCENARIOS,
    subscribeVoiceEvents: (h) => { listeners.add(h); return () => { listeners.delete(h); }; },
  };
}
