import React from 'react';
import {
  CEFR_LEVELS,
  type CefrLevel,
  type RoleplayScenario,
  type SessionPhase,
} from '../types/domain';

export interface SessionProgressPillProps {
  cefrLevel: CefrLevel;
  scenarioId: string;
  scenarios: RoleplayScenario[];
  topic: string;
  phase: SessionPhase;
  turnCount: number;
  maxTurns: number;
  editable: boolean;
  onChangeCefr: (level: CefrLevel) => void;
  onChangeScenario: (scenarioId: string) => void;
  onChangeTopic: (topic: string) => void;
}

export function SessionProgressPill(p: SessionProgressPillProps): React.JSX.Element {
  return (
    <div className="scaffold-pill" data-phase={p.phase}>
      <div className="scaffold-meta">
        <span className="scaffold-badge">{p.phase}</span>
        <span>Turn {p.turnCount}/{p.maxTurns}</span>
      </div>
      <div className="scaffold-inputs">
        <select aria-label="Roleplay Scenario" value={p.scenarioId} disabled={!p.editable} onChange={(e) => p.onChangeScenario(e.target.value)}>
          {p.scenarios.map((s) => <option key={s.id} value={s.id}>{s.title}</option>)}
        </select>
        <select aria-label="CEFR Level" value={p.cefrLevel} onChange={(e) => p.onChangeCefr(e.target.value as CefrLevel)}>
          {CEFR_LEVELS.map((lvl) => <option key={lvl} value={lvl}>{lvl}</option>)}
        </select>
        <input aria-label="Lesson Topic" type="text" value={p.topic} maxLength={80} disabled={!p.editable} placeholder="Everyday Conversation" onChange={(e) => p.onChangeTopic(e.target.value)} />
      </div>
    </div>
  );
}
