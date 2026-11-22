import { v } from 'convex/values';
import { internalMutation, internalQuery } from './_generated/server';
import { ensureLearnerRow } from './preferences';

const MAX_LEARNER_FACTS = 40;

const reviewValidator = v.object({
  sessionId: v.string(),
  scenarioId: v.string(),
  estimateLabel: v.string(),
  summary: v.string(),
  cefrSignal: v.string(),
  completedGoalIds: v.array(v.string()),
  grammarFocuses: v.array(v.string()),
  vocabularyUsed: v.array(v.string()),
  turnCount: v.number(),
  learnerWordCount: v.number(),
  createdAtUnixMs: v.number(),
});

const turnValidator = v.object({
  turnId: v.string(),
  role: v.string(),
  text: v.string(),
  correction: v.optional(v.string()),
  followUpPrompt: v.optional(v.string()),
  timestampUnixMs: v.number(),
});

export const createSession = internalMutation({
  args: {
    learnerId: v.string(),
    sessionId: v.string(),
    correctionMode: v.string(),
    pushToTalk: v.boolean(),
    scenarioId: v.string(),
    topic: v.string(),
    cefrLevel: v.string(),
    maxTurns: v.number(),
    targetVocabulary: v.array(v.string()),
    startedAtUnixMs: v.number(),
    openerTurn: v.optional(turnValidator),
  },
  handler: async (ctx, args) => {
    await ensureLearnerRow(ctx, args.learnerId, args.startedAtUnixMs);
    await ctx.db.insert('conversations', {
      learnerId: args.learnerId,
      sessionId: args.sessionId,
      state: 'Listening',
      correctionMode: args.correctionMode,
      pushToTalk: args.pushToTalk,
      scenarioId: args.scenarioId,
      topic: args.topic,
      cefrLevel: args.cefrLevel,
      phase: 'Opener',
      turnCount: 0,
      maxTurns: args.maxTurns,
      targetVocabulary: args.targetVocabulary,
      completedGoalIds: [],
      startedAtUnixMs: args.startedAtUnixMs,
    });
    if (args.openerTurn) {
      await ctx.db.insert('conversation_turns', {
        learnerId: args.learnerId,
        sessionId: args.sessionId,
        ...args.openerTurn,
      });
    }
    return { ok: true };
  },
});

export const getSession = internalQuery({
  args: { learnerId: v.string(), sessionId: v.string() },
  handler: async (ctx, { learnerId, sessionId }) => {
    const conv = await ctx.db
      .query('conversations')
      .withIndex('by_learner_session', (q) => q.eq('learnerId', learnerId).eq('sessionId', sessionId))
      .first();
    if (!conv) return null;
    const turns = await ctx.db
      .query('conversation_turns')
      .withIndex('by_learner_session', (q) => q.eq('learnerId', learnerId).eq('sessionId', sessionId))
      .collect();
    turns.sort((a, b) => a.timestampUnixMs - b.timestampUnixMs);
    const review = await ctx.db
      .query('session_reviews')
      .withIndex('by_learner_session', (q) => q.eq('learnerId', learnerId).eq('sessionId', sessionId))
      .first();
    return { session: conv, turns, review: review ?? undefined };
  },
});

export const listSessions = internalQuery({
  args: { learnerId: v.string() },
  handler: async (ctx, { learnerId }) => {
    const rows = await ctx.db
      .query('conversations')
      .withIndex('by_learner_id', (q) => q.eq('learnerId', learnerId))
      .collect();
    rows.sort((a, b) => b.startedAtUnixMs - a.startedAtUnixMs);
    return rows.slice(0, 50).map((c) => ({
      sessionId: c.sessionId,
      startedAtUnixMs: c.startedAtUnixMs,
      ...(c.endedAtUnixMs !== undefined ? { endedAtUnixMs: c.endedAtUnixMs } : {}),
      correctionMode: c.correctionMode,
      turnCount: c.turnCount,
    }));
  },
});

