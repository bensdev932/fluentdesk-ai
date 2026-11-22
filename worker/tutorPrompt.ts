import {
  sanitizeUntrustedText,
  type CefrLevel,
  type ConversationTurn,
  type CorrectionMode,
  type ScaffoldProgress,
  type TutorReply,
} from '../src/types/domain';
import { findFallbackScenario } from '../src/services/scenariosFallback';

const WORD_BUDGET: Record<CefrLevel, number> = { A1: 25, A2: 35, B1: 50, B2: 65, C1: 80, C2: 80 };

export interface AssembledPrompt {
  system: string;
  messages: Array<{ role: 'user' | 'assistant'; content: string }>;
}

function correctionRule(mode: CorrectionMode): string {
  switch (mode) {
    case 'Natural':
      return 'Recast real grammar errors smoothly in "spokenReply" and add a brief correction note only for major clarity blockers.';
    case 'Gentle':
      return 'Keep "spokenReply" warm and provide a brief gentle correction in "correction" only for real errors.';
    case 'Detailed':
      return 'Respond warmly in "spokenReply" and explain key grammar or vocabulary improvements in "correction".';
    case 'Off':
      return 'Do NOT provide grammar corrections; always set "correction" to null.';
  }
}

function phaseRule(scaffold: ScaffoldProgress): string {
  const left = Math.max(0, scaffold.maxTurns - scaffold.turnCount);
  if (scaffold.phase === 'Closer' || scaffold.phase === 'Completed' || left <= 1) {
    return 'Phase 4 Closer: Celebrate progress warmly. Set followUpPrompt to null and ask no further question.';
  }
  if (scaffold.phase === 'Opener') return 'Phase 1 Opener: Greet warmly and respond to the learner.';
  if (scaffold.phase === 'FirstQuestion') return 'Phase 2 First Question: Respond briefly and ask one easy opening question.';
  return `Phase 3 Topic Exchange (${left} turns left): Deepen the roleplay naturally.`;
}

export function assembleTutorPrompt(
  history: readonly ConversationTurn[],
  userText: string,
  mode: CorrectionMode,
  scaffold: ScaffoldProgress,
  facts: readonly string[],
): AssembledPrompt {
  const sc = findFallbackScenario(scaffold.scenarioId);
  const pendingGoals = (scaffold.goals ?? sc.goals)
    .filter((g) => !(scaffold.completedGoalIds ?? []).includes(g.id))
    .map((g) => `${g.id} (${g.description})`);
  const vocab = (scaffold.targetVocabulary ?? sc.targetVocabulary).slice(0, 12);
  const cleanFacts = facts.map((f) => sanitizeUntrustedText(f, 140)).filter(Boolean).slice(0, 5);
  const isClosing = scaffold.phase === 'Closer' || scaffold.phase === 'Completed' || scaffold.turnCount + 1 >= scaffold.maxTurns;

  const lines = [
    'You are FluentDesk AI, a 100% pure-English voice-first spoken English tutor in character. Respond ONLY in natural English.',
    'React directly to specific details, names, events, or feelings the learner just shared first.',
    "Never use canned filler like 'That is interesting!' or 'Could you tell me more about that?'.",
    `Keep "spokenReply" conversational, voice-friendly, and under ~${WORD_BUDGET[scaffold.cefrLevel]} words.`,
    correctionRule(mode),
    isClosing ? 'Closing turn rule: Set followUpPrompt to null and ask no further question.' : 'End with one specific follow-up question tied to their topic and put only that question in "followUpPrompt".',
    `Level: CEFR ${scaffold.cefrLevel}. Topic: ${scaffold.topic}.`,
    `Roleplay persona: ${sc.tutorPersona} (learner role: ${sc.learnerRole}).`,
    phaseRule(scaffold),
    ...(vocab.length > 0 ? [`Weave in target vocabulary where natural: ${vocab.join(', ')}`] : []),
    ...(pendingGoals.length > 0 ? [`Remaining Communicative Goals (append newly met IDs to "completedGoalIds"): ${pendingGoals.join('; ')}`] : []),
    ...(cleanFacts.length > 0 ? [`Known Learner Facts: ${cleanFacts.join(' | ')}`] : []),
    'Return ONLY valid JSON: {"spokenReply":"...","correction":null,"followUpPrompt":null,"completedGoalIds":[]}',
  ];

  const messages: Array<{ role: 'user' | 'assistant'; content: string }> = [];
  for (const t of history.slice(-10)) {
    const clean = sanitizeUntrustedText(t.text, 1000);
    if (clean) messages.push({ role: t.role === 'User' ? 'user' : 'assistant', content: clean });
  }
  messages.push({ role: 'user', content: sanitizeUntrustedText(userText, 1000) });
  return { system: lines.join('\n'), messages };
}

