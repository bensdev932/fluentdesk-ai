import {
  sanitizeUntrustedText,
  type CefrLevel,
  type ConversationTurn,
  type CorrectionMode,
  type ScaffoldProgress,
  type SessionState,
} from '../src/types/domain';
import { advanceOfflineScaffold, buildOfflineSessionReview } from '../src/services/offlineSessionHelpers';
import { buildOfflineScenarioOpener, findFallbackScenario } from '../src/services/scenariosFallback';
import { generateTutorCoachingReply } from '../src/services/tutorCoach';
import { routeCloudTurn } from './aiProviders';
import { extractLearnerFact } from './tutorPrompt';
import type { BridgeClient, ByokKeys, StoredSession, StoredTurn, WorkerEnv } from './types';

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store' },
  });
}

function toConversationTurn(sessionId: string, t: StoredTurn): ConversationTurn {
  return {
    id: t.turnId, sessionId, role: t.role, text: t.text, timestampUnixMs: t.timestampUnixMs,
    ...(t.correction ? { correction: t.correction } : {}),
    ...(t.followUpPrompt ? { followUpPrompt: t.followUpPrompt } : {}),
  };
}

function toScaffold(s: StoredSession): ScaffoldProgress {
  const sc = findFallbackScenario(s.scenarioId);
  return {
    phase: s.phase, turnCount: s.turnCount, maxTurns: s.maxTurns, cefrLevel: s.cefrLevel,
    topic: s.topic, scenarioId: s.scenarioId,
    targetVocabulary: s.targetVocabulary.length > 0 ? s.targetVocabulary : sc.targetVocabulary,
    goals: sc.goals, completedGoalIds: s.completedGoalIds,
  };
}

function splitSentences(text: string): string[] {
  const parts = text.match(/[^.!?]+[.!?]+(?:\s+|$)|[^.!?]+$/g);
  return (parts ?? [text]).map((s) => s.trim()).filter(Boolean);
}

function buildSseResponse(spokenReply: string, payload: unknown): Response {
  const encoder = new TextEncoder();
  const sentences = splitSentences(spokenReply);
  const body = new ReadableStream<Uint8Array>({
    start(ctrl) {
      for (const s of sentences) {
        ctrl.enqueue(encoder.encode(`event: sentence\ndata: ${JSON.stringify({ text: s })}\n\n`));
      }
      ctrl.enqueue(encoder.encode(`event: done\ndata: ${JSON.stringify(payload)}\n\n`));
      ctrl.close();
    },
  });
  return new Response(body, {
    status: 200,
    headers: { 'Content-Type': 'text/event-stream; charset=utf-8', 'Cache-Control': 'no-store' },
  });
}

export async function handleStartSession(req: Request, bridge: BridgeClient, learnerId: string): Promise<Response> {
  const body = (await req.json().catch(() => ({}))) as {
    correctionMode?: CorrectionMode; pushToTalk?: boolean; cefrLevel?: CefrLevel;
    scenarioId?: string; topic?: string; targetVocabulary?: string[]; maxTurns?: number;
  };
  const settings = await bridge.getSettings(learnerId);
  const sc = findFallbackScenario(body.scenarioId);
  const sessionId = crypto.randomUUID();
  const startedAtUnixMs = Date.now();
  const correctionMode = body.correctionMode ?? settings.correctionMode;
  const pushToTalk = body.pushToTalk ?? settings.pushToTalkEnabled;
  const cefrLevel = body.cefrLevel ?? (!body.scenarioId ? settings.cefrLevel : sc.defaultCefr);
  const topic = sanitizeUntrustedText(body.topic ?? '', 120) || sc.title;
  const maxTurns = Math.min(20, Math.max(2, Number(body.maxTurns) || sc.defaultMaxTurns));
  const targetVocabulary = Array.isArray(body.targetVocabulary) && body.targetVocabulary.length > 0
    ? body.targetVocabulary.map((w) => sanitizeUntrustedText(String(w), 40)).filter(Boolean).slice(0, 12)
    : sc.targetVocabulary;
  const openerReply = buildOfflineScenarioOpener(sc.id, topic);

  await bridge.createSession(learnerId, {
    sessionId, correctionMode, pushToTalk, scenarioId: sc.id, topic, cefrLevel,
    maxTurns, targetVocabulary, startedAtUnixMs,
    openerTurn: {
      turnId: `turn-op-${startedAtUnixMs}`, role: 'Tutor', text: openerReply.spokenReply,
      ...(openerReply.followUpPrompt ? { followUpPrompt: openerReply.followUpPrompt } : {}),
      timestampUnixMs: startedAtUnixMs,
    },
  });

  return json(200, {
    sessionId, state: 'Listening', correctionMode, pushToTalk, phase: 'Opener', turnCount: 0,
    maxTurns, cefrLevel, topic, scenarioId: sc.id, targetVocabulary, goals: sc.goals,
    completedGoalIds: [], openerReply,
  });
}

