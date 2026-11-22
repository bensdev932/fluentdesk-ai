export function isTauriRuntime(): boolean {
  return '__TAURI_INTERNALS__' in (globalThis as unknown as Record<string, unknown>);
}

const LOOPBACK_URL_RE = /^http:\/\/127\.0\.0\.1:(\d{1,5})$/;

function readInjectedBridgeUrl(): string | null {
  const raw = (globalThis as { __FLUENTDESK_BRIDGE_URL__?: unknown }).__FLUENTDESK_BRIDGE_URL__;
  if (typeof raw !== 'string') return null;
  const match = LOOPBACK_URL_RE.exec(raw.trim());
  if (!match) return null;
  const port = Number(match[1]);
  return Number.isInteger(port) && port >= 1 && port <= 65535 ? match[0] : null;
}

export function resolveApiBaseUrl(): string {
  const env = (import.meta as unknown as { env?: Record<string, string | undefined> }).env;
  const configured = env?.VITE_FLUENTDESK_API_URL?.trim();
  if (configured) return configured.replace(/\/+$/, '');
  if (!isTauriRuntime()) return '';
  return readInjectedBridgeUrl() ?? 'http://127.0.0.1:3420';
}
