import type { CefrLevel, TutorReply } from '../types/domain';
import { CORE_SCRIPTS, type PreConversationScript, type ScriptDialogueLine } from './scriptsDataCore';
import { EXTRA_SCRIPTS } from './scriptsDataExtra';

export type ScriptTier = 'A1-A2' | 'B1-B2' | 'C1-C2';
export type { PreConversationScript, ScriptDialogueLine };

const ALL_SCRIPTS: Record<string, Record<ScriptTier, PreConversationScript>> = {
  ...CORE_SCRIPTS,
  ...EXTRA_SCRIPTS,
};

export function cefrToScriptTier(level: CefrLevel): ScriptTier {
  if (level === 'A1' || level === 'A2') return 'A1-A2';
  if (level === 'C1' || level === 'C2') return 'C1-C2';
  return 'B1-B2';
}

export function getPreConversationScript(scenarioId: string, level: CefrLevel): PreConversationScript {
  const tier = cefrToScriptTier(level);
  const byScenario = ALL_SCRIPTS[scenarioId] ?? ALL_SCRIPTS.free_conversation;
  return byScenario[tier];
}

export function resolveSuggestedReplies(
  reply: TutorReply | null,
  scenarioId: string,
  level: CefrLevel,
): string[] {
  const fromAi = (reply?.suggestedReplies ?? []).map((s) => s.trim()).filter(Boolean).slice(0, 3);
  if (fromAi.length > 0) return fromAi;
  return getPreConversationScript(scenarioId, level).quickReplies;
}