export async function handleSessionTurn(args: {
  req: Request; sessionId: string; streamSse: boolean; learnerId: string;
  ipHash?: string; byok: ByokKeys; env: WorkerEnv; bridge: BridgeClient; fetchImpl?: typeof fetch;
}): Promise<Response> {
  const { req, sessionId, streamSse, learnerId, ipHash, byok, env, bridge, fetchImpl } = args;
  const body = (await req.json().catch(() => null)) as { userText?: unknown; correctionMode?: CorrectionMode } | null;
  if (!body || typeof body.userText !== 'string') return json(400, { error: 'Invalid JSON request body.' });
  const userText = sanitizeUntrustedText(body.userText, 1000);
  if (!userText) return json(400, { error: 'userText must not be empty.' });

  const record = await bridge.getSession(learnerId, sessionId);
  if (!record) return json(404, { error: 'Session not found.' });
  if (record.session.state !== 'Listening') {
    return json(409, { error: `Session cannot accept turns from state ${record.session.state}` });
  }

  const [settings, facts] = await Promise.all([bridge.getSettings(learnerId), bridge.listFacts(learnerId)]);
  const mode = body.correctionMode ?? record.session.correctionMode;
  const scaffold = toScaffold(record.session);
  const history = record.turns.map((t) => toConversationTurn(sessionId, t));
  const limitKeys = [`learner:${learnerId}`, ...(ipHash ? [`ip:${ipHash}`] : [])];
  const rate = await bridge.consumeRateLimit(learnerId, limitKeys);

  const routed = rate.allowed
    ? await routeCloudTurn({ env, settings, byok, history, userText, mode, scaffold, facts: facts.map((f) => f.factText), fetchImpl })
    : { reply: generateTutorCoachingReply(userText, mode, history), completedGoalIds: [], isCloud: false, failoverNote: rate.reason };

  const nextScaffold = advanceOfflineScaffold(scaffold, userText);
  const validGoalIds = new Set((scaffold.goals ?? []).map((g) => g.id));
  const mergedGoals = [...new Set([...(nextScaffold.completedGoalIds ?? []), ...routed.completedGoalIds.filter((id) => validGoalIds.has(id))])];
  nextScaffold.completedGoalIds = mergedGoals;

  const now = Date.now();
  const ended = nextScaffold.phase === 'Completed';
  const finalState: SessionState = ended ? 'Ended' : 'Listening';
  const userTurn: ConversationTurn = { id: `u-${now}`, sessionId, role: 'User', text: userText, timestampUnixMs: now };
  const tutorTurn: ConversationTurn = {
    id: `t-${now + 1}`, sessionId, role: 'Tutor', text: routed.reply.spokenReply, timestampUnixMs: now + 1,
    ...(routed.reply.correction ? { correction: routed.reply.correction } : {}),
    ...(routed.reply.followUpPrompt ? { followUpPrompt: routed.reply.followUpPrompt } : {}),
  };
  const sessionReview = ended ? buildOfflineSessionReview(sessionId, nextScaffold, [...history, userTurn, tutorTurn]) : undefined;
  const factText = extractLearnerFact(userText);

  await bridge.recordTurn(learnerId, {
    sessionId,
    userTurn: { turnId: userTurn.id, role: 'User', text: userTurn.text, timestampUnixMs: userTurn.timestampUnixMs },
    tutorTurn: {
      turnId: tutorTurn.id, role: 'Tutor', text: tutorTurn.text, timestampUnixMs: tutorTurn.timestampUnixMs,
      ...(tutorTurn.correction ? { correction: tutorTurn.correction } : {}),
      ...(tutorTurn.followUpPrompt ? { followUpPrompt: tutorTurn.followUpPrompt } : {}),
    },
    correctionMode: mode, phase: nextScaffold.phase, turnCount: nextScaffold.turnCount,
    completedGoalIds: mergedGoals, state: finalState,
    ...(ended ? { endedAtUnixMs: now } : {}),
    ...(sessionReview ? { sessionReview } : {}),
    ...(factText ? { extractedFact: { factId: `fact-${crypto.randomUUID().slice(0, 8)}`, factText, createdAtUnixMs: now } } : {}),
  });

  const payload = {
    userTurn, tutorTurn, reply: routed.reply, isCloud: routed.isCloud, state: finalState, ...nextScaffold,
    ...(routed.provider ? { provider: routed.provider } : {}),
    ...(routed.model ? { model: routed.model } : {}),
    ...(routed.failoverNote ? { failoverNote: routed.failoverNote } : {}),
    ...(sessionReview ? { sessionReview } : {}),
  };
  return streamSse ? buildSseResponse(routed.reply.spokenReply, payload) : json(200, payload);
}

