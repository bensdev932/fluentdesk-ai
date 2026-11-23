/**
 * Typed frontend adapter for Tauri IPC commands and real-time voice-loop events.
 */

import type {
  CefrLevel,
  CommunicativeGoal,
  ConversationTurn,
  CorrectionMode,
  LearnerFact,
  LearnerPreferencesPatch,
  LearnerPreferencesView,
  LearnerProgress,
  RoleplayScenario,
  ScaffoldProgress,
  SessionError,
  SessionId,
  SessionPhase,
  SessionReview,
  SessionState,
  TutorReply,
} from '../types/domain';

export type AiProviderId = 'ollama' | 'workers_ai' | 'groq' | 'gemini' | 'openrouter' | 'freellmapi';
export type ProviderId = AiProviderId;
export type AiProviderPreference = 'auto' | AiProviderId;
export type ProviderPreference = AiProviderPreference;
export type ProviderKeySource = 'none' | 'env' | 'runtime';

export interface ProviderDescriptorView {
  id: AiProviderId;
  label: string;
  isCloud: boolean;
  configured: boolean;
  available?: boolean;
  keySource: ProviderKeySource;
  defaultModel: string;
  selectedModel: string;
  freeModels: string[];
  cooldownSecs: number;
  authInvalid?: boolean;
  statusDetail?: string;
}

export type ProviderStatusView = ProviderDescriptorView;

export interface ProvidersSnapshot {
  cloudOptIn: boolean;
  preferredProvider: AiProviderPreference;
  providers: ProviderDescriptorView[];
}

export interface RouterConfigPatch {
  cloudOptIn?: boolean;
  preferredProvider?: AiProviderPreference;
  apiKeys?: Partial<Record<AiProviderId, string>>;
  clearKeys?: AiProviderId[];
  modelOverrides?: Partial<Record<AiProviderId, string>>;
  resetCooldowns?: boolean;
}

export interface ReplySourceStatus {
  kind: 'live-local' | 'live-cloud' | 'offline-fallback';
  provider?: ProviderId;
  model?: string;
  label: string;
  isCloud?: boolean;
}

export interface AudioInputDeviceInfo {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface StartSpeakingSessionConfig {
  correctionMode: CorrectionMode;
  pushToTalk: boolean;
  inputDeviceId?: string;
  cefrLevel?: CefrLevel;
  scenarioId?: string;
  topic?: string;
  targetVocabulary?: string[];
  maxTurns?: number;
}

export type StartSpeakingSessionRequest = StartSpeakingSessionConfig;

export interface SessionStatusSnapshot {
  sessionId: SessionId;
  state: SessionState;
  correctionMode: CorrectionMode;
  pushToTalk: boolean;
  phase?: SessionPhase;
  turnCount?: number;
  maxTurns?: number;
  cefrLevel?: CefrLevel;
  topic?: string;
  scenarioId?: string;
  targetVocabulary?: string[];
  goals?: CommunicativeGoal[];
  completedGoalIds?: string[];
  openerReply?: TutorReply;
  sessionReview?: SessionReview;
}

export interface SessionSummary {
  sessionId: SessionId;
  startedAtUnixMs: number;
  endedAtUnixMs?: number;
  correctionMode: CorrectionMode;
  turnCount: number;
}

export interface SessionHistoryView {
  sessionId: SessionId;
  startedAtUnixMs: number;
  endedAtUnixMs?: number;
  correctionMode?: CorrectionMode;
  turns: ConversationTurn[];
}

export type VoiceLoopEvent =
  | {
      type: 'session-state-changed';
      sessionId: SessionId;
      state: SessionState;
      correctionMode?: CorrectionMode;
      warning?: SessionError;
      replySource?: ReplySourceStatus;
      scaffold?: ScaffoldProgress;
      openerReply?: TutorReply;
      sessionReview?: SessionReview;
    }
  | {
      type: 'user-transcript-ready';
      sessionId: SessionId;
      turn: ConversationTurn;
      confidence?: number;
    }
  | {
      type: 'tutor-reply-streaming';
      sessionId: SessionId;
      turnId: string;
      partialText: string;
    }
  | {
      type: 'tutor-reply-ready';
      sessionId: SessionId;
      turn: ConversationTurn;
      reply: TutorReply;
      replySource?: ReplySourceStatus;
      playbackWarning?: SessionError;
    }
  | {
      type: 'session-error';
      sessionId: SessionId;
      error: SessionError;
    };

export interface SpeakingSessionPort {
  hasVoiceCaptureUi?: boolean;
  startSpeakingSession(config: StartSpeakingSessionConfig): Promise<SessionStatusSnapshot>;
  pauseSpeakingSession(sessionId: SessionId): Promise<SessionStatusSnapshot>;
  resumeSpeakingSession(sessionId: SessionId): Promise<SessionStatusSnapshot>;
  stopSpeakingSession(sessionId: SessionId): Promise<SessionStatusSnapshot>;
  bargeIn?(): Promise<void>;
  setPushToTalkHeld?(held: boolean): Promise<void>;
  sendAudioFrame?(pcm: Int16Array | Float32Array): void;
  submitUtteranceText(text: string, confidence?: number): Promise<void>;
  simulateSampleTurn(): Promise<void>;
  listAudioInputDevices(): Promise<AudioInputDeviceInfo[]>;
  setAudioInputDevice(deviceId: string): Promise<void>;
  listSessionHistory(): Promise<SessionSummary[]>;
  getSessionHistory(sessionId: SessionId): Promise<SessionHistoryView>;
  updateCorrectionMode(mode: CorrectionMode): Promise<CorrectionMode>;
  getSessionStatus(sessionId: SessionId): Promise<SessionStatusSnapshot>;
  subscribeVoiceEvents(handler: (event: VoiceLoopEvent) => void): () => void;
  listScenarios?(): Promise<RoleplayScenario[]>;
  getSessionReview?(sessionId: SessionId): Promise<SessionReview | null>;
  listLearnerFacts?(): Promise<LearnerFact[]>;
  deleteLearnerFact?(factId: string): Promise<void>;
  clearLearnerFacts?(): Promise<void>;
  getLearnerProgress?(): Promise<LearnerProgress>;
  getPreferences?(): Promise<LearnerPreferencesView | null>;
  patchPreferences?(patch: LearnerPreferencesPatch): Promise<LearnerPreferencesView | null>;
  replayTutorSpeech?(text: string, rateOverride?: number): void;
}

export { isTauriRuntime, resolveApiBaseUrl } from './runtimeUrl';
