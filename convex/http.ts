import { httpRouter } from 'convex/server';
import { httpAction } from './_generated/server';
import { internal } from './_generated/api';

const http = httpRouter();
const LEARNER_ID_RE = /^[A-Za-z0-9_-]{8,80}$/;

function constantTimeEqual(a: string, b: string): boolean {
  if (!a || !b || a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i += 1) {
    diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  }
  return diff === 0;
}

function jsonResp(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json; charset=utf-8' },
  });
}

http.route({
  path: '/bridge',
  method: 'POST',
  handler: httpAction(async (ctx, request) => {
    const expected = process.env.CONVEX_BRIDGE_SECRET ?? '';
    const provided = request.headers.get('X-Convex-Bridge-Secret') ?? '';
    if (!constantTimeEqual(expected, provided)) {
      return jsonResp(401, { error: 'Unauthorized bridge request.' });
    }
    let body: { action?: string; learnerId?: string; payload?: Record<string, unknown> };
    try {
      body = (await request.json()) as typeof body;
    } catch {
      return jsonResp(400, { error: 'Invalid JSON payload.' });
    }
    const learnerId = typeof body.learnerId === 'string' ? body.learnerId.trim() : '';
    if (!LEARNER_ID_RE.test(learnerId)) {
      return jsonResp(400, { error: 'Invalid learnerId.' });
    }
    const payload = (body.payload ?? {}) as Record<string, unknown>;
    switch (body.action) {
      case 'preferences.get':
        return jsonResp(200, await ctx.runQuery(internal.preferences.getSettings, { learnerId }));
      case 'preferences.patch':
        return jsonResp(200, await ctx.runMutation(internal.preferences.patchSettings, { learnerId, ...(payload as never) }));
      case 'sessions.create':
        return jsonResp(200, await ctx.runMutation(internal.sessions.createSession, { learnerId, ...(payload as never) }));
      case 'sessions.get':
        return jsonResp(200, await ctx.runQuery(internal.sessions.getSession, { learnerId, ...(payload as never) }));
      case 'sessions.list':
        return jsonResp(200, await ctx.runQuery(internal.sessions.listSessions, { learnerId }));
      case 'sessions.recordTurn':
        return jsonResp(200, await ctx.runMutation(internal.sessions.recordTurn, { learnerId, ...(payload as never) }));
      case 'sessions.updateState':
        return jsonResp(200, await ctx.runMutation(internal.sessions.updateSessionState, { learnerId, ...(payload as never) }));
      case 'learner.getReview':
        return jsonResp(200, await ctx.runQuery(internal.learner.getReview, { learnerId, ...(payload as never) }));
      case 'learner.listFacts':
        return jsonResp(200, await ctx.runQuery(internal.learner.listFacts, { learnerId }));
      case 'learner.deleteFact':
        return jsonResp(200, await ctx.runMutation(internal.learner.deleteFact, { learnerId, ...(payload as never) }));
      case 'learner.clearFacts':
        return jsonResp(200, await ctx.runMutation(internal.learner.clearFacts, { learnerId }));
      case 'learner.getProgress':
        return jsonResp(200, await ctx.runQuery(internal.learner.getProgress, { learnerId, ...(payload as never) }));
      case 'learner.consumeRateLimit':
        return jsonResp(200, await ctx.runMutation(internal.learner.consumeRateLimit, payload as never));
      default:
        return jsonResp(400, { error: 'Unknown bridge action.' });
    }
  }),
});

export default http;
