import { describe, expect, it, vi } from 'vitest';
import type { LearnerPreferencesPatch, LearnerPreferencesView } from '../types/domain';
import type { SpeakingSessionPort, StartSpeakingSessionConfig } from '../services/tauriService';
import { createFakeSpeakingSessionPort } from '../test/voiceTestStubs';

const hookState = vi.hoisted(() => ({
  idx: 0,
  effIdx: 0,
  states: [] as unknown[],
  effects: [] as Array<{ fn: () => void | (() => void); cleanup?: void | (() => void); ran?: boolean }>,
  mounted: false,
  render: () => undefined as unknown,
  reset() {
    this.idx = 0; this.effIdx = 0; this.states = []; this.effects = []; this.mounted = false;
  },
}));

vi.mock('react', () => ({
  useState<T>(init: T | (() => T)) {
    const slot = hookState.idx++;
    if (!(slot in hookState.states)) hookState.states[slot] = typeof init === 'function' ? (init as () => T)() : init;
    const set = (up: T | ((p: T) => T)) => {
      const next = typeof up === 'function' ? (up as (p: T) => T)(hookState.states[slot] as T) : up;
      hookState.states[slot] = next;
      queueMicrotask(() => { if (hookState.mounted) hookState.render(); });
    };
    return [hookState.states[slot] as T, set] as const;
  },
  useEffect(fn: () => void | (() => void)) {
    const slot = hookState.effIdx++;
    if (!hookState.effects[slot]) hookState.effects[slot] = { fn, ran: false };
  },
}));

import { useSpeakingRoom, type SpeakingRoomHandle } from './useSpeakingRoom';

describe('useSpeakingRoom A1 CEFR persistence and pre-conversation script toggle', () => {
  it('keeps A1 CEFR across scenario switches, passes A1 on start(), and toggles script default false', async () => {
    hookState.reset();
    hookState.mounted = true;
    const started: StartSpeakingSessionConfig[] = [];
    const patches: LearnerPreferencesPatch[] = [];
    const prefs: LearnerPreferencesView = {
      correctionMode: 'Gentle', speakingSpeed: 1, pushToTalkEnabled: false,
      cefrLevel: 'B1', englishVoiceId: 'en-US', preConversationScriptEnabled: false,
    };
    const base = createFakeSpeakingSessionPort();
    const port: SpeakingSessionPort = {
      ...base,
      getPreferences: vi.fn(async () => prefs),
      patchPreferences: vi.fn(async (p) => { patches.push(p); return { ...prefs, ...p }; }),
      startSpeakingSession: vi.fn(async (cfg) => {
        started.push(cfg);
        return { sessionId: 's1', state: 'Listening' as const, correctionMode: cfg.correctionMode, pushToTalk: cfg.pushToTalk };
      }),
    };
    let room!: SpeakingRoomHandle;
    const render = () => {
      hookState.idx = 0; hookState.effIdx = 0;
      room = useSpeakingRoom(port);
      for (const e of hookState.effects) if (!e.ran) { e.ran = true; e.cleanup = e.fn(); }
    };
    hookState.render = render;
    render();
    await Promise.resolve();
    await Promise.resolve();

    expect(room.vm.preConversationScriptEnabled).toBe(false);
    room.updateCefrLevel('A1');
    await Promise.resolve();
    expect(room.vm.cefrLevel).toBe('A1');

    room.updateScenarioId('job_interview');
    await Promise.resolve();
    expect(room.vm.cefrLevel).toBe('A1');

    room.togglePreConversationScript();
    await Promise.resolve();
    expect(room.vm.preConversationScriptEnabled).toBe(true);
    expect(patches.at(-1)).toEqual({ preConversationScriptEnabled: true });

    room.start();
    expect(started.at(-1)?.cefrLevel).toBe('A1');
    expect(started.at(-1)?.scenarioId).toBe('job_interview');
    hookState.mounted = false;
  });
});
