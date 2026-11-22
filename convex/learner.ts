import { v } from 'convex/values';
import { internalMutation, internalQuery } from './_generated/server';

const MS_PER_DAY = 86_400_000;
const MAX_SESSION_MS = 30 * 60_000;
const MAX_TURNS_PER_MIN = 20;
const MAX_TURNS_PER_DAY = 120;

export const getReview = internalQuery({
  args: { learnerId: v.string(), sessionId: v.string() },
  handler: async (ctx, { learnerId, sessionId }) => {
    const rev = await ctx.db
      .query('session_reviews')
      .withIndex('by_learner_session', (q) => q.eq('learnerId', learnerId).eq('sessionId', sessionId))
      .first();
    if (!rev) return null;
    const { _id, _creationTime, learnerId: _lid, ...rest } = rev;
    return rest;
  },
});

export const listFacts = internalQuery({
  args: { learnerId: v.string() },
  handler: async (ctx, { learnerId }) => {
    const rows = await ctx.db
      .query('learner_facts')
      .withIndex('by_learner_id', (q) => q.eq('learnerId', learnerId))
      .collect();
    rows.sort((a, b) => b.createdAtUnixMs - a.createdAtUnixMs);
    return rows.map((r) => ({
      id: r.factId,
      factText: r.factText,
      ...(r.sourceSessionId ? { sourceSessionId: r.sourceSessionId } : {}),
      createdAtUnixMs: r.createdAtUnixMs,
    }));
  },
});

export const deleteFact = internalMutation({
  args: { learnerId: v.string(), factId: v.string() },
  handler: async (ctx, { learnerId, factId }) => {
    const row = await ctx.db
      .query('learner_facts')
      .withIndex('by_learner_fact', (q) => q.eq('learnerId', learnerId).eq('factId', factId))
      .first();
    if (row) await ctx.db.delete(row._id);
    return { deleted: Boolean(row) };
  },
});

export const clearFacts = internalMutation({
  args: { learnerId: v.string() },
  handler: async (ctx, { learnerId }) => {
    const rows = await ctx.db
      .query('learner_facts')
      .withIndex('by_learner_id', (q) => q.eq('learnerId', learnerId))
      .collect();
    for (const r of rows) await ctx.db.delete(r._id);
    return { deletedCount: rows.length };
  },
});

function computeDayStreak(timestamps: number[], nowUnixMs: number): number {
  const days = new Set(timestamps.filter((t) => t > 0).map((t) => Math.floor(t / MS_PER_DAY)));
  if (days.size === 0) return 0;
  const latestDay = Math.max(...days);
  const today = nowUnixMs > 0 ? Math.floor(nowUnixMs / MS_PER_DAY) : latestDay;
  if (today - latestDay > 1) return 0;
  let cursor = latestDay;
  let streak = 0;
  while (days.has(cursor)) {
    streak += 1;
    cursor -= 1;
  }
  return streak;
}

export const getProgress = internalQuery({
  args: { learnerId: v.string(), nowUnixMs: v.optional(v.number()) },
  handler: async (ctx, { learnerId, nowUnixMs }) => {
    const convs = await ctx.db
      .query('conversations')
      .withIndex('by_learner_id', (q) => q.eq('learnerId', learnerId))
      .collect();
    const practicedConvs = convs.filter((c) => c.endedAtUnixMs !== undefined || c.turnCount > 0);
    const reviews = await ctx.db
      .query('session_reviews')
      .withIndex('by_learner_id', (q) => q.eq('learnerId', learnerId))
      .collect();
    const durationMs = practicedConvs.reduce((acc, c) => {
      const span = Math.max(0, (c.endedAtUnixMs ?? c.startedAtUnixMs) - c.startedAtUnixMs);
      return acc + Math.min(MAX_SESSION_MS, span);
    }, 0);
    const totalWords = reviews.reduce((acc, r) => acc + Math.max(0, r.learnerWordCount), 0);
    const effectiveMs = Math.max(durationMs, Math.floor((totalWords * 60_000) / 110));
    const practiceMinutes = effectiveMs > 0 ? Math.max(1, Math.ceil(effectiveMs / 60_000)) : 0;
    const seenLower = new Set<string>();
    const vocabularyUsed: string[] = [];
    let completedGoalsCount = 0;
    for (const r of reviews) {
      completedGoalsCount += r.completedGoalIds.length;
      for (const word of r.vocabularyUsed) {
        const clean = word.trim();
        if (clean && !seenLower.has(clean.toLowerCase())) {
          seenLower.add(clean.toLowerCase());
          vocabularyUsed.push(clean);
        }
      }
    }
    const timestamps = [
      ...practicedConvs.map((c) => c.endedAtUnixMs ?? c.startedAtUnixMs),
      ...reviews.map((r) => r.createdAtUnixMs),
    ];
    return {
      totalSessions: Math.max(practicedConvs.length, reviews.length),
      practiceMinutes,
      completedGoalsCount,
      vocabularyUsed,
      dayStreak: computeDayStreak(timestamps, nowUnixMs ?? Date.now()),
    };
  },
});

export const consumeRateLimit = internalMutation({
  args: { bucketKeys: v.array(v.string()), nowUnixMs: v.optional(v.number()) },
  handler: async (ctx, { bucketKeys, nowUnixMs }) => {
    const now = nowUnixMs ?? Date.now();
    const minBucket = Math.floor(now / 60_000);
    const dayBucket = new Date(now).toISOString().slice(0, 10);
    const rows = await Promise.all(
      bucketKeys.map((k) =>
        ctx.db.query('rate_limits').withIndex('by_bucket_key', (q) => q.eq('bucketKey', k)).first(),
      ),
    );
    for (let i = 0; i < bucketKeys.length; i += 1) {
      const row = rows[i];
      const minCount = row && row.minuteBucket === minBucket ? row.minuteCount : 0;
      const dayCount = row && row.dayBucket === dayBucket ? row.dayCount : 0;
      if (minCount >= MAX_TURNS_PER_MIN) {
        return { allowed: false, reason: 'Per-minute free cloud turn cap reached — using offline local coach.' };
      }
      if (dayCount >= MAX_TURNS_PER_DAY) {
        return { allowed: false, reason: 'Daily free cloud turn cap reached — using offline local coach.' };
      }
    }
    for (let i = 0; i < bucketKeys.length; i += 1) {
      const row = rows[i];
      const nextMin = (row && row.minuteBucket === minBucket ? row.minuteCount : 0) + 1;
      const nextDay = (row && row.dayBucket === dayBucket ? row.dayCount : 0) + 1;
      if (row) {
        await ctx.db.patch(row._id, { minuteBucket: minBucket, minuteCount: nextMin, dayBucket, dayCount: nextDay });
      } else {
        await ctx.db.insert('rate_limits', {
          bucketKey: bucketKeys[i],
          minuteBucket: minBucket,
          minuteCount: 1,
          dayBucket,
          dayCount: 1,
        });
      }
    }
    return { allowed: true };
  },
});
