import { apiUrl, readErrorDetail, withTimeoutSignal } from './apiTransport';
import { buildClientHeaders, clearByokKey, saveByokKeys } from './clientIdentity';
import { isTauriRuntime, type ProvidersSnapshot, type RouterConfigPatch } from './tauriService';

const SETTINGS_TIMEOUT_MS = 5_000;

export async function fetchProvidersSnapshot(): Promise<ProvidersSnapshot | null> {
  const { signal, cleanup } = withTimeoutSignal(new AbortController().signal, SETTINGS_TIMEOUT_MS);
  try {
    const res = await fetch(apiUrl('/api/providers'), {
      method: 'GET',
      headers: buildClientHeaders(),
      signal,
    });
    return res.ok ? ((await res.json()) as ProvidersSnapshot) : null;
  } catch {
    return null;
  } finally {
    cleanup();
  }
}

export async function updateProvidersConfig(patch: RouterConfigPatch): Promise<ProvidersSnapshot> {
  let payload: RouterConfigPatch = patch;
  if (!isTauriRuntime()) {
    if (patch.apiKeys) saveByokKeys(patch.apiKeys);
    if (patch.clearKeys) patch.clearKeys.forEach((id) => clearByokKey(id));
    const { apiKeys: _a, clearKeys: _c, ...safePatch } = patch;
    payload = safePatch;
  }
  const { signal, cleanup } = withTimeoutSignal(new AbortController().signal, SETTINGS_TIMEOUT_MS);
  try {
    const res = await fetch(apiUrl('/api/providers/config'), {
      method: 'POST',
      headers: buildClientHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify(payload),
      signal,
    });
    if (!res.ok) throw new Error(await readErrorDetail(res));
    return (await res.json()) as ProvidersSnapshot;
  } finally {
    cleanup();
  }
}
