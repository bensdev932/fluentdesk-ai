import { afterEach, describe, expect, it } from 'vitest';
import { isTauriRuntime, resolveApiBaseUrl } from './runtimeUrl';

afterEach(() => {
  delete (globalThis as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  delete (globalThis as { __FLUENTDESK_BRIDGE_URL__?: unknown }).__FLUENTDESK_BRIDGE_URL__;
});

describe('runtimeUrl', () => {
  it('returns empty base URL in web browser mode', () => {
    expect(isTauriRuntime()).toBe(false);
    expect(resolveApiBaseUrl()).toBe('');
  });

  it('defaults to 127.0.0.1:3420 in Tauri mode when no bridge URL is injected', () => {
    (globalThis as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    expect(isTauriRuntime()).toBe(true);
    expect(resolveApiBaseUrl()).toBe('http://127.0.0.1:3420');
  });

  it('uses valid injected loopback bridge URL in Tauri mode', () => {
    (globalThis as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    (globalThis as { __FLUENTDESK_BRIDGE_URL__?: unknown }).__FLUENTDESK_BRIDGE_URL__ =
      'http://127.0.0.1:49152';
    expect(resolveApiBaseUrl()).toBe('http://127.0.0.1:49152');
  });

  it('rejects non-loopback or malformed injected bridge URLs and falls back safely', () => {
    (globalThis as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    for (const bad of [
      'https://evil.example:3420',
      'http://127.0.0.1:3420/path',
      'http://127.0.0.1:0',
      'http://127.0.0.1:99999',
      'javascript:alert(1)',
    ]) {
      (globalThis as { __FLUENTDESK_BRIDGE_URL__?: unknown }).__FLUENTDESK_BRIDGE_URL__ = bad;
      expect(resolveApiBaseUrl()).toBe('http://127.0.0.1:3420');
    }
  });
});
