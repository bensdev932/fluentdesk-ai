import { describe, expect, it } from 'vitest';
import { SESSION_STATES, type SessionState } from '../types/domain';
import { getSessionStatePresentation } from './SessionStateBadge';

describe('SessionStateBadge presentation model (T012)', () => {
  it('provides truthful labels and descriptions for every SessionState', () => {
    for (const state of SESSION_STATES) {
      const presentation = getSessionStatePresentation(state);
      expect(presentation.label.length).toBeGreaterThan(0);
      expect(presentation.description.length).toBeGreaterThan(0);
      expect(presentation.tone.length).toBeGreaterThan(0);
    }
  });

  it('marks microphoneActive true ONLY when state is Listening', () => {
    for (const state of SESSION_STATES) {
      const presentation = getSessionStatePresentation(state);
      if (state === 'Listening') {
        expect(presentation.microphoneActive).toBe(true);
      } else {
        expect(presentation.microphoneActive).toBe(false);
      }
    }
  });

  it('distinguishes Idle, Initializing, Listening, Processing, Thinking, Speaking, Paused, and Error', () => {
    const requiredStates: SessionState[] = [
      'Idle',
      'Initializing',
      'Listening',
      'Processing',
      'Thinking',
      'Speaking',
      'Paused',
      'Error',
    ];
    const labels = new Set(
      requiredStates.map((s) => getSessionStatePresentation(s).label),
    );
    expect(labels.size).toBe(requiredStates.length);
  });
});
