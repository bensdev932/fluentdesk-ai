import React from 'react';
import { cefrToScriptTier, getPreConversationScript } from '../services/preConversationScripts';
import type { CefrLevel } from '../types/domain';

export interface PreConversationScriptCardProps {
  scenarioId: string;
  cefrLevel: CefrLevel;
  onListen: (text: string) => void;
}

export function PreConversationScriptCard(p: PreConversationScriptCardProps): React.JSX.Element {
  const tier = cefrToScriptTier(p.cefrLevel);
  const script = getPreConversationScript(p.scenarioId, p.cefrLevel);
  return (
    <section className="script-card" aria-label="Pre-Conversation Script">
      <div className="script-card-head">
        <strong>Pre-Conversation Script</strong>
        <span className="scaffold-badge">{p.cefrLevel} ({tier})</span>
      </div>
      <p className="script-summary">{script.summary}</p>
      <div className="script-lines">
        {script.lines.map((line, idx) => (
          <div key={idx} className="script-line" data-speaker={line.speaker}>
            <span className="script-speaker">{line.speaker}</span>
            <span className="script-line-text">{line.text}</span>
            <button type="button" className="btn-secondary btn-xs" onClick={() => p.onListen(line.text)}>Listen</button>
          </div>
        ))}
      </div>
    </section>
  );
}
