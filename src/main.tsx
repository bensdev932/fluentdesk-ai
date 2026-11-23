import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './App';
import { createSpeakingSessionClient } from './services/speakingSessionClient';
import './styles/tokens.css';
import './styles/layout.css';
import './styles/controls.css';
import './styles/panels.css';
import './styles/providers.css';
import './styles/scripts.css';

const sessionClient = createSpeakingSessionClient();

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App sessionClient={sessionClient} />
  </React.StrictMode>,
);