function cleanPlaceholder(val: unknown): string | undefined {
  if (typeof val !== 'string') return undefined;
  const cleaned = sanitizeUntrustedText(val, 1500);
  if (!cleaned || /^null|none$/i.test(cleaned)) return undefined;
  return cleaned;
}

export function extractQuestionSentence(text: string): string | undefined {
  const cleaned = sanitizeUntrustedText(text, 1500);
  const qIdx = cleaned.lastIndexOf('?');
  if (qIdx < 0) return undefined;
  const prefix = cleaned.slice(0, qIdx);
  const match = [...prefix.matchAll(/[.!?\n]\s*/g)].pop();
  const start = match && match.index !== undefined ? match.index + match[0].length : 0;
  const candidate = cleaned.slice(start, qIdx + 1).trim();
  return candidate && candidate.toLowerCase() !== cleaned.toLowerCase() ? candidate : undefined;
}

export function parseTutorOutput(
  rawText: string,
  mode: CorrectionMode,
): { reply: TutorReply; completedGoalIds: string[] } | null {
  const withoutThink = rawText.trim().replace(/<think>[\s\S]*?<\/think>/gi, '').trim();
  const unfenced = withoutThink.replace(/^```(?:json)?/i, '').replace(/```$/i, '').trim();
  if (!unfenced) return null;

  const jsonStart = unfenced.indexOf('{');
  const jsonEnd = unfenced.lastIndexOf('}');
  const jsonSlice = jsonStart >= 0 && jsonEnd > jsonStart ? unfenced.slice(jsonStart, jsonEnd + 1) : unfenced;

  try {
    const parsed = JSON.parse(jsonSlice) as {
      spokenReply?: unknown;
      spoken_reply?: unknown;
      correction?: unknown;
      followUpPrompt?: unknown;
      follow_up_prompt?: unknown;
      completedGoalIds?: unknown;
      completed_goal_ids?: unknown;
    };
    const spoken = sanitizeUntrustedText(String(parsed.spokenReply ?? parsed.spoken_reply ?? ''), 1500);
    if (spoken) {
      const correction = mode === 'Off' ? undefined : cleanPlaceholder(parsed.correction);
      const rawFollow = cleanPlaceholder(parsed.followUpPrompt ?? parsed.follow_up_prompt);
      const followUpPrompt = rawFollow && rawFollow.toLowerCase() !== spoken.toLowerCase()
        ? rawFollow
        : extractQuestionSentence(spoken);
      const rawGoals = parsed.completedGoalIds ?? parsed.completed_goal_ids;
      const completedGoalIds = Array.isArray(rawGoals)
        ? [...new Set(rawGoals.map((g) => cleanPlaceholder(g)).filter((g): g is string => Boolean(g)))].slice(0, 8)
        : [];
      return {
        reply: { spokenReply: spoken, ...(correction ? { correction } : {}), ...(followUpPrompt ? { followUpPrompt } : {}) },
        completedGoalIds,
      };
    }
  } catch { /* fall back to sanitized plain text */ }

  const plain = sanitizeUntrustedText(unfenced, 1500);
  if (!plain) return null;
  const followUpPrompt = extractQuestionSentence(plain);
  return {
    reply: { spokenReply: plain, ...(followUpPrompt ? { followUpPrompt } : {}) },
    completedGoalIds: [],
  };
}

const FACT_PATTERN = /\b(i work (?:as|in|at) [^.!?\n]{3,80}|my goal is [^.!?\n]{3,80}|i live in [^.!?\n]{2,60}|i am studying [^.!?\n]{3,70}|i enjoy [^.!?\n]{3,70})/i;

export function extractLearnerFact(userText: string): string | undefined {
  const match = sanitizeUntrustedText(userText, 500).match(FACT_PATTERN);
  if (!match) return undefined;
  const clean = match[0].replace(/\s+/g, ' ').trim().slice(0, 120);
  return clean.length >= 12 ? clean[0].toUpperCase() + clean.slice(1) : undefined;
}
