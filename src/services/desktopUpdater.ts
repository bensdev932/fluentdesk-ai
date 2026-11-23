import { isTauriRuntime } from './runtimeUrl';

export type DesktopUpdateState =
  | 'up_to_date'
  | 'update_available'
  | 'rate_limited'
  | 'development_environment'
  | 'checking'
  | 'installing'
  | 'error';

export interface DesktopUpdateSnapshot {
  status: DesktopUpdateState;
  currentVersion: string;
  latestVersion: string | null;
  notes: string | null;
  message: string;
}

export const CURRENT_APP_VERSION = '0.2.0';

type TauriInvoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

function getTauriInvoke(): TauriInvoke | null {
  const g = globalThis as { __TAURI_INTERNALS__?: { invoke?: unknown } };
  const fn = g.__TAURI_INTERNALS__?.invoke;
  return typeof fn === 'function' ? (fn as TauriInvoke) : null;
}

export function devUpdaterSnapshot(): DesktopUpdateSnapshot {
  return {
    status: 'development_environment',
    currentVersion: CURRENT_APP_VERSION,
    latestVersion: null,
    notes: null,
    message: 'Web / Dev mode — native updater disabled.',
  };
}

export async function checkDesktopUpdate(): Promise<DesktopUpdateSnapshot> {
  if (!isTauriRuntime()) return devUpdaterSnapshot();
  const invoke = getTauriInvoke();
  if (!invoke) {
    return { status: 'error', currentVersion: CURRENT_APP_VERSION, latestVersion: null, notes: null, message: 'Desktop bridge unavailable.' };
  }
  try {
    return (await invoke('check_desktop_update')) as DesktopUpdateSnapshot;
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    const rateLimited = msg.includes('429') || msg.toLowerCase().includes('rate limit');
    return {
      status: rateLimited ? 'rate_limited' : 'error',
      currentVersion: CURRENT_APP_VERSION,
      latestVersion: null,
      notes: null,
      message: msg || 'Update check failed.',
    };
  }
}

export async function installDesktopUpdate(): Promise<DesktopUpdateSnapshot> {
  if (!isTauriRuntime()) return devUpdaterSnapshot();
  const invoke = getTauriInvoke();
  if (!invoke) {
    return { status: 'error', currentVersion: CURRENT_APP_VERSION, latestVersion: null, notes: null, message: 'Desktop bridge unavailable.' };
  }
  return (await invoke('install_desktop_update')) as DesktopUpdateSnapshot;
}
