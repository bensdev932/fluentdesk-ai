import { afterEach, describe, expect, it, vi } from 'vitest';
import { createFakeSpeakingSessionPort } from '../test/voiceTestStubs';
import type { CefrLevel, SessionState } from '../types/domain';
import { cefrHesitationMs, useBrowserVoiceSession, type BrowserVoiceStatus } from './useBrowserVoiceSession';

type Cleanup = void | (() => void);
interface EffectSlot { deps?: unknown[]; fn: () => Cleanup; cleanup?: Cleanup; dirty: boolean }
const hook = vi.hoisted(() => ({ s: 0, r: 0, e: 0, states: [] as unknown[], refs: [] as Array<{ current: unknown }>, effects: [] as EffectSlot[], mounted: false }));
vi.mock('react', () => ({
  useState<T>(init: T | (() => T)) {
    const i = hook.s++;
    if (!(i in hook.states)) hook.states[i] = typeof init === 'function' ? (init as () => T)() : init;
    return [hook.states[i] as T, (u: T | ((p: T) => T)) => { hook.states[i] = typeof u === 'function' ? (u as (p: T) => T)(hook.states[i] as T) : u; }] as const;
  },
  useRef<T>(init: T) { const i = hook.r++; if (!(i in hook.refs)) hook.refs[i] = { current: init }; return hook.refs[i] as { current: T }; },
  useEffect(fn: () => Cleanup, deps?: unknown[]) {
    const i = hook.e++;
    const p = hook.effects[i];
    if (!p) hook.effects[i] = { deps, fn, dirty: true };
    else if (!deps || !p.deps || deps.some((d, k) => !Object.is(d, p.deps![k]))) { p.deps = deps; p.fn = fn; p.dirty = true; }
  },
}));

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

describe('cefrHesitationMs browser hesitation merge grace window', () => {
  it('merges paused A2 speech chunks before submitting', async () => {
    vi.useFakeTimers();
    let rec: { onresult: ((ev: unknown) => void) | null } | null = null;
    vi.stubGlobal('navigator', { mediaDevices: { getUserMedia: vi.fn(async () => ({ getTracks: () => [{ stop: () => undefined }] })) } });
    vi.stubGlobal('SpeechRecognition', class { onresult = null; onerror = null; onend = null; start() { rec = this; } stop() { /* noop */ } });
    const port = createFakeSpeakingSessionPort();
    const submitSpy = vi.spyOn(port, 'submitUtteranceText');
    expect([cefrHesitationMs('A2'), cefrHesitationMs('B1'), cefrHesitationMs('C1')]).toEqual([900, 600, 400]);

    hook.s = 0; hook.r = 0; hook.e = 0; hook.states = []; hook.refs = []; hook.effects = [];
    const render = (state: SessionState, cefr: CefrLevel): BrowserVoiceStatus => {
      hook.s = 0; hook.r = 0; hook.e = 0;
      const out = useBrowserVoiceSession(state, false, false, port, cefr);
      hook.effects.forEach((ef) => { if (ef.dirty) { ef.dirty = false; ef.cleanup?.(); ef.cleanup = ef.fn(); } });
      return out;
    };
    render('Listening', 'A2');
    await Promise.resolve();
    (rec as { onresult: ((ev: unknown) => void) | null } | null)?.onresult?.({ resultIndex: 0, results: [{ isFinal: true, 0: { transcript: 'I would like', confidence: 0.93 } }] });
    await vi.advanceTimersByTimeAsync(500);
    expect(submitSpy).not.toHaveBeenCalled();
    (rec as { onresult: ((ev: unknown) => void) | null } | null)?.onresult?.({ resultIndex: 0, results: [{ isFinal: true, 0: { transcript: 'an iced coffee.', confidence: 0.93 } }] });
    await vi.advanceTimersByTimeAsync(900);
    expect(submitSpy).toHaveBeenCalledWith('I would like an iced coffee.', 0.93);
  });
});
