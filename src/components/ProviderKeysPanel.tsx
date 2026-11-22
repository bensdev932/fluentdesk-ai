import React from 'react';
import type { ProviderDescriptorView } from '../services/tauriService';
import { getProviderStatusBadge, type ProviderSettingsHandle } from './useProviderSettings';

function ProviderConfigRow({ item, cfg }: { item: ProviderDescriptorView; cfg: ProviderSettingsHandle }): React.JSX.Element {
  const modelValue = cfg.modelDrafts[item.id] ?? item.selectedModel;
  const badge = getProviderStatusBadge(item);
  const showBadge = item.isCloud || item.authInvalid || item.cooldownSecs > 0;
  return (
    <div className="provider-key-row">
      <div className="provider-row-head">
        <strong>{item.label}</strong>
        {showBadge && <span className={`key-badge ${badge.className}`} title={item.statusDetail}>{badge.text}</span>}
      </div>
      <div className="provider-row-fields">
        {item.freeModels.length > 0 && (
          <select aria-label={`${item.label} preset`} value={item.freeModels.includes(modelValue) ? modelValue : ''} onChange={(e) => e.target.value && cfg.setModelDraft(item.id, e.target.value)}>
            <option value="">Custom model...</option>
            {item.freeModels.map((m) => <option key={m} value={m}>{m}</option>)}
          </select>
        )}
        <input type="text" aria-label={`${item.label} model`} value={modelValue} placeholder="Model ID" onChange={(e) => cfg.setModelDraft(item.id, e.target.value)} />
        {item.isCloud && item.id !== 'workers_ai' && (
          <div className="key-input-group">
            <input type="password" aria-label={`${item.label} API key`} value={cfg.keyDrafts[item.id] ?? ''} placeholder={item.configured ? 'Replace saved key...' : 'Paste free API key...'} onChange={(e) => cfg.setKeyDraft(item.id, e.target.value)} />
            {item.keySource === 'runtime' && <button type="button" className="btn-secondary btn-xs" onClick={() => cfg.clearKey(item.id)}>Clear</button>}
          </div>
        )}
      </div>
    </div>
  );
}

export function ProviderKeysPanel({ cfg }: { cfg: ProviderSettingsHandle }): React.JSX.Element | null {
  if (!cfg.snapshot) return null;
  return (
    <div className="provider-keys-drawer">
      {cfg.snapshot.providers.map((item) => <ProviderConfigRow key={item.id} item={item} cfg={cfg} />)}
      <div className="provider-drawer-actions">
        <button type="button" className="btn-primary btn-sm" disabled={cfg.busy} onClick={cfg.saveKeysAndModels}>
          {cfg.busy ? 'Saving...' : 'Save Models & Keys'}
        </button>
        <button type="button" className="btn-secondary btn-sm" disabled={cfg.busy} onClick={cfg.retryCloudNow}>
          Retry Cloud Now
        </button>
      </div>
    </div>
  );
}
