import type { Page, Route } from '@playwright/test';

export interface RecordedRequests {
  turnModes: string[];
  preferencePatches: Array<Record<string, unknown>>;
}

const SCENARIOS = [
  {
    id: 'cafe_order',
    title: 'Cafe Ordering',
    summary: 'Order drinks politely at a neighborhood cafe.',
    tutorPersona: 'Friendly barista',
    learnerRole: 'Customer',
    defaultCefr: 'A2',
    defaultMaxTurns: 4,
    targetVocabulary: ['latte', 'iced', 'receipt'],
    goals: [{ id: 'order_drink', description: 'Order a drink cleanly', keywords: ['latte', 'coffee'] }],
  },
];

function jsonReply(route: Route, data: unknown, status = 200): Promise<void> {
  return route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(data) });
}

export async function setupAdaptiveCoachingStubs(page: Page): Promise<RecordedRequests> {
  const recorded: RecordedRequests = { turnModes: [], preferencePatches: [] };
  let prefs = { correctionMode: 'Gentle', speakingSpeed: 1.0, pushToTalkEnabled: false, cefrLevel: 'A2', englishVoiceId: 'en-US' };
  const focuses: string[] = [];
  let sessSeq = 0;
  let activeSessId = 'e2e-sess-1';
  let seq = 0;

  await page.addInitScript(() => {
    const w = window as unknown as {
      __spokenTexts?: string[];
      speechSynthesis: SpeechSynthesis;
      SpeechSynthesisUtterance: new (text?: string) => SpeechSynthesisUtterance;
    };
    w.__spokenTexts = [];
    w.SpeechSynthesisUtterance = class FakeUtterance {
      text: string;
      lang = 'en-US';
      rate = 1;
      voice: SpeechSynthesisVoice | null = null;
      onend: ((ev: SpeechSynthesisEvent) => void) | null = null;
      onerror: ((ev: SpeechSynthesisEvent) => void) | null = null;
      constructor(text = '') { this.text = text; }
    } as unknown as new (text?: string) => SpeechSynthesisUtterance;
    const synthMock = {
      speaking: false,
      pending: false,
      paused: false,
      getVoices: () => [{ name: 'English US', lang: 'en-US', voiceURI: 'en-US', default: true, localService: true }],
      cancel: () => {},
      pause: () => {},
      resume: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => true,
      speak: (utter: SpeechSynthesisUtterance) => {
        w.__spokenTexts?.push(utter.text);
        setTimeout(() => utter.onend?.({ type: 'end', utterance: utter } as unknown as SpeechSynthesisEvent), 5);
      },
    };
    Object.defineProperty(window, 'speechSynthesis', { value: synthMock, configurable: true });
  });

  await page.routeWebSocket('**/api/sessions/*/ws', (ws) => ws.close({ code: 1006, reason: 'Use HTTP stub' }));

  const currentReview = () => ({
    sessionId: activeSessId,
    scenarioId: 'cafe_order',
    estimateLabel: 'AI Estimate',
    summary: 'Cafe Ordering: 1/1 goals met across 2 learner turns (12 words spoken). AI Estimate for practice reflection.',
    cefrSignal: 'A2',
    completedGoalIds: ['order_drink'],
    grammarFocuses: focuses,
    vocabularyUsed: ['latte', 'iced'],
    turnCount: seq,
    learnerWordCount: 12,
    createdAtUnixMs: Date.now(),
  });

  await page.route('**/api/**', async (route) => {
    const req = route.request();
    const path = new URL(req.url()).pathname;
    const method = req.method();

    if (path === '/api/preferences' && method === 'GET') return jsonReply(route, prefs);
    if (path === '/api/preferences' && method === 'PATCH') {
      const patch = req.postDataJSON() as Record<string, unknown>;
      recorded.preferencePatches.push(patch);
      prefs = { ...prefs, ...patch };
      return jsonReply(route, prefs);
    }
    if (path === '/api/providers' && method === 'GET') {
      return jsonReply(route, {
        cloudOptIn: false,
        preferredProvider: 'auto',
        providers: [
          { id: 'ollama', label: 'Ollama (Local)', isCloud: false, configured: true, available: true, keySource: 'none', defaultModel: 'llama3.2:3b', selectedModel: 'llama3.2:3b', freeModels: [], cooldownSecs: 0 },
          { id: 'freellmapi', label: 'FreeLLMAPI (Local Gateway)', isCloud: true, configured: false, available: false, keySource: 'none', defaultModel: 'auto:fast', selectedModel: 'auto:fast', freeModels: ['auto:fast', 'auto:balanced', 'auto:smart', 'auto'], cooldownSecs: 0 },
        ],
      });
    }
    if (path === '/api/scenarios') return jsonReply(route, SCENARIOS);
    if (path === '/api/learner/progress') return jsonReply(route, { totalSessions: 1, practiceMinutes: 5, completedGoalsCount: 1, vocabularyUsed: ['latte'], dayStreak: 1 });
    if (path === '/api/learner/facts') return jsonReply(route, []);
    if (path === '/api/sessions' && method === 'GET') return jsonReply(route, []);
    if (path === '/api/sessions' && method === 'POST') {
      sessSeq += 1;
      activeSessId = `e2e-sess-${sessSeq}`;
      seq = 0;
      const body = req.postDataJSON() as { correctionMode?: string };
      return jsonReply(route, {
        sessionId: activeSessId,
        state: 'Listening',
        correctionMode: body.correctionMode ?? prefs.correctionMode,
        pushToTalk: false,
        phase: 'Opener',
        turnCount: 0,
        maxTurns: 4,
        cefrLevel: 'A2',
        topic: 'Cafe Ordering',
        scenarioId: 'cafe_order',
        targetVocabulary: ['latte', 'iced', 'receipt'],
        goals: SCENARIOS[0].goals,
        completedGoalIds: [],
        openerReply: { spokenReply: 'Welcome to Sunrise Cafe! What can I get for you?' },
      });
    }
    if (path.endsWith('/turn') && method === 'POST') {
      const body = req.postDataJSON() as { userText: string; correctionMode: string };
      recorded.turnModes.push(body.correctionMode);
      seq += 1;
      const lower = body.userText.toLowerCase();
      let reply: Record<string, unknown> = { spokenReply: 'Got it, iced works great! Anything else today?' };
      if (lower.includes('give me')) {
        reply = {
          spokenReply: 'One medium latte coming right up! Hot or iced?',
          properWay: 'Could I get a medium latte, please?',
          why: "At a cafe counter, 'Could I get...' sounds warmer than 'Give me...'.",
          correction: "Could I get a medium latte, please? — At a cafe counter, 'Could I get...' sounds warmer than 'Give me...'.",
          followUpPrompt: 'Hot or iced?',
        };
        focuses.push(String(reply.correction));
      } else if (lower.includes('legacy')) {
        reply = { spokenReply: 'Nice work!', correction: 'Try past tense: "I went".' };
      }
      return jsonReply(route, {
        userTurn: { id: `u-${sessSeq}-${seq}`, sessionId: activeSessId, role: 'User', text: body.userText, timestampUnixMs: Date.now() },
        tutorTurn: { id: `t-${sessSeq}-${seq}`, sessionId: activeSessId, role: 'Tutor', text: reply.spokenReply, ...reply, timestampUnixMs: Date.now() + 1 },
        reply,
        provider: 'ollama',
        model: 'llama3.2:3b',
        isCloud: false,
        state: 'Listening',
        phase: seq === 1 ? 'FirstQuestion' : 'TopicExchange',
        turnCount: seq,
        maxTurns: 4,
        cefrLevel: 'A2',
        topic: 'Cafe Ordering',
      });
    }
    if (path.endsWith('/stop') && method === 'POST') {
      return jsonReply(route, {
        sessionId: activeSessId,
        state: 'Ended',
        correctionMode: prefs.correctionMode,
        pushToTalk: false,
        phase: 'Completed',
        sessionReview: currentReview(),
      });
    }
    if (path.endsWith('/review') && method === 'GET') return jsonReply(route, currentReview());
    return jsonReply(route, {});
  });
  return recorded;
}
