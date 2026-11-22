import { afterEach, describe, expect, it, vi } from 'vitest';
import { createFakeSpeakingSessionPort } from '../test/voiceTestStubs';
import type { SessionState } from '../types/domain';

type Cleanup = void | (() => void);
interface EffectSlot { deps?: unknown[]; fn: () => Cleanup; cleanup?: Cleanup; dirty: boolean }

const hookState = vi.hoisted(() => ({
  sIdx: 0, rIdx: 0, eIdx: 0,
  states: [] as unknown[], refs: [] as Array<{ current: unknown }>, effects: [] as EffectSlot[],
  mounted: false, scheduled: false, renderCurrent: () => undefined as unknown,
  reset() {
    this.sIdx = 0; this.rIdx = 0; this.eIdx = 0;
    this.states = []; this.refs = []; this.effects = [];
    this.mounted = false; this.scheduled = false;
  },
}));

vi.mock('react', () => ({
  useState<T>(init: T | (() => T)) {
    const slot = hookState.sIdx++;
    if (!(slot in hookState.states)) hookState.states[slot] = typeof init === 'function' ? (init as () => T)() : init;
    const set = (updater: T | ((prev: T) => T)) => {
      const prev = hookState.states[slot] as T;
      const next = typeof updater === 'function' ? (updater as (p: T) => T)(prev) : updater;
      if (Object.is(prev, next)) return;
      hookState.states[slot] = next;
      if (hookState.mounted && !hookState.scheduled) {
        hookState.scheduled = true;
        queueMicrotask(() => { hookState.scheduled = false; if (hookState.mounted) hookState.renderCurrent(); });
      }
    };
    return [hookState.states[slot] as T, set] as const;
  },
  useRef<T>(init: T) {
    const slot = hookState.rIdx++;
    if (!(slot in hookState.refs)) hookState.refs[slot] = { current: init };
    return hookState.refs[slot] as { current: T };
  },
  useEffect(fn: () => Cleanup, deps?: unknown[]) {
    const slot = hookState.eIdx++;
    const prev = hookState.effects[slot];
    const changed = !prev || !deps || !prev.deps || deps.length !== prev.deps.length
      || deps.some((d, i) => !Object.is(d, prev.deps![i]));
    if (!prev) hookState.effects[slot] = { deps, fn, dirty: true };
    else { prev.deps = deps; prev.fn = fn; if (changed) prev.dirty = true; }
  },
}));

import { useBrowserVoiceSession, type BrowserVoiceStatus } from './useBrowserVoiceSession';

function mountVoiceHook(
  initial: { state: SessionState; pushToTalk: boolean; pttHeld: boolean },
  port: ReturnType<typeof createFakeSpeakingSessionPort>,
) {
  hookState.reset();
  hookState.mounted = true;
  let props = initial;
  let latest!: BrowserVoiceStatus;
  const run = () => {
    hookState.sIdx = 0; hookState.rIdx = 0; hookState.eIdx = 0;
    latest = useBrowserVoiceSession(props.state, props.pushToTalk, props.pttHeld, port);
    for (const eff of hookState.effects) {
      if (eff.dirty) { eff.dirty = false; eff.cleanup?.(); eff.cleanup = eff.fn(); }
    }
  };
  hookState.renderCurrent = run;
  run();
  return {
    get current() { return latest; },
    rerender(next: { state: SessionState; pushToTalk: boolean; pttHeld: boolean }) { props = next; run(); },
    unmount() { hookState.mounted = false; for (const eff of hookState.effects) eff.cleanup?.(); },
  };
}

