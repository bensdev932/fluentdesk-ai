import React from 'react';
import type { SessionState } from '../types/domain';
import { getSessionStatePresentation } from './sessionStatePresentation';

export { getSessionStatePresentation } from './sessionStatePresentation';

export interface SessionStateBadgeProps {
  state: SessionState;
  errorMessage?: string;
}

export function SessionStateBadge({
  state,
  errorMessage,
}: SessionStateBadgeProps): React.JSX.Element {
  const info = getSessionStatePresentation(state);
  return (
    <section
      aria-live="polite"
      aria-label="Session status"
      className="session-state-badge"
      data-state={state}
      data-tone={info.tone}
    >
      <div className="acoustic-orb" aria-hidden="true">
        <span className="orb-ring" />
        <span className="orb-core" />
      </div>
      <div className="state-copy">
        <div className="state-header-row">
          <strong className="state-label">{info.label}</strong>
          <span className="mic-pill" data-mic-active={info.microphoneActive}>
            <span className="mic-dot" aria-hidden="true" />
            {info.microphoneActive ? 'Mic Live' : 'Mic Off'}
          </span>
        </div>
        <p className="state-description">
          {errorMessage && state === 'Error' ? errorMessage : info.description}
        </p>
      </div>
    </section>
  );
}
