import { sanitizeUntrustedText, type ConversationTurn, type CorrectionMode, type TutorReply } from '../types/domain';

interface GrammarRule {
  test: RegExp;
  gentle: string;
  detailed: string;
}

const GRAMMAR_RULES: GrammarRule[] = [
  {
    test: /\b(yesterday|last night|last week|last year|\w+ ago)\b.*\b(go|come|see|eat|buy|talk|meet|drink|make|take|have|get|give)\b/i,
    gentle: 'Use simple past tense for finished past time (e.g., "went", "saw", "talked").',
    detailed: 'Past time markers ("yesterday", "last week", "ago") pair with simple past verbs instead of base verbs.',
  },
  {
    test: /\bsince\s+(\d+|one|two|three|four|five|six|many|several|a couple)\s+(years?|months?|weeks?|days?|hours?)\b/i,
    gentle: 'Use "for" before a duration ("for three years") and "since" before a starting point.',
    detailed: 'Use "for + duration" with present perfect ("I have been working for three years"), whereas "since" marks a starting point.',
  },
  {
    test: /\b(didn't|did not)\s+(went|saw|ate|came|bought|took|had|made|got)\b/i,
    gentle: 'After "didn’t", keep the main verb in its base form (e.g., "didn’t go", not "didn’t went").',
    detailed: 'Because "did/didn’t" carries past tense, follow it with the base infinitive verb ("didn’t go/see/have").',
  },
  {
    test: /\b(more better|more easier|more faster|more cheaper|more bigger|more harder)\b/i,
    gentle: 'Say "better" / "easier" / "faster" without "more" in front.',
    detailed: 'Short "-er" comparatives and irregular "better" already express comparison without "more".',
  },
  {
    test: /\bi am agree\b/i,
    gentle: 'In English, say "I agree" rather than "I am agree".',
    detailed: '"Agree" is a full action verb, so pair it directly with the subject ("I agree") rather than "am".',
  },
  {
    test: /\b(informations|advices|homeworks|equipments|furnitures)\b/i,
    gentle: 'Words like "information", "advice", and "homework" stay singular in English.',
    detailed: 'Uncountable nouns ("information", "advice", "homework", "equipment") take no "-s"; use "some advice" or "pieces of information".',
  },
];

const STOP_WORDS = new Set([
  'i', 'me', 'my', 'mine', 'myself', 'we', 'our', 'ours', 'us', 'you', 'your', 'yours',
  'he', 'him', 'his', 'she', 'her', 'hers', 'it', 'its', 'they', 'them', 'their',
  'a', 'an', 'the', 'and', 'or', 'but', 'is', 'am', 'are', 'was', 'were', 'be', 'been',
  'being', 'have', 'has', 'had', 'do', 'does', 'did', 'will', 'would', 'could', 'should',
  'can', 'may', 'might', 'to', 'of', 'in', 'on', 'at', 'for', 'with', 'from', 'by', 'about',
  'as', 'into', 'near', 'under', 'over', 'around', 'before', 'after', 'that', 'this',
  'these', 'those', 'then', 'just', 'very', 'really', 'also', 'so', 'well', 'today',
  'yesterday', 'tomorrow', 'now', 'here', 'there', 'because', 'when', 'while', 'if',
  'not', 'no', 'yes', 'all', 'some', 'more', 'most', 'much', 'many', 'already',
  'hello', 'hi', 'hey', 'okay', 'ok', 'thanks', 'thank', 'sure', 'maybe',
]);

function extractKeywords(text: string): string[] {
  const tokens = text.match(/[A-Za-z0-9'’-]+/g) ?? [];
  const seen = new Set<string>();
  const out: string[] = [];
  tokens.forEach((raw, idx) => {
    const clean = raw.replace(/^['’-]+|['’-]+$/g, '');
    const lower = clean.toLowerCase();
    const isProper = idx > 0 && /^[A-Z][a-z]/.test(clean);
    if (!clean || (STOP_WORDS.has(lower) && !isProper) || (clean.length < 3 && !isProper)) return;
    if (!seen.has(lower)) {
      seen.add(lower);
      out.push(clean);
    }
  });
  return out;
}

function mirrorPronouns(text: string): string {
  return text
    .replace(/\bI'm\b/gi, "you're")
    .replace(/\bI\s+am\b/gi, 'you are')
    .replace(/\bI\s+was\b/gi, 'you were')
    .replace(/\bI've\b/gi, "you've")
    .replace(/\bI'd\b/gi, "you'd")
    .replace(/\bI'll\b/gi, "you'll")
    .replace(/\bwe're\b/gi, "you're")
    .replace(/\bwe\s+are\b/gi, 'you are')
    .replace(/\bwe\s+were\b/gi, 'you were')
    .replace(/\bmyself\b/gi, 'yourself')
    .replace(/\bmine\b/gi, 'yours')
    .replace(/\b(my|our)\b/gi, 'your')
    .replace(/\b(I|me|we|us)\b/gi, 'you');
}

function extractClauseFocus(cleaned: string, keywords: string[], hasGrammarErr: boolean): string {
  if (hasGrammarErr && keywords.length > 0) {
    const nouns = keywords.filter((k) => k.length > 4);
    return (nouns.length > 0 ? nouns : keywords).slice(-2).join(' and ');
  }
  const firstClause = cleaned.split(/[.!?;—]/)[0]?.trim() ?? cleaned;
  const trimmedLead = firstClause
    .replace(/^(yesterday|today|tonight|last night|last week|actually|well|so),?\s+/i, '')
    .replace(/\s+(today|yesterday|right now)$/i, '')
    .trim();
  const words = mirrorPronouns(trimmedLead).split(/\s+/).filter(Boolean);
  let polished = (words.length > 11 ? words.slice(0, 10) : words).join(' ');
  const danglingTail = /\s+\b(a|an|the|to|for|with|and|or|in|on|at|of|next|this|that)$/i;
  while (danglingTail.test(polished)) polished = polished.replace(danglingTail, '').trim();
  return words.length >= 2 ? polished : keywords.slice(0, 3).join(' ');
}

function findPriorAnchor(history: readonly ConversationTurn[], currentKeywords: string[]): string | undefined {
  const currentSet = new Set(currentKeywords.map((w) => w.toLowerCase()));
  for (let i = history.length - 1; i >= 0; i--) {
    if (history[i].role !== 'User') continue;
    const prev = extractKeywords(history[i].text).filter((k) => !currentSet.has(k.toLowerCase()));
    const proper = prev.find((k) => /^[A-Z]/.test(k));
    if (proper) return proper;
    if (prev.length > 0) return prev.reduce((best, k) => (k.length > best.length ? k : best), prev[0]);
  }
  return undefined;
}

function pickTopicAnchor(keywords: string[]): string {
  if (keywords.length === 0) return '';
  const proper = keywords.find((k) => /^[A-Z]/.test(k));
  if (proper) return proper;
  const nounLike = keywords.filter((k) => !/(ed|ing|ly)$/i.test(k));
  const pool = nounLike.length > 0 ? nounLike : keywords;
  return pool.reduce((best, k) => (k.length >= best.length ? k : best), pool[0]);
}

function capitalize(text: string): string {
  return text ? `${text[0].toUpperCase()}${text.slice(1)}` : text;
}

function lowercaseLeadingPronoun(text: string): string {
  return text.replace(/^(He|She|It|They|We|You)\b/, (m) => m.toLowerCase());
}

export function generateTutorCoachingReply(
  userText: string,
  mode: CorrectionMode,
  history: readonly ConversationTurn[] = [],
): TutorReply {
  const cleaned = sanitizeUntrustedText(userText, 1000);
  const grammar = GRAMMAR_RULES.find((r) => r.test.test(cleaned));
  const keywords = extractKeywords(cleaned);
  const priorAnchor = findPriorAnchor(history, keywords);
  const focus = extractClauseFocus(cleaned, keywords, Boolean(grammar));
  const topicAnchor = pickTopicAnchor(keywords);

  let spokenReply: string;
  let followUpPrompt: string;
  let suggestedReplies: string[];
  if (keywords.length === 0) {
    spokenReply = priorAnchor
      ? `Speaking of ${priorAnchor}, what happened next with ${priorAnchor}?`
      : `Got it! Tell me a bit more about what you've been up to today.`;
    followUpPrompt = priorAnchor
      ? `What's one quick detail about ${priorAnchor}?`
      : 'What happened earlier today, or what are you working on?';
    suggestedReplies = priorAnchor
      ? [`About ${priorAnchor}, it went really well!`, `We decided to keep going after that.`, `Actually, let's switch to another topic.`]
      : ['Today was pretty busy, actually.', 'I got a coffee and caught up on work.', "Nothing much — how's your day going?"];
  } else if (priorAnchor) {
    spokenReply = `Oh nice — thinking back to ${priorAnchor}, ${lowercaseLeadingPronoun(focus)} sounds like a great moment! How did ${topicAnchor} turn out?`;
    followUpPrompt = `How did ${priorAnchor} connect with ${topicAnchor} for you?`;
    suggestedReplies = [
      `Honestly, ${topicAnchor} was my favorite part!`,
      `After ${priorAnchor}, we spent more time enjoying ${topicAnchor}.`,
      `I want to try ${topicAnchor} again next time.`,
    ];
  } else if (grammar) {
    spokenReply = `Gotcha! Sounds like things were busy with ${focus}. How did ${topicAnchor} turn out?`;
    followUpPrompt = `What happened right after that with ${topicAnchor}?`;
    suggestedReplies = [
      `It turned out great in the end.`,
      `I also want to get better at talking about ${topicAnchor}.`,
      `Wait — could you say that sentence in a more natural way?`,
    ];
  } else {
    spokenReply = `Oh nice! ${capitalize(focus)} sounds great. What stood out most about ${topicAnchor}?`;
    followUpPrompt = `What else happened around ${topicAnchor}?`;
    suggestedReplies = [
      `The best part about ${topicAnchor} was how relaxed it felt.`,
      `I actually learned something new from it.`,
      `Who else were you with? Tell me what you'd say next!`,
    ];
  }

  const correction = mode === 'Off'
    ? undefined
    : grammar
      ? mode === 'Detailed' ? grammar.detailed : mode === 'Gentle' ? grammar.gentle : undefined
      : mode === 'Detailed' && keywords.length > 0
        ? `Natural phrasing around "${topicAnchor}". Try adding a quick time or reason to keep it flowing.`
        : undefined;

  return {
    spokenReply,
    ...(correction ? { correction } : {}),
    followUpPrompt,
    suggestedReplies,
  };
}
