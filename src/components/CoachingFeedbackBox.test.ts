import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { CoachingFeedbackBox } from './CoachingFeedbackBox';
import { TutorReplyCard } from './TutorReplyCard';
import { normalizeReply, normalizeTurn } from '../services/speakingSessionCodec';
import type { ConversationTurn, TutorReply } from '../types/domain';

describe('CoachingFeedbackBox & structured TutorReply codec', () => {
  it('renders Try saying, properWay, Listen button, and why when improvement exists', () => {
    const reply: TutorReply = {
      spokenReply: 'Sure thing! What size would you like?',
      properWay: 'Could I get a medium latte, please?',
      why: "'Could I get...' sounds warmer and more natural at a cafe counter.",
      followUpPrompt: 'What size would you like?',
    };
    const html = renderToStaticMarkup(
      createElement(TutorReplyCard, { reply, playbackWarning: null, onReplay: () => undefined }),
    );
    expect(html).toContain('Try saying');
    expect(html).toContain('Could I get a medium latte, please?');
    expect(html).toContain('Listen');
    expect(html).toContain('sounds warmer and more natural');
  });

  it('hides coaching box cleanly when neither properWay nor correction is present', () => {
    const reply: TutorReply = { spokenReply: 'Got it, one iced oat latte!' };
    const html = renderToStaticMarkup(createElement(CoachingFeedbackBox, { reply }));
    expect(html).toBe('');
  });

  it('falls back gracefully to legacy correction and preserves fields in codec normalizers', () => {
    const legacyHtml = renderToStaticMarkup(
      createElement(CoachingFeedbackBox, {
        reply: { spokenReply: 'Nice!', correction: 'Say "I went" instead of "I go".' },
      }),
    );
    expect(legacyHtml).toContain('Coaching Tip');
    expect(legacyHtml).toContain('Say &quot;I went&quot; instead of &quot;I go&quot;.');

    const normReply = normalizeReply({
      spokenReply: 'Great!',
      properWay: '  I went yesterday.\u0007 ',
      why: ' Use past tense for finished time. ',
    });
    expect(normReply).toEqual({
      spokenReply: 'Great!',
      properWay: 'I went yesterday.',
      why: 'Use past tense for finished time.',
    });

    const normTurn = normalizeTurn({
      id: 't1',
      sessionId: 's1',
      role: 'Tutor',
      text: 'Great!',
      properWay: 'I went yesterday.',
      why: 'Use past tense.',
      timestampUnixMs: 123,
    } as ConversationTurn);
    expect(normTurn.properWay).toBe('I went yesterday.');
    expect(normTurn.why).toBe('Use past tense.');
  });
});
