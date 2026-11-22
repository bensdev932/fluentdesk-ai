# ADR 0002: Cloudflare Worker + Convex Cloud Backend for Web Deployment

- **Status**: Superseded (Desktop-First — 2026-11-22)
- **Date**: 2026-11-21

## Supersession Note
Public web deployment (`workers.dev` and `.github/workflows/deploy.yml`) was disabled on 2026-11-22 in favor of publishing FluentDesk AI as an offline-first Tauri v2 desktop application (`v0.1.0`). The live `fluentdesk-ai` Worker was torn down and `workersDev` set to `false` in `cloudflare.config.ts`. The locked-down Convex deployment (`nombai-badal`, reachable only via `X-Convex-Bridge-Secret`) and the `worker/` + `convex/` source code remain archived in git without public callers.

## Context
FluentDesk AI originally targeted local-first Tauri + SQLite (`http://127.0.0.1:3420`), causing the Cloudflare Workers static deployment to return HTTP 404 on `/api/*` and disable cloud AI providers on the web. We want a free-tier, zero-signup web deployment backed by Convex (`nombai-badal`) and free cloud AI models while preserving the offline-first SQLite desktop app and the strict 100% pure-English zero-raw-audio contract.

## Decision
1. **Unified `/api/*` Worker Gateway**: Deploy [worker/index.ts](../../worker/index.ts) via [cloudflare.config.ts](../../cloudflare.config.ts) with Workers Static Assets (`notFoundHandling: "single-page-application"`, `runWorkerFirst: ["/api/*"]`) serving both the React SPA and `/api/*`.
2. **Free Cloud AI Stack**: Bind Cloudflare Workers AI (`env.AI`) as the built-in zero-key default alongside Groq, Gemini, and OpenRouter. Browser-entered BYOK keys remain strictly in browser `sessionStorage` (`fluentdesk.byok.v1`) sent per request via `X-Byok-*` headers and are never written to Convex.
3. **Locked-Down Convex Persistence**: Model `users`, `session_settings`, `conversations`, `conversation_turns`, `session_reviews`, `learner_facts`, and `rate_limits` in `convex/` as `internalQuery` / `internalMutation` functions reached solely through `POST /bridge` in `convex/http.ts` authenticated with `X-Convex-Bridge-Secret`.
4. **Anonymous Per-Learner Isolation & Quota Guard**: Scope web learner state by browser-generated `X-Learner-Id` (`fluentdesk.learnerId.v1`) and enforce 20 turns/min and 120 turns/day per learner + hashed IP with graceful fallback to the offline coach.
5. **Dual-Runtime Preservation**: Keep `src-tauri/` SQLite + Ollama intact for desktop; `ci.yml` continues gating Rust and frontend checks while `deploy.yml` automates verification + Convex + Cloudflare deployment on `main`.
