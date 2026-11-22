import { handleApiRequest } from './apiRouter';
import { createConvexBridgeClient } from './convexClient';
import type { WorkerEnv } from './types';

export default {
  async fetch(request: Request, env: WorkerEnv): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname.startsWith('/api/')) {
      try {
        return await handleApiRequest(request, env, createConvexBridgeClient(env));
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Internal server error.';
        return new Response(JSON.stringify({ error: message }), {
          status: 500,
          headers: { 'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store' },
        });
      }
    }
    if (!env.ASSETS) return new Response('Static assets binding missing.', { status: 500 });
    return env.ASSETS.fetch(request);
  },
};
