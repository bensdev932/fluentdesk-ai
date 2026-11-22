import React from 'react';
import type { LearnerFact } from '../types/domain';

export interface LearnerFactsCardProps {
  facts: LearnerFact[];
  onDeleteFact(factId: string): void;
  onClearFacts(): void;
}

export function LearnerFactsCard({ facts, onDeleteFact, onClearFacts }: LearnerFactsCardProps): React.JSX.Element {
  return (
    <section aria-label="Learner Memory" className="session-history-panel">
      <div className="review-estimate-head">
        <h2>Learner Memory</h2>
        {facts.length > 0 && (
          <button type="button" className="btn-link-danger" onClick={onClearFacts}>Clear All</button>
        )}
      </div>
      <p className="privacy-note">Text-only remembered context used to personalize future roleplays.</p>
      {facts.length === 0 ? (
        <p className="privacy-note">No saved learner facts yet.</p>
      ) : (
        <ul className="facts-list">
          {facts.map((f) => (
            <li key={f.id} className="fact-row">
              <span>{f.factText}</span>
              <button type="button" aria-label={`Delete fact: ${f.factText}`} onClick={() => onDeleteFact(f.id)}>Delete</button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
