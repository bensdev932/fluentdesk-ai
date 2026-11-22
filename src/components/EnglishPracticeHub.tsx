import React from 'react';
import type { RoleplayScenario } from '../types/domain';

export interface EnglishPracticeHubProps {
  scenarios: RoleplayScenario[];
  selectedScenarioId: string;
  onSelectScenario(scenarioId: string): void;
  onStartScenario(scenarioId: string): void;
}

export function EnglishPracticeHub(p: EnglishPracticeHubProps): React.JSX.Element {
  return (
    <section aria-label="English Roleplay Hub" className="practice-hub">
      <div className="goal-bar-head">
        <strong>Choose an English Speaking Scenario</strong>
        <span>{p.scenarios.length} pure-English roleplays</span>
      </div>
      <div className="scenario-card-grid">
        {p.scenarios.map((sc) => {
          const active = sc.id === p.selectedScenarioId;
          return (
            <article key={sc.id} className="scenario-card" data-selected={active} onClick={() => p.onSelectScenario(sc.id)}>
              <div className="review-estimate-head">
                <strong>{sc.title}</strong>
                <span className="scaffold-badge">{sc.defaultCefr}</span>
              </div>
              <p className="review-detail">{sc.summary}</p>
              <p className="review-detail"><strong>Tutor:</strong> {sc.tutorPersona} · <strong>You:</strong> {sc.learnerRole}</p>
              {sc.targetVocabulary.length > 0 && (
                <div className="goal-chip-list">
                  {sc.targetVocabulary.slice(0, 4).map((w) => <span key={w} className="goal-chip">{w}</span>)}
                </div>
              )}
              <button type="button" className="btn-primary scenario-start-btn" onClick={(e) => { e.stopPropagation(); p.onStartScenario(sc.id); }}>
                Practice This Scenario
              </button>
            </article>
          );
        })}
      </div>
    </section>
  );
}
