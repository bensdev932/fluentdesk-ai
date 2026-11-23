import React from 'react';
import { CORRECTION_MODES, type CorrectionMode } from '../types/domain';

const MODE_LABELS: Record<CorrectionMode, string> = {
  Gentle: 'Balanced coaching (grammar & natural phrasing)',
  Natural: 'Natural conversation (recast smoothly)',
  Detailed: 'Detailed grammar & native phrasing tips',
  Off: 'Off (pure fluency conversation only)',
};

export function formatCorrectionModeDescription(mode: CorrectionMode): string {
  return MODE_LABELS[mode];
}

export interface CorrectionModeSelectorProps {
  value: CorrectionMode;
  disabled: boolean;
  onChange(mode: CorrectionMode): void;
}

export function CorrectionModeSelector(p: CorrectionModeSelectorProps): React.JSX.Element {
  return (
    <section aria-label="Correction Mode" className="correction-mode-selector">
      <label htmlFor="correction-mode-select">Feedback Intensity</label>
      <select
        id="correction-mode-select"
        value={p.value}
        disabled={p.disabled}
        onChange={(e) => p.onChange(e.target.value as CorrectionMode)}
      >
        {CORRECTION_MODES.map((m) => (
          <option key={m} value={m}>
            {m} — {MODE_LABELS[m]}
          </option>
        ))}
      </select>
    </section>
  );
}