export async function handleSessionLifecycle(
  sessionId: string,
  op: 'pause' | 'resume' | 'stop',
  bridge: BridgeClient,
  learnerId: string,
): Promise<Response> {
  const record = await bridge.getSession(learnerId, sessionId);
  if (!record) return json(404, { error: 'Session not found.' });
  const scaffold = toScaffold(record.session);
  const turns = record.turns.map((t) => toConversationTurn(sessionId, t));
  const now = Date.now();
  const sessionReview = op === 'stop' ? buildOfflineSessionReview(sessionId, scaffold, turns) : undefined;
  const res = await bridge.updateSessionState(learnerId, {
    sessionId, op, ...(op === 'stop' ? { endedAtUnixMs: now, sessionReview } : {}),
  });
  if (!res.ok) return json(res.status, { error: res.error });
  return json(200, {
    sessionId, state: res.state ?? record.session.state, correctionMode: record.session.correctionMode,
    pushToTalk: record.session.pushToTalk, ...scaffold, ...(sessionReview ? { sessionReview } : {}),
  });
}

export async function handleGetSessionDetails(
  sessionId: string,
  mode: 'history' | 'status' | 'review',
  bridge: BridgeClient,
  learnerId: string,
): Promise<Response> {
  const record = await bridge.getSession(learnerId, sessionId);
  if (!record) return json(404, { error: 'Session not found.' });
  if (mode === 'review') return record.review ? json(200, record.review) : json(404, { error: 'Session review not found.' });
  if (mode === 'status') {
    return json(200, {
      sessionId, state: record.session.state, correctionMode: record.session.correctionMode,
      pushToTalk: record.session.pushToTalk, ...toScaffold(record.session),
      ...(record.review ? { sessionReview: record.review } : {}),
    });
  }
  return json(200, {
    sessionId, startedAtUnixMs: record.session.startedAtUnixMs,
    ...(record.session.endedAtUnixMs !== undefined ? { endedAtUnixMs: record.session.endedAtUnixMs } : {}),
    correctionMode: record.session.correctionMode, turns: record.turns.map((t) => toConversationTurn(sessionId, t)),
  });
}
