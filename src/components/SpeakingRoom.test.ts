import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { App } from '../App';
import { startBrowserSpeechRecognition } from '../services/browserVoice';
import { generateTutorCoachingReply } from '../services/tutorCoach';
import { createFakeSpeakingSessionPort, stubSpeechRecognition } from '../test/voiceTestStubs';
import {
  createInitialSpeakingRoomState,
  getSpeakingControlPermissions,
  reduceVoiceLoopEvent,
  type SpeakingRoomViewModel,
} from './speakingRoomController';
import { TutorReplyCard } from './TutorReplyCard';

afterEach(() => {
  vi.unstubAllGlobals();
  delete (globalThis as { SpeechRecognition?: unknown }).SpeechRecognition;
});

describe('SpeakingRoom state controller and single SpeakingSessionPort wiring', () => {
  it('defaults CorrectionMode to Natural and enables Start/Stop across canonical states', () => {
    const idleVm = createInitialSpeakingRoomState();
    expect(idleVm.correctionMode).toBe('Natural');
    const idlePerms = getSpeakingControlPermissions(idleVm);
    expect(idlePerms).toMatchObject({ canStart: true, canStop: false, canHoldPtt: false });

    const listenPerms = getSpeakingControlPermissions({ ...idleVm, state: 'Listening', pushToTalk: true });
    expect(listenPerms).toMatchObject({ canStart: false, canPause: true, canStop: true, canHoldPtt: true });

    const pausedPerms = getSpeakingControlPermissions({ ...idleVm, state: 'Paused' });
    expect(pausedPerms).toMatchObject({ canResume: true, canStop: true });
  });

  it('reduces user transcript, tutor reply, and degraded playback warnings in order', () => {
    let vm: SpeakingRoomViewModel = { ...createInitialSpeakingRoomState(), sessionId: 'sess-1', state: 'Listening' };
    vm = reduceVoiceLoopEvent(vm, {
      type: 'user-transcript-ready',
      sessionId: 'sess-1',
      confidence: 0.93,
      turn: { id: 't-1', sessionId: 'sess-1', role: 'User', text: 'Yesterday I go to the beach.', timestampUnixMs: 1000 },
    });
    vm = reduceVoiceLoopEvent(vm, {
      type: 'tutor-reply-ready',
      sessionId: 'sess-1',
      turn: { id: 't-2', sessionId: 'sess-1', role: 'Tutor', text: 'Nice! You went to the beach.', timestampUnixMs: 2000 },
      reply: { spokenReply: 'Nice! You went to the beach.', correction: 'Say "I went" instead of "I go".', followUpPrompt: 'Was the water warm?' },
      playbackWarning: { kind: 'TtsUnavailable', userMessage: 'Voice synthesis unavailable; showing text reply.', recoverable: true },
    });

    expect(vm.turns).toHaveLength(2);
    expect(vm.latestReply?.correction).toBe('Say "I went" instead of "I go".');
    expect(vm.playbackWarning).toContain('Voice synthesis unavailable');
  });

  it('renders App and SpeakingRoom with a single SpeakingSessionPort prop', () => {
    const browserHtml = renderToStaticMarkup(createElement(App, { sessionClient: createFakeSpeakingSessionPort({ hasVoiceCaptureUi: true }) }));
    expect(browserHtml).toContain('FluentDesk AI');
    expect(browserHtml).toContain('Browser Voice Input');

    const headlessHtml = renderToStaticMarkup(createElement(App, { sessionClient: createFakeSpeakingSessionPort({ hasVoiceCaptureUi: false }) }));
    expect(headlessHtml).not.toContain('Browser Voice Input');
  });

  it('drives start, pause, resume, stop, cancellation, and session reset on unified port', async () => {
    let releaseTurn: () => void = () => undefined;
    const port = createFakeSpeakingSessionPort({
      turnGate: () => new Promise<void>((resolve) => { releaseTurn = resolve; }),
    });
    const events: string[] = [];
    const unsubscribe = port.subscribeVoiceEvents((ev) => events.push(ev.type));

    const s1 = await port.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    expect(s1.state).toBe('Listening');
    expect((await port.pauseSpeakingSession(s1.sessionId)).state).toBe('Paused');
    expect((await port.resumeSpeakingSession(s1.sessionId)).state).toBe('Listening');

    const pending = port.submitUtteranceText('In-flight utterance');
    await port.stopSpeakingSession(s1.sessionId);
    releaseTurn();
    await pending;
    expect((await port.getSessionStatus(s1.sessionId)).state).toBe('Ended');
    expect((await port.getSessionHistory(s1.sessionId)).turns).toHaveLength(0);

    const s2 = await port.startSpeakingSession({ correctionMode: 'Gentle', pushToTalk: false });
    expect(s2.sessionId).not.toBe(s1.sessionId);
    expect((await port.getSessionHistory(s2.sessionId)).turns).toHaveLength(0);
    expect(events.length).toBeGreaterThanOrEqual(4);
    unsubscribe();

    const vmWithTurns = reduceVoiceLoopEvent(createInitialSpeakingRoomState(), {
      type: 'user-transcript-ready',
      sessionId: s1.sessionId,
      turn: { id: 'u1', sessionId: s1.sessionId, role: 'User', text: 'old', timestampUnixMs: 1 },
    });
    const resetVm = reduceVoiceLoopEvent({ ...vmWithTurns, sessionId: s1.sessionId }, {
      type: 'session-state-changed',
      sessionId: s2.sessionId,
      state: 'Listening',
    });
    expect(resetVm.turns).toHaveLength(0);
  });

  it('forwards Web Speech transcripts and sample turns through SpeakingSessionPort', async () => {
    const getRec = stubSpeechRecognition();
    const port = createFakeSpeakingSessionPort();
    const started = await port.startSpeakingSession({ correctionMode: 'Gentle', pushToTalk: false });
    let pending = Promise.resolve();
    const stopStt = startBrowserSpeechRecognition({
      onInterim: () => undefined,
      onFinal: (t, c) => { pending = port.submitUtteranceText(t, c); },
      onStatus: () => undefined,
    });
    getRec()?.onresult?.({ resultIndex: 0, results: [{ isFinal: true, 0: { transcript: 'I adopted Mochi yesterday.', confidence: 0.94 } }] });
    await pending;
    stopStt();
    await port.simulateSampleTurn();

    const history = await port.getSessionHistory(started.sessionId);
    expect(history.turns.filter((t) => t.role === 'User').map((t) => t.text)).toEqual([
      'I adopted Mochi yesterday.',
      'Yesterday I go to the park.',
    ]);
    expect(history.turns[1].correction).toBe('Use simple past tense.');
    const offline = generateTutorCoachingReply('My bicycle tire punctured near the harbor.', 'Natural');
    expect(`${offline.spokenReply} ${offline.followUpPrompt ?? ''}`).not.toMatch(/Thanks for sharing that naturally/i);
  });

  it('omits duplicate Next prompt and renders distinct contextual follow-up', () => {
    const dup = renderToStaticMarkup(createElement(TutorReplyCard, { reply: { spokenReply: 'Where did you hike?', followUpPrompt: 'Where did you hike?' }, playbackWarning: null }));
    expect(dup).not.toContain('Next prompt:');
    const distinct = renderToStaticMarkup(createElement(TutorReplyCard, { reply: { spokenReply: 'Mount Rainier has steep trails!', followUpPrompt: 'Which route did you pick?' }, playbackWarning: null }));
    expect(distinct).toContain('Next prompt:');
    expect(distinct).toContain('Which route did you pick?');
  });

  it('tracks scaffold phase, turn counter, CEFR level, and topic cleanly in UI state', () => {
    const html = renderToStaticMarkup(createElement(App, { sessionClient: createFakeSpeakingSessionPort({ hasVoiceCaptureUi: true }) }));
    expect(html).toContain('Opener');
    expect(html).toContain('Turn 0/6');
    expect(html).toContain('CEFR Level');
    const updated = reduceVoiceLoopEvent(createInitialSpeakingRoomState(), {
      type: 'session-state-changed',
      sessionId: 's-9',
      state: 'Ended',
      scaffold: { phase: 'Completed', turnCount: 6, maxTurns: 6, cefrLevel: 'B2', topic: 'Travel' },
    });
    expect(updated).toMatchObject({ state: 'Ended', phase: 'Completed', turnCount: 6, maxTurns: 6, cefrLevel: 'B2', topic: 'Travel' });
  });

  it('surfaces streaming tutor sentences progressively and replaces cleanly on tutor-reply-ready without duplicates', () => {
    let vm: SpeakingRoomViewModel = { ...createInitialSpeakingRoomState(), sessionId: 's-1', state: 'Speaking' };
    vm = reduceVoiceLoopEvent(vm, { type: 'tutor-reply-streaming', sessionId: 's-1', turnId: 'stream-s-1-1', partialText: 'First sentence!' });
    expect(vm.latestReply?.spokenReply).toBe('First sentence!');
    expect(vm.turns).toHaveLength(1);
    expect(vm.turns[0]).toMatchObject({ id: 'stream-s-1-1', role: 'Tutor', text: 'First sentence!' });

    vm = reduceVoiceLoopEvent(vm, { type: 'tutor-reply-streaming', sessionId: 's-1', turnId: 'stream-s-1-1', partialText: 'First sentence! Second part?' });
    expect(vm.latestReply?.spokenReply).toBe('First sentence! Second part?');
    expect(vm.turns).toHaveLength(1);

    vm = reduceVoiceLoopEvent(vm, {
      type: 'user-transcript-ready', sessionId: 's-1', confidence: 0.96,
      turn: { id: 'u-1', sessionId: 's-1', role: 'User', text: 'Hello!', timestampUnixMs: 100 },
    });
    expect(vm.turns.map((t) => t.role)).toEqual(['User', 'Tutor']);

    vm = reduceVoiceLoopEvent(vm, {
      type: 'tutor-reply-ready', sessionId: 's-1',
      turn: { id: 't-1', sessionId: 's-1', role: 'Tutor', text: 'First sentence! Second part?', timestampUnixMs: 200 },
      reply: { spokenReply: 'First sentence! Second part?', correction: 'Try greeting warmly.', followUpPrompt: 'How are you?' },
    });
    expect(vm.turns).toHaveLength(2);
    expect(vm.turns.map((t) => ({ id: t.id, role: t.role }))).toEqual([
      { id: 'u-1', role: 'User' },
      { id: 't-1', role: 'Tutor' },
    ]);
    expect(vm.streamingTurnId).toBeNull();
    expect(vm.latestReply?.correction).toBe('Try greeting warmly.');
  });
});
