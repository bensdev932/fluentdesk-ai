/**
 * Shared typed domain contracts crossing the Tauri IPC boundary.
 * Mirrors `src-tauri/src/domain/mod.rs` without exposing provider internals.
 */

export type SessionId = string;

export const SESSION_STATES = [
  'Idle',
  'Initializing',
  'Listening',
  'Processing',
  'Thinking',
  'Speaking',
  'Paused',
  'Error',
  'Ended',
] as const;

export type SessionState = (typeof SESSION_STATES)[number];

export const CORRECTION_MODES = [
  'Natural',
  'Gentle',
  'Detailed',
  'Off',
] as const;

export type CorrectionMode = (typeof CORRECTION_MODES)[number];

export const CEFR_LEVELS = ['A1', 'A2', 'B1', 'B2', 'C1', 'C2'] as const;
export type CefrLevel = (typeof CEFR_LEVELS)[number];

export const SESSION_PHASES = [
  'Opener',
  'FirstQuestion',
  'TopicExchange',
  'Closer',
  'Completed',
] as const;
export type SessionPhase = (typeof SESSION_PHASES)[number];

export interface CommunicativeGoal {
  id: string;
  description: string;
  keywords?: string[];
}

export interface RoleplayScenario {
  id: string;
  title: string;
  summary: string;
  tutorPersona: string;
  learnerRole: string;
  defaultCefr: CefrLevel;
  defaultMaxTurns: number;
  targetVocabulary: string[];
  goals: CommunicativeGoal[];
}

export interface LearnerFact {
  id: string;
  factText: string;
  sourceSessionId?: string;
  createdAtUnixMs: number;
}

export interface SessionReview {
  sessionId: SessionId;
  scenarioId: string;
  estimateLabel: string;
  summary: string;
  cefrSignal: CefrLevel;
  completedGoalIds: string[];
  grammarFocuses: string[];
  vocabularyUsed: string[];
  turnCount: number;
  learnerWordCount: number;
  createdAtUnixMs: number;
}

export interface ScaffoldProgress {
  phase: SessionPhase;
  turnCount: number;
  maxTurns: number;
  cefrLevel: CefrLevel;
  topic: string;
  scenarioId?: string;
  targetVocabulary?: string[];
  goals?: CommunicativeGoal[];
  completedGoalIds?: string[];
}

export interface LearnerProgress {
  totalSessions: number;
  practiceMinutes: number;
  completedGoalsCount: number;
  vocabularyUsed: string[];
  dayStreak: number;
}

export const SPEAKING_SPEED_PRESETS = [0.75, 1.0, 1.15] as const;

export const ENGLISH_VOICE_PRESETS = [
  { id: 'en-US', label: 'English (US)' },
  { id: 'en-GB', label: 'English (UK)' },
  { id: 'en-AU', label: 'English (AU)' },
] as const;

export interface LearnerPreferencesView {
  correctionMode: CorrectionMode;
  speakingSpeed: number;
  pushToTalkEnabled: boolean;
  cefrLevel: CefrLevel;
  englishVoiceId: string;
}

export interface LearnerPreferencesPatch {
  correctionMode?: CorrectionMode;
  speakingSpeed?: number;
  pushToTalkEnabled?: boolean;
  cefrLevel?: CefrLevel;
  englishVoiceId?: string;
}

export type ProviderHealth = 'Available' | 'Unavailable' | 'Misconfigured';

export type ConversationRole = 'User' | 'Tutor';

export interface Transcript {
  text: string;
  language: string;
  durationMs: number;
  confidence?: number;
}

export interface TurnStageMetadata {
  sttDurationMs?: number;
  llmDurationMs?: number;
  ttsDurationMs?: number;
  correctionMode?: CorrectionMode;
}

export interface ConversationTurn {
  id: string;
  sessionId: SessionId;
  role: ConversationRole;
  text: string;
  correction?: string;
  followUpPrompt?: string;
  timestampUnixMs: number;
  stageMetadata?: TurnStageMetadata;
}

export interface TutorReply {
  spokenReply: string;
  correction?: string;
  followUpPrompt?: string;
}

export type SessionErrorKind =
  | 'AudioDeviceUnavailable'
  | 'MicrophonePermissionDenied'
  | 'SttUnavailable'
  | 'SttFailed'
  | 'AiProviderUnavailable'
  | 'AiInferenceFailed'
  | 'TtsUnavailable'
  | 'TtsFailed'
  | 'PlaybackFailed'
  | 'PersistenceFailed'
  | 'InvalidStateTransition'
  | 'Cancelled';

export interface SessionError {
  kind: SessionErrorKind;
  userMessage: string;
  recoverable: boolean;
}

/**
 * Returns true if and only if the session is actively capturing microphone input.
 * Enforces Constitution Principle 8: never visualize microphone as active when not capturing.
 */
export function isMicrophoneCapturingState(state: SessionState): boolean {
  return state === 'Listening';
}

/**
 * Returns true when a session is currently active (not Idle or Ended).
 */
export function isSessionActiveState(state: SessionState): boolean {
  return state !== 'Idle' && state !== 'Ended';
}

/**
 * Treats model output as untrusted input: strips ASCII control characters
 * (preserving standard newlines/tabs) and bounds text length.
 */
export function sanitizeUntrustedText(input: string, maxChars = 2000): string {
  const withoutControlChars = input.replace(
    // eslint-disable-next-line no-control-regex
    /[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F]/g,
    '',
  );
  return withoutControlChars.trim().slice(0, maxChars);
}
