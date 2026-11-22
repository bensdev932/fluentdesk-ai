import React, { useState } from 'react';
import type { BrowserVoiceStatus } from './useBrowserVoiceSession';

export interface EmulatorVoiceBarProps {
  canSpeak: boolean;
  voice: BrowserVoiceStatus;
  onSubmitText(text: string): void;
  onSimulateSample(): void;
}

export function EmulatorVoiceBar(p: EmulatorVoiceBarProps): React.JSX.Element {
  const [draft, setDraft] = useState('');
  const previewText = p.voice.interimText || p.voice.finalText;
  const previewKind = p.voice.interimText ? 'interim' : p.voice.finalText ? 'final' : 'note';
  const inputVal = draft || p.voice.interimText || (!p.canSpeak ? p.voice.finalText : '');
  const submit = (e: React.FormEvent): void => {
    e.preventDefault();
    const cleaned = draft.trim();
    if (!cleaned || !p.canSpeak) return;
    setDraft('');
    p.onSubmitText(cleaned);
  };
  return (
    <section aria-label="Browser Voice Input" className="emulator-bar">
      <div className="emulator-status-row">
        <div className="mic-meter" aria-hidden="true">
          <span className="mic-meter-fill" style={{ width: `${Math.min(100, Math.round(p.voice.micLevel * 180))}%` }} />
        </div>
        <div className="live-heard-preview" data-stt-stage={previewKind} aria-live="polite">
          {previewText ? (
            <>
              <span className="stt-badge">{previewKind === 'interim' ? 'Hearing' : 'Heard'}</span>
              <span className="heard-text">“{previewText}”</span>
            </>
          ) : (
            <span className="voice-note">{p.voice.voiceNote || 'Speak or type your response when Listening'}</span>
          )}
        </div>
      </div>
      <form className="voice-input-row" onSubmit={submit}>
        <input type="text" value={inputVal} disabled={!p.canSpeak} placeholder={p.canSpeak ? 'Speak into mic, or type your English sentence here...' : 'Click Start Speaking first'} onChange={(e) => setDraft(e.target.value)} />
        <button type="submit" className="btn-secondary" disabled={!p.canSpeak || !draft.trim()}>Send</button>
        <button type="button" className="btn-accent" onClick={p.onSimulateSample} disabled={!p.canSpeak}>Speak Sample Turn</button>
      </form>
    </section>
  );
}
