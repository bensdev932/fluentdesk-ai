import React from 'react';
import { SpeakingRoom } from './components/SpeakingRoom';
import { isTauriRuntime, type SpeakingSessionPort } from './services/tauriService';

export interface AppProps {
  sessionClient: SpeakingSessionPort;
}

export function App({ sessionClient }: AppProps): React.JSX.Element {
  return (
    <main className="app-shell" aria-label="FluentDesk AI Speaking Room">
      <header className="studio-header">
        <div className="brand-group">
          <span className="brand-mark" aria-hidden="true" />
          <div>
            <h1>FluentDesk AI</h1>
            <p className="brand-subtitle">Voice-First English Speaking Studio</p>
          </div>
        </div>
        <span className="local-engine-pill">
          {isTauriRuntime() ? 'Local Engine · Private by Default' : 'Browser Studio · Web Speech & Local LLM'}
        </span>
      </header>
      <SpeakingRoom sessionClient={sessionClient} />
    </main>
  );
}
