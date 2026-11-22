import { describe, expect, it, vi } from 'vitest';
import type { SpeakingSessionPort, VoiceLoopEvent } from '../services/tauriService';
import { createFakeSpeakingSessionPort } from '../test/voiceTestStubs';

type Cleanup = void | (() => void);
interface EffectSlot {
  deps?: unknown[];
  fn: () => Cleanup;
  cleanup?: Cleanup;
  dirty: boolean;
}

const hookState = vi.hoisted(() => ({
  stateIdx: 0,
  effectIdx: 0,
  states: [] as unknown[],
  effects: [] as EffectSlot[],
  mounted: false,
  scheduled: false,
  renderCurrent: () => undefined as unknown,
  reset() {
    this.stateIdx = 0;
    this.effectIdx = 0;
    this.states = [];
    this.effects = [];
    this.mounted = false;
    this.scheduled = false;
  },
}));

vi.mock('react', () => ({
  useState<T>(init: T | (() => T)) {
    const slot = hookState.stateIdx++;
    if (!(slot in hookState.states)) {
      hookState.states[slot] = typeof init === 'function' ? (init as () => T)() : init;
    }
    const set = (updater: T | ((prev: T) => T)) => {
      const prev = hookState.states[slot] as T;
      const next = typeof updater === 'function' ? (updater as (p: T) => T)(prev) : updater;
      if (Object.is(prev, next)) return;
      hookState.states[slot] = next;
      if (hookState.mounted && !hookState.scheduled) {
        hookState.scheduled = true;
        queueMicrotask(() => {
          hookState.scheduled = false;
          if (hookState.mounted) hookState.renderCurrent();
        });
      }
    };
    return [hookState.states[slot] as T, set] as const;
  },
  useEffect(fn: () => Cleanup, deps?: unknown[]) {
    const slot = hookState.effectIdx++;
    const prev = hookState.effects[slot];
    const changed = !prev || !deps || !prev.deps || deps.length !== prev.deps.length
      || deps.some((d, i) => !Object.is(d, prev.deps![i]));
    if (!prev) {
      hookState.effects[slot] = { deps, fn, dirty: true };
    } else {
      prev.deps = deps;
      prev.fn = fn;
      if (changed) prev.dirty = true;
    }
  },
}));

import { useSpeakingRoom, type SpeakingRoomHandle } from './useSpeakingRoom';

function mountSpeakingRoom(port: SpeakingSessionPort) {
  hookState.reset();
  hookState.mounted = true;
  let latest!: SpeakingRoomHandle;
  const run = () => {
    hookState.stateIdx = 0;
    hookState.effectIdx = 0;
    latest = useSpeakingRoom(port);
    for (const eff of hookState.effects) {
      if (eff.dirty) {
        eff.dirty = false;
        eff.cleanup?.();
        eff.cleanup = eff.fn();
      }
    }
  };
  hookState.renderCurrent = run;
  run();
  return {
    get current() { return latest; },
    unmount() {
      hookState.mounted = false;
      for (const eff of hookState.effects) eff.cleanup?.();
    },
  };
}

const flush = async () => {
  await Promise.resolve();
  await Promise.resolve();
};

describe('useSpeakingRoom progress, preferences, and in-stage SessionReview lifecycle', () => {
  it('loads LearnerProgress and Preferences on mount and refreshes progress/review on Ended without polling history mid-turn', async () => {
    const listeners = new Set<(ev: VoiceLoopEvent) => void>();
    const base = createFakeSpeakingSessionPort();
    const progressSpy = vi.fn(async () => ({
      totalSessions: 2,
      practiceMinutes: 11,
      completedGoalsCount: 4,
      vocabularyUsed: ['oat milk'],
      dayStreak: 2,
    }));
    const prefsSpy = vi.fn(async () => ({
      correctionMode: 'Gentle' as const,
      speakingSpeed: 1.15,
      pushToTalkEnabled: false,
      cefrLevel: 'B2' as const,
      englishVoiceId: 'en-GB',
    }));
    const patchSpy = vi.fn(async () => null);
    const port: SpeakingSessionPort = {
      ...base,
      getLearnerProgress: progressSpy,
      getPreferences: prefsSpy,
      patchPreferences: patchSpy,
      subscribeVoiceEvents: (h) => {
        listeners.add(h);
        return () => { listeners.delete(h); };
      },
    };
    const emit = (ev: VoiceLoopEvent) => { listeners.forEach((fn) => fn(ev)); };

    const hook = mountSpeakingRoom(port);
    await flush();
    expect(progressSpy).toHaveBeenCalledTimes(1);
    expect(prefsSpy).toHaveBeenCalledTimes(1);
    expect(hook.current.vm.speakingSpeed).toBe(1.15);
    expect(hook.current.vm.englishVoiceId).toBe('en-GB');
    expect(hook.current.learnerProgress.practiceMinutes).toBe(11);

    emit({
      type: 'session-state-changed',
      sessionId: 's-1',
      state: 'Listening',
      correctionMode: 'Gentle',
      openerReply: { spokenReply: 'Hello! Welcome to Sunrise Cafe.' },
    });
    await flush();
    expect(hook.current.vm.latestReply?.spokenReply).toContain('Sunrise Cafe');
    expect(progressSpy).toHaveBeenCalledTimes(1);

    for (let i = 1; i <= 2; i++) {
      emit({ type: 'session-state-changed', sessionId: 's-1', state: 'Thinking' });
      emit({ type: 'session-state-changed', sessionId: 's-1', state: 'Speaking' });
      emit({ type: 'session-state-changed', sessionId: 's-1', state: 'Listening' });
    }
    await flush();
    expect(progressSpy).toHaveBeenCalledTimes(1);

    emit({
      type: 'session-state-changed',
      sessionId: 's-1',
      state: 'Ended',
      sessionReview: {
        sessionId: 's-1',
        scenarioId: 'cafe_order',
        estimateLabel: 'AI Estimate',
        summary: 'Great cafe order. AI Estimate.',
        cefrSignal: 'A2',
        completedGoalIds: ['order_drink'],
        grammarFocuses: [],
        vocabularyUsed: ['oat milk'],
        turnCount: 2,
        learnerWordCount: 16,
        createdAtUnixMs: 1000,
      },
    });
    await flush();
    expect(progressSpy).toHaveBeenCalledTimes(2);
    expect(hook.current.vm.sessionReview?.estimateLabel).toBe('AI Estimate');

    hook.current.updateSpeakingSpeed(0.75);
    hook.current.updateEnglishVoice('en-AU');
    expect(patchSpy).toHaveBeenCalledWith({ speakingSpeed: 0.75 });
    expect(patchSpy).toHaveBeenCalledWith({ englishVoiceId: 'en-AU' });

    hook.current.returnToHub();
    await flush();
    expect(hook.current.vm.state).toBe('Idle');
    expect(hook.current.vm.sessionReview).toBeNull();
    hook.unmount();
  });
});
