import { describe, expect, it, vi } from 'vitest';
import { handleApiRequest } from './apiRouter';
import { createInMemoryBridgeClient } from './memoryBridge';
import type { WorkerEnv } from './types';

function req(path: string, opts: { method?: string; body?: unknown; learnerId?: string; headers?: Record<string, string> } = {}): Request {
  const headers: Record<string, string> = { ...(opts.learnerId ? { 'X-Learner-Id': opts.learnerId } : {}), ...opts.headers };
  if (opts.body !== undefined) headers['Content-Type'] = 'application/json';
  return new Request(`https://fluentdesk.test${path}`, {
    method: opts.method ?? 'GET',
    headers,
    ...(opts.body !== undefined ? { body: JSON.stringify(opts.body) } : {}),
  });
}

describe('Cloudflare Worker /api gateway', () => {
  it('serves 8 scenarios, Workers AI snapshot with cloudOptIn=true, and keeps BYOK out of storage', async () => {
    const bridge = createInMemoryBridgeClient();
    const patchSpy = vi.spyOn(bridge, 'patchSettings');
    const env: WorkerEnv = { AI: { run: async () => ({ response: '{}' }) } };

    const scenRes = await handleApiRequest(req('/api/scenarios'), env, bridge);
    expect(scenRes.status).toBe(200);
    expect((await scenRes.json()) as unknown[]).toHaveLength(8);

    const provRes = await handleApiRequest(req('/api/providers', { learnerId: 'learner-alpha-1', headers: { 'X-Byok-Groq': 'gsk_secret' } }), env, bridge);
    const snap = (await provRes.json()) as { cloudOptIn: boolean; providers: Array<{ id: string; configured: boolean; keySource: string }> };
    expect(snap.cloudOptIn).toBe(true);
    expect(snap.providers.find((p) => p.id === 'workers_ai')).toMatchObject({ configured: true, keySource: 'env' });
    expect(snap.providers.find((p) => p.id === 'groq')).toMatchObject({ configured: true, keySource: 'runtime' });

    await handleApiRequest(
      req('/api/providers/config', { method: 'POST', learnerId: 'learner-alpha-1', body: { preferredProvider: 'groq', apiKeys: { groq: 'gsk_do_not_store' } } }),
      env,
      bridge,
    );
    const patchArg = patchSpy.mock.calls[0]?.[1] as Record<string, unknown>;
    expect(patchArg).not.toHaveProperty('apiKeys');
  });

  it('runs a full session lifecycle with Workers AI, SSE streaming, learner isolation, and review', async () => {
    const bridge = createInMemoryBridgeClient();
    const env: WorkerEnv = {
      AI: {
        run: async () => ({
          response: JSON.stringify({
            spokenReply: 'Nice to meet a nurse! What shifts do you usually work?',
            correction: null,
            followUpPrompt: 'What shifts do you usually work?',
            completedGoalIds: ['share_update'],
          }),
        }),
      },
    };

    const startRes = await handleApiRequest(
      req('/api/sessions', { method: 'POST', learnerId: 'learner-alpha-1', body: { scenarioId: 'free_conversation', maxTurns: 4 } }),
      env,
      bridge,
    );
    expect(startRes.status).toBe(200);
    const started = (await startRes.json()) as { sessionId: string; openerReply: { spokenReply: string } };
    expect(started.openerReply.spokenReply.length).toBeGreaterThan(10);

    const turnRes = await handleApiRequest(
      req(`/api/sessions/${started.sessionId}/turn`, {
        method: 'POST',
        learnerId: 'learner-alpha-1',
        headers: { Accept: 'text/event-stream' },
        body: { userText: 'I work as a nurse in a clinic today.' },
      }),
      env,
      bridge,
    );
    expect(turnRes.headers.get('content-type')).toContain('text/event-stream');
    const sseBody = await turnRes.text();
    expect(sseBody).toContain('event: sentence');
    expect(sseBody).toContain('event: done');
    expect(sseBody).toContain('"provider":"workers_ai"');

    // Second visitor cannot access alpha's session or facts
    const isoFacts = (await (await handleApiRequest(req('/api/learner/facts', { learnerId: 'learner-beta-2' }), env, bridge)).json()) as unknown[];
    expect(isoFacts).toHaveLength(0);
    const alphaFacts = (await (await handleApiRequest(req('/api/learner/facts', { learnerId: 'learner-alpha-1' }), env, bridge)).json()) as Array<{ factText: string }>;
    expect(alphaFacts[0]?.factText).toContain('I work as a nurse');

    const stopRes = await handleApiRequest(
      req(`/api/sessions/${started.sessionId}/stop`, { method: 'POST', learnerId: 'learner-alpha-1' }),
      env,
      bridge,
    );
    const stopped = (await stopRes.json()) as { state: string; sessionReview: { estimateLabel: string } };
    expect(stopped.state).toBe('Ended');
    expect(stopped.sessionReview.estimateLabel).toBe('AI Estimate');

    const prog = (await (await handleApiRequest(req('/api/learner/progress', { learnerId: 'learner-alpha-1' }), env, bridge)).json()) as { totalSessions: number };
    expect(prog.totalSessions).toBe(1);
  });

  it('fails over to BYOK Groq, redacts leaked secrets, and enforces rate limit with offline fallback', async () => {
    const bridge = createInMemoryBridgeClient({ perMin: 1, perDay: 10 });
    const env: WorkerEnv = {
      AI: { run: async () => { throw new Error('Workers AI quota busy'); } },
    };
    const fetchMock = vi.fn(async () => new Response(JSON.stringify({
      choices: [{ message: { content: '{"spokenReply":"Great! I ordered an iced latte too.","correction":null,"followUpPrompt":"Want any pastry?"}' } }],
    }), { status: 200 }));

    const startRes = await handleApiRequest(
      req('/api/sessions', { method: 'POST', learnerId: 'learner-failover-1', body: { scenarioId: 'cafe_order' } }),
      env,
      bridge,
    );
    const { sessionId } = (await startRes.json()) as { sessionId: string };

    const firstTurn = await handleApiRequest(
      req(`/api/sessions/${sessionId}/turn`, {
        method: 'POST',
        learnerId: 'learner-failover-1',
        headers: { 'X-Byok-Groq': 'gsk_live_secret_token_999' },
        body: { userText: 'Can I get an iced latte please?' },
      }),
      env,
      bridge,
      fetchMock as unknown as typeof fetch,
    );
    const t1 = (await firstTurn.json()) as { provider: string; isCloud: boolean; failoverNote?: string };
    expect(t1.provider).toBe('groq');
    expect(t1.isCloud).toBe(true);
    expect(t1.failoverNote).toContain('Recovered via groq');
    expect(t1.failoverNote).not.toContain('gsk_live_secret_token_999');

    const secondTurn = await handleApiRequest(
      req(`/api/sessions/${sessionId}/turn`, { method: 'POST', learnerId: 'learner-failover-1', body: { userText: 'For here please.' } }),
      env,
      bridge,
      fetchMock as unknown as typeof fetch,
    );
    const t2 = (await secondTurn.json()) as { isCloud: boolean; failoverNote?: string; reply: { spokenReply: string } };
    expect(t2.isCloud).toBe(false);
    expect(t2.failoverNote).toContain('turn cap reached');
    expect(t2.reply.spokenReply.length).toBeGreaterThan(5);
  });
});
