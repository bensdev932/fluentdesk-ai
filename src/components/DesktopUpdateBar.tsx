import React, { useCallback, useEffect, useState } from 'react';
import { checkDesktopUpdate, CURRENT_APP_VERSION, devUpdaterSnapshot, installDesktopUpdate, type DesktopUpdateSnapshot } from '../services/desktopUpdater';
import { isTauriRuntime } from '../services/runtimeUrl';

export function DesktopUpdateBar(): React.JSX.Element {
  const [snap, setSnap] = useState<DesktopUpdateSnapshot>(devUpdaterSnapshot);
  const tauri = isTauriRuntime();
  const runCheck = useCallback(async () => {
    setSnap((prev) => ({ ...prev, status: 'checking', message: 'Checking for updates...' }));
    setSnap(await checkDesktopUpdate());
  }, []);
  useEffect(() => {
    if (!tauri) return;
    void runCheck();
    const onFocus = (): void => { void runCheck(); };
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, [tauri, runCheck]);
  const runInstall = async (): Promise<void> => {
    setSnap((prev) => ({ ...prev, status: 'installing', message: `Installing v${prev.latestVersion ?? ''}...` }));
    try { setSnap(await installDesktopUpdate()); } catch (err) {
      setSnap((prev) => ({ ...prev, status: 'error', message: err instanceof Error ? err.message : String(err) }));
    }
  };
  if (!tauri) return <span className="updater-pill" title={snap.message}>v{CURRENT_APP_VERSION} · Web Mode</span>;
  const busy = snap.status === 'checking' || snap.status === 'installing';
  const hasUpdate = snap.status === 'update_available' && snap.latestVersion;
  return (
    <div className="updater-bar" aria-label="Desktop Auto-Updater">
      <span className="updater-pill" title={snap.message}>v{snap.currentVersion} · {snap.message}</span>
      {hasUpdate ? (
        <button type="button" className="btn-accent btn-xs" disabled={busy} onClick={() => void runInstall()}>Update to v{snap.latestVersion} &amp; Restart</button>
      ) : (
        <button type="button" className="btn-secondary btn-xs" disabled={busy} onClick={() => void runCheck()}>{snap.status === 'checking' ? 'Checking...' : 'Check for Updates'}</button>
      )}
    </div>
  );
}
