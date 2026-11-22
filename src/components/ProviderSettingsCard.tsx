import React, { useState } from 'react';
import type { AiProviderPreference } from '../services/tauriService';
import { ProviderKeysPanel } from './ProviderKeysPanel';
import { getProviderSelectOptions, type ProviderSettingsHandle } from './useProviderSettings';

export function ProviderSettingsCard({ cfg, defaultOpenKeys = false }: { cfg: ProviderSettingsHandle; defaultOpenKeys?: boolean }): React.JSX.Element {
  const [openKeys, setOpenKeys] = useState(defaultOpenKeys);
  const snap = cfg.snapshot;
  const hasWorkersAi = Boolean(snap?.providers.some((p) => p.id === 'workers_ai'));
  const readyCloud = snap?.providers.filter((p) => p.isCloud && p.configured).map((p) => p.label.split(' ')[0]) ?? [];
  return (
    <section aria-label="AI Provider Settings" className="provider-settings-card">
      <div className="card-header-row">
        <h2>AI Engine</h2>
        <button type="button" className="btn-secondary btn-xs" onClick={() => setOpenKeys((v) => !v)}>
          {openKeys ? 'Hide Keys' : 'Models & Keys'}
        </button>
      </div>
      <label className="provider-select-label">
        <span>Active Provider</span>
        <select value={snap?.preferredProvider ?? 'auto'} disabled={!snap || cfg.busy} onChange={(e) => cfg.selectProvider(e.target.value as AiProviderPreference)}>
          <option value="auto">{hasWorkersAi ? 'Auto Cycle (Workers AI → Cloud)' : 'Auto Cycle (Cloud → Local Ollama)'}</option>
          {getProviderSelectOptions(snap).map((p) => (
            <option key={p.id} value={p.id} disabled={p.isCloud && !snap?.cloudOptIn}>{p.label}</option>
          ))}
        </select>
      </label>
      <label className="cloud-consent-row">
        <input type="checkbox" checked={Boolean(snap?.cloudOptIn)} disabled={!snap || cfg.busy} onChange={(e) => cfg.toggleCloudOptIn(e.target.checked)} />
        <span>Cloud AI Opt-In {readyCloud.length > 0 ? `(Ready: ${readyCloud.join(', ')})` : '(Transcribed English text only — zero audio stored)'}</span>
      </label>
      {cfg.statusNote && <p role="status" className="playback-warning">{cfg.statusNote}</p>}
      {!snap && <p className="privacy-note">Local bridge offline — using offline local coach.</p>}
      {openKeys && <ProviderKeysPanel cfg={cfg} />}
    </section>
  );
}
