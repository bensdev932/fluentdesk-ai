import React from 'react';
import type { SessionReview } from '../types/domain';

export interface SessionReviewCardProps {
  review: SessionReview | null;
}

export function SessionReviewCard({ review }: SessionReviewCardProps): React.JSX.Element | null {
  if (!review) return null;
  return (
    <div className="review-estimate-card" aria-label="Session Review">
      <div className="review-estimate-head">
        <strong>Practice Reflection</strong>
        <span className="estimate-badge">{review.estimateLabel} · CEFR {review.cefrSignal}</span>
      </div>
      <p className="review-summary">{review.summary}</p>
      {review.grammarFocuses.length > 0 && (
        <p className="review-detail"><strong>Grammar focuses:</strong> {review.grammarFocuses.join(' · ')}</p>
      )}
      {review.vocabularyUsed.length > 0 && (
        <p className="review-detail"><strong>Target vocabulary used:</strong> {review.vocabularyUsed.join(', ')}</p>
      )}
    </div>
  );
}