export const recordTurn = internalMutation({
  args: {
    learnerId: v.string(),
    sessionId: v.string(),
    userTurn: turnValidator,
    tutorTurn: turnValidator,
    correctionMode: v.string(),
    phase: v.string(),
    turnCount: v.number(),
    completedGoalIds: v.array(v.string()),
    state: v.string(),
    endedAtUnixMs: v.optional(v.number()),
    sessionReview: v.optional(reviewValidator),
    extractedFact: v.optional(v.object({ factId: v.string(), factText: v.string(), createdAtUnixMs: v.number() })),
  },
  handler: async (ctx, args) => {
    const conv = await ctx.db
      .query('conversations')
      .withIndex('by_learner_session', (q) => q.eq('learnerId', args.learnerId).eq('sessionId', args.sessionId))
      .first();
    if (!conv) return { ok: false as const, status: 404, error: 'Session not found.' };
    await ctx.db.insert('conversation_turns', { learnerId: args.learnerId, sessionId: args.sessionId, ...args.userTurn });
    await ctx.db.insert('conversation_turns', { learnerId: args.learnerId, sessionId: args.sessionId, ...args.tutorTurn });
    await ctx.db.patch(conv._id, {
      correctionMode: args.correctionMode,
      phase: args.phase,
      turnCount: args.turnCount,
      completedGoalIds: args.completedGoalIds,
      state: args.state,
      ...(args.endedAtUnixMs !== undefined ? { endedAtUnixMs: args.endedAtUnixMs } : {}),
    });
    if (args.sessionReview) {
      const existingRev = await ctx.db
        .query('session_reviews')
        .withIndex('by_learner_session', (q) => q.eq('learnerId', args.learnerId).eq('sessionId', args.sessionId))
        .first();
      if (!existingRev) {
        await ctx.db.insert('session_reviews', { learnerId: args.learnerId, ...args.sessionReview });
      }
    }
    if (args.extractedFact) {
      const facts = await ctx.db
        .query('learner_facts')
        .withIndex('by_learner_id', (q) => q.eq('learnerId', args.learnerId))
        .collect();
      const dup = facts.some((f) => f.factText.toLowerCase() === args.extractedFact!.factText.toLowerCase());
      if (!dup && facts.length < MAX_LEARNER_FACTS) {
        await ctx.db.insert('learner_facts', {
          learnerId: args.learnerId,
          factId: args.extractedFact.factId,
          factText: args.extractedFact.factText,
          sourceSessionId: args.sessionId,
          createdAtUnixMs: args.extractedFact.createdAtUnixMs,
        });
      }
    }
    return { ok: true as const };
  },
});

export const updateSessionState = internalMutation({
  args: {
    learnerId: v.string(),
    sessionId: v.string(),
    op: v.union(v.literal('pause'), v.literal('resume'), v.literal('stop')),
    endedAtUnixMs: v.optional(v.number()),
    sessionReview: v.optional(reviewValidator),
  },
  handler: async (ctx, args) => {
    const conv = await ctx.db
      .query('conversations')
      .withIndex('by_learner_session', (q) => q.eq('learnerId', args.learnerId).eq('sessionId', args.sessionId))
      .first();
    if (!conv) return { ok: false as const, status: 404, error: 'Session not found.' };
    const valid =
      (args.op === 'pause' && (conv.state === 'Listening' || conv.state === 'Thinking' || conv.state === 'Speaking')) ||
      (args.op === 'resume' && (conv.state === 'Paused' || conv.state === 'Error')) ||
      (args.op === 'stop' && conv.state !== 'Idle' && conv.state !== 'Ended');
    if (!valid) return { ok: false as const, status: 409, error: `Cannot ${args.op} session from state ${conv.state}` };
    const nextState = args.op === 'pause' ? 'Paused' : args.op === 'resume' ? 'Listening' : 'Ended';
    await ctx.db.patch(conv._id, {
      state: nextState,
      ...(args.op === 'stop' ? { endedAtUnixMs: args.endedAtUnixMs ?? Date.now() } : {}),
    });
    if (args.op === 'stop' && args.sessionReview) {
      const existingRev = await ctx.db
        .query('session_reviews')
        .withIndex('by_learner_session', (q) => q.eq('learnerId', args.learnerId).eq('sessionId', args.sessionId))
        .first();
      if (!existingRev) {
        await ctx.db.insert('session_reviews', { learnerId: args.learnerId, ...args.sessionReview });
      }
    }
    return { ok: true as const, state: nextState };
  },
});
