import { describe, expect, it } from 'vitest';
import {
  CORRECTION_MODES,
  SESSION_STATES,
  isMicrophoneCapturingState,
  isSessionActiveState,
  sanitizeUntrustedText,
  type ConversationTurn,
  type CorrectionMode,
  type SessionState,
  type TutorReply,
} from './domain';

describe('Domain contracts and voice-first state guards', () => {
  it('defines all canonical session states from the specification', () => {
    const expected: SessionState[] = [
      'Idle',
      'Initializing',
      'Listening',
      'Processing',
      'Thinking',
      'Speaking',
      'Paused',
      'Error',
      'Ended',
    ];
    expect(SESSION_STATES).toEqual(expected);
  });

  it('reports microphone capturing truthfully only when state is Listening', () => {
    for (const state of SESSION_STATES) {
      if (state === 'Listening') {
        expect(isMicrophoneCapturingState(state)).toBe(true);
      } else {
        expect(isMicrophoneCapturingState(state)).toBe(false);
      }
    }
  });

  it('identifies active vs terminal/idle session states', () => {
    expect(isSessionActiveState('Idle')).toBe(false);
    expect(isSessionActiveState('Ended')).toBe(false);
    expect(isSessionActiveState('Listening')).toBe(true);
    expect(isSessionActiveState('Speaking')).toBe(true);
    expect(isSessionActiveState('Paused')).toBe(true);
  });

  it('defines all four canonical correction modes', () => {
    const expected: CorrectionMode[] = ['Natural', 'Gentle', 'Detailed', 'Off'];
    expect(CORRECTION_MODES).toEqual(expected);
  });

  it('sanitizes untrusted model text by stripping control characters and bounding length', () => {
    const raw = 'Hello\u0000\u0007 learner! Let us practice speaking.';
    expect(sanitizeUntrustedText(raw, 100)).toBe(
      'Hello learner! Let us practice speaking.',
    );

    const reply: TutorReply = {
      spokenReply: sanitizeUntrustedText('Great job! What did you do next?', 500),
      correction: 'Say "I went to the store" instead of "I goed to the store".',
      followUpPrompt: 'Where did you go after that?',
    };
    expect(reply.spokenReply).toContain('Great job!');

    const turnWithCoaching: ConversationTurn = {
      id: 't-1',
      sessionId: 'sess-1',
      role: 'Tutor',
      text: 'Sounds fun! When did you go?',
      correction: 'Use past tense: "I went".',
      followUpPrompt: 'What did you order there?',
      timestampUnixMs: 1700000000000,
    };
    expect(turnWithCoaching.correction).toBe('Use past tense: "I went".');
    expect(turnWithCoaching.followUpPrompt).toBe('What did you order there?');
  });
});
