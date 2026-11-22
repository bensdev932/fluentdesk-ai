import React from 'react';
import type { SessionReview, SessionState } from '../types/domain';
import { SessionReviewCard } from './SessionReviewCard';

export interface VoiceOrbStageProps {
  state: SessionState;
  review: SessionReview | null;
  onBargeIn(): void;
  onRestart(): void;
  onReturnToHub(): void;
}

export function VoiceOrbStage(p: VoiceOrbStageProps): React.JSX.Element | null {
  if (p.state === 'Thinking' || p.state === 'Speaking') {
    return (
      <div className="voice-orb-action-bar">
        <button type="button" className="btn-accent" onClick={p.onBargeIn}>
          {p.state === 'Thinking' ? 'Cancel & Retry' : 'Interrupt Tutor & Speak Now'}
        </button>
      </div>
    );
  }
  if (p.state === 'Ended') {
    return (
      <section aria-label="Completed Session Summary" className="ended-review-stage">
        {p.review && <SessionReviewCard review={p.review} />}
        <div className="primary-buttons">
          <button type="button" className="btn-primary" onClick={p.onRestart}>Practice Again</button>
          <button type="button" className="btn-secondary" onClick={p.onReturnToHub}>Back to Practice Hub</button>
        </div>
      </section>
    );
  }
  return null;
}
