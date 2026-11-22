import React from 'react';
import type { CommunicativeGoal } from '../types/domain';

export interface ScenarioGoalBarProps {
  goals: CommunicativeGoal[];
  completedGoalIds: string[];
  targetVocabulary?: string[];
  usedVocabulary?: string[];
}

export function ScenarioGoalBar({ goals, completedGoalIds, targetVocabulary = [], usedVocabulary = [] }: ScenarioGoalBarProps): React.JSX.Element | null {
  if (goals.length === 0 && targetVocabulary.length === 0) return null;
  const doneSet = new Set(completedGoalIds);
  const usedSet = new Set(usedVocabulary.map((w) => w.toLowerCase()));
  return (
    <section aria-label="Communicative Goals" className="goal-bar">
      {goals.length > 0 && (
        <>
          <div className="goal-bar-head">
            <strong>Scenario Goals</strong>
            <span>{completedGoalIds.length}/{goals.length} completed</span>
          </div>
          <div className="goal-chip-list">
            {goals.map((g) => (
              <span key={g.id} className="goal-chip" data-done={doneSet.has(g.id)}>
                {doneSet.has(g.id) ? '✓ ' : '○ '}{g.description}
              </span>
            ))}
          </div>
        </>
      )}
      {targetVocabulary.length > 0 && (
        <div className="goal-chip-list">
          <strong className="vocab-chip-label">Target English Vocabulary:</strong>
          {targetVocabulary.map((w) => (
            <span key={w} className="goal-chip" data-done={usedSet.has(w.toLowerCase())}>
              {usedSet.has(w.toLowerCase()) ? '✓ ' : ''}{w}
            </span>
          ))}
        </div>
      )}
    </section>
  );
}
