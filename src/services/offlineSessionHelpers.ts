import type {
  ConversationTurn,
  LearnerPreferencesPatch,
  LearnerPreferencesView,
  LearnerProgress,
  ScaffoldProgress,
  SessionId,
  SessionPhase,
  SessionReview,
} from '../types/domain';
import { findFallbackScenario } from './scenariosFallback';
import type { StartSpeakingSessionConfig } from './tauriService';

const PROGRESS_STORAGE_KEY = 'fluentdesk.offlineProgress.v1';
const PREFS_STORAGE_KEY = 'fluentdesk.offlinePreferences.v1';

interface StoredProgress extends LearnerProgress {
  lastPracticeDay?: string;
}

const DEFAULT_OFFLINE_PREFS: LearnerPreferencesView = {
  correctionMode: 'Gentle',
  speakingSpeed: 1.0,
  pushToTalkEnabled: false,
  cefrLevel: 'B1',
  englishVoiceId: 'en-US',
  preConversationScriptEnabled: false,
};

export function buildOfflineScaffold(cfg: StartSpeakingSessionConfig): ScaffoldProgress {
  const sc = findFallbackScenario(cfg.scenarioId);
  return {
    phase: 'Opener',
    turnCount: 0,
    maxTurns: cfg.maxTurns ?? sc.defaultMaxTurns,
    cefrLevel: cfg.cefrLevel ?? sc.defaultCefr,
    topic: (cfg.topic ?? '').trim() || sc.title,
    scenarioId: sc.id,
    targetVocabulary: cfg.targetVocabulary?.length ? cfg.targetVocabulary : sc.targetVocabulary,
    goals: sc.goals,
    completedGoalIds: [],
  };
}

function nextPhaseFor(done: number, maxTurns: number): SessionPhase {
  if (done >= maxTurns) return 'Completed';
  if (done === 0) return 'Opener';
  if (done === 1 && maxTurns > 2) return 'FirstQuestion';
  if (done + 1 >= maxTurns) return 'Closer';
  return 'TopicExchange';
}

export function advanceOfflineScaffold(scaffold: ScaffoldProgress, userText: string): ScaffoldProgress {
  const lower = userText.toLowerCase();
  const completed = new Set(scaffold.completedGoalIds ?? []);
  for (const g of scaffold.goals ?? []) {
    if (completed.has(g.id)) continue;
    if ((g.keywords ?? []).some((kw) => kw && lower.includes(kw.toLowerCase()))) {
      completed.add(g.id);
    }
  }
  const turnCount = Math.min(scaffold.maxTurns, scaffold.turnCount + 1);
  return {
    ...scaffold,
    turnCount,
    phase: nextPhaseFor(turnCount, scaffold.maxTurns),
    completedGoalIds: [...completed],
  };
}

export function buildOfflineSessionReview(
  sessionId: SessionId,
  scaffold: ScaffoldProgress | undefined,
  turns: readonly ConversationTurn[],
): SessionReview {
  const sc = findFallbackScenario(scaffold?.scenarioId);
  const userTurns = turns.filter((t) => t.role === 'User');
  const wordCount = userTurns.reduce((n, t) => n + t.text.split(/\s+/).filter(Boolean).length, 0);
  const completedGoalIds = scaffold?.completedGoalIds ?? [];
  const targetVocab = scaffold?.targetVocabulary ?? sc.targetVocabulary;
  const joinedUser = userTurns.map((t) => t.text.toLowerCase()).join(' ');
  const vocabularyUsed = targetVocab.filter((w) => joinedUser.includes(w.toLowerCase()));
  const grammarFocuses = turns
    .filter((t) => t.role === 'Tutor' && Boolean(t.properWay?.trim() || t.correction?.trim()))
    .map((t) => (t.properWay?.trim() ? (t.why?.trim() ? `${t.properWay.trim()} — ${t.why.trim()}` : t.properWay.trim()) : t.correction!.trim()))
    .slice(0, 4);
  const totalGoals = scaffold?.goals?.length ?? sc.goals.length;
  return {
    sessionId,
    scenarioId: scaffold?.scenarioId ?? sc.id,
    estimateLabel: 'AI Estimate',
    summary: `${sc.title}: ${completedGoalIds.length}/${totalGoals} goals met across ${userTurns.length} learner turns (${wordCount} words spoken). AI Estimate for practice reflection.`,
    cefrSignal: scaffold?.cefrLevel ?? sc.defaultCefr,
    completedGoalIds,
    grammarFocuses,
    vocabularyUsed,
    turnCount: userTurns.length,
    learnerWordCount: wordCount,
    createdAtUnixMs: Date.now(),
  };
}

interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

function getStorage(): StorageLike | null {
  try {
    return (globalThis as { localStorage?: StorageLike }).localStorage ?? null;
  } catch {
    return null;
  }
}

export function readLocalProgress(memoryFallback?: LearnerProgress): LearnerProgress {
  const store = getStorage();
  if (!store) return memoryFallback ?? { totalSessions: 0, practiceMinutes: 0, completedGoalsCount: 0, vocabularyUsed: [], dayStreak: 0 };
  try {
    const raw = store.getItem(PROGRESS_STORAGE_KEY);
    if (!raw) return { totalSessions: 0, practiceMinutes: 0, completedGoalsCount: 0, vocabularyUsed: [], dayStreak: 0 };
    const parsed = JSON.parse(raw) as StoredProgress;
    return {
      totalSessions: parsed.totalSessions ?? 0,
      practiceMinutes: parsed.practiceMinutes ?? 0,
      completedGoalsCount: parsed.completedGoalsCount ?? 0,
      vocabularyUsed: Array.isArray(parsed.vocabularyUsed) ? parsed.vocabularyUsed : [],
      dayStreak: parsed.dayStreak ?? 0,
    };
  } catch {
    return memoryFallback ?? { totalSessions: 0, practiceMinutes: 0, completedGoalsCount: 0, vocabularyUsed: [], dayStreak: 0 };
  }
}

export function recordLocalSessionProgress(
  review: SessionReview,
  startedAtMs: number,
  memoryFallback?: LearnerProgress,
): LearnerProgress {
  const prev = readLocalProgress(memoryFallback);
  const elapsedMin = Math.ceil(Math.min(30 * 60_000, Math.max(review.learnerWordCount * 60_000 / 110, Date.now() - startedAtMs)) / 60_000);
  const vocabSet = new Set(prev.vocabularyUsed.map((w) => w.toLowerCase()));
  const vocabularyUsed = [...prev.vocabularyUsed];
  for (const w of review.vocabularyUsed) {
    if (!vocabSet.has(w.toLowerCase())) { vocabSet.add(w.toLowerCase()); vocabularyUsed.push(w); }
  }
  const today = new Date().toISOString().slice(0, 10);
  const yesterday = new Date(Date.now() - 86_400_000).toISOString().slice(0, 10);
  const store = getStorage();
  let lastDay: string | undefined;
  try { lastDay = store ? (JSON.parse(store.getItem(PROGRESS_STORAGE_KEY) || '{}') as StoredProgress).lastPracticeDay : undefined; } catch { /* ignore */ }
  const dayStreak = lastDay === today ? Math.max(1, prev.dayStreak) : lastDay === yesterday ? prev.dayStreak + 1 : 1;
  const next: StoredProgress = {
    totalSessions: prev.totalSessions + 1,
    practiceMinutes: prev.practiceMinutes + Math.max(review.turnCount > 0 ? 1 : 0, elapsedMin),
    completedGoalsCount: prev.completedGoalsCount + review.completedGoalIds.length,
    vocabularyUsed,
    dayStreak,
    lastPracticeDay: today,
  };
  try { store?.setItem(PROGRESS_STORAGE_KEY, JSON.stringify(next)); } catch { /* ignore */ }
  if (memoryFallback) Object.assign(memoryFallback, next);
  return { totalSessions: next.totalSessions, practiceMinutes: next.practiceMinutes, completedGoalsCount: next.completedGoalsCount, vocabularyUsed: next.vocabularyUsed, dayStreak: next.dayStreak };
}

export function readLocalPreferences(memoryFallback?: LearnerPreferencesView): LearnerPreferencesView {
  const store = getStorage();
  if (!store) return memoryFallback ?? { ...DEFAULT_OFFLINE_PREFS };
  try {
    const raw = store.getItem(PREFS_STORAGE_KEY);
    return raw ? { ...DEFAULT_OFFLINE_PREFS, ...(JSON.parse(raw) as Partial<LearnerPreferencesView>) } : { ...DEFAULT_OFFLINE_PREFS };
  } catch {
    return memoryFallback ?? { ...DEFAULT_OFFLINE_PREFS };
  }
}

export function saveLocalPreferences(patch: LearnerPreferencesPatch, memoryFallback?: LearnerPreferencesView): LearnerPreferencesView {
  const next: LearnerPreferencesView = { ...readLocalPreferences(memoryFallback), ...patch };
  try { getStorage()?.setItem(PREFS_STORAGE_KEY, JSON.stringify(next)); } catch { /* ignore */ }
  if (memoryFallback) Object.assign(memoryFallback, next);
  return next;
}
