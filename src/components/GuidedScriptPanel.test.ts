import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { GuidedScriptPanel } from './GuidedScriptPanel';

describe('GuidedScriptPanel', () => {
  it('renders nothing when pre-conversation script mode is disabled (default)', () => {
    const html = renderToStaticMarkup(
      createElement(GuidedScriptPanel, {
        scriptEnabled: false,
        state: 'Idle',
        scenarioId: 'cafe_order',
        cefrLevel: 'A1',
        latestReply: null,
        canSpeak: false,
        onListen: vi.fn(),
        onUseLine: vi.fn(),
      }),
    );
    expect(html).toBe('');
  });

  it('renders CEFR-tiered warmup script when enabled in Idle state', () => {
    const html = renderToStaticMarkup(
      createElement(GuidedScriptPanel, {
        scriptEnabled: true,
        state: 'Idle',
        scenarioId: 'cafe_order',
        cefrLevel: 'A1',
        latestReply: null,
        canSpeak: false,
        onListen: vi.fn(),
        onUseLine: vi.fn(),
      }),
    );
    expect(html).toContain('Pre-Conversation Script');
    expect(html).toContain('A1 (A1-A2)');
    expect(html).toContain('Can I get a medium latte, please?');
    expect(html).toContain('Listen');
  });

  it('renders Suggested Replies bar during active session using AI suggestedReplies when present', () => {
    const html = renderToStaticMarkup(
      createElement(GuidedScriptPanel, {
        scriptEnabled: true,
        state: 'Listening',
        scenarioId: 'cafe_order',
        cefrLevel: 'A1',
        latestReply: {
          spokenReply: 'Hot or iced?',
          suggestedReplies: ['Iced, please!', 'Hot latte is great.'],
        },
        canSpeak: true,
        onListen: vi.fn(),
        onUseLine: vi.fn(),
      }),
    );
    expect(html).toContain('Suggested Replies');
    expect(html).toContain('Iced, please!');
    expect(html).toContain('Use line');
  });
});
