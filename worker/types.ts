import type {
  CefrLevel,
  CorrectionMode,
  LearnerFact,
  LearnerProgress,
  SessionPhase,
  SessionReview,
  SessionState,
} from '../src/types/domain';
import type { SessionSummary } from '../src/services/tauriService';

export type WorkerAiProviderId = 'workers_ai' | 'groq' | 'gemini' | 'openrouter';
export type WorkerProviderPreference = 'auto' | WorkerAiProviderId;

export interface WorkersAiBinding {
  run(
    model: string,
    input: {
      messages?: Array<{ role: string; content: string }>;
      prompt?: string;
      max_tokens?: number;
      temperature?: number;
    },
  ): Promise<unknown>;
}

export interface WorkerEnv {
  ASSETS?: { fetch(request: Request): Promise<Response> };
  AI?: WorkersAiBinding;
  CONVEX_SITE_URL?: string;
  CONVEX_BRIDGE_SECRET?: string;
  GROQ_API_KEY?: string;
  GEMINI_API_KEY?: string;
  OPENROUTER_API_KEY?: string;
  FLUENTDESK_GROQ_BASE_URL?: string;
  FLUENTDESK_GEMINI_BASE_URL?: string;
  FLUENTDESK_OPENROUTER_BASE_URL?: string;
}

export type ByokKeys = Partial<Record<'groq' | 'gemini' | 'openrouter', string>>;

export interface StoredSettings {
  correctionMode: CorrectionMode;
  speakingSpeed: number;
  pushToTalkEnabled: boolean;
  cefrLevel: CefrLevel;
  englishVoiceId: string;
  cloudOptIn: boolean;
  preferredProvider: WorkerProviderPreference;
  modelOverridesJson: string;
}

export interface StoredSession {
  sessionId: string;
  state: SessionState;
  correctionMode: CorrectionMode;
  pushToTalk: boolean;
  scenarioId: string;
  topic: string;
  cefrLevel: CefrLevel;
  phase: SessionPhase;
  turnCount: number;
  maxTurns: number;
  targetVocabulary: string[];
  completedGoalIds: string[];
  startedAtUnixMs: number;
  endedAtUnixMs?: number;
}

export interface StoredTurn {
  turnId: string;
  role: 'User' | 'Tutor';
  text: string;
  correction?: string;
  followUpPrompt?: string;
  timestampUnixMs: number;
}

export type SessionMutationResult =
  | { ok: true; state?: SessionState }
  | { ok: false; status: number; error: string };

export interface RateLimitResult {
  allowed: boolean;
  reason?: string;
}

export interface BridgeClient {
  getSettings(learnerId: string): Promise<StoredSettings>;
  patchSettings(
    learnerId: string,
    patch: Partial<Omit<StoredSettings, 'modelOverridesJson'>> & { modelOverrides?: Record<string, string> },
  ): Promise<StoredSettings>;
  createSession(
    learnerId: string,
    args: Omit<StoredSession, 'state' | 'phase' | 'turnCount' | 'completedGoalIds'> & { openerTurn?: StoredTurn },
  ): Promise<{ ok: boolean }>;
  getSession(
    learnerId: string,
    sessionId: string,
  ): Promise<{ session: StoredSession; turns: StoredTurn[]; review?: SessionReview } | null>;
  listSessions(learnerId: string): Promise<SessionSummary[]>;
  recordTurn(
    learnerId: string,
    args: {
      sessionId: string;
      userTurn: StoredTurn;
      tutorTurn: StoredTurn;
      correctionMode: CorrectionMode;
      phase: SessionPhase;
      turnCount: number;
      completedGoalIds: string[];
      state: SessionState;
      endedAtUnixMs?: number;
      sessionReview?: SessionReview;
      extractedFact?: { factId: string; factText: string; createdAtUnixMs: number };
    },
  ): Promise<SessionMutationResult>;
  updateSessionState(
    learnerId: string,
    args: {
      sessionId: string;
      op: 'pause' | 'resume' | 'stop';
      endedAtUnixMs?: number;
      sessionReview?: SessionReview;
    },
  ): Promise<SessionMutationResult>;
  getReview(learnerId: string, sessionId: string): Promise<SessionReview | null>;
  listFacts(learnerId: string): Promise<LearnerFact[]>;
  deleteFact(learnerId: string, factId: string): Promise<void>;
  clearFacts(learnerId: string): Promise<void>;
  getProgress(learnerId: string, nowUnixMs?: number): Promise<LearnerProgress>;
  consumeRateLimit(learnerId: string, bucketKeys: string[], nowUnixMs?: number): Promise<RateLimitResult>;
}
