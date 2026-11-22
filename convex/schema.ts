import { defineSchema, defineTable } from 'convex/server';
import { v } from 'convex/values';

export default defineSchema({
  users: defineTable({
    learnerId: v.string(),
    createdAtUnixMs: v.number(),
  }).index('by_learner_id', ['learnerId']),

  session_settings: defineTable({
    learnerId: v.string(),
    correctionMode: v.string(),
    speakingSpeed: v.number(),
    pushToTalkEnabled: v.boolean(),
    cefrLevel: v.string(),
    englishVoiceId: v.string(),
    cloudOptIn: v.boolean(),
    preferredProvider: v.string(),
    modelOverridesJson: v.string(),
    updatedAtUnixMs: v.number(),
  }).index('by_learner_id', ['learnerId']),

  conversations: defineTable({
    learnerId: v.string(),
    sessionId: v.string(),
    state: v.string(),
    correctionMode: v.string(),
    pushToTalk: v.boolean(),
    scenarioId: v.string(),
    topic: v.string(),
    cefrLevel: v.string(),
    phase: v.string(),
    turnCount: v.number(),
    maxTurns: v.number(),
    targetVocabulary: v.array(v.string()),
    completedGoalIds: v.array(v.string()),
    startedAtUnixMs: v.number(),
    endedAtUnixMs: v.optional(v.number()),
  })
    .index('by_learner_id', ['learnerId'])
    .index('by_learner_session', ['learnerId', 'sessionId']),

  conversation_turns: defineTable({
    learnerId: v.string(),
    sessionId: v.string(),
    turnId: v.string(),
    role: v.string(),
    text: v.string(),
    correction: v.optional(v.string()),
    followUpPrompt: v.optional(v.string()),
    timestampUnixMs: v.number(),
  }).index('by_learner_session', ['learnerId', 'sessionId']),

  session_reviews: defineTable({
    learnerId: v.string(),
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
  })
    .index('by_learner_id', ['learnerId'])
    .index('by_learner_session', ['learnerId', 'sessionId']),

  learner_facts: defineTable({
    learnerId: v.string(),
    factId: v.string(),
    factText: v.string(),
    sourceSessionId: v.optional(v.string()),
    createdAtUnixMs: v.number(),
  })
    .index('by_learner_id', ['learnerId'])
    .index('by_learner_fact', ['learnerId', 'factId']),

  rate_limits: defineTable({
    bucketKey: v.string(),
    minuteBucket: v.number(),
    minuteCount: v.number(),
    dayBucket: v.string(),
    dayCount: v.number(),
  }).index('by_bucket_key', ['bucketKey']),
});
