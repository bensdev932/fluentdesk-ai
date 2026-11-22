import { describe, expect, it } from 'vitest';
import type { TranscriptItem } from './TranscriptPanel';

describe('TranscriptPanel item contracts (T027)', () => {
  it('clearly distinguishes User recognized speech from Tutor responses', () => {
    const items: TranscriptItem[] = [
      {
        id: 'turn-1',
        role: 'User',
        text: 'I like learning English.',
        timestampUnixMs: 1700000000000,
        confidence: 0.95,
      },
      {
        id: 'turn-2',
        role: 'Tutor',
        text: 'That is wonderful! What topics do you enjoy most?',
        timestampUnixMs: 1700000002000,
      },
    ];

    expect(items[0].role).toBe('User');
    expect(items[0].confidence).toBe(0.95);
    expect(items[1].role).toBe('Tutor');
    expect(items[1].confidence).toBeUndefined();
  });
});
