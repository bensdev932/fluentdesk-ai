import { useEffect, useState } from 'react';
import type {
  AiProviderId,
  AiProviderPreference,
  ProviderDescriptorView,
  ProvidersSnapshot,
  RouterConfigPatch,
} from '../services/tauriService';
import { fetchProvidersSnapshot, updateProvidersConfig } from '../services/tutorApi';

export interface ProviderSettingsHandle {
  snapshot: ProvidersSnapshot | null;
  statusNote: string | null;
  busy: boolean;
  keyDrafts: Partial<Record<AiProviderId, string>>;
  modelDrafts: Partial<Record<AiProviderId, string>>;
  setKeyDraft(id: AiProviderId, value: string): void;
  setModelDraft(id: AiProviderId, value: string): void;
  toggleCloudOptIn(enabled: boolean): void;
  selectProvider(pref: AiProviderPreference): void;
  saveKeysAndModels(): void;
  clearKey(id: AiProviderId): void;
  retryCloudNow(): void;
}

export function getProviderStatusBadge(item: ProviderDescriptorView): { text: string; className: string } {
  if (item.authInvalid) return { text: 'Invalid Key', className: 'key-invalid' };
  if (item.cooldownSecs > 0) return { text: `Cooldown ${item.cooldownSecs}s`, className: 'key-cooldown' };
  if (item.id === 'workers_ai' && item.configured) return { text: 'Built-in Free', className: 'key-env' };
  if (item.keySource === 'env') return { text: 'Env', className: 'key-env' };
  if (item.keySource === 'runtime') return { text: 'Saved', className: 'key-runtime' };
  return { text: 'Not set', className: 'key-none' };
}

const DESKTOP_OPTIONS: ReadonlyArray<{ id: AiProviderId; label: string; isCloud: boolean }> = [
  { id: 'ollama', label: 'Ollama (Local Free)', isCloud: false },
  { id: 'groq', label: 'Groq (Free Cloud)', isCloud: true },
  { id: 'gemini', label: 'Gemini (Free Cloud)', isCloud: true },
  { id: 'openrouter', label: 'OpenRouter (Free Cloud)', isCloud: true },
];

export function getProviderSelectOptions(snap: ProvidersSnapshot | null): Array<{ id: AiProviderId; label: string; isCloud: boolean }> {
  const list = snap?.providers ?? [];
  if (list.some((p) => p.id === 'workers_ai')) return list;
  const byId = new Map(list.map((p) => [p.id, p]));
  return DESKTOP_OPTIONS.map((d) => byId.get(d.id) ?? d);
}

export function buildProviderActions(
  applyPatch: (patch: RouterConfigPatch, clearDrafts?: boolean) => Promise<void>,
  drafts: { keyDrafts: Partial<Record<AiProviderId, string>>; modelDrafts: Partial<Record<AiProviderId, string>> },
): Pick<ProviderSettingsHandle, 'toggleCloudOptIn' | 'selectProvider' | 'saveKeysAndModels' | 'clearKey' | 'retryCloudNow'> {
  return {
    toggleCloudOptIn: (cloudOptIn) => void applyPatch({ cloudOptIn }),
    selectProvider: (preferredProvider) => void applyPatch({ preferredProvider }),
    saveKeysAndModels: () => {
      const apiKeys = Object.fromEntries(Object.entries(drafts.keyDrafts).filter(([, v]) => Boolean(v?.trim())));
      const modelOverrides = Object.fromEntries(Object.entries(drafts.modelDrafts).filter(([, v]) => v !== undefined));
      void applyPatch({ apiKeys, modelOverrides }, true);
    },
    clearKey: (id) => void applyPatch({ clearKeys: [id] }),
    retryCloudNow: () => void applyPatch({ resetCooldowns: true }),
  };
}

export function useProviderSettings(): ProviderSettingsHandle {
  const [snapshot, setSnapshot] = useState<ProvidersSnapshot | null>(null);
  const [statusNote, setStatusNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [keyDrafts, setKeyDrafts] = useState<Partial<Record<AiProviderId, string>>>({});
  const [modelDrafts, setModelDrafts] = useState<Partial<Record<AiProviderId, string>>>({});

  useEffect(() => {
    void fetchProvidersSnapshot().then((snap) => {
      if (snap) setSnapshot(snap);
    });
  }, []);

  const applyPatch = async (patch: RouterConfigPatch, clearDrafts = false): Promise<void> => {
    setBusy(true);
    setStatusNote(null);
    try {
      const updated = await updateProvidersConfig(patch);
      setSnapshot(updated);
      if (clearDrafts) setKeyDrafts({});
    } catch (err) {
      setStatusNote(err instanceof Error ? err.message : 'Could not update provider settings.');
    } finally {
      setBusy(false);
    }
  };

  return {
    snapshot,
    statusNote,
    busy,
    keyDrafts,
    modelDrafts,
    setKeyDraft: (id, value) => setKeyDrafts((s) => ({ ...s, [id]: value })),
    setModelDraft: (id, value) => setModelDrafts((s) => ({ ...s, [id]: value })),
    ...buildProviderActions(applyPatch, { keyDrafts, modelDrafts }),
  };
}
