import React, { useCallback, useEffect, useRef, useState } from 'react';
import { checkDesktopUpdate, CURRENT_APP_VERSION, devUpdaterSnapshot, installDesktopUpdate, type DesktopUpdateSnapshot } from '../services/desktopUpdater';
import { isTauriRuntime } from '../services/runtimeUrl';

const FOCUS_THROTTLE_MS = 5 * 60 * 1000;
const POLL_INTERVAL_MS = 30 * 60 * 1000;

export function DesktopUpdateBar(): React.JSX.Element {
  const [snap, setSnap] = useState<DesktopUpdateSnapshot>(devUpdaterSnapshot);
  const tauri = isTauriRuntime();
  const lastCheckRef = useRef(0);
  const installingVerRef = useRef<string | null>(null);
  const runAutoCheck = useCallback(async (force = false) => {
    const now = Date.now();
    if (!force && now - lastCheckRef.current < FOCUS_THROTTLE_MS) return;
    if (installingVerRef.current) return;
    lastCheckRef.current = now;
    setSnap((prev) => ({ ...prev, status: 'checking', message: 'Checking for updates...' }));
    const res = await checkDesktopUpdate();
    if (res.status === 'update_available' && res.latestVersion && installingVerRef.current !== res.latestVersion) {
      installingVerRef.current = res.latestVersion;
      setSnap({ ...res, status: 'installing', message: `Updating to v${res.latestVersion}...` });
      try {
        setSnap(await installDesktopUpdate());
      } catch (err) {
        installingVerRef.current = null;
        setSnap((prev) => ({ ...prev, status: 'error', message: err instanceof Error ? err.message : String(err) }));
      }
      return;
    }
    setSnap(res);
  }, []);
  useEffect(() => {
    if (!tauri) return;
    void runAutoCheck(true);
    const onFocus = (): void => { void runAutoCheck(false); };
    const timer = window.setInterval(() => { void runAutoCheck(true); }, POLL_INTERVAL_MS);
    window.addEventListener('focus', onFocus);
    return () => { window.removeEventListener('focus', onFocus); window.clearInterval(timer); };
  }, [tauri, runAutoCheck]);
  if (!tauri) return <span className="updater-pill" title={snap.message}>v{CURRENT_APP_VERSION} · Web Mode</span>;
  return (
    <div className="updater-bar" aria-label="Desktop Auto-Updater">
      <span className="updater-pill" title={snap.message}>v{snap.currentVersion} · {snap.message}</span>
    </div>
  );
}
