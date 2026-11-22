import { v } from 'convex/values';
import { internalMutation, internalQuery, type MutationCtx } from './_generated/server';

const VALID_MODES = new Set(['Natural', 'Gentle', 'Detailed', 'Off']);
const VALID_CEFR = new Set(['A1', 'A2', 'B1', 'B2', 'C1', 'C2']);
const VALID_PROVIDERS = new Set(['auto', 'workers_ai', 'groq', 'gemini', 'openrouter']);

export interface StoredSettingsRecord {
  correctionMode: string;
  speakingSpeed: number;
  pushToTalkEnabled: boolean;
  cefrLevel: string;
  englishVoiceId: string;
  cloudOptIn: boolean;
  preferredProvider: string;
  modelOverridesJson: string;
}

export const DEFAULT_SETTINGS: StoredSettingsRecord = {
  correctionMode: 'Natural',
  speakingSpeed: 1.0,
  pushToTalkEnabled: false,
  cefrLevel: 'B1',
  englishVoiceId: 'en-US',
  cloudOptIn: true,
  preferredProvider: 'auto',
  modelOverridesJson: '{}',
};

export async function ensureLearnerRow(
  ctx: MutationCtx,
  learnerId: string,
  nowUnixMs: number,
): Promise<void> {
  const existing = await ctx.db
    .query('users')
    .withIndex('by_learner_id', (q) => q.eq('learnerId', learnerId))
    .first();
  if (!existing) {
    await ctx.db.insert('users', { learnerId, createdAtUnixMs: nowUnixMs });
  }
}

function sanitizeEnglishVoice(raw?: string): string | null {
  if (raw === undefined) return null;
  const trimmed = raw.trim();
  if (!/^en-[A-Za-z]{2,8}$/i.test(trimmed)) return null;
  const [lang, region] = trimmed.split('-');
  return `${lang.toLowerCase()}-${region.toUpperCase()}`;
}

function sanitizeModelOverrides(currentJson: string, patch?: Record<string, string>): string {
  if (!patch) return currentJson;
  let merged: Record<string, string> = {};
  try {
    merged = JSON.parse(currentJson) as Record<string, string>;
  } catch {
    merged = {};
  }
  for (const [k, val] of Object.entries(patch)) {
    if (!VALID_PROVIDERS.has(k) || k === 'auto') continue;
    const clean = val.trim().slice(0, 120);
    if (clean) merged[k] = clean;
    else delete merged[k];
  }
  return JSON.stringify(merged);
}

export const getSettings = internalQuery({
  args: { learnerId: v.string() },
  handler: async (ctx, { learnerId }): Promise<StoredSettingsRecord> => {
    const row = await ctx.db
      .query('session_settings')
      .withIndex('by_learner_id', (q) => q.eq('learnerId', learnerId))
      .first();
    if (!row) return { ...DEFAULT_SETTINGS };
    return {
      correctionMode: row.correctionMode,
      speakingSpeed: row.speakingSpeed,
      pushToTalkEnabled: row.pushToTalkEnabled,
      cefrLevel: row.cefrLevel,
      englishVoiceId: row.englishVoiceId,
      cloudOptIn: row.cloudOptIn,
      preferredProvider: row.preferredProvider,
      modelOverridesJson: row.modelOverridesJson,
    };
  },
});

export const patchSettings = internalMutation({
  args: {
    learnerId: v.string(),
    correctionMode: v.optional(v.string()),
    speakingSpeed: v.optional(v.number()),
    pushToTalkEnabled: v.optional(v.boolean()),
    cefrLevel: v.optional(v.string()),
    englishVoiceId: v.optional(v.string()),
    cloudOptIn: v.optional(v.boolean()),
    preferredProvider: v.optional(v.string()),
    modelOverrides: v.optional(v.record(v.string(), v.string())),
  },
  handler: async (ctx, args): Promise<StoredSettingsRecord> => {
    const now = Date.now();
    await ensureLearnerRow(ctx, args.learnerId, now);
    const row = await ctx.db
      .query('session_settings')
      .withIndex('by_learner_id', (q) => q.eq('learnerId', args.learnerId))
      .first();
    const base = row ?? { ...DEFAULT_SETTINGS };
    const voice = args.englishVoiceId !== undefined ? sanitizeEnglishVoice(args.englishVoiceId) : null;
    const next: StoredSettingsRecord = {
      correctionMode: args.correctionMode && VALID_MODES.has(args.correctionMode) ? args.correctionMode : base.correctionMode,
      speakingSpeed: args.speakingSpeed !== undefined ? Math.min(1.5, Math.max(0.5, Number(args.speakingSpeed) || 1.0)) : base.speakingSpeed,
      pushToTalkEnabled: args.pushToTalkEnabled ?? base.pushToTalkEnabled,
      cefrLevel: args.cefrLevel && VALID_CEFR.has(args.cefrLevel) ? args.cefrLevel : base.cefrLevel,
      englishVoiceId: voice ?? base.englishVoiceId,
      cloudOptIn: args.cloudOptIn ?? base.cloudOptIn,
      preferredProvider: args.preferredProvider && VALID_PROVIDERS.has(args.preferredProvider) ? args.preferredProvider : base.preferredProvider,
      modelOverridesJson: sanitizeModelOverrides(base.modelOverridesJson, args.modelOverrides),
    };
    if (row) {
      await ctx.db.patch(row._id, { ...next, updatedAtUnixMs: now });
    } else {
      await ctx.db.insert('session_settings', { learnerId: args.learnerId, ...next, updatedAtUnixMs: now });
    }
    return next;
  },
});
