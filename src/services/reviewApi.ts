import { apiUrl, requestJson } from './apiTransport';
import type {
  LearnerFact,
  LearnerPreferencesPatch,
  LearnerPreferencesView,
  LearnerProgress,
  SessionId,
  SessionReview,
} from '../types/domain';

const EMPTY_PROGRESS: LearnerProgress = {
  totalSessions: 0,
  practiceMinutes: 0,
  completedGoalsCount: 0,
  vocabularyUsed: [],
  dayStreak: 0,
};

export async function fetchSessionReview(baseUrl: string, sid: SessionId): Promise<SessionReview | null> {
  const res = await requestJson<SessionReview>(apiUrl(`/api/sessions/${encodeURIComponent(sid)}/review`, baseUrl));
  return res.ok ? res.data : null;
}

export async function fetchLearnerFacts(baseUrl: string): Promise<LearnerFact[]> {
  const res = await requestJson<LearnerFact[]>(apiUrl('/api/learner/facts', baseUrl));
  return res.ok ? res.data : [];
}

export async function removeLearnerFact(baseUrl: string, factId: string): Promise<void> {
  await requestJson(apiUrl(`/api/learner/facts/${encodeURIComponent(factId)}`, baseUrl), { method: 'DELETE' });
}

export async function clearAllLearnerFacts(baseUrl: string): Promise<void> {
  await requestJson(apiUrl('/api/learner/facts', baseUrl), { method: 'DELETE' });
}

export async function fetchLearnerProgress(baseUrl: string): Promise<LearnerProgress> {
  const res = await requestJson<LearnerProgress>(apiUrl('/api/learner/progress', baseUrl));
  return res.ok ? res.data : EMPTY_PROGRESS;
}

export async function fetchLearnerPreferences(baseUrl: string): Promise<LearnerPreferencesView | null> {
  const res = await requestJson<LearnerPreferencesView>(apiUrl('/api/preferences', baseUrl));
  return res.ok ? res.data : null;
}

export async function patchLearnerPreferences(
  baseUrl: string,
  patch: LearnerPreferencesPatch,
): Promise<LearnerPreferencesView | null> {
  const res = await requestJson<LearnerPreferencesView>(apiUrl('/api/preferences', baseUrl), {
    method: 'PATCH',
    body: patch,
  });
  return res.ok ? res.data : null;
}
