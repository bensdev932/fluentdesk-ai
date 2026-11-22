import type { LearnerFact, LearnerProgress, SessionReview } from '../src/types/domain';
import type { BridgeClient, StoredSession, StoredSettings, StoredTurn } from './types';

interface BucketCounter { min: number; minCount: number; day: string; dayCount: number }
interface LearnerBucket {
  settings: StoredSettings;
  sessions: Map<string, { session: StoredSession; turns: StoredTurn[]; review?: SessionReview }>;
  facts: LearnerFact[];
}

const DEFAULT_SETTINGS: StoredSettings = {
  correctionMode: 'Natural', speakingSpeed: 1.0, pushToTalkEnabled: false,
  cefrLevel: 'B1', englishVoiceId: 'en-US', cloudOptIn: true,
  preferredProvider: 'auto', modelOverridesJson: '{}',
};

export function createInMemoryBridgeClient(limits = { perMin: 20, perDay: 120 }): BridgeClient {
  const learners = new Map<string, LearnerBucket>();
  const rateBuckets = new Map<string, BucketCounter>();

  const getBucket = (id: string): LearnerBucket => {
    let b = learners.get(id);
    if (!b) {
      b = { settings: { ...DEFAULT_SETTINGS }, sessions: new Map(), facts: [] };
      learners.set(id, b);
    }
    return b;
  };

  return {
    async getSettings(lid) { return { ...getBucket(lid).settings }; },
    async patchSettings(lid, patch) {
      const b = getBucket(lid);
      const { modelOverrides, ...rest } = patch;
      let merged = JSON.parse(b.settings.modelOverridesJson || '{}') as Record<string, string>;
      if (modelOverrides) {
        merged = { ...merged };
        for (const [k, v] of Object.entries(modelOverrides)) {
          if (v.trim()) merged[k] = v.trim();
          else delete merged[k];
        }
      }
      b.settings = { ...b.settings, ...rest, modelOverridesJson: JSON.stringify(merged) };
      return { ...b.settings };
    },
    async createSession(lid, args) {
      const b = getBucket(lid);
      const { openerTurn, ...rest } = args;
      b.sessions.set(args.sessionId, {
        session: { ...rest, state: 'Listening', phase: 'Opener', turnCount: 0, completedGoalIds: [] },
        turns: openerTurn ? [openerTurn] : [],
      });
      return { ok: true };
    },
    async getSession(lid, sid) {
      const item = getBucket(lid).sessions.get(sid);
      return item ? { session: { ...item.session }, turns: [...item.turns], review: item.review } : null;
    },
    async listSessions(lid) {
      return [...getBucket(lid).sessions.values()]
        .map((s) => s.session)
        .sort((a, b) => b.startedAtUnixMs - a.startedAtUnixMs)
        .map((c) => ({ sessionId: c.sessionId, startedAtUnixMs: c.startedAtUnixMs, ...(c.endedAtUnixMs !== undefined ? { endedAtUnixMs: c.endedAtUnixMs } : {}), correctionMode: c.correctionMode, turnCount: c.turnCount }));
    },
    async recordTurn(lid, args) {
      const b = getBucket(lid);
      const entry = b.sessions.get(args.sessionId);
      if (!entry) return { ok: false, status: 404, error: 'Session not found.' };
      entry.turns.push(args.userTurn, args.tutorTurn);
      entry.session = { ...entry.session, correctionMode: args.correctionMode, phase: args.phase, turnCount: args.turnCount, completedGoalIds: args.completedGoalIds, state: args.state, ...(args.endedAtUnixMs !== undefined ? { endedAtUnixMs: args.endedAtUnixMs } : {}) };
      if (args.sessionReview) entry.review = args.sessionReview;
      if (args.extractedFact && !b.facts.some((f) => f.factText.toLowerCase() === args.extractedFact!.factText.toLowerCase()) && b.facts.length < 40) {
        b.facts.unshift({ id: args.extractedFact.factId, factText: args.extractedFact.factText, sourceSessionId: args.sessionId, createdAtUnixMs: args.extractedFact.createdAtUnixMs });
      }
      return { ok: true };
    },
    async updateSessionState(lid, args) {
      const entry = getBucket(lid).sessions.get(args.sessionId);
      if (!entry) return { ok: false, status: 404, error: 'Session not found.' };
      const st = entry.session.state;
      const ok =
        (args.op === 'pause' && (st === 'Listening' || st === 'Thinking' || st === 'Speaking')) ||
        (args.op === 'resume' && (st === 'Paused' || st === 'Error')) ||
        (args.op === 'stop' && st !== 'Idle' && st !== 'Ended');
      if (!ok) return { ok: false, status: 409, error: `Cannot ${args.op} session from state ${st}` };
      entry.session.state = args.op === 'pause' ? 'Paused' : args.op === 'resume' ? 'Listening' : 'Ended';
      if (args.op === 'stop') {
        entry.session.endedAtUnixMs = args.endedAtUnixMs ?? Date.now();
        if (args.sessionReview) entry.review = args.sessionReview;
      }
      return { ok: true, state: entry.session.state };
    },
    async getReview(lid, sid) { return getBucket(lid).sessions.get(sid)?.review ?? null; },
    async listFacts(lid) { return [...getBucket(lid).facts]; },
    async deleteFact(lid, fid) { const b = getBucket(lid); b.facts = b.facts.filter((f) => f.id !== fid); },
    async clearFacts(lid) { getBucket(lid).facts = []; },
    async getProgress(lid): Promise<LearnerProgress> {
      const b = getBucket(lid);
      const all = [...b.sessions.values()];
      const reviews = all.map((x) => x.review).filter((r): r is SessionReview => Boolean(r));
      const practiced = all.filter((x) => x.session.endedAtUnixMs !== undefined || x.session.turnCount > 0);
      const vocab = [...new Set(reviews.flatMap((r) => r.vocabularyUsed))];
      const mins = practiced.reduce((n, x) => n + Math.max(1, Math.ceil(((x.session.endedAtUnixMs ?? x.session.startedAtUnixMs) - x.session.startedAtUnixMs) / 60_000)), 0);
      return {
        totalSessions: Math.max(practiced.length, reviews.length),
        practiceMinutes: mins,
        completedGoalsCount: reviews.reduce((n, r) => n + r.completedGoalIds.length, 0),
        vocabularyUsed: vocab,
        dayStreak: practiced.length > 0 ? 1 : 0,
      };
    },
    async consumeRateLimit(_lid, keys, now = Date.now()) {
      const min = Math.floor(now / 60_000);
      const day = new Date(now).toISOString().slice(0, 10);
      for (const k of keys) {
        const cur = rateBuckets.get(k);
        if (cur && cur.min === min && cur.minCount >= limits.perMin) return { allowed: false, reason: 'Per-minute free cloud turn cap reached — using offline local coach.' };
        if (cur && cur.day === day && cur.dayCount >= limits.perDay) return { allowed: false, reason: 'Daily free cloud turn cap reached — using offline local coach.' };
      }
      for (const k of keys) {
        const cur = rateBuckets.get(k);
        rateBuckets.set(k, {
          min,
          minCount: (cur && cur.min === min ? cur.minCount : 0) + 1,
          day,
          dayCount: (cur && cur.day === day ? cur.dayCount : 0) + 1,
        });
      }
      return { allowed: true };
    },
  };
}
