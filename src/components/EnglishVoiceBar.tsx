import React from 'react';
import type { EnglishVoiceOption } from '../services/browserVoice';
import { SPEAKING_SPEED_PRESETS } from '../types/domain';

export interface EnglishVoiceBarProps {
  speakingSpeed: number;
  englishVoiceId: string;
  voices: EnglishVoiceOption[];
  scriptEnabled?: boolean;
  onChangeSpeed(speed: number): void;
  onChangeVoice(voiceId: string): void;
  onToggleScript?(): void;
}

export function EnglishVoiceBar(p: EnglishVoiceBarProps): React.JSX.Element {
  return (
    <section aria-label="English Voice and Pace" className="correction-mode-selector">
      <label htmlFor="english-voice-select">English Voice &amp; Pace</label>
      <div className="scaffold-inputs">
        <select id="english-voice-select" aria-label="English Voice" value={p.englishVoiceId} onChange={(e) => p.onChangeVoice(e.target.value)}>
          {p.voices.map((v) => <option key={v.id} value={v.id}>{v.label}</option>)}
        </select>
        <select aria-label="Speaking Speed" value={String(p.speakingSpeed)} onChange={(e) => p.onChangeSpeed(Number(e.target.value))}>
          {SPEAKING_SPEED_PRESETS.map((s) => <option key={s} value={String(s)}>{s}x</option>)}
        </select>
        {p.onToggleScript && (
          <button type="button" className={`btn-secondary btn-xs ${p.scriptEnabled ? 'script-toggle-on' : ''}`} aria-pressed={Boolean(p.scriptEnabled)} title="Optional pre-conversation script and reply hints" onClick={p.onToggleScript}>
            {p.scriptEnabled ? 'Script: On' : 'Script: Off'}
          </button>
        )}
      </div>
    </section>
  );
}