function stubBrowserCapture() {
  let getUserMediaCalls = 0;
  let trackStopCalls = 0;
  let ctxCloseCalls = 0;
  let recStartCalls = 0;
  let recStopCalls = 0;
  let rafCallback: ((t: number) => void) | null = null;
  let activeRec: { onresult: ((ev: unknown) => void) | null } | null = null;

  vi.stubGlobal('navigator', {
    mediaDevices: {
      getUserMedia: vi.fn(async () => {
        getUserMediaCalls += 1;
        return { getTracks: () => [{ stop: () => { trackStopCalls += 1; } }] };
      }),
    },
  });
  vi.stubGlobal('AudioContext', class {
    createMediaStreamSource() { return { connect: () => undefined }; }
    createAnalyser() {
      return { fftSize: 256, frequencyBinCount: 4, getByteTimeDomainData: (arr: Uint8Array) => arr.fill(200) };
    }
    close() { ctxCloseCalls += 1; return Promise.resolve(); }
  });
  vi.stubGlobal('requestAnimationFrame', (cb: (t: number) => void) => { rafCallback = cb; return 1; });
  vi.stubGlobal('cancelAnimationFrame', () => { rafCallback = null; });
  vi.stubGlobal('SpeechRecognition', class {
    lang = ''; continuous = false; interimResults = false;
    onresult: ((ev: unknown) => void) | null = null;
    onerror: ((ev: unknown) => void) | null = null;
    onend: (() => void) | null = null;
    start() { recStartCalls += 1; activeRec = this; }
    stop() { recStopCalls += 1; }
  });

  return {
    counts: () => ({ getUserMediaCalls, trackStopCalls, ctxCloseCalls, recStartCalls, recStopCalls }),
    fireAudioTick: () => rafCallback?.(16),
    emitSpeech: (transcript: string, isFinal: boolean) => {
      activeRec?.onresult?.({ resultIndex: 0, results: [{ isFinal, 0: { transcript, confidence: 0.93 } }] });
    },
  };
}

const flush = async () => { await Promise.resolve(); await Promise.resolve(); };

afterEach(() => {
  vi.unstubAllGlobals();
  delete (globalThis as { SpeechRecognition?: unknown }).SpeechRecognition;
});

describe('useBrowserVoiceSession warm capture reuse and ref gating', () => {
  it('keeps mic and STT warm across Listening -> Thinking -> Speaking -> Listening and gates echo', async () => {
    const env = stubBrowserCapture();
    const port = createFakeSpeakingSessionPort();
    const submitSpy = vi.spyOn(port, 'submitUtteranceText');

    const hook = mountVoiceHook({ state: 'Listening', pushToTalk: false, pttHeld: false }, port);
    await flush();
    expect(env.counts()).toMatchObject({ getUserMediaCalls: 1, trackStopCalls: 0, recStartCalls: 1, recStopCalls: 0 });

    env.fireAudioTick();
    await flush();
    expect(hook.current.micLevel).toBeGreaterThan(0);

    hook.rerender({ state: 'Thinking', pushToTalk: false, pttHeld: false });
    await flush();
    expect(hook.current.micLevel).toBe(0);

    hook.rerender({ state: 'Speaking', pushToTalk: false, pttHeld: false });
    await flush();
    env.fireAudioTick();
    env.emitSpeech('Tutor echo should be ignored', true);
    await flush();
    expect(hook.current.micLevel).toBe(0);
    expect(submitSpy).not.toHaveBeenCalled();

    hook.rerender({ state: 'Listening', pushToTalk: false, pttHeld: false });
    await flush();
    expect(env.counts()).toMatchObject({ getUserMediaCalls: 1, trackStopCalls: 0, recStartCalls: 1, recStopCalls: 0 });

    env.emitSpeech('User second turn', true);
    await flush();
    expect(submitSpy).toHaveBeenCalledWith('User second turn', 0.93);

    hook.rerender({ state: 'Ended', pushToTalk: false, pttHeld: false });
    await flush();
    expect(env.counts()).toMatchObject({ trackStopCalls: 1, ctxCloseCalls: 1, recStopCalls: 1 });
    hook.unmount();
  });

  it('keeps capture warm in PTT and flushes pending interim on release in Listening', async () => {
    const env = stubBrowserCapture();
    const port = createFakeSpeakingSessionPort();
    const submitSpy = vi.spyOn(port, 'submitUtteranceText');

    const hook = mountVoiceHook({ state: 'Listening', pushToTalk: true, pttHeld: false }, port);
    await flush();
    expect(env.counts()).toMatchObject({ getUserMediaCalls: 1, recStartCalls: 1 });

    env.emitSpeech('Ignored while unheld', false);
    await flush();
    expect(hook.current.interimText).toBe('');

    hook.rerender({ state: 'Listening', pushToTalk: true, pttHeld: true });
    await flush();
    expect(env.counts()).toMatchObject({ getUserMediaCalls: 1, recStartCalls: 1, trackStopCalls: 0 });

    env.emitSpeech('Held PTT sentence', false);
    await flush();
    expect(hook.current.interimText).toBe('Held PTT sentence');

    hook.rerender({ state: 'Listening', pushToTalk: true, pttHeld: false });
    await flush();
    expect(submitSpy).toHaveBeenCalledWith('Held PTT sentence', 0.88);
    expect(hook.current.interimText).toBe('');
    hook.unmount();
  });
});
