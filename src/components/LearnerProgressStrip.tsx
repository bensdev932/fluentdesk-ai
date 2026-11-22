import React from 'react';
import type { LearnerProgress } from '../types/domain';

export interface LearnerProgressStripProps {
  progress: LearnerProgress;
}

export function LearnerProgressStrip({ progress }: LearnerProgressStripProps): React.JSX.Element {
  return (
    <section aria-label="English Learner Progress" className="progress-strip">
      <div className="progress-stat">
        <span className="progress-value">{progress.practiceMinutes}</span>
        <span className="progress-label">Minutes Practiced</span>
      </div>
      <div className="progress-stat">
        <span className="progress-value">{progress.completedGoalsCount}</span>
        <span className="progress-label">Goals Completed</span>
      </div>
      <div className="progress-stat">
        <span className="progress-value">{progress.vocabularyUsed.length}</span>
        <span className="progress-label">English Vocab Used</span>
      </div>
      <div className="progress-stat">
        <span className="progress-value">{progress.dayStreak}</span>
        <span className="progress-label">Day Streak</span>
      </div>
    </section>
  );
}
