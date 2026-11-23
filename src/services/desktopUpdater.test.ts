import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { App } from '../App';
import { createFakeSpeakingSessionPort } from '../test/voiceTestStubs';
import {
  checkDesktopUpdate,
  CURRENT_APP_VERSION,
  installDesktopUpdate,
} from './desktopUpdater';

afterEach(() => {
  delete (globalThis as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
});

describe('desktopUpdater', () => {
  it('returns development_environment when running outside Tauri runtime', async () => {
    const snap = await checkDesktopUpdate();
    expect(snap.status).toBe('development_environment');
    expect(snap.currentVersion).toBe(CURRENT_APP_VERSION);
    expect(snap.message).toContain('disabled');
  });

  it('returns up_to_date when Tauri updater finds no newer release', async () => {
    const invoke = vi.fn(async () => ({
      status: 'up_to_date',
      currentVersion: CURRENT_APP_VERSION,
      latestVersion: null,
      notes: null,
      message: `Up to date (v${CURRENT_APP_VERSION}).`,
    }));
    (globalThis as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };

    const snap = await checkDesktopUpdate();
    expect(invoke).toHaveBeenCalledWith('check_desktop_update');
    expect(snap.status).toBe('up_to_date');
    expect(snap.message).toContain(CURRENT_APP_VERSION);
  });

  it('returns update_available and triggers install_desktop_update when invoked', async () => {
    const invoke = vi.fn(async (cmd: string) =>
      cmd === 'check_desktop_update'
        ? { status: 'update_available', currentVersion: '0.1.0', latestVersion: '0.1.1', notes: 'Fixes', message: 'Update v0.1.1 available' }
        : { status: 'installing', currentVersion: '0.1.0', latestVersion: '0.1.1', notes: null, message: 'Installing update' },
    );
    (globalThis as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };

    const check = await checkDesktopUpdate();
    expect(check.status).toBe('update_available');
    expect(check.latestVersion).toBe('0.1.1');

    const inst = await installDesktopUpdate();
    expect(invoke).toHaveBeenCalledWith('install_desktop_update');
    expect(inst.status).toBe('installing');
  });

  it('maps GitHub API rate limit errors cleanly to rate_limited status', async () => {
    const invoke = vi.fn(async () => {
      throw new Error('HTTP 429 API rate limit exceeded');
    });
    (globalThis as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };

    const snap = await checkDesktopUpdate();
    expect(snap.status).toBe('rate_limited');
    expect(snap.message).toContain('rate limit');
  });

  it('renders Desktop Auto-Updater status pill without an update button inside App header under Tauri runtime', () => {
    (globalThis as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke: vi.fn() };
    const html = renderToStaticMarkup(createElement(App, { sessionClient: createFakeSpeakingSessionPort() }));
    expect(html).toContain('Local Engine · Private by Default');
    expect(html).toContain('Desktop Auto-Updater');
    expect(html).toContain(`v${CURRENT_APP_VERSION}`);
    expect(html).not.toContain('Check for Updates');
    expect(html).not.toContain('Update to v');
  });
});
